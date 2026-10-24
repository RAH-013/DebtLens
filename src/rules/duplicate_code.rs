use std::collections::{BTreeMap, HashMap};

use crate::model::{Finding, FindingLocation, ProjectAnalysis, Severity};
use crate::rules::rule::Rule;

pub struct DuplicateCodeRule;

#[derive(Debug, PartialEq)]
struct DuplicateClassification {
    severity: Severity,
    reason: &'static str,
    recommendation: &'static str,
}

impl Rule for DuplicateCodeRule {
    fn id(&self) -> &'static str {
        "CODE001"
    }

    fn name(&self) -> &'static str {
        "Código duplicado"
    }

    fn evaluate(&self, analysis: &ProjectAnalysis) -> Vec<Finding> {
        let mut occurrences: HashMap<(u64, String), Vec<(FindingLocation, usize)>> = HashMap::new();

        for file in &analysis.files {
            for block in &file.code_blocks {
                occurrences
                    .entry((block.fingerprint, block.normalized_content.clone()))
                    .or_default()
                    .push((
                        FindingLocation {
                            file: file.path.clone(),
                            line: block.start_line,
                        },
                        block.lines,
                    ));
            }
        }

        let mut findings = Vec::new();

        for ((_fingerprint, content), occurrences) in occurrences {
            let mut by_file: BTreeMap<String, Vec<(usize, usize)>> = BTreeMap::new();

            for (location, lines) in occurrences {
                by_file
                    .entry(location.file)
                    .or_default()
                    .push((location.line, lines));
            }

            let mut consolidated = Vec::new();
            let mut longest_block = 0;

            for (file, mut blocks) in by_file {
                blocks.sort_unstable_by_key(|(line, _)| *line);

                let mut previous_end: Option<usize> = None;

                for (line, lines) in blocks {
                    if previous_end.is_some_and(|end| line <= end) {
                        continue;
                    }

                    consolidated.push(FindingLocation {
                        file: file.clone(),
                        line,
                    });

                    previous_end = Some(line.saturating_add(lines.saturating_sub(1)));
                    longest_block = longest_block.max(lines);
                }
            }

            if consolidated.len() < 2 {
                continue;
            }

            consolidated.sort_by(|a, b| a.file.cmp(&b.file).then_with(|| a.line.cmp(&b.line)));

            let classification = classify_duplicate(&content);
            let primary = &consolidated[0];
            let location_count = consolidated.len();

            findings.push(Finding {
                rule_id: self.id().to_string(),
                title: self.name().to_string(),
                description: format!(
                    "Se detectó una secuencia de instrucciones duplicada de aproximadamente {longest_block} líneas en {location_count} ubicaciones. {}",
                    classification.reason
                ),
                file: primary.file.clone(),
                line: primary.line,
                locations: consolidated,
                severity: classification.severity,
                recommendation: classification.recommendation.to_string(),
            });
        }

        findings.sort_by(|a, b| a.file.cmp(&b.file).then_with(|| a.line.cmp(&b.line)));

        findings
    }
}

fn classify_duplicate(content: &str) -> DuplicateClassification {
    let code = content.to_ascii_lowercase();

    let has_control_flow = [
        "if ",
        "if(",
        "switch",
        "case ",
        "for ",
        "for(",
        "while ",
        "while(",
        "do ",
        "&&",
        "||",
        "catch",
        "ternary_expression",
    ]
    .iter()
    .any(|token| code.contains(token));

    let has_sensitive_operation = [
        "auth",
        "authoriz",
        "permission",
        "isadmin",
        "is_admin",
        "password",
        "credential",
        "bcrypt",
        "jwt",
        "transaction",
        "rollback",
        "commit",
        "payment",
        "refund",
        "balance",
        "accesscontrol",
        "access_control",
    ]
    .iter()
    .any(|token| code.contains(token));

    let is_boilerplate = code.contains("console")
        || code.contains("logger")
        || code.contains("res.status")
        || code.contains("res.json")
        || code.contains("res.send");

    if has_sensitive_operation && has_control_flow {
        return DuplicateClassification {
            severity: Severity::High,
            reason: "La secuencia repetida combina decisiones de control con indicios de operaciones sensibles.",
            recommendation: "Revisa las condiciones, validaciones, permisos y efectos secundarios antes de refactorizar. Confirma que ambas ubicaciones deban tener el mismo comportamiento.",
        };
    }

    if is_boilerplate && !has_sensitive_operation {
        return DuplicateClassification {
            severity: Severity::Info,
            reason: "La secuencia repetida parece concentrarse en registros o respuestas sencillas.",
            recommendation: "Evalúa si extraer la lógica repetida a una función auxiliar aporta claridad, sin introducir abstracciones innecesarias.",
        };
    }

    if has_control_flow {
        return DuplicateClassification {
            severity: Severity::Medium,
            reason: "La secuencia repetida contiene decisiones o estructuras de control.",
            recommendation: "Compara las condiciones y sus efectos. Si el comportamiento es equivalente, considera extraer la lógica a una función reutilizable.",
        };
    }

    DuplicateClassification {
        severity: Severity::Low,
        reason: "Se encontró una secuencia de instrucciones repetida sin indicios suficientes de impacto elevado.",
        recommendation: "Evalúa si extraer el fragmento a una función compartida simplifica el mantenimiento.",
    }
}

