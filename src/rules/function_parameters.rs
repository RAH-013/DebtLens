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
                        id: None,
                        impact_factor: None,
                        depth: None,
                        roi_technical: None,
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

#[cfg(test)]
mod tests {
    use super::FunctionParametersRule;
    use crate::rules::rule::Rule;
    use crate::rules::test_fixtures::{function, project};

    #[test]
    fn exactly_five_parameters_does_not_trigger() {
        let analysis = project(vec![function("limite", 1, 10, Some(5), Some(2), 0)]);

        assert!(
            FunctionParametersRule::default()
                .evaluate(&analysis)
                .is_empty(),
            "FUNC003 debe disparar solo con > 5 parámetros"
        );
    }

    #[test]
    fn six_parameters_trigger() {
        let analysis = project(vec![function("excede", 1, 10, Some(6), Some(2), 0)]);
        let findings = FunctionParametersRule::default().evaluate(&analysis);

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_id, "FUNC003");
    }
}
