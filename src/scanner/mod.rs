pub mod javascript;

use anyhow::{Context, Result};
use ignore::{DirEntry, WalkBuilder};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tree_sitter::{Node, Parser};

use crate::model::{CodeBlockMetric, FileAnalysis, ProjectAnalysis, TodoMarker};

const MIN_DUPLICATE_STATEMENTS: usize = 5;
const MIN_DUPLICATE_LINES: usize = 5;
const MIN_DUPLICATE_TOKENS: usize = 12;

const FINGERPRINT_OFFSET: u64 = 0xcbf29ce484222325;
const FINGERPRINT_PRIME: u64 = 0x100000001b3;

fn should_visit(entry: &DirEntry) -> bool {
    if !entry.file_type().is_some_and(|kind| kind.is_dir()) {
        return true;
    }

    let name = entry.file_name().to_string_lossy();

    !matches!(
        name.as_ref(),
        ".git"
            | "node_modules"
            | "dist"
            | "build"
            | "coverage"
            | ".next"
            | ".turbo"
            | ".cache"
            | "vendor"
    )
}

fn language_for(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()? {
        "js" | "jsx" | "mjs" | "cjs" => Some("javascript"),
        _ => None,
    }
}

fn analyze_file(root: &Path, path: &Path, language: &str) -> Result<FileAnalysis> {
    let source = std::fs::read_to_string(path)
        .with_context(|| format!("No se pudo leer {}", path.display()))?;

    let relative_path = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");

    let mut file = match language {
        "javascript" => javascript::analyze_file(Path::new(&relative_path), &source)?,
        _ => anyhow::bail!("Lenguaje no compatible: {language}"),
    };

    let (_, todo_markers) = collect_comment_data(&source);

    file.todo_markers = todo_markers;
    file.code_blocks = collect_code_blocks(&source);

    Ok(file)
}

fn contains_marker(text: &str, marker: &str) -> bool {
    text.match_indices(marker).any(|(start, matched)| {
        let bytes = text.as_bytes();
        let end = start + matched.len();

        let valid_before =
            start == 0 || !(bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_');

        let valid_after =
            end == bytes.len() || !(bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_');

        valid_before && valid_after
    })
}

fn collect_comment_data(source: &str) -> (HashSet<usize>, Vec<TodoMarker>) {
    let mut parser = Parser::new();

    if parser
        .set_language(&tree_sitter_javascript::LANGUAGE.into())
        .is_err()
    {
        return (HashSet::new(), Vec::new());
    }

    let Some(tree) = parser.parse(source, None) else {
        return (HashSet::new(), Vec::new());
    };

    let mut comment_lines = HashSet::new();
    let mut markers = Vec::new();

    collect_comment_nodes(tree.root_node(), source, &mut comment_lines, &mut markers);

    markers.sort_by(|a, b| a.line.cmp(&b.line).then_with(|| a.marker.cmp(&b.marker)));
    markers.dedup_by(|a, b| a.line == b.line && a.marker == b.marker);

    (comment_lines, markers)
}

fn collect_comment_nodes(
    node: Node<'_>,
    source: &str,
    comment_lines: &mut HashSet<usize>,
    markers: &mut Vec<TodoMarker>,
) {
    if node.kind() == "comment" {
        let start_line = node.start_position().row + 1;
        let end_line = node.end_position().row + 1;

        comment_lines.extend(start_line..=end_line);

        if let Ok(text) = node.utf8_text(source.as_bytes()) {
            for (offset, line) in text.lines().enumerate() {
                let upper = line.to_ascii_uppercase();
                let line_number = start_line + offset;

                for marker in ["TODO", "FIXME"] {
                    if contains_marker(&upper, marker) {
                        markers.push(TodoMarker {
                            marker: marker.to_string(),
                            line: line_number,
                        });
                    }
                }
            }
        }
    }

    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        collect_comment_nodes(child, source, comment_lines, markers);
    }
}

#[cfg(test)]
fn collect_todo_markers(source: &str) -> Vec<TodoMarker> {
    collect_comment_data(source).1
}

fn is_executable_statement(node: Node<'_>) -> bool {
    matches!(
        node.kind(),
        "lexical_declaration"
            | "variable_declaration"
            | "expression_statement"
            | "return_statement"
            | "throw_statement"
            | "if_statement"
            | "switch_statement"
            | "for_statement"
            | "for_in_statement"
            | "while_statement"
            | "do_statement"
            | "try_statement"
            | "break_statement"
            | "continue_statement"
            | "function_declaration"
            | "class_declaration"
            | "debugger_statement"
            | "labeled_statement"
    )
}

fn normalize_ast_tokens(node: Node<'_>, source: &str, tokens: &mut Vec<String>) {
    if node.kind() == "comment" {
        return;
    }

    if node.child_count() == 0 {
        if let Ok(text) = node.utf8_text(source.as_bytes()) {
            let token = text.trim();

            if !token.is_empty() {
                tokens.push(token.to_string());
            }
        }

        return;
    }

    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        normalize_ast_tokens(child, source, tokens);
    }
}

fn fingerprint_content(content: &str) -> u64 {
    let mut fingerprint = FINGERPRINT_OFFSET;

    for byte in content.bytes() {
        fingerprint ^= u64::from(byte);
        fingerprint = fingerprint.wrapping_mul(FINGERPRINT_PRIME);
    }

    fingerprint
}

