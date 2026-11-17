use anyhow::{Context, Result, bail};
use reqwest::StatusCode;
use reqwest::blocking::Client;
use reqwest::header::HeaderMap;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::model::TokenUsageSummary;

const GEMINI_API_BASE: &str = "https://generativelanguage.googleapis.com/v1beta";
const KEYRING_SERVICE: &str = "debtlens";
const KEYRING_USER: &str = "gemini-api-key";

static CANCEL_REQUESTED: AtomicBool = AtomicBool::new(false);

pub fn request_cancel() {
    CANCEL_REQUESTED.store(true, Ordering::SeqCst);
}

pub fn is_cancelled() -> bool {
    CANCEL_REQUESTED.load(Ordering::SeqCst)
}

const SPANISH_SYSTEM_INSTRUCTION: &str = "\
Eres un arquitecto de software senior experto en calidad de código, seguridad y deuda técnica. \
REGLA DE IDIOMA OBLIGATORIA: Debes redactar ABSOLUTAMENTE TODOS los campos textuales de tu respuesta JSON \
(descripciones, evaluaciones generales, riesgos principales, acciones recomendadas, explicaciones, \
impactos, títulos, evidencias y recomendaciones) exclusivamente en español técnico claro y natural. \
Nunca respondas en inglés ni mezcles idiomas, excepto al citar nombres exactos de archivos, \
identificadores de funciones/variables o fragmentos literales de código.";

#[derive(Clone)]
pub struct ApiKey(String);

impl ApiKey {
    pub fn new(raw: impl Into<String>) -> Result<Self> {
        let trimmed = raw.into().trim().to_string();
        if trimmed.is_empty() {
            bail!("La API Key de Gemini no puede estar vacía.");
        }
        Ok(Self(trimmed))
    }

    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Devuelve una representación segura de la credencial.
    ///
    /// Nunca revela caracteres de la clave: solo su longitud, para que el
    /// operador pueda distinguir una clave configurada de otra sin exponer
    /// material sensible en la terminal (RNF03).
    pub fn masked(&self) -> String {
        format!("•••••••• ({} caracteres)", self.0.chars().count())
    }
}

impl fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ApiKey([REDACTED])")
    }
}

#[derive(Debug, Clone)]
pub struct AvailableModel {
    pub id: String,
    pub tier_label: String,
    pub api_input_token_limit: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct GeminiGenerationOutput {
    pub text: String,
    pub usage: TokenUsageSummary,
}

pub trait SecretStore {
    fn set_secret(&self, secret: &ApiKey) -> Result<()>;
    fn get_secret(&self) -> Result<Option<ApiKey>>;
    fn delete_secret(&self) -> Result<bool>;
}

pub struct SystemKeyringStore;

impl SecretStore for SystemKeyringStore {
    fn set_secret(&self, secret: &ApiKey) -> Result<()> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
            .context("No se pudo inicializar el almacén seguro de credenciales")?;
        entry
            .set_password(secret.expose())
            .context("No se pudo guardar la API Key en el gestor de credenciales del sistema")
    }

    fn get_secret(&self) -> Result<Option<ApiKey>> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
            .context("No se pudo acceder al almacén seguro de credenciales")?;
        match entry.get_password() {
            Ok(secret) => Ok(Some(ApiKey::new(secret)?)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(err) => Err(err).context("Error al leer la API Key desde el gestor del sistema"),
        }
    }

    fn delete_secret(&self) -> Result<bool> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
            .context("No se pudo acceder al almacén seguro de credenciales")?;
        match entry.delete_credential() {
            Ok(()) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(err) => {
                Err(err).context("No se pudo eliminar la API Key del gestor de credenciales")
            }
        }
    }
}

pub trait GeminiTransport {
    fn generate_content(
        &self,
        api_key: &ApiKey,
        model: &str,
        prompt: &str,
        timeout: Duration,
    ) -> Result<String>;

