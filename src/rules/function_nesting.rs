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
