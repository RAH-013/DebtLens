use crate::model::AnalysisReport;
use anyhow::{Context, Result};
use serde_json::Value;
use std::fs;
use std::path::Path;

pub fn render(report: &AnalysisReport, verbose: bool) -> Result<String> {
    let mut value =
        serde_json::to_value(report).context("No se pudo convertir el análisis a JSON")?;

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
