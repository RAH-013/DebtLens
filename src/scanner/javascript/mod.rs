mod complexity;
mod functions;
mod parameters;
mod syntax;

use anyhow::{Context, Result};
use std::path::Path;
use tree_sitter::Parser;

use crate::model::FileAnalysis;

#[cfg(test)]
mod tests;

pub fn analyze_file(path: &Path, source: &str) -> Result<FileAnalysis> {
    let mut parser = Parser::new();

    parser
        .set_language(&tree_sitter_javascript::LANGUAGE.into())
        .context("No se pudo inicializar el parser de JavaScript")?;

    let tree = parser
        .parse(source, None)
        .context("No se pudo analizar el archivo JavaScript")?;

    let root = tree.root_node();
    let mut syntax_errors = Vec::new();
    let mut functions = Vec::new();

    syntax::collect_syntax_errors(root, &mut syntax_errors);
    functions::collect_functions(root, source, &mut functions);

    syntax_errors.sort_unstable();
    syntax_errors.dedup();

    Ok(FileAnalysis {
        path: path.to_string_lossy().replace('\\', "/"),
        language: "javascript".to_string(),
        lines: source.lines().count(),
        bytes: source.len() as u64,
        syntax_errors,
        functions,
        todo_markers: Vec::new(),
        code_blocks: Vec::new(),
    })
}
