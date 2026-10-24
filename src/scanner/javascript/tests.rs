use std::path::Path;

use super::analyze_file;

fn analyze(source: &str) -> crate::model::FileAnalysis {
    analyze_file(Path::new("test.js"), source).unwrap()
}

#[test]
fn detects_function_declaration() {
    let file = analyze("function add(a, b) { return a + b; }");

    assert_eq!(file.functions.len(), 1);
    assert_eq!(file.functions[0].name.as_deref(), Some("add"));
    assert_eq!(file.functions[0].parameters, Some(2));
}

#[test]
fn detects_arrow_function() {
    let file = analyze("const add = (a, b) => a + b;");

    assert_eq!(file.functions.len(), 1);
    assert_eq!(file.functions[0].parameters, Some(2));
}

#[test]
fn measures_long_function() {
    let mut source = String::from("function longFunction() {\n");

    for _ in 0..55 {
        source.push_str("    performTask();\n");
    }

    source.push_str("}\n");

    let file = analyze(&source);

    assert!(file.functions[0].lines > 50);
}

#[test]
fn detects_syntax_errors() {
    let file = analyze("function broken( {");

    assert!(!file.syntax_errors.is_empty());
}

#[test]
fn accepts_empty_file() {
    let file = analyze("");

    assert_eq!(file.lines, 0);
    assert!(file.functions.is_empty());
    assert!(file.syntax_errors.is_empty());
}

#[test]
fn calculates_function_complexity() {
    let file = analyze(
        "function choose(value) {
            if (value) {
                return 1;
            }
            return 0;
        }",
    );

    assert_eq!(file.functions.len(), 1);
    assert!(file.functions[0].complexity.unwrap() >= 2);
}

#[test]
fn excludes_nested_function_complexity() {
    let file = analyze(
        "function outer() {
            function inner(value) {
                if (value) {
                    return 1;
                }
                return 0;
            }
            return inner(true);
        }",
    );

    let outer = file
        .functions
        .iter()
        .find(|function| function.name.as_deref() == Some("outer"))
        .unwrap();

    let inner = file
        .functions
        .iter()
        .find(|function| function.name.as_deref() == Some("inner"))
        .unwrap();

    assert_eq!(outer.complexity, Some(1));
    assert!(inner.complexity.unwrap() >= 2);
}
