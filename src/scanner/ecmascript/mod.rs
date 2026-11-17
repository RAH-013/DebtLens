mod complexity;
mod functions;
mod parameters;
mod syntax;

use anyhow::{Context, Result};
use std::path::Path;
use tree_sitter::{Language, Parser, Tree};

use crate::model::FileAnalysis;

#[cfg(test)]
mod tests;

/// Lenguajes que comparten la familia de gramáticas ECMAScript.
///
/// El motor de métricas se implementa una sola vez y recibe la gramática
/// concreta por inyección, de modo que no existen reglas duplicadas para
/// JavaScript y TypeScript.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EcmaLanguage {
    JavaScript,
    TypeScript,
    Tsx,
}

impl EcmaLanguage {
    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension {
            "js" | "jsx" | "mjs" | "cjs" => Some(Self::JavaScript),
            "ts" | "mts" | "cts" => Some(Self::TypeScript),
            "tsx" => Some(Self::Tsx),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::JavaScript => "javascript",
            Self::TypeScript => "typescript",
            Self::Tsx => "tsx",
        }
    }

    fn grammar(self) -> Language {
        match self {
            Self::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            Self::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Self::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
        }
    }

    fn parser(self) -> Result<Parser> {
        let mut parser = Parser::new();

        parser
            .set_language(&self.grammar())
            .with_context(|| format!("No se pudo inicializar el parser de {}", self.label()))?;

        Ok(parser)
    }
}

/// Genera el árbol sintáctico una sola vez por archivo.
pub fn parse(source: &str, language: EcmaLanguage) -> Result<Tree> {
    language
        .parser()?
        .parse(source, None)
        .context("No se pudo generar el árbol sintáctico")
}

/// Extrae métricas de funciones y errores de sintaxis de un árbol ya parseado.
pub fn analyze_tree(
    path: &Path,
    source: &str,
    tree: &Tree,
    language: EcmaLanguage,
) -> FileAnalysis {
    let root = tree.root_node();

    let mut syntax_errors = Vec::new();
    let mut functions = Vec::new();

    syntax::collect_syntax_errors(root, &mut syntax_errors);
    functions::collect_functions(root, source, &mut functions);

    syntax_errors.sort_unstable();
    syntax_errors.dedup();

    FileAnalysis {
        path: path.to_string_lossy().replace('\\', "/"),
        language: language.label().to_string(),
        lines: source.lines().count(),
        bytes: source.len() as u64,
        syntax_errors,
        functions,
        todo_markers: Vec::new(),
        code_blocks: Vec::new(),
    }
}
