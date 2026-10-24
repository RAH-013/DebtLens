use crate::model::{Finding, ProjectAnalysis, Severity};
use crate::rules::rule::Rule;

pub struct SyntaxRule;

impl Rule for SyntaxRule {
    fn id(&self) -> &'static str {
        "SYNTAX001"
    }

    fn name(&self) -> &'static str {
        "Errores sintácticos"
    }

    fn evaluate(&self, analysis: &ProjectAnalysis) -> Vec<Finding> {
        analysis
            .files
            .iter()
            .flat_map(|file| {
                file.syntax_errors.iter().map(|line| Finding {
                    rule_id: self.id().to_string(),
                    title: self.name().to_string(),
                    description: "Se detectó un error sintáctico en esta línea.".to_string(),
                    file: file.path.clone(),
                    line: *line,
                    locations: Vec::new(),
                    severity: Severity::High,
                    recommendation: "Revisa la sintaxis de la línea y el contexto cercano."
                        .to_string(),
                })
            })
            .collect()
    }
}
