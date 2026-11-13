use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};

const TEMPLATE: &str = include_str!("templates/report.html");
const STYLES: &str = concat!(
    include_str!("templates/layout.css"),
    "\n",
    include_str!("templates/style.css")
);
const SCRIPT: &str = include_str!("templates/report.js");

const ASSETS: &[(&str, &str)] = &[
    ("favicon.svg", include_str!("templates/assets/favicon.svg")),
    ("logo.svg", include_str!("templates/assets/logo.svg")),
    ("gemini.svg", include_str!("templates/assets/gemini.svg")),
    (
        "theme-system.svg",
        include_str!("templates/assets/theme-system.svg"),
    ),
    (
        "theme-light.svg",
        include_str!("templates/assets/theme-light.svg"),
    ),
    (
        "theme-dark.svg",
        include_str!("templates/assets/theme-dark.svg"),
    ),
    (
        "icon-folder.svg",
        include_str!("templates/assets/icon-folder.svg"),
    ),
    (
        "icon-files.svg",
        include_str!("templates/assets/icon-files.svg"),
    ),
    (
        "icon-shield-alert.svg",
        include_str!("templates/assets/icon-shield-alert.svg"),
    ),
    (
        "icon-percent.svg",
        include_str!("templates/assets/icon-percent.svg"),
    ),
    (
        "icon-warning.svg",
        include_str!("templates/assets/icon-warning.svg"),
    ),
    (
        "icon-trend.svg",
        include_str!("templates/assets/icon-trend.svg"),
    ),
    (
        "icon-code.svg",
        include_str!("templates/assets/icon-code.svg"),
    ),
    (
        "icon-chart.svg",
        include_str!("templates/assets/icon-chart.svg"),
    ),
    (
        "icon-info.svg",
        include_str!("templates/assets/icon-info.svg"),
    ),
    (
        "icon-filter.svg",
        include_str!("templates/assets/icon-filter.svg"),
    ),
    (
        "icon-search.svg",
        include_str!("templates/assets/icon-search.svg"),
    ),
];

