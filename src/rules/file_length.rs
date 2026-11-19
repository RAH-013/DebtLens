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
                id: None,
                impact_factor: None,
                depth: None,
                roi_technical: None,
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

#[cfg(test)]
mod tests {
    use super::FileLengthRule;
    use crate::rules::rule::Rule;
    use crate::rules::test_fixtures::project_with_file;

    #[test]
    fn exactly_five_hundred_lines_does_not_trigger() {
        let analysis = project_with_file(500, Vec::new());

        assert!(
            FileLengthRule::default().evaluate(&analysis).is_empty(),
            "FILE001 debe disparar solo con > 500 líneas"
        );
    }

    #[test]
    fn five_hundred_one_lines_trigger() {
        let analysis = project_with_file(501, Vec::new());
        let findings = FileLengthRule::default().evaluate(&analysis);

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_id, "FILE001");
        assert_eq!(findings[0].line, 1);
    }
}
