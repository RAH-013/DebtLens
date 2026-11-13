use crate::gemini::context::{ProjectContext, normalize_rel_path};
use crate::model::{
    AiAnalysis, AiAssessment, AiCategory, AiFinding, AiPriority, ExecutiveSummary, ProjectSummary,
};
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Deserialize)]
struct RawGeminiResponse {
    project_summary: ProjectSummary,
    #[serde(default)]
    existing_findings: Vec<RawAiAssessment>,
    #[serde(default)]
    new_findings: Vec<RawAiFinding>,
    #[serde(default)]
    executive_summary: ExecutiveSummary,
}

#[derive(Debug, Deserialize)]
struct RawAiAssessment {
    finding_id: String,
    explanation: String,
    impact: String,
    recommendation: String,
    priority: String,
    confidence: f32,
    #[serde(default)]
    possible_false_positive: bool,
}

#[derive(Debug, Deserialize)]
struct RawAiFinding {
    title: String,
    #[serde(default = "default_category")]
    category: AiCategory,
    description: String,
    file: String,
    #[serde(default)]
    start_line: Option<usize>,
    #[serde(default)]
    end_line: Option<usize>,
    evidence: String,
    impact: String,
    recommendation: String,
    priority: String,
    confidence: f32,
}

fn default_category() -> AiCategory {
    AiCategory::Other
}

fn strip_markdown_fences(raw: &str) -> &str {
    let trimmed = raw.trim();
    if let Some(without_prefix) = trimmed.strip_prefix("```") {
        let body = if let Some(after_lang) = without_prefix.strip_prefix("json") {
            after_lang.trim_start()
        } else if let Some(after_lang) = without_prefix.strip_prefix("JSON") {
            after_lang.trim_start()
        } else {
            without_prefix.trim_start()
        };

        if let Some(without_suffix) = body.strip_suffix("```") {
            return without_suffix.trim();
        }
    }
    trimmed
}

fn parse_priority(raw: &str) -> Result<AiPriority> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "critical" => Ok(AiPriority::Critical),
        "high" => Ok(AiPriority::High),
        "medium" => Ok(AiPriority::Medium),
        "low" => Ok(AiPriority::Low),
        other => bail!(
            "Prioridad no permitida en respuesta de Gemini: '{other}'. Se esperaba critical, high, medium o low."
        ),
    }
}

fn validate_confidence(confidence: f32, label: &str) -> Result<f32> {
    if !confidence.is_finite() || !(0.0..=1.0).contains(&confidence) {
        bail!("Nivel de confianza fuera de rango [0.0, 1.0] en {label}: {confidence}");
    }
    Ok(confidence)
}

fn require_non_empty(value: &str, field_name: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("Campo obligatorio vacío en respuesta de Gemini: '{field_name}'.");
    }
    Ok(trimmed.to_string())
}

pub fn parse_and_validate_response(
    raw_text: &str,
    model: &str,
    context: &ProjectContext,
) -> Result<AiAnalysis> {
    let cleaned = strip_markdown_fences(raw_text);
    let parsed: RawGeminiResponse = serde_json::from_str(cleaned)
        .context("La respuesta de Gemini no es un objeto JSON válido con el esquema esperado")?;

    let valid_finding_ids: HashMap<String, (String, usize)> = context
        .static_findings
        .iter()
        .filter_map(|f| {
            f.id.as_ref()
                .map(|id| (id.trim().to_string(), (f.file.clone(), f.line)))
        })
        .collect();

    let mut seen_assessment_ids = HashSet::new();
    let mut existing_findings = Vec::with_capacity(parsed.existing_findings.len());

    for raw in parsed.existing_findings {
        let finding_id = require_non_empty(&raw.finding_id, "existing_findings.finding_id")?;
        if !valid_finding_ids.contains_key(&finding_id) {
            bail!(
                "Gemini devolvió un finding_id desconocido o inexistente en el análisis estático: '{finding_id}'."
            );
        }

        if !seen_assessment_ids.insert(finding_id.clone()) {
            continue;
        }

        let explanation = require_non_empty(&raw.explanation, "existing_findings.explanation")?;
        let impact = require_non_empty(&raw.impact, "existing_findings.impact")?;
        let recommendation =
            require_non_empty(&raw.recommendation, "existing_findings.recommendation")?;
        let priority = parse_priority(&raw.priority)?;
        let confidence = validate_confidence(raw.confidence, &finding_id)?;

        existing_findings.push(AiAssessment {
            finding_id,
            explanation,
            impact,
            recommendation,
            priority,
            confidence,
            possible_false_positive: raw.possible_false_positive,
        });
    }

    let static_anchors: HashSet<(String, usize)> = context
        .static_findings
        .iter()
        .map(|f| (normalize_rel_path(&f.file), f.line))
        .collect();

    let mut seen_new_findings: HashSet<(String, usize, String)> = HashSet::new();
    let mut new_findings = Vec::with_capacity(parsed.new_findings.len());

    for raw in parsed.new_findings {
        let title = require_non_empty(&raw.title, "new_findings.title")?;
        let description = require_non_empty(&raw.description, "new_findings.description")?;
        let raw_file = require_non_empty(&raw.file, "new_findings.file")?;
        let file = normalize_rel_path(&raw_file);
        let evidence = require_non_empty(&raw.evidence, "new_findings.evidence")?;
        let impact = require_non_empty(&raw.impact, "new_findings.impact")?;
        let recommendation = require_non_empty(&raw.recommendation, "new_findings.recommendation")?;
        let priority = parse_priority(&raw.priority)?;
        let confidence = validate_confidence(raw.confidence, &title)?;

        let Some(total_lines) = context.file_line_count(&file) else {
            bail!(
                "Ubicación inválida en hallazgo de Gemini: el archivo '{file}' no forma parte del proyecto analizado."
            );
        };

        if let Some(start) = raw.start_line
            && (start == 0 || start > total_lines)
        {
            bail!("Línea inicial inválida ({start}) en '{file}' (máximo {total_lines} líneas).");
        }

        if let Some(end) = raw.end_line
            && (end == 0 || end > total_lines)
        {
            bail!("Línea final inválida ({end}) en '{file}' (máximo {total_lines} líneas).");
        }

        if let (Some(start), Some(end)) = (raw.start_line, raw.end_line)
            && start > end
        {
            bail!("Rango de líneas inválido en '{file}': start_line ({start}) > end_line ({end}).");
        }

        let anchor_line = raw.start_line.unwrap_or(0);
        if anchor_line > 0 && static_anchors.contains(&(file.clone(), anchor_line)) {
            continue;
        }

        let dedup_key = (file.clone(), anchor_line, title.to_ascii_lowercase());
        if !seen_new_findings.insert(dedup_key) {
            continue;
        }

        new_findings.push(AiFinding {
            title,
            category: raw.category,
            description,
            file,
            start_line: raw.start_line,
            end_line: raw.end_line,
            evidence,
            impact,
            recommendation,
            priority,
            confidence,
        });
    }

    Ok(AiAnalysis {
        model: model.to_string(),
        from_cache: false,
        project_summary: parsed.project_summary,
        existing_findings,
        new_findings,
        executive_summary: parsed.executive_summary,
        token_usage: None,
    })
}