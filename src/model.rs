use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionMetric {
    pub name: Option<String>,
    pub start_line: usize,
    pub end_line: usize,
    pub lines: usize,
    pub parameters: Option<usize>,
    pub complexity: Option<usize>,
    pub nesting_depth: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoMarker {
    pub marker: String,
    pub line: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeBlockMetric {
    pub fingerprint: u64,
    pub start_line: usize,
    pub lines: usize,
    #[serde(skip)]
    pub normalized_content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAnalysis {
    pub path: String,
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub lines: usize,
    #[serde(default)]
    pub bytes: u64,
    #[serde(default)]
    pub syntax_errors: Vec<usize>,
    #[serde(default)]
    pub functions: Vec<FunctionMetric>,
    #[serde(default)]
    pub todo_markers: Vec<TodoMarker>,
    #[serde(default)]
    pub code_blocks: Vec<CodeBlockMetric>,
}

/// Telemetría experimental de los subprocesos del análisis (RNF01).
///
/// Todos los valores están en milisegundos (fracción incluida, de modo que
/// proyectos pequeños no reporten 0). `walk_ms` y `rules_ms` son tiempo de
/// pared; `ast_ms` y `clones_ms` son la suma acumulada por archivo, de modo
/// que la suma de las cuatro fases aproxima el tiempo total del análisis.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
pub struct Telemetry {
    #[serde(default)]
    pub walk_ms: f64,
    #[serde(default)]
    pub ast_ms: f64,
    #[serde(default)]
    pub clones_ms: f64,
    #[serde(default)]
    pub rules_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectAnalysis {
    pub root: String,
    #[serde(default)]
    pub files_scanned: usize,
    #[serde(default)]
    pub total_lines: usize,
    #[serde(default)]
    pub total_bytes: u64,
    #[serde(default)]
    pub total_functions: usize,
    #[serde(default)]
    pub files: Vec<FileAnalysis>,
    /// Campo aditivo (C4): ausente en los JSON generados por versiones previas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub telemetry: Option<Telemetry>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Critical,
    Info,
    Low,
    Medium,
    High,
}

impl Severity {
    pub fn debt_weight(self) -> f64 {
        match self {
            Self::Critical => 20.0,
            Self::High => 10.0,
            Self::Medium => 5.0,
            Self::Low => 2.0,
            Self::Info => 0.5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FindingLocation {
    pub file: String,
    pub line: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub rule_id: String,
    pub title: String,
    pub description: String,
    pub file: String,
    pub line: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub locations: Vec<FindingLocation>,
    pub severity: Severity,
    pub recommendation: String,
    /// Multiplicador de impacto $M_i$ del hallazgo (C4: aditivo y opcional).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub impact_factor: Option<f64>,
    /// Profundidad de contexto del hallazgo para $ROI_{\text{técnico}}$ (C4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<usize>,
    /// Retorno de inversión técnico $ROI_{\text{técnico}}$ calculado en Fase 2 (C4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roi_technical: Option<f64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum AiPriority {
    Critical,
    High,
    Medium,
    Low,
}

impl AiPriority {
    pub fn debt_weight(self) -> f64 {
        match self {
            Self::Critical => 12.0,
            Self::High => 8.0,
            Self::Medium => 4.0,
            Self::Low => 1.5,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Critical => "critical",
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AiCategory {
    Architecture,
    Coupling,
    Cohesion,
    DesignAntipattern,
    Duplication,
    Maintainability,
    Scalability,
    Complexity,
    ErrorHandling,
    Security,
    #[serde(other)]
    Other,
}

impl AiCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Architecture => "architecture",
            Self::Coupling => "coupling",
            Self::Cohesion => "cohesion",
            Self::DesignAntipattern => "design_antipattern",
            Self::Duplication => "duplication",
            Self::Maintainability => "maintainability",
            Self::Scalability => "scalability",
            Self::Complexity => "complexity",
            Self::ErrorHandling => "error_handling",
            Self::Security => "security",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectSummary {
    pub description: String,
    pub overall_assessment: String,
    #[serde(default)]
    pub main_risks: Vec<String>,
    #[serde(default)]
    pub recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiAssessment {
    pub finding_id: String,
    pub explanation: String,
    pub impact: String,
    pub recommendation: String,
    pub priority: AiPriority,
    pub confidence: f32,
    pub possible_false_positive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiFinding {
    pub title: String,
    pub category: AiCategory,
    pub description: String,
    pub file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_line: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_line: Option<usize>,
    pub evidence: String,
    pub impact: String,
    pub recommendation: String,
    pub priority: AiPriority,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExecutiveSummary {
    #[serde(default)]
    pub critical_issues: Vec<String>,
    #[serde(default)]
    pub high_priority_issues: Vec<String>,
    #[serde(default)]
    pub medium_priority_issues: Vec<String>,
    #[serde(default)]
    pub low_priority_issues: Vec<String>,
    #[serde(default)]
    pub recommended_actions: Vec<String>,
}


#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsageSummary {
    pub prompt_tokens: usize,
    pub output_tokens: usize,
    pub total_tokens: usize,
    pub token_limit: usize,
    pub remaining_tokens: usize,
    #[serde(default)]
    pub from_rate_limit_header: bool,
    #[serde(default)]
    pub tier_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiAnalysis {
    pub model: String,
    pub from_cache: bool,
    pub project_summary: ProjectSummary,
    #[serde(default)]
    pub existing_findings: Vec<AiAssessment>,
    #[serde(default)]
    pub new_findings: Vec<AiFinding>,
    pub executive_summary: ExecutiveSummary,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_usage: Option<TokenUsageSummary>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AiExecutionState {
    Disabled,
    Completed,
    Cached,
    SkippedContextLimit,
    FailedFallback,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiStatusSummary {
    pub enabled: bool,
    pub state: AiExecutionState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub message: String,
}

impl AiStatusSummary {
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            state: AiExecutionState::Disabled,
            model: None,
            message: "Análisis semántico con Gemini deshabilitado.".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DebtLevel {
    Low,
    Moderate,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TechnicalDebtIndex {
    /// Puntuación normalizada en escala determinista 0.0..=100.0
    pub score: f64,
    pub level: DebtLevel,
    pub static_points: f64,
    pub semantic_points: f64,
    pub static_findings_count: usize,
    pub semantic_findings_count: usize,
    pub deduplicated_semantic_count: usize,
}

/// Calcula un índice determinista y reproducible combinando hallazgos estáticos
/// y nuevos hallazgos semánticos sin alterar los resultados estáticos originales.
pub fn calculate_technical_debt_index(
    analysis: &ProjectAnalysis,
    static_findings: &[Finding],
    ai_analysis: Option<&AiAnalysis>,
) -> TechnicalDebtIndex {
    let static_points: f64 = static_findings
        .iter()
        .map(|finding| finding.severity.debt_weight())
        .sum();

    let mut static_anchors: HashSet<(String, usize)> = HashSet::new();
    for finding in static_findings {
        static_anchors.insert((finding.file.clone(), finding.line));
        for loc in &finding.locations {
            static_anchors.insert((loc.file.clone(), loc.line));
        }
    }

    let mut semantic_points = 0.0;
    let mut semantic_findings_count = 0;
    let mut deduplicated_semantic_count = 0;

    if let Some(ai) = ai_analysis {
        for finding in &ai.new_findings {
            let anchor_line = finding.start_line.unwrap_or(0);
            if anchor_line > 0 && static_anchors.contains(&(finding.file.clone(), anchor_line)) {
                deduplicated_semantic_count += 1;
                continue;
            }

            semantic_findings_count += 1;
            semantic_points += finding.priority.debt_weight();
        }
    }

    let raw_points = static_points + semantic_points;
    let kloc = (analysis.total_lines.max(1) as f64) / 1000.0;
    let normalization_base = (kloc * 25.0).max(25.0);
    let normalized =
        ((raw_points / (raw_points + normalization_base)) * 100.0 * 100.0).round() / 100.0;

    let level = if normalized < 20.0 {
        DebtLevel::Low
    } else if normalized < 45.0 {
        DebtLevel::Moderate
    } else if normalized < 70.0 {
        DebtLevel::High
    } else {
        DebtLevel::Critical
    };

    TechnicalDebtIndex {
        score: normalized,
        level,
        static_points: (static_points * 100.0).round() / 100.0,
        semantic_points: (semantic_points * 100.0).round() / 100.0,
        static_findings_count: static_findings.len(),
        semantic_findings_count,
        deduplicated_semantic_count,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisReport {
    pub analysis: ProjectAnalysis,
    pub findings: Vec<Finding>,
    pub debt_index: TechnicalDebtIndex,
    pub ai_status: AiStatusSummary,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_analysis: Option<AiAnalysis>,
}
