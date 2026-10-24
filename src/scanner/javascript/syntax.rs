use tree_sitter::Node;

pub(super) fn collect_syntax_errors(node: Node<'_>, errors: &mut Vec<usize>) {
    if node.is_error() || node.is_missing() {
        errors.push(node.start_position().row + 1);
    }

    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        collect_syntax_errors(child, errors);
    }
}