    fn generate_content_detailed(
        &self,
        api_key: &ApiKey,
        model: &str,
        prompt: &str,
        configured_token_limit: usize,
        timeout: Duration,
    ) -> Result<GeminiGenerationOutput> {
        let text = self.generate_content(api_key, model, prompt, timeout)?;
        Ok(GeminiGenerationOutput {
            text,
            usage: TokenUsageSummary {
                token_limit: configured_token_limit,
                remaining_tokens: configured_token_limit,
                ..Default::default()
            },
        })
    }
}

pub struct HttpGeminiClient;

#[derive(Serialize)]
struct GenerateContentRequest<'a> {
    #[serde(rename = "systemInstruction")]
    system_instruction: RequestContent<'a>,
    contents: Vec<RequestContent<'a>>,
    #[serde(rename = "generationConfig")]
    generation_config: GenerationConfig<'a>,
}

#[derive(Serialize)]
struct CountTokensRequest<'a> {
    contents: Vec<RequestContent<'a>>,
}

#[derive(Serialize)]
struct RequestContent<'a> {
    parts: Vec<RequestPart<'a>>,
}

#[derive(Serialize)]
struct RequestPart<'a> {
    text: &'a str,
}

#[derive(Serialize)]
struct GenerationConfig<'a> {
    temperature: f32,
    #[serde(rename = "responseMimeType")]
    response_mime_type: &'a str,
}

#[derive(Deserialize)]
struct GenerateContentResponse {
    #[serde(default)]
    candidates: Vec<Candidate>,
    #[serde(default, rename = "usageMetadata")]
    usage_metadata: Option<UsageMetadata>,
}

#[derive(Deserialize)]
struct UsageMetadata {
    #[serde(default, rename = "promptTokenCount")]
    prompt_token_count: usize,
    #[serde(default, rename = "candidatesTokenCount")]
    candidates_token_count: usize,
    #[serde(default, rename = "totalTokenCount")]
    total_token_count: usize,
}

#[derive(Deserialize)]
struct Candidate {
    content: Option<CandidateContent>,
}

#[derive(Deserialize)]
struct CandidateContent {
    #[serde(default)]
    parts: Vec<CandidatePart>,
}

#[derive(Deserialize)]
struct CandidatePart {
    text: Option<String>,
}

#[derive(Deserialize)]
struct ListModelsResponse {
    #[serde(default)]
    models: Vec<ModelInfo>,
}

#[derive(Deserialize)]
struct ModelInfo {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default, rename = "inputTokenLimit")]
    input_token_limit: Option<usize>,
    #[serde(default, rename = "supportedGenerationMethods")]
    supported_generation_methods: Vec<String>,
}

#[derive(Deserialize)]
struct GoogleErrorEnvelope {
    error: Option<GoogleErrorDetail>,
}

#[derive(Deserialize)]
struct GoogleErrorDetail {
    message: Option<String>,
}

fn normalize_model_name(model: &str) -> &str {
    let trimmed = model.trim();
    trimmed.strip_prefix("models/").unwrap_or(trimmed)
}

fn is_excluded_or_retired_model(id: &str, description: Option<&str>) -> bool {
    let lower = id.to_ascii_lowercase();
    let desc_lower = description.unwrap_or("").to_ascii_lowercase();

    if desc_lower.contains("no longer available")
        || desc_lower.contains("deprecated")
        || desc_lower.contains("discontinued")
    {
        return true;
    }

    let blocked_tokens = [
        "-latest",
        "-tts",
        "-image",
        "nano-banana",
        "lyria-",
        "-transcribe",
        "robotics",
        "computer-use",
        "deep-research",
        "antigravity",
        "embedding",
        "customtools",
        "gemma-",
        "gemini-1.0",
        "gemini-1.5",
        "gemini-2.0",
        "gemini-2.5-flash",
        "gemini-2.5-pro",
        "gemini-2.5-flash-lite",
        "gemini-4-",
        "gemini-omni",
    ];

    blocked_tokens.iter().any(|tag| lower.contains(tag))
}

