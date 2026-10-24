use crate::model::{Finding, ProjectAnalysis, Severity};
use crate::rules::rule::Rule;

pub struct FileLengthRule {
    pub max_lines: usize,
}

impl Default for FileLengthRule {
    fn default() -> Self {
        Self { max_lines: 500 }
    }
}

impl Rule for FileLengthRule {
    fn id(&self) -> &'static str {
        "FILE001"
    }

    fn name(&self) -> &'static str {
        "Archivo demasiado largo"
    }

    fn evaluate(&self, analysis: &ProjectAnalysis) -> Vec<Finding> {
        analysis
            .files
            .iter()
            .filter(|file| file.lines > self.max_lines)
            .map(|file| Finding {
                rule_id: self.id().to_string(),
                title: self.name().to_string(),
                description: format!(
                    "El archivo tiene {} líneas; el límite es {}.",
                    file.lines, self.max_lines
                ),
                file: file.path.clone(),
                line: 1,
                locations: Vec::new(),
                severity: Severity::Low,
                recommendation: "Evalúa separar el archivo por responsabilidades.".to_string(),
            })
            .collect()
    }
}
