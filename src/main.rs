mod model;
mod report;
mod rules;
mod scanner;

use anyhow::{Context, Result, bail};
use clap::Parser;
use std::fs;
use std::path::{Path, PathBuf};

use crate::rules::analyze;

#[derive(Debug, Parser)]
#[command(
    name = "dl",
    version,
    about = "DebtLens — analizador estático de deuda técnica",
    long_about = "Analiza un proyecto, resume sus hallazgos y opcionalmente genera un informe JSON o HTML."
)]
struct Cli {
    #[arg(value_name = "PROJECT_PATH")]
    path: PathBuf,

    #[arg(short, long, value_name = "DIRECTORY")]
    output: Option<PathBuf>,

    #[arg(long, requires = "output")]
    html: bool,

    #[arg(short, long)]
    verbose: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if !cli.path.is_dir() {
        bail!(
            "La ruta del proyecto no es un directorio válido: {}",
            cli.path.display()
        );
    }

    let analysis = scanner::scan_project(&cli.path)?;
    let report = analyze(analysis);

    print_summary(&report);

    if cli.verbose {
        print_verbose(&report);
    }

    if let Some(output_dir) = cli.output {
        fs::create_dir_all(&output_dir)
            .with_context(|| format!("No se pudo crear {}", output_dir.display()))?;

        let raw = report::render_raw(&report, cli.verbose)?;

        if cli.html {
            let html = report::render_html(&raw)?;
            let html_path = output_dir.join("report.html");
            let json_path = output_dir.join("report.json");

            remove_previous_report(&json_path)?;

            fs::write(&html_path, html)
                .with_context(|| format!("No se pudo escribir {}", html_path.display()))?;

            println!("Reporte HTML: {}", html_path.display());
        } else {
            let json_path = output_dir.join("report.json");
            let html_path = output_dir.join("report.html");

            remove_previous_report(&html_path)?;

            fs::write(&json_path, raw)
                .with_context(|| format!("No se pudo escribir {}", json_path.display()))?;

            println!("Reporte JSON: {}", json_path.display());
        }
    }

    Ok(())
}

fn remove_previous_report(path: &Path) -> Result<()> {
    if path.is_file() {
        fs::remove_file(path).with_context(|| {
            format!("No se pudo eliminar el informe anterior {}", path.display())
        })?;
    }

    Ok(())
}

fn print_summary(report: &model::AnalysisReport) {
    let syntax_errors = report
        .analysis
        .files
        .iter()
        .filter(|file| !file.syntax_errors.is_empty())
        .count();

    println!("DebtLens — Análisis completado");
    println!("Proyecto: {}", report.analysis.root);
    println!("Archivos analizados: {}", report.analysis.files_scanned);
    println!("Líneas analizadas: {}", report.analysis.total_lines);
    println!("Funciones detectadas: {}", report.analysis.total_functions);
    println!("Hallazgos: {}", report.findings.len());
    println!("Archivos con errores sintácticos: {syntax_errors}");
}

fn print_verbose(report: &model::AnalysisReport) {
    use model::Severity;

    let count = |severity: &Severity| {
        report
            .findings
            .iter()
            .filter(|finding| {
                std::mem::discriminant(&finding.severity) == std::mem::discriminant(severity)
            })
            .count()
    };

    println!();
    println!("Distribución por severidad");
    println!("  Alta: {}", count(&Severity::High));
    println!("  Media: {}", count(&Severity::Medium));
    println!("  Baja: {}", count(&Severity::Low));
    println!("  Info: {}", count(&Severity::Info));

    if report.findings.is_empty() {
        println!("\nNo se detectaron hallazgos.");
        return;
    }

    println!("\nDetalle de hallazgos");

    for finding in &report.findings {
        println!(
            "\n[{:?}] {} — {}",
            finding.severity, finding.rule_id, finding.title
        );
        println!("  Archivo: {}:{}", finding.file, finding.line);
        println!("  {}", finding.description);
        println!("  Recomendación: {}", finding.recommendation);

        print_code_context(&report.analysis.root, &finding.file, finding.line);

        for location in &finding.locations {
            if location.file == finding.file && location.line == finding.line {
                continue;
            }

            println!(
                "\n  Ubicación relacionada: {}:{}",
                location.file, location.line
            );

            print_code_context(&report.analysis.root, &location.file, location.line);
        }
    }
}

fn print_code_context(root: &str, file: &str, line: usize) {
    let path = Path::new(root).join(file);

    let Ok(source) = fs::read_to_string(&path) else {
        println!("  No se pudo leer el archivo fuente: {}", path.display());
        return;
    };

    let lines: Vec<&str> = source.lines().collect();

    if line == 0 || line > lines.len() {
        println!("  No se encontró la línea indicada en el archivo.");
        return;
    }

    let start = line.saturating_sub(4);
    let end = line.saturating_add(3).min(lines.len());

    println!("  Código fuente:");

    for (index, content) in lines.iter().enumerate().take(end).skip(start) {
        let number = index + 1;
        let marker = if number == line { ">" } else { " " };

        println!("  {marker} {number:4} | {content}");
    }
}
