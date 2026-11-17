mod config;
mod gemini;
mod model;
mod progress;
mod report;
mod rules;
mod scanner;

use anyhow::{Context, Result, bail};
use clap::{CommandFactory, Parser, Subcommand};
use dialoguer::{Input, Password, Select};
use std::fmt;
use std::fs;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use crate::config::{AppConfig, DEFAULT_GEMINI_MODEL};
use crate::gemini::gemini::{
    format_model_select_items, format_model_select_items_with_failures, is_cancelled,
    request_cancel,
};
use crate::gemini::{
    ApiKey, HttpGeminiClient, SecretStore, SystemKeyringStore, run_semantic_analysis_with_progress,
};
use crate::model::{
    AiExecutionState, AnalysisReport, DebtLevel, Severity, calculate_technical_debt_index,
};
use crate::progress::AnalysisProgress;
use crate::rules::analyze_with_progress;

#[derive(Debug)]
struct UserCancelled;

impl fmt::Display for UserCancelled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("El usuario canceló el análisis.")
    }
}

impl std::error::Error for UserCancelled {}

fn check_cancelled() -> Result<()> {
    if is_cancelled() {
        bail!(UserCancelled);
    }
    Ok(())
}

fn restore_terminal_cursor() {
    if std::io::stderr().is_terminal() {
        eprint!("\x1b[?25h");
        let _ = std::io::stderr().flush();
    }
    if std::io::stdout().is_terminal() {
        print!("\x1b[?25h");
        let _ = std::io::stdout().flush();
    }
}

fn install_ctrlc_handler() {
    let _ = ctrlc::set_handler(move || {
        let already_cancelled = is_cancelled();
        request_cancel();
        restore_terminal_cursor();

        if already_cancelled {
            let palette = TermPalette::stderr();
            eprintln!(
                "\n{} {}",
                palette.bold_yellow("⚠"),
                palette.bold_yellow("El usuario canceló el análisis.")
            );
            std::process::exit(130);
        }
    });
}

#[derive(Debug, Parser)]
#[command(
    name = "DebtLens",
    bin_name = "dl",
    version = concat!(env!("CARGO_PKG_VERSION"), " (+ ✦Gemini AI✦)"),
    about = "DebtLens — analizador estático y semántico de deuda técnica",
    long_about = "Analiza un proyecto, resume sus hallazgos estáticos y semánticos y opcionalmente genera un informe JSON o HTML.",
    after_help = "\
Subcomandos de IA (dl ai <COMANDO>):
  dl ai setup               Configura de forma interactiva la API Key y el modelo de Gemini
  dl ai enable              Habilita el análisis semántico con Google Gemini
  dl ai disable             Deshabilita el análisis semántico (solo análisis estático)
  dl ai status              Muestra el estado de configuración, límites y credenciales
  dl ai timeout <SEGUNDOS>  Configura el tiempo límite de espera para Gemini (ej. 120)
  dl ai remove              Elimina la API Key del almacén seguro y deshabilita la IA

Ejemplos:
  dl ./mi-proyecto
  dl ./mi-proyecto --verbose
  dl ./mi-proyecto -o ./reporte --html --verbose
  dl ai setup
  dl ai timeout 120",
    args_conflicts_with_subcommands = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Ruta al directorio raíz del proyecto a analizar
    #[arg(value_name = "PROJECT_PATH")]
    path: Option<PathBuf>,

    /// Directorio donde se guardará el informe generado (report.json o bundle HTML)
    #[arg(short, long, value_name = "DIRECTORY")]
    output: Option<PathBuf>,

    /// Genera el informe en formato HTML autocontenido (requiere --output)
    #[arg(long, requires = "output")]
    html: bool,

    /// Muestra el detalle completo de hallazgos, fragmentos de código y evaluación de IA
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Gestiona la integración con Google Gemini (setup, enable, disable, status, timeout, remove)
    #[command(
        long_about = "Configura, habilita, deshabilita, consulta, ajusta el timeout o elimina la integración con Google Gemini."
    )]
    Ai {
        #[command(subcommand)]
        action: AiCommand,
    },
}

#[derive(Debug, Subcommand)]
enum AiCommand {
    /// Configura de forma interactiva la API Key y preferencias de Gemini
    Setup,
    /// Habilita el análisis semántico con Gemini
    Enable,
    /// Deshabilita el análisis semántico con Gemini
    Disable,
    /// Muestra el estado actual de configuración y credenciales de Gemini
    Status,
    /// Configura el tiempo límite de espera (en segundos) para consultas a Gemini
    Timeout {
        /// Tiempo máximo de espera en segundos (por ejemplo: 60, 120, 180)
        #[arg(value_name = "SEGUNDOS")]
        seconds: u64,
    },
    /// Elimina la API Key del almacén seguro y deshabilita la integración
    Remove,
}

#[derive(Debug, Clone, Copy)]
struct TermPalette {
    enabled: bool,
}

impl TermPalette {
    fn stdout() -> Self {
        Self {
            enabled: std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none(),
        }
    }

    fn stderr() -> Self {
        Self {
            enabled: std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none(),
        }
    }

