use tree_sitter::Node;

use super::functions::is_function;

/// Operadores lógicos que suman una decisión en la complejidad ciclomática.
const LOGICAL_OPERATORS: [&str; 3] = ["&&", "||", "??"];

fn is_decision(node: Node<'_>) -> bool {
    match node.kind() {
        "if_statement" | "for_statement" | "for_in_statement" | "while_statement"
        | "do_statement" | "catch_clause" | "ternary_expression" | "switch_case" => true,
        _ => false,
    }
}

/// Un `if` que cuelga directamente de un `else_clause` es la continuación de una
/// cadena `else if`: añade una decisión a v(G) pero **no** un nivel de anidamiento.
fn is_else_if_continuation(node: Node<'_>) -> bool {
    node.kind() == "if_statement"
        && node
            .parent()
            .is_some_and(|parent| parent.kind() == "else_clause")
}

pub(super) fn calculate_complexity(node: Node<'_>, source: &str) -> usize {
    fn visit(node: Node<'_>, source: &str) -> usize {
        if is_function(node) {
            return 0;
        }

        let mut complexity = usize::from(is_decision(node));

        if node.kind() == "binary_expression" {
            let mut cursor = node.walk();

            for child in node.children(&mut cursor) {
                if LOGICAL_OPERATORS.contains(&child.kind()) {
                    complexity += 1;
                }
            }
        }

        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            complexity += visit(child, source);
        }

        complexity
    }

    let mut complexity = 1;
    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        complexity += visit(child, source);
    }

    complexity
}

pub(super) fn calculate_nesting(node: Node<'_>) -> usize {
    fn visit(node: Node<'_>, depth: usize, maximum: &mut usize) {
        if is_function(node) {
            return;
        }

        let nested = matches!(
            node.kind(),
            "if_statement"
                | "for_statement"
                | "for_in_statement"
                | "while_statement"
                | "do_statement"
                | "switch_statement"
                | "catch_clause"
                | "ternary_expression"
        ) && !is_else_if_continuation(node);

        let current = depth + usize::from(nested);
        *maximum = (*maximum).max(current);

        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            visit(child, current, maximum);
        }
    }

    let mut maximum = 0;
    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        visit(child, 0, &mut maximum);
    }

    maximum
}
