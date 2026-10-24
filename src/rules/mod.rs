pub mod duplicate_code;
pub mod file_length;
pub mod function_complexity;
pub mod function_length;
pub mod function_nesting;
pub mod function_parameters;
pub mod rule;
pub mod syntax;
pub mod todo;

use crate::model::{AnalysisReport, ProjectAnalysis};
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

pub fn analyze(analysis: ProjectAnalysis) -> AnalysisReport {
    let findings = default_rules()
        .iter()
        .flat_map(|rule| rule.evaluate(&analysis))
        .collect();

    AnalysisReport { analysis, findings }
}
