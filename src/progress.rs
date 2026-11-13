use indicatif::{ProgressBar, ProgressDrawTarget, ProgressState, ProgressStyle};
use std::fmt::Write as FmtWrite;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressMode {
    /// Barra de progreso estándar para análisis estático sin IA.
    Standard,
    /// Indicador animado con estrellas de IA cuando Gemini está activo.
    AiSparkles,
}

pub struct AnalysisProgress {
    mode: ProgressMode,
    bar: ProgressBar,
}

impl AnalysisProgress {
    pub fn new(ai_enabled: bool) -> Self {
        let mode = if ai_enabled {
            ProgressMode::AiSparkles
        } else {
            ProgressMode::Standard
        };
        Self::with_mode(mode)
    }

    pub fn with_mode(mode: ProgressMode) -> Self {
        let draw_target = ProgressDrawTarget::stderr_with_hz(15);

        let bar = match mode {
            ProgressMode::Standard => {
                let pb = ProgressBar::with_draw_target(Some(100), draw_target);
                let style = ProgressStyle::with_template(
                    "  {secs:.dim} │ {spinner:.cyan.bold} {bar:32.cyan/blue} {pos:>3}% │ {msg}",
                )
                .unwrap_or_else(|_| ProgressStyle::default_bar())
                .with_key("secs", |state: &ProgressState, w: &mut dyn FmtWrite| {
                    let _ = write!(w, "{:>3}s", state.elapsed().as_secs());
                })
                .progress_chars("━╸─")
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏", "✓"]);

                pb.set_style(style);
                pb.set_position(0);
                pb.set_message("Iniciando análisis del proyecto...");
                pb.enable_steady_tick(Duration::from_millis(80));
                pb
            }
            ProgressMode::AiSparkles => {
                let pb = ProgressBar::with_draw_target(None, draw_target);
                let style =
                    ProgressStyle::with_template("  {secs:.dim} │ {msg} {spinner:.magenta.bold}")
                        .unwrap_or_else(|_| ProgressStyle::default_spinner())
                        .with_key("secs", |state: &ProgressState, w: &mut dyn FmtWrite| {
                            let _ = write!(w, "{:>3}s", state.elapsed().as_secs());
                        })
                        .tick_strings(&[
                            "✦ ✧ ⋆",
                            "✧ ✨ ✦",
                            "⋆ ✦ ✧",
                            "✨ ✧ ✦",
                            "✦ ⋆ ✨",
                            "✧ ✦ ⋆",
                            "✦ ✨ ✦",
                        ]);

                pb.set_style(style);
                pb.set_message("Iniciando análisis inteligente con Gemini...");
                pb.enable_steady_tick(Duration::from_millis(110));
                pb
            }
        };

        Self { mode, bar }
    }

    pub fn ai_spinner(message: impl Into<String>) -> Self {
        let progress = Self::with_mode(ProgressMode::AiSparkles);
        progress.bar.set_message(message.into());
        progress
    }

    #[cfg(test)]
    pub fn mode(&self) -> ProgressMode {
        self.mode
    }

    pub fn update(&self, percent: u64, message: impl Into<String>) {
        if self.mode == ProgressMode::Standard {
            self.bar.set_position(percent.min(100));
        }
        self.bar.set_message(message.into());
    }

    pub fn finish(&self) {
        self.bar.finish_and_clear();
    }
}

impl Drop for AnalysisProgress {
    fn drop(&mut self) {
        if !self.bar.is_finished() {
            self.bar.finish_and_clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_expected_visual_mode_based_on_ai_flag() {
        let normal = AnalysisProgress::new(false);
        assert_eq!(normal.mode(), ProgressMode::Standard);
        normal.update(45, "Evaluando reglas...");
        normal.finish();

        let ai = AnalysisProgress::new(true);
        assert_eq!(ai.mode(), ProgressMode::AiSparkles);
        ai.update(75, "Consultando Gemini...");
        ai.finish();
    }
}