pub fn render(raw_report: &str) -> Result<String> {
    let report: Value =
        serde_json::from_str(raw_report).context("No se pudo leer el reporte bruto")?;

    let analysis = &report["analysis"];
    let findings = report["findings"]
        .as_array()
        .context("El reporte no contiene una lista de hallazgos válida")?;

    let ai_analysis = report.get("ai_analysis").filter(|v| !v.is_null());
    let empty_vec = Vec::new();
    let ai_new_findings = ai_analysis
        .and_then(|ai| ai["new_findings"].as_array())
        .unwrap_or(&empty_vec);

    let assessments_by_id: HashMap<&str, &Value> = ai_analysis
        .and_then(|ai| ai["existing_findings"].as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item["finding_id"].as_str().map(|id| (id, item)))
                .collect()
        })
        .unwrap_or_default();

    let files = number(analysis, "files_scanned");
    let lines = number(analysis, "total_lines");
    let affected = affected_files(findings, ai_new_findings);
    let affected_count = affected.len();
    let static_count = findings.len();
    let semantic_count = ai_new_findings.len();
    let total_finding_count = static_count + semantic_count;

    let high = count_severity(findings, "high")
        + count_priority(ai_new_findings, "critical")
        + count_priority(ai_new_findings, "high");
    let medium = count_severity(findings, "medium") + count_priority(ai_new_findings, "medium");
    let low = count_severity(findings, "low") + count_priority(ai_new_findings, "low");
    let info = count_severity(findings, "info");

    let syntax_errors = analysis["files"]
        .as_array()
        .map(|files| {
            files
                .iter()
                .map(|file| file["syntax_errors"].as_array().map_or(0, Vec::len))
                .sum::<usize>()
        })
        .unwrap_or(0);

    let density = if lines == 0 {
        0.0
    } else {
        total_finding_count as f64 * 1000.0 / lines as f64
    };

    let debt_score = report["debt_index"]["score"].as_f64().unwrap_or(0.0);
    let debt_level = match string(&report["debt_index"], "level") {
        "low" => "Baja",
        "moderate" => "Moderada",
        "high" => "Alta",
        "critical" => "Crítica",
        _ => "Calculada",
    };

    let ai_state = string(&report["ai_status"], "state");
    let ai_active = matches!(ai_state, "completed" | "cached");
    let ai_badge_label = if ai_active {
        "Estático + Gemini IA"
    } else {
        "Análisis estático"
    };

    let generated_at = string(&report, "generated_at");
    let generated_label = if generated_at.is_empty() {
        "Fecha actual"
    } else {
        generated_at
    };

    let mut rendered_findings = String::new();

    if total_finding_count == 0 {
        rendered_findings.push_str(
            r#"<div class="empty"><h3>No se detectaron hallazgos</h3><p>Esto no garantiza la ausencia de deuda técnica; significa que las reglas actuales no registraron problemas.</p></div>"#,
        );
    } else {
        for finding in findings {
            let assessment = finding["id"]
                .as_str()
                .and_then(|id| assessments_by_id.get(id).copied());
            render_static_finding(&mut rendered_findings, finding, assessment);
        }

        for ai_finding in ai_new_findings {
            render_semantic_finding(&mut rendered_findings, ai_finding);
        }
    }

    let executive_section = render_executive_section(&report["ai_status"], ai_analysis);

    let mut html = TEMPLATE.to_owned();

    let replacements = [
        ("{{PROJECT}}", escape_html(string(analysis, "root"))),
        ("{{GENERATED_AT}}", escape_html(generated_label)),
        ("{{AI_BADGE}}", escape_html(ai_badge_label)),
        ("{{DEBT_SCORE}}", format!("{debt_score:.1}")),
        ("{{DEBT_LEVEL}}", escape_html(debt_level)),
        ("{{FILES}}", files.to_string()),
        ("{{AFFECTED}}", affected_count.to_string()),
        (
            "{{AFFECTED_PERCENTAGE}}",
            format!("{:.1}", percentage(affected_count, files)),
        ),
        ("{{FINDING_COUNT}}", total_finding_count.to_string()),
        ("{{STATIC_COUNT}}", static_count.to_string()),
        ("{{SEMANTIC_COUNT}}", semantic_count.to_string()),
        ("{{DENSITY}}", format!("{density:.2}")),
        ("{{SYNTAX_ERRORS}}", syntax_errors.to_string()),
        ("{{HIGH}}", high.to_string()),
        ("{{MEDIUM}}", medium.to_string()),
        ("{{LOW}}", low.to_string()),
        ("{{INFO}}", info.to_string()),
        (
            "{{HIGH_WIDTH}}",
            format!("{:.2}", percentage(high, total_finding_count)),
        ),
        (
            "{{MEDIUM_WIDTH}}",
            format!("{:.2}", percentage(medium, total_finding_count)),
        ),
        (
            "{{LOW_WIDTH}}",
            format!("{:.2}", percentage(low, total_finding_count)),
        ),
        (
            "{{INFO_WIDTH}}",
            format!("{:.2}", percentage(info, total_finding_count)),
        ),
        ("{{EXECUTIVE_SECTION}}", executive_section),
        ("{{FINDINGS}}", rendered_findings),
    ];

    for (placeholder, value) in replacements {
        html = html.replace(placeholder, &value);
    }

    Ok(bundle_single_html(html))
}

