use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct FunctionMetric {
    pub name: Option<String>,
    pub start_line: usize,
    pub end_line: usize,
    pub lines: usize,
    pub parameters: Option<usize>,
    pub complexity: Option<usize>,
    pub nesting_depth: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct TodoMarker {
    pub marker: String,
    pub line: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct CodeBlockMetric {
    pub fingerprint: u64,
    pub start_line: usize,
    pub lines: usize,
    #[serde(skip)]
    pub normalized_content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileAnalysis {
    pub path: String,
    pub language: String,
    pub lines: usize,
    pub bytes: u64,
    pub syntax_errors: Vec<usize>,
    pub functions: Vec<FunctionMetric>,
    pub todo_markers: Vec<TodoMarker>,
    pub code_blocks: Vec<CodeBlockMetric>,
}

#[derive(Debug, Serialize)]
pub struct ProjectAnalysis {
    pub root: String,
    pub files_scanned: usize,
    pub total_lines: usize,
    pub total_bytes: u64,
    pub total_functions: usize,
    pub files: Vec<FileAnalysis>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize)]
pub struct FindingLocation {
    pub file: String,
    pub line: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub rule_id: String,
    pub title: String,
    pub description: String,
    pub file: String,
    pub line: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub locations: Vec<FindingLocation>,
    pub severity: Severity,
    pub recommendation: String,
}

#[derive(Debug, Serialize)]
pub struct AnalysisReport {
    pub analysis: ProjectAnalysis,
    pub findings: Vec<Finding>,
}