#[cfg(test)]
mod tests {
    use super::{DuplicateCodeRule, classify_duplicate};
    use crate::model::{CodeBlockMetric, FileAnalysis, ProjectAnalysis, Severity};
    use crate::rules::rule::Rule;

    fn block(fingerprint: u64, line: usize, lines: usize, content: &str) -> CodeBlockMetric {
        CodeBlockMetric {
            fingerprint,
            start_line: line,
            lines,
            normalized_content: content.to_string(),
        }
    }

    fn file(path: &str, blocks: Vec<CodeBlockMetric>) -> FileAnalysis {
        FileAnalysis {
            path: path.to_string(),
            language: "javascript".to_string(),
            lines: 30,
            bytes: 500,
            syntax_errors: Vec::new(),
            functions: Vec::new(),
            todo_markers: Vec::new(),
            code_blocks: blocks,
        }
    }

    fn analysis(files: Vec<FileAnalysis>) -> ProjectAnalysis {
        ProjectAnalysis {
            root: ".".to_string(),
            files_scanned: files.len(),
            total_lines: files.iter().map(|file| file.lines).sum(),
            total_bytes: files.iter().map(|file| file.bytes).sum(),
            total_functions: 0,
            files,
        }
    }

    #[test]
    fn detects_duplicate_blocks_across_files() {
        let content = "const a = 1; const b = 2; const c = 3; const d = 4; const e = 5;";
        let project = analysis(vec![
            file("a.js", vec![block(10, 1, 5, content)]),
            file("b.js", vec![block(10, 8, 5, content)]),
        ]);

        let findings = DuplicateCodeRule.evaluate(&project);

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].locations.len(), 2);
        assert_eq!(findings[0].file, "a.js");
        assert_eq!(findings[0].line, 1);
        assert!(matches!(findings[0].severity, Severity::Low));
    }

    #[test]
    fn rejects_hash_collisions_with_different_content() {
        let project = analysis(vec![
            file("a.js", vec![block(42, 1, 5, "const a = 1;")]),
            file("b.js", vec![block(42, 8, 5, "const b = 2;")]),
        ]);

        assert!(DuplicateCodeRule.evaluate(&project).is_empty());
    }

    #[test]
    fn ignores_single_occurrences() {
        let project = analysis(vec![file("a.js", vec![block(42, 1, 5, "const a = 1;")])]);

        assert!(DuplicateCodeRule.evaluate(&project).is_empty());
    }

    #[test]
    fn preserves_actual_block_length() {
        let content = "if (user.active) { return user; }";
        let project = analysis(vec![
            file("a.js", vec![block(10, 2, 9, content)]),
            file("b.js", vec![block(10, 5, 9, content)]),
        ]);

        let findings = DuplicateCodeRule.evaluate(&project);

        assert_eq!(findings.len(), 1);
        assert!(findings[0].description.contains("9 líneas"));
    }

    #[test]
    fn does_not_count_overlapping_occurrences_in_one_file_twice() {
        let content = "const a = 1; const b = 2; const c = 3;";
        let project = analysis(vec![file(
            "a.js",
            vec![block(10, 1, 5, content), block(10, 2, 5, content)],
        )]);

        assert!(DuplicateCodeRule.evaluate(&project).is_empty());
    }

    #[test]
    fn classifies_repeated_messages_as_info() {
        let result = classify_duplicate(r#"console.error("Error al obtener tareas");"#);

        assert!(matches!(result.severity, Severity::Info));
    }

    #[test]
    fn classifies_repeated_conditionals_as_medium() {
        let result = classify_duplicate("if ( user . active ) { return user ; }");

        assert!(matches!(result.severity, Severity::Medium));
    }

    #[test]
    fn classifies_sensitive_control_logic_as_high() {
        let result =
            classify_duplicate("if ( ! user . isAdmin ) { throw new Error ( Unauthorized ) ; }");

        assert!(matches!(result.severity, Severity::High));
    }

    #[test]
    fn classifies_plain_repeated_code_as_low() {
        let result = classify_duplicate("const total = price * quantity;");

        assert!(matches!(result.severity, Severity::Low));
    }
}
