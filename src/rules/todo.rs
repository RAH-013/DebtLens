use crate::model::{Finding, ProjectAnalysis, Severity};
use crate::rules::rule::Rule;

pub struct TodoRule;

impl Rule for TodoRule {
    fn id(&self) -> &'static str {
        "CODE002"
    }

    fn name(&self) -> &'static str {
        "Marcador de trabajo pendiente"
    }

    fn evaluate(&self, analysis: &ProjectAnalysis) -> Vec<Finding> {
        analysis
            .files
            .iter()
            .flat_map(|file| {
                file.todo_markers.iter().map(|marker| Finding {
                    rule_id: self.id().to_string(),
                    title: format!("Marcador {}", marker.marker),
                    description: format!(
                        "Se encontró el marcador {} en el código.",
                        marker.marker
                    ),
                    file: file.path.clone(),
                    line: marker.line,
                    locations: Vec::new(),
                    severity: Severity::Info,
                    recommendation: "Verifica si el trabajo pendiente sigue siendo necesario y documenta su seguimiento.".to_string(),
                })
            })
            .collect()
    }
}
