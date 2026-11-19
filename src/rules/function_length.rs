use crate::model::{Finding, ProjectAnalysis, Severity};
use crate::rules::rule::Rule;

pub struct FunctionLengthRule {
    pub max_lines: usize,
}

impl Default for FunctionLengthRule {
    fn default() -> Self {
        Self { max_lines: 50 }
    }
}

impl Rule for FunctionLengthRule {
    fn id(&self) -> &'static str {
        "FUNC001"
    }

    fn name(&self) -> &'static str {
        "Longitud excesiva de función"
    }

    fn evaluate(&self, analysis: &ProjectAnalysis) -> Vec<Finding> {
        analysis
            .files
            .iter()
            .flat_map(|file| {
                file.functions
                    .iter()
                    .filter(|function| function.lines > self.max_lines)
                    .map(|function| Finding {
                id: None,
                        impact_factor: None,
                        depth: None,
                        roi_technical: None,
                        rule_id: self.id().to_string(),
                        title: self.name().to_string(),
                        description: format!(
                            "La función ocupa {} líneas y supera el límite de {}.",
                            function.lines, self.max_lines
                        ),
                        file: file.path.clone(),
                        line: function.start_line,
                        locations: Vec::new(),
                        severity: Severity::Medium,
                        recommendation: "Evalúa dividir la función en unidades más pequeñas con responsabilidades claras.".to_string(),
                    })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::FunctionLengthRule;
    use crate::rules::rule::Rule;
    use crate::rules::test_fixtures::{function, project};

    #[test]
    fn exactly_fifty_lines_does_not_trigger() {
        let analysis = project(vec![function("limite", 1, 50, None, None, 0)]);

        assert!(
            FunctionLengthRule::default().evaluate(&analysis).is_empty(),
            "FUNC001 debe disparar solo con > 50 líneas"
        );
    }

    #[test]
    fn fifty_one_lines_triggers() {
        let analysis = project(vec![function("excede", 1, 51, None, None, 0)]);
        let findings = FunctionLengthRule::default().evaluate(&analysis);

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_id, "FUNC001");
        assert_eq!(findings[0].line, 1);
    }
}