    fn paint(&self, code: &str, text: &str) -> String {
        if self.enabled {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    fn bold(&self, text: &str) -> String {
        self.paint("1", text)
    }

    fn dim(&self, text: &str) -> String {
        self.paint("2", text)
    }

    fn bold_cyan(&self, text: &str) -> String {
        self.paint("1;36", text)
    }

    fn bold_magenta(&self, text: &str) -> String {
        self.paint("1;35", text)
    }

    fn green(&self, text: &str) -> String {
        self.paint("32", text)
    }

    fn bold_green(&self, text: &str) -> String {
        self.paint("1;32", text)
    }

    fn yellow(&self, text: &str) -> String {
        self.paint("33", text)
    }

    fn bold_yellow(&self, text: &str) -> String {
        self.paint("1;33", text)
    }

    fn bold_red(&self, text: &str) -> String {
        self.paint("1;31", text)
    }

    fn underline_cyan(&self, text: &str) -> String {
        self.paint("4;36", text)
    }

    fn badge_warn(&self, text: &str) -> String {
        self.paint("1;97;43", &format!(" {text} "))
    }

    fn badge_critical(&self, text: &str) -> String {
        self.paint("1;97;41", &format!(" {text} "))
    }

    fn rule(&self) -> String {
        self.dim("────────────────────────────────────────────────────────────────────────")
    }

    fn severity_tag(&self, severity: Severity) -> String {
        let label = format!("[{}]", format_severity_es(severity));
        match severity {
            Severity::Critical => self.bold_red(&label),
            Severity::High => self.bold_red(&label),
            Severity::Medium => self.bold_yellow(&label),
            Severity::Low => self.bold_cyan(&label),
            Severity::Info => self.dim(&label),
        }
    }

    fn priority_tag(&self, priority: &str) -> String {
        let label = format!("[{priority}]");
        match priority.to_ascii_lowercase().as_str() {
            "critical" | "high" => self.bold_red(&label),
            "medium" => self.bold_yellow(&label),
            "low" => self.bold_cyan(&label),
            _ => self.dim(&label),
        }
    }

    fn debt_level_label(&self, level: DebtLevel) -> String {
        let text = format_debt_level_es(level);
        match level {
            DebtLevel::Low => self.bold_green(text),
            DebtLevel::Moderate => self.bold_yellow(text),
            DebtLevel::High => self.bold_red(text),
            DebtLevel::Critical => self.badge_critical(text),
        }
    }
}

fn clear_terminal() {
    if std::io::stdout().is_terminal() {
        print!("\x1b[2J\x1b[H");
        let _ = std::io::stdout().flush();
    } else if std::io::stderr().is_terminal() {
        eprint!("\x1b[2J\x1b[H");
        let _ = std::io::stderr().flush();
    }
}

fn extract_failure_reason(message: &str) -> &str {
    let trimmed = message
        .trim()
        .strip_suffix("Se conserva el análisis estático.")
        .unwrap_or(message)
        .trim();

    for prefix in [
        "No se pudo completar el análisis semántico con Gemini (",
        "La respuesta de Gemini no superó la validación (",
    ] {
        if let Some(rest) = trimmed.strip_prefix(prefix)
            && let Some(inner) = rest.strip_suffix(')')
        {
            return inner.trim();
        }
    }

    trimmed
}

fn main() -> ExitCode {
    install_ctrlc_handler();

    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            restore_terminal_cursor();
            if err.downcast_ref::<UserCancelled>().is_some() || is_cancelled() {
                let palette = TermPalette::stderr();
                eprintln!(
                    "\n{} {}",
                    palette.bold_yellow("⚠"),
                    palette.bold_yellow("El usuario canceló el análisis.")
                );
                return ExitCode::from(130);
            }

            let palette = TermPalette::stderr();
            eprintln!("{} {err:#}", palette.bold_red("Error:"));
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    if let Some(Commands::Ai { action }) = cli.command {
        return handle_ai_command(action);
    }

    let Some(project_path) = cli.path else {
        let mut cmd = Cli::command();
        cmd.print_help()?;
        println!();
        bail!("Debes indicar <PROJECT_PATH> o un subcomando válido (por ejemplo: `dl ai setup`).");
    };

    if !project_path.is_dir() {
        bail!(
            "La ruta del proyecto no es un directorio válido: {}",
            project_path.display()
        );
    }

    let mut config = match AppConfig::load() {
        Ok(cfg) => cfg,
        Err(err) => {
            eprintln!("Advertencia al cargar la configuración de IA: {err:#}");
            AppConfig::default()
        }
    };

    check_cancelled()?;

    let mut progress = AnalysisProgress::new(config.ai.enabled);

    progress.update(
        10,
        "Escaneando archivos y árboles sintácticos (Tree-sitter)...",
    );
    let analysis = scanner::scan_project(&project_path)?;
    check_cancelled()?;

    progress.update(30, "Evaluando reglas estáticas de deuda técnica...");
    let mut report = analyze_with_progress(analysis, |current, total| {
        if is_cancelled() {
            return;
        }
        let pct = 30 + ((current as u64 * 45) / total.max(1) as u64);
        progress.update(
            pct,
            format!("Evaluando reglas estáticas ({current}/{total})..."),
        );
    });
    check_cancelled()?;

    let mut fallback_acknowledged = false;

    if config.ai.enabled {
        let mut failed_models: Vec<String> = Vec::new();
        let mut had_previous_failure = false;

        let outcome = loop {
            check_cancelled()?;

            let attempt_outcome = run_semantic_analysis_with_progress(
                &config,
                &report.analysis,
                &report.findings,
                &SystemKeyringStore,
                &HttpGeminiClient,
                |stage_msg| {
                    progress.update(82, stage_msg);
                },
            );

            check_cancelled()?;

            if attempt_outcome.status.state != AiExecutionState::FailedFallback
                || !std::io::stdin().is_terminal()
            {
                break attempt_outcome;
            }

            progress.finish();

            let current_model = config.ai.model.clone();
            if !failed_models.contains(&current_model) {
                failed_models.push(current_model.clone());
            }

            clear_terminal();
            let err_palette = TermPalette::stderr();
            let clean_reason = extract_failure_reason(&attempt_outcome.status.message);

            if !had_previous_failure {
                eprintln!(
                    "{} No se pudo completar el análisis semántico con {}.",
                    err_palette.badge_warn("AVISO"),
                    err_palette.bold(&current_model)
                );
                had_previous_failure = true;
            } else {
                eprintln!(
                    "{} El modelo {} tampoco está disponible en este momento.",
                    err_palette.badge_warn("AVISO"),
                    err_palette.bold(&current_model)
                );
            }
            eprintln!("{} {}\n", err_palette.dim("Detalle:"), clean_reason);

            let recovery_options = [
                "Cambiar de modelo y volver a intentar",
                "Continuar solo con el análisis estático (sin IA)",
                "Cancelar",
            ];

            let should_retry = loop {
                check_cancelled()?;

                let choice = Select::new()
                    .with_prompt("¿Qué deseas hacer? (usa ↑/↓ y Enter)")
                    .items(recovery_options)
                    .default(0)
                    .interact()
                    .map_err(|err| {
                        if is_cancelled() {
                            anyhow::anyhow!(UserCancelled)
                        } else {
                            anyhow::anyhow!(err).context("No se pudo leer la opción seleccionada")
                        }
                    })?;

                check_cancelled()?;

                match choice {
                    0 => {
                        if prompt_switch_gemini_model(&mut config, &failed_models)? {
                            break true;
                        }
                    }
                    1 => {
                        fallback_acknowledged = true;
                        break false;
                    }
                    _ => {
                        bail!(UserCancelled);
                    }
                }
            };

            if should_retry {
                clear_terminal();
                progress = AnalysisProgress::new(true);
                continue;
            } else {
                progress = AnalysisProgress::new(false);
                break attempt_outcome;
            }
        };

        check_cancelled()?;

        progress.update(90, "Calculando índice de deuda técnica...");
        report.ai_status = outcome.status;
        report.ai_analysis = outcome.analysis;
        report.debt_index = calculate_technical_debt_index(
            &report.analysis,
            &report.findings,
            report.ai_analysis.as_ref(),
        );

        if let Some(ai) = &report.ai_analysis
            && let Some(usage) = &ai.token_usage
            && usage.token_limit > 0
            && usage.token_limit != config.ai.limits.max_context_tokens
        {
            config.ai.limits.max_context_tokens = usage.token_limit;
            let _ = config.save();
        }
    } else {
        progress.update(85, "Calculando índice de deuda técnica...");
    }

    check_cancelled()?;

    if let Some(output_dir) = cli.output {
        progress.update(94, "Generando informe de resultados...");
        fs::create_dir_all(&output_dir)
            .with_context(|| format!("No se pudo crear {}", output_dir.display()))?;

        let raw = report::render_raw(&report, cli.verbose)?;
        check_cancelled()?;

        if cli.html {
            let html_path = report::render_html(&raw, &output_dir)?;
            progress.update(100, "Análisis completado.");
            progress.finish();
            clear_terminal();

            print_summary(&report, !fallback_acknowledged);
            if cli.verbose {
                print_verbose(&report);
            }
            let palette = TermPalette::stdout();
            println!(
                "\n{} {}",
                palette.bold_green("Reporte HTML:"),
                palette.underline_cyan(&html_path.display().to_string())
            );
        } else {
            let json_path = output_dir.join("report.json");
            fs::write(&json_path, raw)
                .with_context(|| format!("No se pudo escribir {}", json_path.display()))?;
            progress.update(100, "Análisis completado.");
            progress.finish();
            clear_terminal();

            print_summary(&report, !fallback_acknowledged);
            if cli.verbose {
                print_verbose(&report);
            }
            let palette = TermPalette::stdout();
            println!(
                "\n{} {}",
                palette.bold_green("Reporte JSON:"),
                palette.underline_cyan(&json_path.display().to_string())
            );
        }
    } else {
        progress.update(100, "Análisis completado.");
        progress.finish();
        clear_terminal();

        print_summary(&report, !fallback_acknowledged);
        if cli.verbose {
            print_verbose(&report);
        }
    }

    Ok(())
}

fn handle_ai_command(action: AiCommand) -> Result<()> {
    match action {
        AiCommand::Setup => run_ai_setup(),
        AiCommand::Enable => run_ai_enable(),
        AiCommand::Disable => run_ai_disable(),
        AiCommand::Status => run_ai_status(),
        AiCommand::Timeout { seconds } => run_ai_timeout(seconds),
        AiCommand::Remove => run_ai_remove(),
    }
}

fn confirm_si_no(prompt: &str, default_yes: bool) -> Result<bool> {
    let hint = if default_yes { "[S/n]" } else { "[s/N]" };
    loop {
        check_cancelled()?;

        let answer: String = Input::new()
            .with_prompt(format!("{prompt} {hint}"))
            .allow_empty(true)
            .interact_text()
            .map_err(|err| {
                if is_cancelled() {
                    anyhow::anyhow!(UserCancelled)
                } else {
                    anyhow::anyhow!(err).context("No se pudo leer la respuesta desde la terminal")
                }
            })?;

        check_cancelled()?;

        match answer.trim().to_ascii_lowercase().as_str() {
            "" => return Ok(default_yes),
            "s" | "si" | "sí" => return Ok(true),
            "n" | "no" => return Ok(false),
            _ => println!("Por favor responde 's' (sí) o 'n' (no)."),
        }
    }
}

fn prompt_switch_gemini_model(config: &mut AppConfig, failed_models: &[String]) -> Result<bool> {
    check_cancelled()?;

    let Some(api_key) = SystemKeyringStore
        .get_secret()
        .context("No se pudo leer la API Key desde el gestor seguro")?
    else {
        eprintln!("No hay una API Key configurada. Ejecuta `dl ai setup` primero.\n");
        return Ok(false);
    };

    let spinner =
        AnalysisProgress::ai_spinner("Consultando modelos disponibles en Google Gemini...");
    let timeout = Duration::from_secs(config.ai.limits.timeout_seconds.min(30));
    let models_result = HttpGeminiClient.list_available_models(&api_key, timeout);
    spinner.finish();

    check_cancelled()?;

    let available_models = match models_result {
        Ok(models) => models,
        Err(err) => {
            if is_cancelled() {
                bail!(UserCancelled);
            }
            eprintln!("No se pudo obtener la lista de modelos: {err:#}\n");
            return Ok(false);
        }
    };

    let Some(default_index) = available_models
        .iter()
        .position(|m| !failed_models.contains(&m.id))
    else {
        let err_palette = TermPalette::stderr();
        eprintln!(
            "{} Todos los modelos disponibles ya fallaron en esta sesión.\n",
            err_palette.bold_yellow("Aviso:")
        );
        return Ok(false);
    };

    let color_enabled =
        std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none();
    let items = format_model_select_items_with_failures(
        &available_models,
        failed_models,
        color_enabled,
    );

    let mut current_default = default_index;
    loop {
        check_cancelled()?;

        let selected_index = Select::new()
            .with_prompt("Selecciona otro modelo de Gemini (usa ↑/↓ y Enter)")
            .items(&items)
            .default(current_default)
            .interact()
            .map_err(|err| {
                if is_cancelled() {
                    anyhow::anyhow!(UserCancelled)
                } else {
                    anyhow::anyhow!(err).context("No se pudo leer la selección del modelo")
                }
            })?;

        check_cancelled()?;

        let chosen = &available_models[selected_index];
        if failed_models.contains(&chosen.id) {
            current_default = default_index;
            let err_palette = TermPalette::stderr();
            eprintln!(
                "{} El modelo {} falló en esta sesión y no se puede seleccionar; elige otro.",
                err_palette.bold_red("✕"),
                err_palette.bold_red(&chosen.id)
            );
            continue;
        }

        config.ai.model = chosen.id.clone();
        config.apply_api_token_limit(chosen.api_input_token_limit);
        config.validate()?;
        let _ = config.save();

        return Ok(true);
    }
}

fn run_ai_setup() -> Result<()> {
    let mut config = AppConfig::load().unwrap_or_default();
    let existing_key = SystemKeyringStore.get_secret().ok().flatten();

    let api_key = if let Some(current_key) = existing_key {
        let prompt = format!(
            "Introduce tu API Key de Google Gemini ({}) [Enter para mantener]",
            current_key.masked()
        );
        let raw_key = Password::new()
            .with_prompt(prompt)
            .allow_empty_password(true)
            .interact()
            .map_err(|err| {
                if is_cancelled() {
                    anyhow::anyhow!(UserCancelled)
                } else {
                    anyhow::anyhow!(err).context("No se pudo leer la API Key desde la terminal")
                }
            })?;

        check_cancelled()?;

        if raw_key.trim().is_empty() {
            current_key
        } else {
            ApiKey::new(raw_key)?
        }
    } else {
        let raw_key = Password::new()
            .with_prompt("Introduce tu API Key de Google Gemini")
            .interact()
            .map_err(|err| {
                if is_cancelled() {
                    anyhow::anyhow!(UserCancelled)
                } else {
                    anyhow::anyhow!(err).context("No se pudo leer la API Key desde la terminal")
                }
            })?;

        check_cancelled()?;
        ApiKey::new(raw_key)?
    };

    let spinner = AnalysisProgress::ai_spinner(
        "Verificando API Key y consultando modelos disponibles en Google Gemini...",
    );
    let timeout = Duration::from_secs(config.ai.limits.timeout_seconds.min(30));
    let models_result = HttpGeminiClient.list_available_models(&api_key, timeout);
    spinner.finish();

    check_cancelled()?;

    let available_models = models_result.context(
        "No se pudo validar la API Key con Google Gemini; no se modificaron las credenciales",
    )?;

    let preferred_model = if config.ai.model.trim().is_empty() {
        DEFAULT_GEMINI_MODEL
    } else {
        config.ai.model.trim()
    };

    let default_index = available_models
        .iter()
        .position(|m| m.id == preferred_model)
        .unwrap_or(0);

    let items = format_model_select_items(&available_models);

    let selected_index = Select::new()
        .with_prompt("Selecciona el modelo de Gemini a utilizar (usa ↑/↓ y Enter)")
        .items(&items)
        .default(default_index)
        .interact()
        .map_err(|err| {
            if is_cancelled() {
                anyhow::anyhow!(UserCancelled)
            } else {
                anyhow::anyhow!(err).context("No se pudo leer la selección del modelo")
            }
        })?;

    check_cancelled()?;

    let chosen = &available_models[selected_index];
    config.ai.model = chosen.id.clone();
    config.apply_api_token_limit(chosen.api_input_token_limit);
    config.validate()?;

    SystemKeyringStore.set_secret(&api_key)?;
    let persisted = SystemKeyringStore
        .get_secret()
        .context("No se pudo verificar la persistencia en el almacén seguro del sistema")?;

    let Some(saved_key) = persisted else {
        bail!(
            "El servicio de credenciales del sistema operativo no persistió la API Key. \
             Asegúrate de tener activo `gnome-keyring-daemon` o `kwalletd` en tu sesión."
        );
    };

    let palette = TermPalette::stdout();
    println!(
        "\n{} {}",
        palette.badge_warn("⚠ ADVERTENCIA"),
        palette.bold_yellow(
            "Google Gemini es una inteligencia artificial y puede fallar o generar resultados imprecisos."
        )
    );
    println!(
        "  {}\n",
        palette.yellow(
            "Revisa siempre sus sugerencias antes de aplicar cambios; si el servicio falla, DebtLens conservará el análisis estático."
        )
    );

    let enable_now = confirm_si_no("¿Deseas habilitar el análisis con IA ahora?", true)?;

    config.ai.enabled = enable_now;
    let saved_path = config.save()?;

    println!(
        "{} {}",
        palette.dim("API Key verificada y guardada:"),
        palette.bold(&saved_key.masked())
    );
    println!(
        "{} {} ({})",
        palette.dim("Modelo seleccionado:"),
        palette.bold_cyan(&chosen.id),
        chosen.tier_label
    );
    println!(
        "{} {} tokens",
        palette.dim("Límite de contexto configurado:"),
        palette.bold(&config.ai.limits.max_context_tokens.to_string())
    );
    println!(
        "{} {}",
        palette.dim("Configuración guardada en:"),
        saved_path.display()
    );
    println!(
        "{} {}",
        palette.dim("Estado del análisis con Gemini:"),
        if config.ai.enabled {
            palette.bold_green("habilitado")
        } else {
            palette.bold_yellow("deshabilitado")
        }
    );

    Ok(())
}

fn run_ai_enable() -> Result<()> {
    let mut config = AppConfig::load().unwrap_or_default();
    config.ai.enabled = true;
    let saved_path = config.save()?;

    let palette = TermPalette::stdout();
    println!(
        "{} ({}).",
        palette.bold_green("Análisis con Gemini habilitado"),
        saved_path.display()
    );

    match SystemKeyringStore.get_secret() {
        Ok(Some(_)) => {}
        Ok(None) => {
            println!(
                "{} aún no hay una API Key almacenada. Ejecuta `dl ai setup` antes de analizar un proyecto.",
                palette.bold_yellow("Aviso:")
            );
        }
        Err(err) => {
            println!(
                "{} no se pudo verificar el almacén seguro: {err:#}",
                palette.bold_yellow("Aviso:")
            );
        }
    }

    Ok(())
}

fn run_ai_disable() -> Result<()> {
    let mut config = AppConfig::load().unwrap_or_default();
    config.ai.enabled = false;
    let saved_path = config.save()?;

    let palette = TermPalette::stdout();
    println!(
        "{} ({}).",
        palette.bold_yellow("Análisis con Gemini deshabilitado"),
        saved_path.display()
    );

    Ok(())
}

fn run_ai_timeout(seconds: u64) -> Result<()> {
    if seconds == 0 {
        bail!("El tiempo de espera debe ser mayor que 0 segundos.");
    }

    let mut config = AppConfig::load().unwrap_or_default();
    config.ai.limits.timeout_seconds = seconds;
    let saved_path = config.save()?;

    let palette = TermPalette::stdout();
    println!(
        "{} {} ({}).",
        palette.bold_green("Tiempo límite de Gemini actualizado a:"),
        palette.bold_cyan(&format!("{seconds}s")),
        saved_path.display()
    );

    Ok(())
}

fn run_ai_status() -> Result<()> {
    let config_path = AppConfig::default_config_path()?;
    let config = AppConfig::load_from(&config_path).unwrap_or_default();
    let palette = TermPalette::stdout();

    let key_status = match SystemKeyringStore.get_secret() {
        Ok(Some(key)) => palette.green(&format!("Configurada en almacén seguro ({})", key.masked())),
        Ok(None) => palette.yellow("No configurada"),
        Err(_) => palette.bold_red("Almacén seguro no disponible"),
    };

    println!(
        "{} {}",
        palette.bold_cyan("✦ DebtLens"),
        palette.bold("— Estado de integración con Gemini")
    );
    println!("{}", palette.rule());
    println!(
        "  {} {}",
        palette.dim("Habilitado:"),
        if config.ai.enabled {
            palette.bold_green("sí")
        } else {
            palette.bold_yellow("no")
        }
    );
    println!("  {} {}", palette.dim("Proveedor:"), config.ai.provider);
    println!(
        "  {} {}",
        palette.dim("Modelo:"),
        palette.bold_cyan(&config.ai.model)
    );
    println!("  {} {key_status}", palette.dim("API Key:"));
    println!(
        "  {} {}",
        palette.dim("Caché (RAM, solo sesión):"),
        if config.ai.cache.enabled {
            palette.green("habilitada")
        } else {
            palette.dim("deshabilitada")
        }
    );
    println!(
        "  {} {} tokens máx. | {} archivos máx. | {} KB/archivo | {}s timeout",
        palette.dim("Límites:"),
        config.ai.limits.max_context_tokens,
        config.ai.limits.max_files,
        config.ai.limits.max_file_size_kb,
        config.ai.limits.timeout_seconds
    );
    println!(
        "  {} {}",
        palette.dim("Archivo de configuración:"),
        config_path.display()
    );

    Ok(())
}

fn run_ai_remove() -> Result<()> {
    let removed = SystemKeyringStore.delete_secret()?;
    let mut config = AppConfig::load().unwrap_or_default();
    config.ai.enabled = false;
    let saved_path = config.save()?;

    let palette = TermPalette::stdout();
    if removed {
        println!(
            "{}",
            palette.green("API Key eliminada del almacén seguro del sistema operativo.")
        );
    } else {
        println!(
            "{}",
            palette.dim("No había ninguna API Key almacenada en el gestor seguro.")
        );
    }
    println!(
        "{} ({}).",
        palette.bold_yellow("Análisis con Gemini deshabilitado"),
        saved_path.display()
    );

    Ok(())
}

fn format_debt_level_es(level: DebtLevel) -> &'static str {
    match level {
        DebtLevel::Low => "Bajo",
        DebtLevel::Moderate => "Medio",
        DebtLevel::High => "Alto",
        DebtLevel::Critical => "Crítico",
    }
}

fn format_severity_es(severity: Severity) -> &'static str {
    match severity {
        Severity::Critical => "Crítica",
        Severity::High => "Alta",
        Severity::Medium => "Media",
        Severity::Low => "Baja",
        Severity::Info => "Info",
    }
}