fn classify_model_tier(id: &str) -> &'static str {
    let lower = id.to_ascii_lowercase();
    if lower.contains("flash") {
        "Gratis"
    } else {
        "De pago"
    }
}

pub fn format_token_limit(tokens: usize) -> String {
    if tokens >= 1_000_000 {
        let millions = tokens as f64 / 1_048_576.0;
        if (millions - millions.round()).abs() < 0.05 {
            format!("{}M tokens", millions.round() as usize)
        } else {
            format!("{millions:.1}M tokens")
        }
    } else if tokens >= 1_000 {
        format!("{}k tokens", tokens / 1_024)
    } else {
        format!("{tokens} tokens")
    }
}

pub fn format_model_select_items(models: &[AvailableModel]) -> Vec<String> {
    format_model_select_items_with_failures(models, &[], false)
}

pub fn format_model_select_items_with_failures(
    models: &[AvailableModel],
    failed_models: &[String],
    color_enabled: bool,
) -> Vec<String> {
    models
        .iter()
        .map(|m| {
            let tier = format!("[{}]", m.tier_label);
            let limit = m
                .api_input_token_limit
                .map(format_token_limit)
                .unwrap_or_default();

            let base = if limit.is_empty() {
                format!("{:<28} {:<9}", m.id, tier)
            } else {
                format!("{:<28} {:<9} · {}", m.id, tier, limit)
            };

            if failed_models.contains(&m.id) {
                let marked = format!("{base} [no disponible]");
                if color_enabled {
                    format!("\x1b[1;31m{marked}\x1b[0m")
                } else {
                    marked
                }
            } else {
                base
            }
        })
        .collect()
}

fn model_priority_rank(id: &str) -> usize {
    match id {
        "gemini-3.8-flash" => 0,
        "gemini-flash-latest" => 1,
        "gemini-3.7-flash" => 2,
        "gemini-3.6-flash" => 3,
        "gemini-3.5-flash" => 4,
        "gemini-3.5-flash-lite" => 5,
        "gemini-3.1-flash-lite" => 6,
        "gemini-3.1-pro-preview" => 7,
        "gemini-pro-latest" => 8,
        _ if id.starts_with("gemini-3") => 9,
        _ if id.starts_with("gemini-") => 10,
        _ => 11,
    }
}

fn parse_header_usize(headers: &HeaderMap, names: &[&str]) -> Option<usize> {
    for name in names {
        if let Some(val) = headers.get(*name).and_then(|v| v.to_str().ok())
            && let Ok(parsed) = val.trim().parse::<usize>()
        {
            return Some(parsed);
        }
    }
    None
}

fn extract_google_error_message(body: &str) -> Option<String> {
    serde_json::from_str::<GoogleErrorEnvelope>(body)
        .ok()
        .and_then(|env| env.error)
        .and_then(|err| err.message)
        .map(|msg| msg.trim().to_string())
        .filter(|msg| !msg.is_empty())
}

fn format_api_http_error(status: StatusCode, body: &str, model: &str) -> anyhow::Error {
    let detail = extract_google_error_message(body)
        .unwrap_or_else(|| "sin detalle adicional en la respuesta".to_string());

    match status {
        StatusCode::BAD_REQUEST => {
            anyhow::anyhow!("Gemini API rechazó la solicitud (HTTP 400): {detail}")
        }
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => anyhow::anyhow!(
            "API Key inválida o sin permisos para Google Gemini (HTTP {}): {detail}",
            status.as_u16()
        ),
        StatusCode::NOT_FOUND => {
            anyhow::anyhow!("Gemini API respondió HTTP 404 para el modelo '{model}': {detail}")
        }
        StatusCode::TOO_MANY_REQUESTS => anyhow::anyhow!(
            "Cuota o límite de peticiones excedido en Gemini API (HTTP 429): {detail}"
        ),
        StatusCode::SERVICE_UNAVAILABLE => anyhow::anyhow!(
            "El modelo '{model}' está saturado o no disponible temporalmente (HTTP 503): {detail}"
        ),
        other => anyhow::anyhow!(
            "Gemini API devolvió un error inesperado (HTTP {}): {detail}",
            other.as_u16()
        ),
    }
}

