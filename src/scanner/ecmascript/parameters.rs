use tree_sitter::Node;

/// Nodos que representan un parámetro individual en las gramáticas
/// JavaScript y TypeScript.
fn is_parameter_node(kind: &str) -> bool {
    matches!(
        kind,
        "identifier"
            | "assignment_pattern"
            | "object_pattern"
            | "array_pattern"
            | "rest_pattern"
            | "required_parameter"
            | "optional_parameter"
            | "this_parameter"
    )
}

pub(super) fn find_parameters(node: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        if child.kind() == "formal_parameters" {
            return Some(child);
        }
    }

    if node.kind() == "arrow_function" {
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            if matches!(
                child.kind(),
                "identifier" | "object_pattern" | "array_pattern"
            ) {
                return Some(child);
            }
        }
    }

    None
}

pub(super) fn count_parameters(node: Node<'_>) -> usize {
    // `required_parameter` / `optional_parameter` envuelven un único parámetro.
    if is_parameter_node(node.kind()) && node.kind() != "assignment_pattern" {
        return 1;
    }

    let mut count = 0;
    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        if is_parameter_node(child.kind()) {
            count += 1;
        }
    }

    count
}