fn print_summary(report: &AnalysisReport, show_ai_warning: bool) {
    let palette = TermPalette::stdout();
    let syntax_errors = report
        .analysis
        .files
        .iter()
        .filter(|file| !file.syntax_errors.is_empty())
        .count();

    let findings_count = report.findings.len().to_string();
    let styled_findings = if report.findings.is_empty() {
        palette.bold_green(&findings_count)
    } else {
        palette.bold_yellow(&findings_count)
    };

    let syntax_count = syntax_errors.to_string();
    let styled_syntax = if syntax_errors == 0 {
        palette.green(&syntax_count)
    } else {
        palette.bold_red(&syntax_count)
    };

    let score_str = format!("{:.2}/100", report.debt_index.score);

    println!(
        "{} {}",
        palette.bold_cyan("✦ DebtLens"),
        palette.bold("— Análisis completado")
    );
    println!("{}", palette.rule());
    println!(
        "  {} {}",
        palette.dim("Proyecto:"),
        palette.bold(&report.analysis.root)
    );
    println!(
        "  {} {}  {}  {} {}  {}  {} {}",
        palette.dim("Archivos:"),
        palette.bold_cyan(&report.analysis.files_scanned.to_string()),
        palette.dim("│"),
        palette.dim("Líneas:"),
        palette.bold_cyan(&report.analysis.total_lines.to_string()),
        palette.dim("│"),
        palette.dim("Funciones:"),
        palette.bold_cyan(&report.analysis.total_functions.to_string())
    );
    println!(
        "  {} {styled_findings}  {}  {} {styled_syntax}",
        palette.dim("Hallazgos estáticos:"),
        palette.dim("│"),
        palette.dim("Errores sintácticos:")
    );
    println!(
        "  {} {} ({})",
        palette.dim("Índice de deuda técnica:"),
        palette.bold(&score_str),
        palette.debt_level_label(report.debt_index.level)
    );

    match report.ai_status.state {
        AiExecutionState::Completed | AiExecutionState::Cached => {
            println!(
                "  {} {} {}",
                palette.dim("Gemini IA:"),
                palette.bold_green("✓"),
                palette.green(&report.ai_status.message)
            );
        }
        AiExecutionState::Disabled => {
            println!(
                "  {} {}",
                palette.dim("Gemini IA:"),
                palette.dim(&report.ai_status.message)
            );
        }
        AiExecutionState::SkippedContextLimit | AiExecutionState::FailedFallback => {
            println!(
                "  {} {}",
                palette.dim("Gemini IA:"),
                palette.yellow("respaldo estático activo (análisis semántico omitido)")
            );
            if show_ai_warning {
                let err_palette = TermPalette::stderr();
                let clean_reason = extract_failure_reason(&report.ai_status.message);
                eprintln!(
                    "  {} {}",
                    err_palette.bold_yellow("Aviso Gemini:"),
                    clean_reason
                );
            }
        }
    }

    if let Some(ai) = &report.ai_analysis {
        println!(
            "  {} {}",
            palette.dim("Nuevos hallazgos semánticos (Gemini):"),
            palette.bold_magenta(&ai.new_findings.len().to_string())
        );
        if let Some(usage) = &ai.token_usage {
            let fuente = if usage.from_rate_limit_header {
                "cuota API"
            } else {
                "límite del modelo API"
            };
            println!(
                "  {} {} usados ({} entrada + {} salida) {} {} restantes de {} ({})",
                palette.dim("Tokens Gemini:"),
                palette.bold_cyan(&usage.total_tokens.to_string()),
                usage.prompt_tokens,
                usage.output_tokens,
                palette.dim("│"),
                palette.bold_green(&usage.remaining_tokens.to_string()),
                usage.token_limit,
                palette.dim(fuente)
            );
        }
    }
    println!("{}", palette.rule());
}

