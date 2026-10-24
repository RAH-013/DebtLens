use tree_sitter::Node;

use crate::model::FunctionMetric;

use super::complexity::{calculate_complexity, calculate_nesting};
use super::parameters::{count_parameters, find_parameters};

pub(super) fn collect_functions(node: Node<'_>, source: &str, functions: &mut Vec<FunctionMetric>) {
    if is_function(node) {
        let start_line = node.start_position().row + 1;
        let end_line = node.end_position().row + 1;

        functions.push(FunctionMetric {
            name: find_function_name(node, source),
            start_line,
            end_line,
            lines: end_line - start_line + 1,
            parameters: find_parameters(node).map(count_parameters),
            complexity: Some(calculate_complexity(node, source)),
            nesting_depth: calculate_nesting(node),
        });
    }

    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        collect_functions(child, source, functions);
    }
}

pub(super) fn is_function(node: Node<'_>) -> bool {
    matches!(
        node.kind(),
        "function_declaration" | "function_expression" | "arrow_function" | "method_definition"
    )
}

fn find_function_name(node: Node<'_>, source: &str) -> Option<String> {
    if !matches!(
        node.kind(),
        "function_declaration" | "function_expression" | "method_definition"
    ) {
        return None;
    }

    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        if matches!(child.kind(), "identifier" | "property_identifier") {
            return child.utf8_text(source.as_bytes()).ok().map(str::to_string);
        }

        if matches!(child.kind(), "formal_parameters" | "statement_block") {
            break;
        }
    }

    None
}
