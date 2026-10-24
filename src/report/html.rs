use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashSet;
use std::fmt::Write;

const TEMPLATE: &str = include_str!("templates/report.html");

pub fn render(raw_report: &str) -> Result<String> {
    let report: Value =
        serde_json::from_str(raw_report).context("No se pudo leer el reporte bruto")?;

    let analysis = &report["analysis"];
    let findings = report["findings"]
        .as_array()
        .context("El reporte no contiene una lista de hallazgos válida")?;

    let files = number(analysis, "files_scanned");
    let lines = number(analysis, "total_lines");
    let affected = affected_files(findings);
    let affected_count = affected.len();
    let finding_count = findings.len();

    let high = count_severity(findings, "high");
    let medium = count_severity(findings, "medium");
    let low = count_severity(findings, "low");
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
        finding_count as f64 * 1000.0 / lines as f64
    };

    let mut rendered_findings = String::new();

    if findings.is_empty() {
        rendered_findings.push_str(
            r#"<div class="empty"><h3>No se detectaron hallazgos</h3><p>Esto no garantiza la ausencia de deuda técnica; significa que las reglas actuales no registraron problemas.</p></div>"#,
        );
    } else {
        for finding in findings {
            render_finding(&mut rendered_findings, finding);
        }
    }

    let mut html = TEMPLATE.to_owned();

    let replacements = [
        ("{{PROJECT}}", escape_html(string(analysis, "root"))),
        ("{{FILES}}", files.to_string()),
        ("{{AFFECTED}}", affected_count.to_string()),
        (
            "{{AFFECTED_PERCENTAGE}}",
            format!("{:.1}", percentage(affected_count, files)),
        ),
        ("{{FINDING_COUNT}}", finding_count.to_string()),
        ("{{DENSITY}}", format!("{density:.2}")),
        ("{{SYNTAX_ERRORS}}", syntax_errors.to_string()),
        ("{{HIGH}}", high.to_string()),
        ("{{MEDIUM}}", medium.to_string()),
        ("{{LOW}}", low.to_string()),
        ("{{INFO}}", info.to_string()),
        (
            "{{HIGH_WIDTH}}",
            format!("{:.2}", percentage(high, finding_count)),
        ),
        (
            "{{MEDIUM_WIDTH}}",
            format!("{:.2}", percentage(medium, finding_count)),
        ),
        (
            "{{LOW_WIDTH}}",
            format!("{:.2}", percentage(low, finding_count)),
        ),
        (
            "{{INFO_WIDTH}}",
            format!("{:.2}", percentage(info, finding_count)),
        ),
        ("{{FINDINGS}}", rendered_findings),
    ];

    for (placeholder, value) in replacements {
        html = html.replace(placeholder, &value);
    }

    Ok(html)
}

fn render_finding(output: &mut String, finding: &Value) {
    let severity = string(finding, "severity");
    let label = match severity {
        "high" => "Alta",
        "medium" => "Media",
        "low" => "Baja",
        _ => "Info",
    };

    let _ = write!(
        output,
        r#"<article class="finding" data-severity="{severity}">
<div class="finding-meta"><span class="severity {severity}">{label}</span><code>{rule}</code></div>
<h3>{title}</h3>
<p>{description}</p>
<div class="finding-meta"><span>Archivo: <code>{file}</code></span><span>Línea: {line}</span></div>
{source_code}
{locations}
<div class="detail"><strong>Recomendación</strong><p>{recommendation}</p></div>
</article>"#,
        severity = escape_html(severity),
        label = label,
        rule = escape_html(string(finding, "rule_id")),
        title = escape_html(string(finding, "title")),
        description = escape_html(string(finding, "description")),
        file = escape_html(string(finding, "file")),
        line = number(finding, "line"),
        source_code = render_source_code(&finding["source_code"]),
        locations = render_locations(finding),
        recommendation = escape_html(string(finding, "recommendation")),
    );
}

fn render_source_code(source_code: &Value) -> String {
    let Some(lines) = source_code.as_array() else {
        return String::new();
    };

    if lines.is_empty() {
        return String::from(
            r#"<div class="detail"><strong>Código fuente</strong><p class="muted">No se pudo recuperar el fragmento del archivo.</p></div>"#,
        );
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

fn affected_files(findings: &[Value]) -> HashSet<String> {
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

    files
}

fn count_severity(findings: &[Value], expected: &str) -> usize {
    findings
        .iter()
        .filter(|finding| string(finding, "severity") == expected)
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