fn bundle_single_html(rendered_html: String) -> String {
    let encoded_js = STANDARD.encode(SCRIPT.as_bytes());
    let inline_script = format!(
        r#"<script>(()=>{{const _=Uint8Array.from(atob("{encoded_js}"),c=>c.charCodeAt(0));(0,eval)(new TextDecoder().decode(_))}})()</script>"#
    );

    let inline_style = format!("<style>{STYLES}</style>");
    let mut bundled = rendered_html
        .replace(r#"<link rel="stylesheet" href="./style.css" />"#, &inline_style)
        .replace(r#"<link rel="stylesheet" href="style.css" />"#, &inline_style)
        .replace(r#"<link rel="stylesheet" href="./style.css">"#, &inline_style)
        .replace(r#"<link rel="stylesheet" href="style.css">"#, &inline_style)
        .replace(r#"<script src="./report.js"></script>"#, &inline_script)
        .replace(r#"<script src="report.js"></script>"#, &inline_script);

    for (filename, content) in ASSETS {
        let data_uri = format!(
            "data:image/svg+xml;base64,{}",
            STANDARD.encode(content.as_bytes())
        );
        let dot_attr = format!(r#"="./assets/{filename}""#);
        let rel_attr = format!(r#"="assets/{filename}""#);
        let replacement_attr = format!(r#"="{data_uri}""#);

        bundled = bundled
            .replace(&dot_attr, &replacement_attr)
            .replace(&rel_attr, &replacement_attr);
    }

    let mut cfg = minify_html::Cfg::new();
    cfg.minify_css = true;
    cfg.minify_js = false;
    cfg.keep_comments = false;

    let minified = minify_html::minify(bundled.as_bytes(), &cfg);
    String::from_utf8(minified).unwrap_or(bundled)
}

pub fn render_to_dir(raw_report: &str, output_dir: &Path) -> Result<PathBuf> {
    let html = render(raw_report)?;
    write_html_bundle(output_dir, &html)?;
    Ok(output_dir.join("report.html"))
}

pub fn write_html_bundle(output_dir: &Path, html: &str) -> Result<()> {
    fs::create_dir_all(output_dir)
        .with_context(|| format!("No se pudo crear {}", output_dir.display()))?;

    fs::write(output_dir.join("report.html"), html).context("No se pudo escribir report.html")?;

    Ok(())
}

fn render_executive_section(ai_status: &Value, ai_analysis: Option<&Value>) -> String {
    let message = escape_html(string(ai_status, "message"));
    let model = escape_html(string(ai_status, "model"));

    let Some(ai) = ai_analysis else {
        return format!(
            r#"<section class="panel executive-panel" aria-labelledby="ai-status-heading">
  <div class="panel-heading">
    <span class="panel-icon" aria-hidden="true"><img src="./assets/gemini.svg" alt="" /></span>
    <h2 id="ai-status-heading">Estado del análisis semántico (Google Gemini)</h2>
  </div>
  <p class="panel-description">{message}</p>
</section>"#
        );
    };

    let summary = &ai["project_summary"];
    let exec = &ai["executive_summary"];
    let from_cache = ai["from_cache"].as_bool().unwrap_or(false);
    let cache_tag = if from_cache {
        "Caché local"
    } else {
        "Análisis en vivo"
    };

    let token_block = if let Some(usage) = ai.get("token_usage").filter(|v| !v.is_null()) {
        let prompt_tokens = number(usage, "prompt_tokens");
        let output_tokens = number(usage, "output_tokens");
        let total_tokens = number(usage, "total_tokens");
        let token_limit = number(usage, "token_limit");
        let remaining_tokens = number(usage, "remaining_tokens");
        let from_header = usage["from_rate_limit_header"].as_bool().unwrap_or(false);
        let tier_label = string(usage, "tier_label");
        let source_label = if from_header {
            "cuota API en tiempo real"
        } else {
            "límite del modelo API"
        };
        let tier_html = if tier_label.is_empty() {
            String::new()
        } else {
            format!(
                " &middot; <strong>Plan:</strong> {}",
                escape_html(tier_label)
            )
        };
        format!(
            r#"<div class="detail ai-tokens"><strong>Uso de tokens (Gemini)</strong><p>{total_tokens} usados ({prompt_tokens} entrada + {output_tokens} salida) &middot; <strong>Tokens restantes:</strong> {remaining_tokens} de {token_limit} ({source_label}){tier_html}</p></div>"#
        )
    } else {
        String::new()
    };

    let mut html = format!(
        r#"<section class="panel executive-panel" aria-labelledby="exec-heading">
  <div class="panel-heading">
    <span class="panel-icon" aria-hidden="true"><img src="./assets/gemini.svg" alt="" /></span>
    <h2 id="exec-heading">Resumen ejecutivo — Google Gemini ({model} · {cache_tag})</h2>
  </div>
  <p class="panel-description">{desc}</p>
  <div class="detail"><strong>Evaluación general de mantenibilidad</strong><p>{overall}</p></div>
  {token_block}
  <div class="executive-grid">"#,
        model = model,
        cache_tag = cache_tag,
        desc = escape_html(string(summary, "description")),
        overall = escape_html(string(summary, "overall_assessment")),
        token_block = token_block,
    );

    html.push_str(&render_bullet_group(
        "Riesgos principales",
        summary["main_risks"].as_array(),
    ));
    html.push_str(&render_bullet_group(
        "Problemas críticos",
        exec["critical_issues"].as_array(),
    ));
    html.push_str(&render_bullet_group(
        "Problemas de alta prioridad",
        exec["high_priority_issues"].as_array(),
    ));
    html.push_str(&render_bullet_group(
        "Acciones recomendadas",
        exec["recommended_actions"].as_array(),
    ));

    html.push_str("</div></section>");
    html
}

fn render_bullet_group(title: &str, items: Option<&Vec<Value>>) -> String {
    let list: Vec<&str> = items
        .map(|arr| arr.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();

    if list.is_empty() {
        return String::new();
    }

    let mut out = format!(
        r#"<div class="exec-block"><h4>{}</h4><ul>"#,
        escape_html(title)
    );
    for item in list {
        let _ = write!(out, "<li>{}</li>", escape_html(item));
    }
    out.push_str("</ul></div>");
    out
}

fn render_static_finding(output: &mut String, finding: &Value, assessment: Option<&Value>) {
    let severity = string(finding, "severity");
    let label = match severity {
        "high" => "Alta",
        "medium" => "Media",
        "low" => "Baja",
        _ => "Info",
    };

    let id = string(finding, "id");
    let id_badge = if id.is_empty() {
        String::new()
    } else {
        format!(r#"<code>{}</code>"#, escape_html(id))
    };

    let ai_block = if let Some(ai) = assessment {
        let priority = string(ai, "priority");
        let prio_label = match priority {
            "critical" => "Crítica",
            "high" => "Alta",
            "medium" => "Media",
            "low" => "Baja",
            _ => priority,
        };
        let confidence = (ai["confidence"].as_f64().unwrap_or(0.0) * 100.0).round();
        let fp = ai["possible_false_positive"].as_bool().unwrap_or(false);
        let fp_badge = if fp {
            r#"<span class="origin-badge warning">Posible falso positivo</span>"#
        } else {
            ""
        };

        format!(
            r#"<div class="detail ai-assessment">
  <div class="finding-meta">
    <span class="origin-badge semantic"><img src="./assets/gemini.svg" alt="" class="badge-icon" width="14" height="14" /> Evaluación Gemini IA</span>
    <span class="severity {prio_class}">Prioridad: {prio}</span>
    <span>Confianza: {confidence:.0}%</span>
    {fp_badge}
  </div>
  <p><strong>Explicación:</strong> {explanation}</p>
  <p><strong>Impacto:</strong> {impact}</p>
  <p><strong>Solución propuesta:</strong> {rec}</p>
</div>"#,
            prio_class = escape_html(normalize_severity_bucket(priority)),
            prio = escape_html(prio_label),
            confidence = confidence,
            fp_badge = fp_badge,
            explanation = escape_html(string(ai, "explanation")),
            impact = escape_html(string(ai, "impact")),
            rec = escape_html(string(ai, "recommendation")),
        )
    } else {
        String::new()
    };

    let _ = write!(
        output,
        r#"<article class="finding finding-static" data-severity="{severity}" data-origin="static">
<div class="finding-meta">
  <span class="origin-badge static">Regla estática</span>
  <span class="severity {severity}">{label}</span>
  {id_badge}
  <code>{rule}</code>
</div>
<h3>{title}</h3>
<p>{description}</p>
<div class="finding-meta"><span>Archivo: <code>{file}</code></span><span>Línea: {line}</span></div>
{source_code}
{locations}
<div class="detail"><strong>Recomendación</strong><p>{recommendation}</p></div>
{ai_block}
</article>"#,
        severity = escape_html(severity),
        label = label,
        id_badge = id_badge,
        rule = escape_html(string(finding, "rule_id")),
        title = escape_html(string(finding, "title")),
        description = escape_html(string(finding, "description")),
        file = escape_html(string(finding, "file")),
        line = number(finding, "line"),
        source_code = render_source_code(&finding["source_code"]),
        locations = render_locations(finding),
        recommendation = escape_html(string(finding, "recommendation")),
        ai_block = ai_block,
    );
}

fn render_semantic_finding(output: &mut String, finding: &Value) {
    let priority = string(finding, "priority");
    let bucket = normalize_severity_bucket(priority);
    let label = match priority {
        "critical" => "Crítica",
        "high" => "Alta",
        "medium" => "Media",
        _ => "Baja",
    };

    let category = string(finding, "category");
    let start_line = number(finding, "start_line");
    let end_line = number(finding, "end_line");
    let line_label = if start_line > 0 && end_line > start_line {
        format!("{start_line}–{end_line}")
    } else if start_line > 0 {
        start_line.to_string()
    } else {
        "General".to_string()
    };

    let confidence = (finding["confidence"].as_f64().unwrap_or(0.0) * 100.0).round();

    let _ = write!(
        output,
        r#"<article class="finding finding-semantic" data-severity="{bucket}" data-origin="semantic">
<div class="finding-meta">
  <span class="origin-badge semantic"><img src="./assets/gemini.svg" alt="" class="badge-icon" width="14" height="14" /> Hallazgo semántico (Gemini)</span>
  <span class="severity {bucket}">{label}</span>
  <code>{category}</code>
  <span>Confianza: {confidence:.0}%</span>
</div>
<h3>{title}</h3>
<p>{description}</p>
<div class="finding-meta"><span>Archivo: <code>{file}</code></span><span>Ubicación: {line_label}</span></div>
<div class="detail"><strong>Evidencia</strong><p>{evidence}</p></div>
{source_code}
<div class="detail"><strong>Impacto en mantenibilidad</strong><p>{impact}</p></div>
<div class="detail"><strong>Recomendación</strong><p>{recommendation}</p></div>
</article>"#,
        bucket = escape_html(bucket),
        label = label,
        category = escape_html(category),
        confidence = confidence,
        title = escape_html(string(finding, "title")),
        description = escape_html(string(finding, "description")),
        file = escape_html(string(finding, "file")),
        line_label = escape_html(&line_label),
        evidence = escape_html(string(finding, "evidence")),
        source_code = render_source_code(&finding["source_code"]),
        impact = escape_html(string(finding, "impact")),
        recommendation = escape_html(string(finding, "recommendation")),
    );
}

fn normalize_severity_bucket(priority: &str) -> &'static str {
    match priority {
        "critical" | "high" => "high",
        "medium" => "medium",
        "low" => "low",
        _ => "info",
    }
}

fn render_source_code(source_code: &Value) -> String {
    let Some(lines) = source_code.as_array() else {
        return String::new();
    };

    if lines.is_empty() {
        return String::new();
    }

    let mut output = String::from(
        r#"<div class="detail"><strong>Código fuente</strong><pre class="source-code"><code>"#,
    );

    for line in lines {
        let number = line["line"].as_u64().unwrap_or(0);
        let content = line["content"].as_str().unwrap_or("");
        let is_target = line["is_target"].as_bool().unwrap_or(false);

        if is_target {
            let _ = write!(
                output,
                r#"<span class="source-line target"><span class="line-number">{number:>4}</span> │ {content}</span>"#,
                number = number,
                content = escape_html(content),
            );
        } else {
            let _ = write!(
                output,
                r#"<span class="source-line"><span class="line-number">{number:>4}</span> │ {content}</span>"#,
                number = number,
                content = escape_html(content),
            );
        }

        output.push('\n');
    }

    output.push_str("</code></pre></div>");
    output
}

fn render_locations(finding: &Value) -> String {
    let Some(locations) = finding["locations"].as_array() else {
        return String::new();
    };

    let primary_file = string(finding, "file");
    let primary_line = number(finding, "line");

    let related: Vec<&Value> = locations
        .iter()
        .filter(|location| {
            string(location, "file") != primary_file || number(location, "line") != primary_line
        })
        .collect();

    if related.is_empty() {
        return String::new();
    }

    let mut output =
        String::from(r#"<div class="detail"><strong>Ubicaciones relacionadas</strong>"#);

    for location in related {
        let _ = write!(
            output,
            r#"<p><code>{}</code>, línea {}</p>"#,
            escape_html(string(location, "file")),
            number(location, "line")
        );

        output.push_str(&render_source_code(&location["source_code"]));
    }

    output.push_str("</div>");
    output
}

fn affected_files(findings: &[Value], ai_findings: &[Value]) -> HashSet<String> {
    let mut files = HashSet::new();

    for finding in findings {
        let file = string(finding, "file");
        if !file.is_empty() {
            files.insert(file.to_owned());
        }

        if let Some(locations) = finding["locations"].as_array() {
            for location in locations {
                let file = string(location, "file");
                if !file.is_empty() {
                    files.insert(file.to_owned());
                }
            }
        }
    }

    for finding in ai_findings {
        let file = string(finding, "file");
        if !file.is_empty() {
            files.insert(file.to_owned());
        }
    }

    files
}

fn count_severity(findings: &[Value], expected: &str) -> usize {
    findings
        .iter()
        .filter(|finding| string(finding, "severity") == expected)
        .count()
}

fn count_priority(findings: &[Value], expected: &str) -> usize {
    findings
        .iter()
        .filter(|finding| string(finding, "priority") == expected)
        .count()
}

fn percentage(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 * 100.0 / denominator as f64
    }
}

fn number(value: &Value, key: &str) -> usize {
    value[key].as_u64().unwrap_or(0) as usize
}

fn string<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or("")
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}