fn run_with_deadline_and_cancel<T, F>(timeout: Duration, operation: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(operation());
    });

    let start = Instant::now();
    loop {
        if is_cancelled() {
            bail!("El usuario canceló el análisis.");
        }

        let elapsed = start.elapsed();
        if elapsed >= timeout {
            bail!(
                "Tiempo de espera agotado ({}s) al contactar con Gemini API.",
                timeout.as_secs()
            );
        }

        let slice = (timeout - elapsed).min(Duration::from_millis(50));
        match rx.recv_timeout(slice) {
            Ok(result) => return result,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                bail!("La solicitud a Gemini API se interrumpió inesperadamente.");
            }
        }
    }
}

impl HttpGeminiClient {
    fn probe_model_usable(client: &Client, api_key: &ApiKey, model_id: &str) -> bool {
        if is_cancelled() {
            return false;
        }
        let url = format!("{GEMINI_API_BASE}/models/{model_id}:countTokens");
        let payload = CountTokensRequest {
            contents: vec![RequestContent {
                parts: vec![RequestPart { text: "ping" }],
            }],
        };

        client
            .post(&url)
            .header("x-goog-api-key", api_key.expose())
            .json(&payload)
            .send()
            .map(|resp| resp.status().is_success())
            .unwrap_or(false)
    }

    pub fn fetch_model_input_limit(
        &self,
        api_key: &ApiKey,
        model: &str,
        timeout: Duration,
    ) -> Option<usize> {
        if is_cancelled() {
            return None;
        }
        let model_id = normalize_model_name(model).to_string();
        let api_key_owned = api_key.clone();

        run_with_deadline_and_cancel(timeout, move || {
            let client = Client::builder()
                .connect_timeout(timeout)
                .timeout(timeout)
                .build()?;
            let url = format!("{GEMINI_API_BASE}/models/{model_id}");
            let resp = client
                .get(&url)
                .header("x-goog-api-key", api_key_owned.expose())
                .send()?;

            if !resp.status().is_success() {
                return Ok(None);
            }

            let info: ModelInfo = resp.json()?;
            Ok(info.input_token_limit.filter(|&n| n > 0))
        })
        .ok()
        .flatten()
    }

