use crate::model::AnalysisReport;
use anyhow::{Context, Result};
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn render(report: &AnalysisReport, verbose: bool) -> Result<String> {
    let mut value =
        serde_json::to_value(report).context("No se pudo convertir el análisis a JSON")?;

    value["generated_at"] = Value::String(current_utc_timestamp());

    let root = value["analysis"]["root"].as_str().unwrap_or("").to_owned();

    if let Some(findings) = value["findings"].as_array_mut() {
        for finding in findings {
            let is_duplicate = finding["rule_id"].as_str() == Some("CODE001");
            let length = if is_duplicate {
                duplicate_block_length(&finding["description"])
            } else {
                7
            };

            let file = finding["file"].as_str().unwrap_or("").to_owned();
            let line = finding["line"].as_u64().unwrap_or(0) as usize;

            finding["source_code"] = source_context(&root, &file, line, length, is_duplicate);

            if let Some(locations) = finding["locations"].as_array_mut() {
                for location in locations {
                    let file = location["file"].as_str().unwrap_or("").to_owned();
                    let line = location["line"].as_u64().unwrap_or(0) as usize;

                    location["source_code"] =
                        source_context(&root, &file, line, length, is_duplicate);
                }
            }
        }
    }

    if let Some(new_findings) = value
        .get_mut("ai_analysis")
        .and_then(|ai| ai.get_mut("new_findings"))
        .and_then(Value::as_array_mut)
    {
        for finding in new_findings {
            let file = finding["file"].as_str().unwrap_or("").to_owned();
            let start = finding["start_line"].as_u64().unwrap_or(1) as usize;
            let end = finding["end_line"].as_u64().unwrap_or(start as u64) as usize;
            let span = end.saturating_sub(start).saturating_add(1).clamp(1, 15);
            finding["source_code"] = source_context(&root, &file, start, span, true);
        }
    }

    if !verbose
        && let Some(files) = value
            .get_mut("analysis")
            .and_then(|analysis| analysis.get_mut("files"))
            .and_then(Value::as_array_mut)
    {
        for file in files {
            let path = file.get("path").cloned().unwrap_or(Value::Null);
            let syntax_errors = file
                .get("syntax_errors")
                .cloned()
                .unwrap_or_else(|| Value::Array(Vec::new()));

            *file = serde_json::json!({
                "path": path,
                "syntax_errors": syntax_errors
            });
        }
    }

    serde_json::to_string_pretty(&value).context("No se pudo generar el reporte JSON")
}

fn current_utc_timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let days = (secs / 86_400) as i64;
    let time_of_day = secs % 86_400;
    let hour = time_of_day / 3600;
    let minute = (time_of_day % 3600) / 60;
    let second = time_of_day % 60;

    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02} UTC")
}

fn civil_from_days(days_since_epoch: i64) -> (i64, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = y + i64::from(m <= 2);
    (year, m as u32, d as u32)
}

fn duplicate_block_length(description: &Value) -> usize {
    let description = description.as_str().unwrap_or("");

    description
        .split(|character: char| !character.is_ascii_digit())
        .find_map(|part| part.parse::<usize>().ok())
        .filter(|length| *length > 0)
        .unwrap_or(5)
}

fn source_context(root: &str, file: &str, line: usize, length: usize, exact_block: bool) -> Value {
    let path = Path::new(root).join(file);

    let Ok(source) = fs::read_to_string(path) else {
        return serde_json::json!([]);
    };

    let lines: Vec<&str> = source.lines().collect();

    if line == 0 || line > lines.len() {
        return serde_json::json!([]);
    }

    let (start, end) = if exact_block {
        let start = line - 1;
        (start, start.saturating_add(length).min(lines.len()))
    } else {
        (
            line.saturating_sub(4),
            line.saturating_add(3).min(lines.len()),
        )
    };

    let context: Vec<Value> = (start..end)
        .map(|index| {
            serde_json::json!({
                "line": index + 1,
                "content": lines[index],
                "is_target": index + 1 == line
            })
        })
        .collect();

    serde_json::json!(context)
}
