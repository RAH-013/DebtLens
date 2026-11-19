use crate::model::{Finding, ProjectAnalysis, Severity};
use crate::rules::rule::Rule;

pub struct FunctionNestingRule {
    pub max_depth: usize,
}

impl Default for FunctionNestingRule {
    fn default() -> Self {
        Self { max_depth: 4 }
    }
}

impl Rule for FunctionNestingRule {
    fn id(&self) -> &'static str {
        "FUNC004"
    }

    fn name(&self) -> &'static str {
        "Anidamiento excesivo"
    }

    fn evaluate(&self, analysis: &ProjectAnalysis) -> Vec<Finding> {
        analysis
            .files
            .iter()
            .flat_map(|file| {
                file.functions
                    .iter()
                    .filter(|function| function.nesting_depth > self.max_depth)
                    .map(|function| Finding {
                id: None,
                        impact_factor: None,
                        depth: None,
                        roi_technical: None,
                        rule_id: self.id().to_string(),
                        title: self.name().to_string(),
                        description: format!(
                            "La función alcanza una profundidad de anidamiento de {}; el límite es {}.",
                            function.nesting_depth, self.max_depth
                        ),
                        file: file.path.clone(),
                        line: function.start_line,
                        locations: Vec::new(),
                        severity: Severity::Medium,
                        recommendation: "Reduce los niveles de anidamiento mediante retornos tempranos o funciones auxiliares.".to_string(),
                    })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::FunctionNestingRule;
    use crate::rules::rule::Rule;
    use crate::rules::test_fixtures::{function, project};

    #[test]
    fn exactly_four_levels_does_not_trigger() {
        let analysis = project(vec![function("limite", 1, 30, Some(2), Some(4), 4)]);

        assert!(
            FunctionNestingRule::default()
                .evaluate(&analysis)
                .is_empty(),
            "FUNC004 debe disparar solo con anidamiento > 4"
        );
    }

    #[test]
    fn five_levels_trigger() {
        let analysis = project(vec![function("excede", 1, 30, Some(2), Some(4), 5)]);
        let findings = FunctionNestingRule::default().evaluate(&analysis);

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_id, "FUNC004");
    }
}
