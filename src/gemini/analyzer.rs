use crate::config::AppConfig;
use crate::gemini::cache::AnalysisCache;
use crate::gemini::context::{ContextBuildError, build_project_context};
use crate::gemini::gemini::{GeminiTransport, SecretStore, SingleRequestClient};
use crate::gemini::prompt::{PROMPT_VERSION, build_analysis_prompt};
use crate::gemini::response::parse_and_validate_response;
use crate::model::{AiAnalysis, AiExecutionState, AiStatusSummary, Finding, ProjectAnalysis};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct SemanticAnalysisOutcome {
    pub status: AiStatusSummary,
    pub analysis: Option<AiAnalysis>,
}

#[cfg(test)]
pub fn run_semantic_analysis<S, T>(
    config: &AppConfig,
    project: &ProjectAnalysis,
    static_findings: &[Finding],
    secret_store: &S,
    transport: &T,
) -> SemanticAnalysisOutcome
where
    S: SecretStore + ?Sized,
    T: GeminiTransport + ?Sized,
{
    run_semantic_analysis_with_progress(
        config,
        project,
        static_findings,
        secret_store,
        transport,
        |_| {},
    )
}

pub fn run_semantic_analysis_with_progress<S, T, F>(
    config: &AppConfig,
    project: &ProjectAnalysis,
    static_findings: &[Finding],
    secret_store: &S,
    transport: &T,
    mut on_stage: F,
) -> SemanticAnalysisOutcome
where
    S: SecretStore + ?Sized,
    T: GeminiTransport + ?Sized,
    F: FnMut(&str),
{
    if !config.ai.enabled {
        return SemanticAnalysisOutcome {
            status: AiStatusSummary::disabled(),
            analysis: None,
        };
    }

    let model = config.ai.model.clone();

    on_stage("Verificando credenciales de Gemini...");
    let api_key = match secret_store.get_secret() {
        Ok(Some(key)) => key,
        Ok(None) => {
            return SemanticAnalysisOutcome {
                status: AiStatusSummary {
                    enabled: true,
                    state: AiExecutionState::FailedFallback,
                    model: Some(model),
                    message: "No hay una API Key de Gemini configurada. Ejecuta `dl ai setup` para configurarla.".to_string(),
                },
                analysis: None,
            };
        }
        Err(err) => {
            return SemanticAnalysisOutcome {
                status: AiStatusSummary {
                    enabled: true,
                    state: AiExecutionState::FailedFallback,
                    model: Some(model),
                    message: format!(
                        "No se pudo obtener la API Key desde el gestor seguro: {err:#}"
                    ),
                },
                analysis: None,
            };
        }
    };

    on_stage("Construyendo contexto integral del proyecto...");
    let context = match build_project_context(project, static_findings, &config.ai.limits, &model) {
        Ok(ctx) => ctx,
        Err(ContextBuildError::LimitExceeded(reason)) => {
            return SemanticAnalysisOutcome {
                status: AiStatusSummary {
                    enabled: true,
                    state: AiExecutionState::SkippedContextLimit,
                    model: Some(model),
                    message: reason,
                },
                analysis: None,
            };
        }
        Err(ContextBuildError::Io(reason)) => {
            return SemanticAnalysisOutcome {
                status: AiStatusSummary {
                    enabled: true,
                    state: AiExecutionState::FailedFallback,
                    model: Some(model),
                    message: reason,
                },
                analysis: None,
            };
        }
    };

    on_stage("Consultando caché semántica en memoria...");
    let cache = AnalysisCache::new(config.ai.cache.enabled);
    let cache_key = AnalysisCache::compute_key(&context, &model, PROMPT_VERSION, &config.ai.limits);

    if let Ok(Some(cached_analysis)) = cache.get(&cache_key) {
        return SemanticAnalysisOutcome {
            status: AiStatusSummary {
                enabled: true,
                state: AiExecutionState::Cached,
                model: Some(model),
                message: "Análisis semántico recuperado desde la caché en memoria.".to_string(),
            },
            analysis: Some(cached_analysis),
        };
    }

    on_stage(&format!(
        "Analizando arquitectura y semántica con Gemini ({model})"
    ));
    let prompt = build_analysis_prompt(&context);
    let single_client = SingleRequestClient::new(transport);
    let timeout = Duration::from_secs(config.ai.limits.timeout_seconds);

    let generation = match single_client.analyze_once_detailed(
        &api_key,
        &model,
        &prompt,
        config.ai.limits.max_context_tokens,
        timeout,
    ) {
        Ok(output) => output,
        Err(err) => {
            return SemanticAnalysisOutcome {
                status: AiStatusSummary {
                    enabled: true,
                    state: AiExecutionState::FailedFallback,
                    model: Some(model),
                    message: format!(
                        "No se pudo completar el análisis semántico con Gemini ({err:#}). Se conserva el análisis estático."
                    ),
                },
                analysis: None,
            };
        }
    };

    on_stage("Validando hallazgos semánticos...");
    match parse_and_validate_response(&generation.text, &model, &context) {
        Ok(mut validated) => {
            validated.token_usage = Some(generation.usage);
            let _ = cache.put(&cache_key, &validated);
            SemanticAnalysisOutcome {
                status: AiStatusSummary {
                    enabled: true,
                    state: AiExecutionState::Completed,
                    model: Some(model),
                    message: "Análisis semántico completado correctamente en una única solicitud."
                        .to_string(),
                },
                analysis: Some(validated),
            }
        }
        Err(err) => SemanticAnalysisOutcome {
            status: AiStatusSummary {
                enabled: true,
                state: AiExecutionState::FailedFallback,
                model: Some(model),
                message: format!(
                    "La respuesta de Gemini no superó la validación ({err:#}). Se conserva el análisis estático."
                ),
            },
            analysis: None,
        },
    }
}