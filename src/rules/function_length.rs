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