fn collect_statement_blocks(parent: Node<'_>, source: &str, blocks: &mut Vec<CodeBlockMetric>) {
    let mut statements = Vec::new();
    let mut cursor = parent.walk();

    for child in parent.named_children(&mut cursor) {
        if is_executable_statement(child) {
            statements.push(child);
        }
    }

    for window in statements.windows(MIN_DUPLICATE_STATEMENTS) {
        let start_line = window[0].start_position().row + 1;
        let end_line = window[window.len() - 1].end_position().row + 1;
        let line_count = end_line.saturating_sub(start_line) + 1;

        if line_count < MIN_DUPLICATE_LINES {
            continue;
        }

        let mut tokens = Vec::new();

        for statement in window {
            normalize_ast_tokens(*statement, source, &mut tokens);
        }

        if tokens.len() < MIN_DUPLICATE_TOKENS {
            continue;
        }

        let normalized_content = tokens.join(" ");

        blocks.push(CodeBlockMetric {
            fingerprint: fingerprint_content(&normalized_content),
            start_line,
            lines: line_count,
            normalized_content,
        });
    }

    let mut cursor = parent.walk();

    for child in parent.named_children(&mut cursor) {
        collect_statement_blocks(child, source, blocks);
    }
}

fn collect_code_blocks(source: &str) -> Vec<CodeBlockMetric> {
    let mut parser = Parser::new();

    if parser
        .set_language(&tree_sitter_javascript::LANGUAGE.into())
        .is_err()
    {
        return Vec::new();
    }

    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };

    let mut blocks = Vec::new();
    collect_statement_blocks(tree.root_node(), source, &mut blocks);

    blocks.sort_by(|a, b| {
        a.start_line
            .cmp(&b.start_line)
            .then_with(|| a.normalized_content.cmp(&b.normalized_content))
    });

    blocks.dedup_by(|a, b| {
        a.start_line == b.start_line && a.normalized_content == b.normalized_content
    });

    blocks
}

pub fn scan_project(path: &Path) -> Result<ProjectAnalysis> {
    let root = path
        .canonicalize()
        .with_context(|| format!("No existe la ruta {}", path.display()))?;

    anyhow::ensure!(root.is_dir(), "La ruta debe ser un directorio");

    let mut files = Vec::new();

    for entry in WalkBuilder::new(&root)
        .hidden(false)
        .filter_entry(should_visit)
        .build()
    {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                eprintln!("Advertencia de recorrido: {error}");
                continue;
            }
        };

        let file_path: PathBuf = entry.path().to_path_buf();

        if !file_path.is_file() {
            continue;
        }

        let Some(language) = language_for(&file_path) else {
            continue;
        };

        match analyze_file(&root, &file_path, language) {
            Ok(file) => files.push(file),
            Err(error) => {
                eprintln!(
                    "Advertencia: no se pudo analizar {}: {error:#}",
                    file_path.display()
                );
            }
        }
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(ProjectAnalysis {
        root: root.display().to_string(),
        files_scanned: files.len(),
        total_lines: files.iter().map(|file| file.lines).sum(),
        total_bytes: files.iter().map(|file| file.bytes).sum(),
        total_functions: files.iter().map(|file| file.functions.len()).sum(),
        files,
    })
}

#[cfg(test)]
mod tests {
    use super::{collect_code_blocks, collect_todo_markers};

    #[test]
    fn detects_repeated_sequence_of_statements() {
        let source = r#"
function first() {
    const a = 1;
    const b = 2;
    const c = 3;
    const d = 4;
    const e = 5;
}

function second() {
    const a = 1;
    const b = 2;
    const c = 3;
    const d = 4;
    const e = 5;
}
"#;

        let blocks = collect_code_blocks(source);

        assert!(blocks.len() >= 2);
        assert!(blocks.iter().any(|block| block.start_line == 3));
        assert!(blocks.iter().any(|block| block.start_line == 11));
    }

    #[test]
    fn ignores_comments_and_indentation_in_normalization() {
        let first = r#"
function first() {
    const a = 1;
    const b = 2;
    const c = 3;
    const d = 4;
    const e = 5;
}
"#;

        let second = r#"
function second() {
        const a = 1; // comentario
        const b = 2;
        const c = 3;
        const d = 4;
        const e = 5;
}
"#;

        let first_blocks = collect_code_blocks(first);
        let second_blocks = collect_code_blocks(second);

        assert!(first_blocks.iter().any(|a| {
            second_blocks.iter().any(|b| {
                a.normalized_content == b.normalized_content && a.fingerprint == b.fingerprint
            })
        }));
    }

    #[test]
    fn does_not_treat_object_properties_as_standalone_statements() {
        let source = r#"
const first = {
    method: "PUT",
    path: "/items",
    headers: {},
};

const second = {
    method: "PUT",
    path: "/items",
    headers: {},
};
"#;

        assert!(collect_code_blocks(source).is_empty());
    }

    #[test]
    fn detects_todo_and_fixme_markers_in_comments() {
        let source = "// TODO: revisar\n/* FIXME: corregir */";
        let markers = collect_todo_markers(source);

        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].marker, "TODO");
        assert_eq!(markers[0].line, 1);
        assert_eq!(markers[1].marker, "FIXME");
        assert_eq!(markers[1].line, 2);
    }

    #[test]
    fn ignores_todo_and_fixme_inside_strings() {
        let source = r#"
const message = "TODO: esto no es un comentario";
const other = 'FIXME: tampoco';
"#;

        assert!(collect_todo_markers(source).is_empty());
    }

    #[test]
    fn detects_markers_on_correct_lines_of_multiline_comments() {
        let source = "/*\n * TODO: revisar esta sección\n * FIXME: corregir\n */";
        let markers = collect_todo_markers(source);

        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].marker, "TODO");
        assert_eq!(markers[0].line, 2);
        assert_eq!(markers[1].marker, "FIXME");
        assert_eq!(markers[1].line, 3);
    }

    #[test]
    fn ignores_markers_inside_larger_words() {
        let source = "// NOTODO and FIXMEING are not markers";
        assert!(collect_todo_markers(source).is_empty());
    }
}
