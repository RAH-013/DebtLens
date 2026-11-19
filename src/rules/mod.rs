pub mod duplicate_code;
pub mod file_length;
pub mod function_complexity;
pub mod function_length;
pub mod function_nesting;
pub mod function_parameters;
pub mod rule;
pub mod syntax;
pub mod todo;

use std::collections::HashMap;
use std::time::Instant;

use crate::model::{
    AiStatusSummary, AnalysisReport, Finding, ProjectAnalysis, Telemetry,
    calculate_technical_debt_index,
};
use crate::rules::duplicate_code::DuplicateCodeRule;
use crate::rules::file_length::FileLengthRule;
use crate::rules::function_complexity::FunctionComplexityRule;
use crate::rules::function_length::FunctionLengthRule;
use crate::rules::function_nesting::FunctionNestingRule;
use crate::rules::function_parameters::FunctionParametersRule;
use crate::rules::rule::Rule;
use crate::rules::syntax::SyntaxRule;
use crate::rules::todo::TodoRule;

pub fn default_rules() -> Vec<Box<dyn Rule>> {
    vec![
        Box::new(FunctionLengthRule::default()),
        Box::new(FunctionComplexityRule::default()),
        Box::new(FunctionParametersRule::default()),
        Box::new(DuplicateCodeRule),
        Box::new(FileLengthRule::default()),
        Box::new(FunctionNestingRule::default()),
        Box::new(SyntaxRule),
        Box::new(TodoRule),
    ]
}

pub fn analyze_with_progress<F>(
    analysis: ProjectAnalysis,
    on_rule_evaluated: F,
) -> AnalysisReport
where
    F: FnMut(usize, usize),
{
    analyze_with_rules(default_rules(), analysis, on_rule_evaluated)
}

/// Núcleo compartido por `analyze_with_progress`: acepta cualquier conjunto de
/// reglas para poder verificar (E1.10) que los IDs no dependen del orden.
pub fn analyze_with_rules<F>(
    rules: Vec<Box<dyn Rule>>,
    mut analysis: ProjectAnalysis,
    mut on_rule_evaluated: F,
) -> AnalysisReport
where
    F: FnMut(usize, usize),
{
    let total_rules = rules.len();
    let mut findings: Vec<Finding> = Vec::new();

    let rules_started = Instant::now();

    for (idx, rule) in rules.iter().enumerate() {
        findings.extend(rule.evaluate(&analysis));
        on_rule_evaluated(idx + 1, total_rules);
    }

    let rules_ms = rules_started.elapsed().as_secs_f64() * 1000.0;

    let telemetry = analysis.telemetry.get_or_insert_with(Telemetry::default);
    telemetry.rules_ms = rules_ms;

    assign_stable_ids(&mut findings);

    let debt_index = calculate_technical_debt_index(&analysis, &findings, None);

    AnalysisReport {
        analysis,
        findings,
        debt_index,
        ai_status: AiStatusSummary::disabled(),
        ai_analysis: None,
    }
}

/// Asigna IDs estables derivados de `(rule_id, file, line)` mediante FNV-1a
/// 64-bit (E1.10): reordenar las reglas **no** renumera hallazgos. Las tuplas
/// idénticas (p. ej. `TODO` y `FIXME` en la misma línea) reciben un sufijo de
/// ocurrencia que tampoco depende del orden *entre* reglas, porque dos reglas
/// distintas jamás comparten `rule_id` dentro de una misma tupla.
fn assign_stable_ids(findings: &mut [Finding]) {
    let mut occurrences: HashMap<(String, String, usize), usize> = HashMap::new();
    let mut assigned: Vec<String> = Vec::with_capacity(findings.len());

    for finding in findings.iter() {
        let base = stable_finding_id(&finding.rule_id, &finding.file, finding.line);
        let count = occurrences
            .entry((
                finding.rule_id.clone(),
                finding.file.clone(),
                finding.line,
            ))
            .or_insert(0);
        *count += 1;

        assigned.push(if *count == 1 {
            base
        } else {
            format!("{base}-{count}")
        });
    }

    for (finding, id) in findings.iter_mut().zip(assigned) {
        finding.id = Some(id);
    }
}

/// Huella FNV-1a 64-bit (misma constante que el detector de clones) sobre
/// `rule_id \0 file \0 line`, recortada a 48 bits para una representación
/// fija de 12 dígitos hexadecimales.
fn stable_finding_id(rule_id: &str, file: &str, line: usize) -> String {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
    const MASK: u64 = 0x0000_FFFF_FFFF_FFFF;

    let mut hash = OFFSET;

    for byte in rule_id
        .bytes()
        .chain(std::iter::once(0))
        .chain(file.bytes())
        .chain(std::iter::once(0))
        .chain(line.to_le_bytes())
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(PRIME);
    }

    format!("F{:012x}", hash & MASK)
}

