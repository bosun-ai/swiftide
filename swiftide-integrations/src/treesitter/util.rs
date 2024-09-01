use tree_sitter::{Node, TreeCursor};

/// `print_node_tree `prints the tree of a node with its children
/// and their children recursively.
/// This is useful for debugging and understanding the structure of the tree.
pub fn print_node_tree(node: Node, source: &str, indent: usize) {
    let cursor = node.walk();
    print_node(&cursor, source, indent);
}

fn print_node(cursor: &TreeCursor, source: &str, indent: usize) {
    let node = cursor.node();
    let indent_str = " ".repeat(indent);

    println!("{}Node ({} children) {{", indent_str, node.child_count());
    println!("{}  id: {},", indent_str, node.id());
    println!("{}  kind: \"{}\",", indent_str, node.kind());

    // Print unnamed children
    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        println!(
            "{}  child: {} (field: {:?}, id: {}),",
            indent_str,
            child.kind(),
            node.field_name_for_child(i as u32),
            child.id()
        );
    }

    println!("{}}}", indent_str);

    // Recursively print children
    let mut child_cursor = cursor.clone();
    if child_cursor.goto_first_child() {
        loop {
            print_node(&child_cursor, source, indent + 2);
            if !child_cursor.goto_next_sibling() {
                break;
            }
        }
    }
}