    pub fn list_available_models(
        &self,
        api_key: &ApiKey,
        timeout: Duration,
    ) -> Result<Vec<AvailableModel>> {
        let api_key_owned = api_key.clone();
        let (status, body) = run_with_deadline_and_cancel(timeout, move || {
            let client = Client::builder()
                .connect_timeout(timeout.min(Duration::from_secs(15)))
                .timeout(timeout)
                .build()
                .context("No se pudo construir el cliente HTTP para consultar los modelos de Gemini")?;

            let list_url = format!("{GEMINI_API_BASE}/models?pageSize=100");
            let response = client
                .get(&list_url)
                .header("x-goog-api-key", api_key_owned.expose())
                .send()
                .context("No se pudo conectar con Google Gemini para verificar la API Key")?;

            let status = response.status();
            let body = response
                .text()
                .context("No se pudo leer la lista de modelos devuelta por Gemini")?;
            Ok((status, body))
        })?;

        if !status.is_success() {
            return Err(format_api_http_error(status, &body, "models"));
        }

        let parsed: ListModelsResponse = serde_json::from_str(&body)
            .context("No se pudo interpretar el catálogo de modelos devuelto por Gemini")?;

        let candidates: Vec<AvailableModel> = parsed
            .models
            .into_iter()
            .filter(|m| {
                m.supported_generation_methods
                    .iter()
                    .any(|method| method == "generateContent")
            })
            .filter_map(|m| {
                let id = normalize_model_name(&m.name).to_string();
                if is_excluded_or_retired_model(&id, m.description.as_deref()) {
                    return None;
                }

                let tier_label = classify_model_tier(&id).to_string();
                let api_input_token_limit = m.input_token_limit.filter(|&n| n > 0);

                Some(AvailableModel {
                    id,
                    tier_label,
                    api_input_token_limit,
                })
            })
            .collect();

        if candidates.is_empty() {
            bail!(
                "La API Key es válida, pero no se encontraron modelos compatibles activos."
            );
        }

        let probe_timeout = timeout.min(Duration::from_secs(8));
        let probe_client = Client::builder()
            .connect_timeout(probe_timeout)
            .timeout(probe_timeout)
            .build()
            .context("No se pudo construir el cliente de verificación de modelos")?;

        let mut verified: Vec<AvailableModel> = std::thread::scope(|scope| {
            let handles: Vec<_> = candidates
                .into_iter()
                .map(|model| {
                    let c = &probe_client;
                    let k = api_key;
                    scope.spawn(move || {
                        if Self::probe_model_usable(c, k, &model.id) {
                            Some(model)
                        } else {
                            None
                        }
                    })
                })
                .collect();

            handles
                .into_iter()
                .filter_map(|h| h.join().ok().flatten())
                .collect()
        });

        if is_cancelled() {
            bail!("El usuario canceló el análisis.");
        }

        if verified.is_empty() {
            bail!(
                "Tu API Key se autenticó, pero ningún modelo respondió como disponible en este momento."
            );
        }

        verified.sort_by(|a, b| {
            model_priority_rank(&a.id)
                .cmp(&model_priority_rank(&b.id))
                .then_with(|| a.id.cmp(&b.id))
        });

        Ok(verified)
    }

    pub fn generate_content_with_usage(
        &self,
        api_key: &ApiKey,
        model: &str,
        prompt: &str,
        configured_token_limit: usize,
        timeout: Duration,
    ) -> Result<GeminiGenerationOutput> {
        let model_id = normalize_model_name(model).to_string();
        let api_key_owned = api_key.clone();
        let prompt_owned = prompt.to_string();
        let model_for_req = model_id.clone();

        let (status, headers, body) = run_with_deadline_and_cancel(timeout, move || {
            let client = Client::builder()
                .connect_timeout(timeout.min(Duration::from_secs(15)))
                .timeout(timeout)
                .build()
                .context("No se pudo construir el cliente HTTP para Gemini")?;

            let url = format!("{GEMINI_API_BASE}/models/{model_for_req}:generateContent");
            let payload = GenerateContentRequest {
                system_instruction: RequestContent {
                    parts: vec![RequestPart {
                        text: SPANISH_SYSTEM_INSTRUCTION,
                    }],
                },
                contents: vec![RequestContent {
                    parts: vec![RequestPart {
                        text: &prompt_owned,
                    }],
                }],
                generation_config: GenerationConfig {
                    temperature: 0.2,
                    response_mime_type: "application/json",
                },
            };

            let response = client
                .post(&url)
                .header("x-goog-api-key", api_key_owned.expose())
                .json(&payload)
                .send()
                .context("Error de red al contactar con Gemini API")?;

            let status = response.status();
            let headers = response.headers().clone();
            let body = response
                .text()
                .context("No se pudo leer el cuerpo de respuesta de Gemini API")?;

            Ok((status, headers, body))
        })?;

        if !status.is_success() {
            return Err(format_api_http_error(status, &body, &model_id));
        }

        let parsed: GenerateContentResponse = serde_json::from_str(&body)
            .context("La respuesta HTTP de Gemini no tiene la estructura esperada")?;

        let text = parsed
            .candidates
            .into_iter()
            .filter_map(|c| c.content)
            .flat_map(|c| c.parts)
            .filter_map(|p| p.text)
            .collect::<Vec<_>>()
            .join("\n");

        if text.trim().is_empty() {
            bail!("Gemini API respondió correctamente pero no devolvió contenido textual.");
        }

        let meta = parsed.usage_metadata.unwrap_or(UsageMetadata {
            prompt_token_count: 0,
            candidates_token_count: 0,
            total_token_count: 0,
        });

        let total_tokens = if meta.total_token_count > 0 {
            meta.total_token_count
        } else {
            meta.prompt_token_count + meta.candidates_token_count
        };

        let header_limit = parse_header_usize(
            &headers,
            &["x-ratelimit-limit-tokens", "x-ratelimit-tokens-limit"],
        );
        let header_remaining = parse_header_usize(
            &headers,
            &[
                "x-ratelimit-remaining-tokens",
                "x-ratelimit-tokens-remaining",
            ],
        );

        let api_reported_limit = header_limit
            .or_else(|| self.fetch_model_input_limit(api_key, &model_id, Duration::from_secs(5)));

        let effective_limit = match api_reported_limit {
            Some(api_lim) if api_lim > 0 => api_lim,
            _ => configured_token_limit.max(total_tokens),
        };

        let (remaining_tokens, from_rate_limit_header) = match header_remaining {
            Some(rem) => (rem, true),
            None => (effective_limit.saturating_sub(total_tokens), false),
        };

        Ok(GeminiGenerationOutput {
            text,
            usage: TokenUsageSummary {
                prompt_tokens: meta.prompt_token_count,
                output_tokens: meta.candidates_token_count,
                total_tokens,
                token_limit: effective_limit,
                remaining_tokens,
                from_rate_limit_header,
                tier_label: classify_model_tier(&model_id).to_string(),
            },
        })
    }
}

