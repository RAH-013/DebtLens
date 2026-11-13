pub mod duplicate_code;
pub mod file_length;
pub mod function_complexity;
pub mod function_length;
pub mod function_nesting;
pub mod function_parameters;
pub mod rule;
pub mod syntax;
pub mod todo;

use crate::model::{
    AiStatusSummary, AnalysisReport, Finding, ProjectAnalysis, calculate_technical_debt_index,
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
    mut on_rule_evaluated: F,
) -> AnalysisReport
where
    F: FnMut(usize, usize),
{
    let rules = default_rules();
    let total_rules = rules.len();
    let mut findings: Vec<Finding> = Vec::new();

    for (idx, rule) in rules.iter().enumerate() {
        findings.extend(rule.evaluate(&analysis));
        on_rule_evaluated(idx + 1, total_rules);
    }

    for (index, finding) in findings.iter_mut().enumerate() {
        finding.id = Some(format!("F{:03}", index + 1));
    }

    let debt_index = calculate_technical_debt_index(&analysis, &findings, None);

    AnalysisReport {
        analysis,
        findings,
        debt_index,
        ai_status: AiStatusSummary::disabled(),
        ai_analysis: None,
    }
}
