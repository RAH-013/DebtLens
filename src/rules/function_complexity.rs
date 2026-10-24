use crate::model::{Finding, ProjectAnalysis, Severity};
use crate::rules::rule::Rule;

pub struct FunctionComplexityRule {
    pub max_complexity: usize,
}

impl Default for FunctionComplexityRule {
    fn default() -> Self {
        Self { max_complexity: 10 }
    }
}

impl Rule for FunctionComplexityRule {
    fn id(&self) -> &'static str {
        "FUNC002"
    }

    fn name(&self) -> &'static str {
        "Complejidad ciclomática elevada"
    }

    fn evaluate(&self, analysis: &ProjectAnalysis) -> Vec<Finding> {
        analysis
            .files
            .iter()
            .flat_map(|file| {
                file.functions.iter().filter_map(|function| {
                    let complexity = function.complexity?;

                    (complexity > self.max_complexity).then(|| Finding {
                        rule_id: self.id().to_string(),
                        title: self.name().to_string(),
                        description: format!(
                            "La función tiene una complejidad de {complexity}; el límite es {}.",
                            self.max_complexity
                        ),
                        file: file.path.clone(),
                        line: function.start_line,
                        locations: Vec::new(),
                        severity: Severity::High,
                        recommendation:
                            "Reduce las decisiones y divide la lógica en funciones más pequeñas."
                                .to_string(),
                    })
                })
            })
            .collect()
    }
}
