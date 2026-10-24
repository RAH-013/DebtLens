use crate::model::{Finding, ProjectAnalysis, Severity};
use crate::rules::rule::Rule;

pub struct FunctionParametersRule {
    pub max_parameters: usize,
}

impl Default for FunctionParametersRule {
    fn default() -> Self {
        Self { max_parameters: 5 }
    }
}

impl Rule for FunctionParametersRule {
    fn id(&self) -> &'static str {
        "FUNC003"
    }

    fn name(&self) -> &'static str {
        "Demasiados parámetros"
    }

    fn evaluate(&self, analysis: &ProjectAnalysis) -> Vec<Finding> {
        analysis
            .files
            .iter()
            .flat_map(|file| {
                file.functions.iter().filter_map(|function| {
                    let parameters = function.parameters?;

                    (parameters > self.max_parameters).then(|| Finding {
                        rule_id: self.id().to_string(),
                        title: self.name().to_string(),
                        description: format!(
                            "La función recibe {parameters} parámetros; el límite es {}.",
                            self.max_parameters
                        ),
                        file: file.path.clone(),
                        line: function.start_line,
                        locations: Vec::new(),
                        severity: Severity::Medium,
                        recommendation: "Considera agrupar parámetros relacionados en un objeto."
                            .to_string(),
                    })
                })
            })
            .collect()
    }
}
