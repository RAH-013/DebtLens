use crate::model::{Finding, ProjectAnalysis};

pub trait Rule: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn evaluate(&self, analysis: &ProjectAnalysis) -> Vec<Finding>;
}
