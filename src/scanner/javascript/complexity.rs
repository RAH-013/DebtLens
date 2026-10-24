use tree_sitter::Node;

use super::functions::is_function;

pub(super) fn calculate_complexity(node: Node<'_>, source: &str) -> usize {
    fn visit(node: Node<'_>, source: &str) -> usize {
        if is_function(node) {
            return 0;
        }

        let mut complexity = match node.kind() {
            "if_statement" | "for_statement" | "for_in_statement" | "while_statement"
            | "do_statement" | "catch_clause" | "ternary_expression" => 1,
            "switch_case" => {
                let text = node
                    .utf8_text(source.as_bytes())
                    .unwrap_or_default()
                    .trim_start();

                usize::from(text.starts_with("case "))
            }
            _ => 0,
        };

        if node.kind() == "binary_expression" {
            let mut cursor = node.walk();

            for child in node.children(&mut cursor) {
                if matches!(child.kind(), "&&" | "||") {
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
        );

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