fn print_verbose(report: &AnalysisReport) {
    let palette = TermPalette::stdout();
    let count = |severity: Severity| {
        report
            .findings
            .iter()
            .filter(|finding| finding.severity == severity)
            .count()
    };

    println!();
    println!("{}", palette.bold("Distribución por severidad (estático)"));
    println!(
        "  {} {}   {} {}   {} {}   {} {}",
        palette.bold_red("● Alta:"),
        palette.bold(&count(Severity::High).to_string()),
        palette.bold_yellow("● Media:"),
        palette.bold(&count(Severity::Medium).to_string()),
        palette.bold_cyan("● Baja:"),
        palette.bold(&count(Severity::Low).to_string()),
        palette.dim("● Info:"),
        palette.bold(&count(Severity::Info).to_string()),
    );

    if report.findings.is_empty() {
        println!(
            "\n{}",
            palette.bold_green("✓ No se detectaron hallazgos estáticos.")
        );
    } else {
        println!(
            "\n{}",
            palette.bold_cyan("Detalle de hallazgos estáticos")
        );
        println!("{}", palette.rule());

        for finding in &report.findings {
            let id_label = finding.id.as_deref().unwrap_or("-");
            let location_str = format!("{}:{}", finding.file, finding.line);
            println!(
                "\n{} {} {} — {}",
                palette.severity_tag(finding.severity),
                palette.bold(&finding.rule_id),
                palette.dim(&format!("({id_label})")),
                palette.bold(&finding.title)
            );
            println!(
                "  {} {}",
                palette.dim("Archivo:"),
                palette.underline_cyan(&location_str)
            );
            println!("  {}", finding.description);
            println!(
                "  {} {}",
                palette.bold_green("Recomendación:"),
                finding.recommendation
            );

            if let Some(ai) = &report.ai_analysis
                && let Some(id) = &finding.id
                && let Some(assessment) = ai.existing_findings.iter().find(|a| &a.finding_id == id)
            {
                let fp_label = if assessment.possible_false_positive {
                    palette.bold_yellow("sí")
                } else {
                    palette.green("no")
                };
                println!(
                    "  {} {} {}:",
                    palette.bold_magenta("✦ Evaluación Gemini"),
                    palette.priority_tag(assessment.priority.as_str()),
                    palette.dim(&format!(
                        "(confianza: {:.0}%, posible falso positivo: {fp_label})",
                        assessment.confidence * 100.0
                    ))
                );
                println!(
                    "    {} {}",
                    palette.dim("Explicación:"),
                    assessment.explanation
                );
                println!("    {} {}", palette.dim("Impacto:"), assessment.impact);
                println!(
                    "    {} {}",
                    palette.green("Solución propuesta:"),
                    assessment.recommendation
                );
            }

            print_code_context(&report.analysis.root, &finding.file, finding.line);

            for location in &finding.locations {
                if location.file == finding.file && location.line == finding.line {
                    continue;
                }

                let rel_loc = format!("{}:{}", location.file, location.line);
                println!(
                    "\n  {} {}",
                    palette.dim("Ubicación relacionada:"),
                    palette.underline_cyan(&rel_loc)
                );

                print_code_context(&report.analysis.root, &location.file, location.line);
            }
        }
    }

    if let Some(ai) = &report.ai_analysis {
        println!(
            "\n{}",
            palette.bold_magenta(&format!(
                "✦ Resumen ejecutivo (Google Gemini — {})",
                ai.model
            ))
        );
        println!("{}", palette.rule());
        println!(
            "  {} {}",
            palette.bold("Descripción:"),
            ai.project_summary.description
        );
        println!(
            "  {} {}",
            palette.bold("Evaluación general:"),
            ai.project_summary.overall_assessment
        );

        if !ai.project_summary.main_risks.is_empty() {
            println!("  {}", palette.bold_yellow("Riesgos principales:"));
            for risk in &ai.project_summary.main_risks {
                println!("    {} {risk}", palette.bold_red("▸"));
            }
        }

        if !ai.executive_summary.recommended_actions.is_empty() {
            println!("  {}", palette.bold_green("Acciones recomendadas:"));
            for action in &ai.executive_summary.recommended_actions {
                println!("    {} {action}", palette.bold_green("✓"));
            }
        }

        if !ai.new_findings.is_empty() {
            println!(
                "\n{}",
                palette.bold_magenta("✦ Nuevos hallazgos semánticos detectados por Gemini")
            );
            println!("{}", palette.rule());
            for finding in &ai.new_findings {
                let line = finding.start_line.unwrap_or(1);
                let loc_str = format!("{}:{}", finding.file, line);
                println!(
                    "\n{} {} {} {}",
                    palette.priority_tag(finding.priority.as_str()),
                    palette.bold_cyan(&format!("[{}]", finding.category.as_str())),
                    palette.bold(&finding.title),
                    palette.dim(&format!("(confianza: {:.0}%)", finding.confidence * 100.0))
                );
                println!(
                    "  {} {}",
                    palette.dim("Archivo:"),
                    palette.underline_cyan(&loc_str)
                );
                println!(
                    "  {} {}",
                    palette.dim("Descripción:"),
                    finding.description
                );
                println!(
                    "  {} {}",
                    palette.dim("Evidencia:"),
                    palette.yellow(&finding.evidence)
                );
                println!("  {} {}", palette.dim("Impacto:"), finding.impact);
                println!(
                    "  {} {}",
                    palette.bold_green("Recomendación:"),
                    finding.recommendation
                );
            }
        }
    }
}

