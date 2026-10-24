use tree_sitter::Node;

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

            if child.kind() == "parenthesized_expression" {
                let mut inner_cursor = child.walk();

                for inner in child.children(&mut inner_cursor) {
                    if inner.kind() == "formal_parameters" {
                        return Some(inner);
                    }
                }
            }
        }
    }

    None
}

pub(super) fn count_parameters(node: Node<'_>) -> usize {
    if matches!(
        node.kind(),
        "identifier" | "object_pattern" | "array_pattern"
    ) {
        return 1;
    }

    let mut count = 0;
    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        if matches!(
            child.kind(),
            "identifier"
                | "assignment_pattern"
                | "object_pattern"
                | "array_pattern"
                | "rest_pattern"
        ) {
            count += 1;
        }
    }

    count
}