impl GeminiTransport for HttpGeminiClient {
    fn generate_content(
        &self,
        api_key: &ApiKey,
        model: &str,
        prompt: &str,
        timeout: Duration,
    ) -> Result<String> {
        let out = self.generate_content_with_usage(api_key, model, prompt, 0, timeout)?;
        Ok(out.text)
    }

    fn generate_content_detailed(
        &self,
        api_key: &ApiKey,
        model: &str,
        prompt: &str,
        configured_token_limit: usize,
        timeout: Duration,
    ) -> Result<GeminiGenerationOutput> {
        self.generate_content_with_usage(api_key, model, prompt, configured_token_limit, timeout)
    }
}

pub struct SingleRequestClient<'a, T: GeminiTransport + ?Sized> {
    transport: &'a T,
    request_sent: AtomicBool,
}

impl<'a, T: GeminiTransport + ?Sized> SingleRequestClient<'a, T> {
    pub fn new(transport: &'a T) -> Self {
        Self {
            transport,
            request_sent: AtomicBool::new(false),
        }
    }

    #[cfg(test)]
    pub fn analyze_once(
        &self,
        api_key: &ApiKey,
        model: &str,
        prompt: &str,
        timeout: Duration,
    ) -> Result<String> {
        if self
            .request_sent
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            bail!(
                "Se bloqueó una segunda solicitud a Gemini en el mismo análisis (límite estricto: 1 solicitud)."
            );
        }

        self.transport
            .generate_content(api_key, model, prompt, timeout)
    }

    pub fn analyze_once_detailed(
        &self,
        api_key: &ApiKey,
        model: &str,
        prompt: &str,
        configured_token_limit: usize,
        timeout: Duration,
    ) -> Result<GeminiGenerationOutput> {
        if self
            .request_sent
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            bail!(
                "Se bloqueó una segunda solicitud a Gemini en el mismo análisis (límite estricto: 1 solicitud)."
            );
        }

        self.transport.generate_content_detailed(
            api_key,
            model,
            prompt,
            configured_token_limit,
            timeout,
        )
    }
}