fn print_code_context(root: &str, file: &str, line: usize) {
    let palette = TermPalette::stdout();
    let path = Path::new(root).join(file);

    let Ok(source) = fs::read_to_string(&path) else {
        println!(
            "  {}",
            palette.dim(&format!(
                "No se pudo leer el archivo fuente: {}",
                path.display()
            ))
        );
        return;
    };

    let lines: Vec<&str> = source.lines().collect();

    if line == 0 || line > lines.len() {
        println!(
            "  {}",
            palette.dim("No se encontró la línea indicada en el archivo.")
        );
        return;
    }

    // RNF05: mismo contexto simétrico de ±4 líneas que el reporte.
    let target = line - 1;
    let start = target.saturating_sub(report::raw::CONTEXT_RADIUS);
    let end = target
        .saturating_add(report::raw::CONTEXT_RADIUS + 1)
        .min(lines.len());

    println!("  {}", palette.dim("Código fuente:"));

    for (index, content) in lines.iter().enumerate().take(end).skip(start) {
        let number = index + 1;
        if number == line {
            let line_text = format!("  > {number:4} │ {content}");
            println!("{}", palette.bold_yellow(&line_text));
        } else {
            let prefix = format!("    {number:4} │ ");
            println!("{}{}", palette.dim(&prefix), content);
        }
    }
}

#[cfg(test)]
mod cli_contract_tests {
    use super::*;

    #[test]
    fn parses_documented_invocation() {
        let cli = Cli::try_parse_from(["dl", ".", "-o", "salida", "--html", "-v"])
            .expect("la invocación documentada debe seguir siendo válida");

        assert!(cli.html);
        assert!(cli.verbose);
        assert_eq!(cli.output.as_deref(), Some(Path::new("salida")));
        assert_eq!(cli.path.as_deref(), Some(Path::new(".")));
    }

    #[test]
    fn html_flag_still_requires_output() {
        assert!(
            Cli::try_parse_from(["dl", ".", "--html"]).is_err(),
            "--html sin --output debe seguir siendo un error"
        );
    }

    #[test]
    fn keeps_ai_subcommands() {
        for args in [
            vec!["dl", "ai", "setup"],
            vec!["dl", "ai", "enable"],
            vec!["dl", "ai", "disable"],
            vec!["dl", "ai", "status"],
            vec!["dl", "ai", "timeout", "30"],
            vec!["dl", "ai", "remove"],
        ] {
            assert!(
                Cli::try_parse_from(&args).is_ok(),
                "el subcomando {args:?} debe seguir existiendo"
            );
        }
    }
}