/// Constructores compactos para las pruebas de las reglas (E1.9/E1.10).
#[cfg(test)]
pub(crate) mod test_fixtures {
    use crate::model::{FileAnalysis, FunctionMetric, ProjectAnalysis, Telemetry};

    pub(crate) fn function(
        name: &str,
        start_line: usize,
        lines: usize,
        parameters: Option<usize>,
        complexity: Option<usize>,
        nesting_depth: usize,
    ) -> FunctionMetric {
        FunctionMetric {
            name: Some(name.to_string()),
            start_line,
            end_line: start_line + lines.saturating_sub(1),
            lines,
            parameters,
            complexity,
            nesting_depth,
        }
    }

    /// Proyecto de un archivo con 600 líneas (dispara FILE001) y las
    /// funciones indicadas.
    pub(crate) fn project(functions: Vec<FunctionMetric>) -> ProjectAnalysis {
        project_with_file(600, functions)
    }

    /// Proyecto de un archivo con la longitud de archivo indicada.
    pub(crate) fn project_with_file(
        lines: usize,
        functions: Vec<FunctionMetric>,
    ) -> ProjectAnalysis {
        let total_functions = functions.len();

        ProjectAnalysis {
            root: ".".to_string(),
            files_scanned: 1,
            total_lines: lines,
            total_bytes: 0,
            total_functions,
            telemetry: Some(Telemetry::default()),
            files: vec![FileAnalysis {
                path: "src/app.js".to_string(),
                language: "javascript".to_string(),
                lines,
                bytes: 0,
                syntax_errors: Vec::new(),
                functions,
                todo_markers: Vec::new(),
                code_blocks: Vec::new(),
            }],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::TodoMarker;
    use crate::rules::test_fixtures::{function, project, project_with_file};

    fn ids_by_anchor(
        rules: Vec<Box<dyn Rule>>,
        analysis: ProjectAnalysis,
    ) -> HashMap<(String, String, usize), String> {
        analyze_with_rules(rules, analysis, |_, _| {})
            .findings
            .into_iter()
            .filter_map(|finding| {
                let id = finding.id?;
                Some(((finding.rule_id, finding.file, finding.line), id))
            })
            .collect()
    }

    #[test]
    fn stable_ids_survive_rule_reordering() {
        let analysis = project(vec![
            function("larguísima", 3, 60, Some(7), Some(15), 6),
            function("sana", 100, 10, Some(1), Some(2), 1),
        ]);

        let forward = ids_by_anchor(default_rules(), analysis.clone());

        let mut reversed_rules = default_rules();
        reversed_rules.reverse();
        let backward = ids_by_anchor(reversed_rules, analysis);

        assert!(
            forward.len() >= 3,
            "la función problemática debe disparar varias reglas ({} hallazgos)",
            forward.len()
        );
        assert_eq!(
            forward, backward,
            "reordenar las reglas no debe renumerar hallazgos"
        );
    }

    #[test]
    fn identical_anchor_tuples_get_distinct_ids() {
        // Archivo por debajo del umbral de FILE001 para aislar los marcadores.
        let mut analysis = project_with_file(10, Vec::new());
        analysis.files[0].todo_markers = vec![
            TodoMarker {
                marker: "TODO".to_string(),
                line: 7,
            },
            TodoMarker {
                marker: "FIXME".to_string(),
                line: 7,
            },
        ];

        let report = analyze_with_rules(default_rules(), analysis, |_, _| {});
        let ids: Vec<String> = report.findings.iter().filter_map(|finding| finding.id.clone()).collect();

        assert_eq!(ids.len(), 2, "deben emitirse los dos marcadores");
        assert_ne!(
            ids[0], ids[1],
            "las tuplas idénticas (rule_id, file, line) no deben colisionar"
        );
        assert!(
            ids[1].starts_with(&format!("{}-", ids[0])),
            "la segunda ocurrencia debe recibir sufijo de ocurrencia"
        );
    }

    #[test]
    fn telemetry_rules_ms_is_recorded_by_application_layer() {
        let analysis = project(vec![function("larguísima", 3, 60, Some(7), Some(15), 6)]);
        let report = analyze_with_rules(default_rules(), analysis, |_, _| {});

        let telemetry = report.analysis.telemetry.expect("telemetría presente");
        assert!(telemetry.rules_ms.is_finite() && telemetry.rules_ms >= 0.0);
    }
}
