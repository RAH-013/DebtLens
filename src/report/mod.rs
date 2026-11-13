mod html;
mod raw;

use crate::model::AnalysisReport;
use anyhow::Result;
use std::path::{Path, PathBuf};

pub fn render_raw(report: &AnalysisReport, verbose: bool) -> Result<String> {
    raw::render(report, verbose)
}

pub fn render_html(raw_report: &str, output_dir: &Path) -> Result<PathBuf> {
    html::render_to_dir(raw_report, output_dir)
}
