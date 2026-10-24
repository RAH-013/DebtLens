mod html;
mod raw;

use crate::model::AnalysisReport;
use anyhow::Result;

pub fn render_raw(report: &AnalysisReport, verbose: bool) -> Result<String> {
    raw::render(report, verbose)
}

pub fn render_html(raw_report: &str) -> Result<String> {
    html::render(raw_report)
}
