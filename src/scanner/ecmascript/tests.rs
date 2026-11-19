use std::path::Path;

use super::{EcmaLanguage, analyze_tree, parse};
use crate::model::FileAnalysis;

fn analyze_with(source: &str, language: EcmaLanguage) -> FileAnalysis {
    let tree = parse(source, language).expect("el árbol sintáctico debe generarse");
    analyze_tree(Path::new("test.js"), source, &tree, language)
}

fn analyze(source: &str) -> FileAnalysis {
    analyze_with(source, EcmaLanguage::JavaScript)
}

fn analyze_ts(source: &str) -> FileAnalysis {
    analyze_with(source, EcmaLanguage::TypeScript)
}

fn function<'a>(file: &'a FileAnalysis, name: &str) -> &'a crate::model::FunctionMetric {
    file.functions
        .iter()
        .find(|function| function.name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("no se encontró la función `{name}`"))
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
    assert_eq!(file.functions[0].complexity, Some(2));
}

#[test]
fn counts_logical_operators_in_complexity() {
    let file = analyze(
        "function evaluate(a, b, c) {
            if (a && b || c) {
                return 1;
            }
            return 0;
        }",
    );

    assert_eq!(file.functions[0].complexity, Some(4));
}

#[test]
fn counts_nullish_coalescing_in_complexity() {
    let file = analyze("function pick(a, b) { return a ?? b; }");

    assert_eq!(file.functions[0].complexity, Some(2));
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

    assert_eq!(function(&file, "outer").complexity, Some(1));
    assert_eq!(function(&file, "inner").complexity, Some(2));
}

#[test]
fn measures_generators_independently_from_their_container() {
    let file = analyze(
        "function outer() {
            function* inner(value) {
                if (value) {
                    yield 1;
                }
            }
            return inner(true);
        }",
    );

    assert_eq!(function(&file, "outer").complexity, Some(1));
    assert_eq!(function(&file, "outer").nesting_depth, 0);
    assert_eq!(function(&file, "inner").complexity, Some(2));
}

#[test]
fn else_if_chain_does_not_multiply_nesting_depth() {
    let file = analyze(
        "function classify(value) {
            if (value === 1) {
                return 'one';
            } else if (value === 2) {
                return 'two';
            } else if (value === 3) {
                return 'three';
            } else {
                return 'many';
            }
        }",
    );

    let metric = function(&file, "classify");

    assert_eq!(metric.nesting_depth, 1);
    assert_eq!(metric.complexity, Some(4));
}

#[test]
fn nested_block_inside_else_if_still_increases_depth() {
    let file = analyze(
        "function run(a) {
            if (a === 1) {
                return 1;
            } else if (a === 2) {
                for (let index = 0; index < 3; index += 1) {
                    if (index) {
                        return index;
                    }
                }
            }
        }",
    );

    assert_eq!(function(&file, "run").nesting_depth, 3);
}

#[test]
fn supports_typescript_functions_and_optional_parameters() {
    let file = analyze_ts(
        "export function greet(name: string, times?: number): string {
            if (times && times > 1) {
                return name.repeat(times);
            }
            return name;
        }",
    );

    assert_eq!(file.language, "typescript");
    let metric = function(&file, "greet");

    assert_eq!(metric.parameters, Some(2));
    assert_eq!(metric.complexity, Some(3));
}

#[test]
fn typescript_interfaces_are_not_counted_as_functions() {
    let file = analyze_ts(
        "export interface User { id: number; name: string; }
         export type Role = 'admin' | 'user';
         export class Service { run(): void { return; } }",
    );

    assert_eq!(file.functions.len(), 1);
    assert_eq!(file.functions[0].name.as_deref(), Some("run"));
}

#[test]
fn supports_tsx_components() {
    let file = analyze_with(
        "export function Card({ title }: { title: string }) {
            return <div className=\"card\">{title}</div>;
        }",
        EcmaLanguage::Tsx,
    );

    assert_eq!(file.language, "tsx");
    assert_eq!(file.functions.len(), 1);
    assert_eq!(file.functions[0].parameters, Some(1));
}

#[test]
fn maps_extensions_to_languages() {
    assert_eq!(
        EcmaLanguage::from_extension("js"),
        Some(EcmaLanguage::JavaScript)
    );
    assert_eq!(
        EcmaLanguage::from_extension("cjs"),
        Some(EcmaLanguage::JavaScript)
    );
    assert_eq!(
        EcmaLanguage::from_extension("ts"),
        Some(EcmaLanguage::TypeScript)
    );
    assert_eq!(EcmaLanguage::from_extension("tsx"), Some(EcmaLanguage::Tsx));
    assert_eq!(EcmaLanguage::from_extension("json"), None);
}

#[test]
fn cyclomatic_complexity_matches_thesis_formula_exactly() {
    // E1.9 (regresión de v(G) exacta), según la ecuación del Capítulo IV:
    //   v(G) = 1 + if + for + while + catch + ternary + casos(case) + &&/||/??
    // Desglose de este fragmento:
    //   if(1) + for-of(1) + while(1) + case(2) + ternario(1) + &&(1) + ??(1) = 8
    //   => v(G) = 1 + 8 = 9
    let file = analyze(
        "function modelo(a, b, c) {
            if (a && b) {
                return 1;
            }
            for (const x of c) {
                while (x) {
                    break;
                }
            }
            switch (a) {
                case 1:
                    break;
                case 2:
                    break;
            }
            const t = a ? 1 : 2;
            return a ?? b;
        }",
    );

    assert_eq!(
        function(&file, "modelo").complexity,
        Some(9),
        "v(G) debe seguir exactamente la fórmula del Capítulo IV"
    );
}
