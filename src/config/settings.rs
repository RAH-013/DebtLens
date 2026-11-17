use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_GEMINI_MODEL: &str = "gemini-3.8-flash";
pub const SUPPORTED_PROVIDER: &str = "gemini";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AiLimitsConfig {
    #[serde(default = "default_max_context_tokens")]
    pub max_context_tokens: usize,
    #[serde(default = "default_max_files")]
    pub max_files: usize,
    #[serde(default = "default_max_file_size_kb")]
    pub max_file_size_kb: usize,
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: u64,
}

impl Default for AiLimitsConfig {
    fn default() -> Self {
        Self {
            max_context_tokens: default_max_context_tokens(),
            max_files: default_max_files(),
            max_file_size_kb: default_max_file_size_kb(),
            timeout_seconds: default_timeout_seconds(),
        }
    }
}

fn default_max_context_tokens() -> usize {
    30_000
}

fn default_max_files() -> usize {
    200
}

fn default_max_file_size_kb() -> usize {
    100
}

fn default_timeout_seconds() -> u64 {
    120
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AiCacheConfig {
    #[serde(default = "default_cache_enabled")]
    pub enabled: bool,
}

impl Default for AiCacheConfig {
    fn default() -> Self {
        Self {
            enabled: default_cache_enabled(),
        }
    }
}

fn default_cache_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AiConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default)]
    pub limits: AiLimitsConfig,
    #[serde(default)]
    pub cache: AiCacheConfig,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: default_provider(),
            model: default_model(),
            limits: AiLimitsConfig::default(),
            cache: AiCacheConfig::default(),
        }
    }
}

fn default_provider() -> String {
    SUPPORTED_PROVIDER.to_string()
}

fn default_model() -> String {
    DEFAULT_GEMINI_MODEL.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct AppConfig {
    #[serde(default)]
    pub ai: AiConfig,
}

impl AppConfig {
    pub fn default_config_path() -> Result<PathBuf> {
        let base = dirs::config_dir()
            .context("No se pudo determinar el directorio de configuración del usuario")?;
        Ok(base.join("debtlens").join("config.toml"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::default_config_path()?;
        Self::load_from(&path)
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(path)
            .with_context(|| format!("No se pudo leer la configuración en {}", path.display()))?;

        let config: Self = toml::from_str(&content)
            .with_context(|| format!("Configuración TOML inválida en {}", path.display()))?;

        config.validate()?;
        Ok(config)
    }

    pub fn save(&self) -> Result<PathBuf> {
        let path = Self::default_config_path()?;
        self.save_to(&path)?;
        Ok(path)
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        self.validate()?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("No se pudo crear {}", parent.display()))?;
        }

        let serialized = toml::to_string_pretty(self)
            .context("No se pudo serializar la configuración a TOML")?;

        fs::write(path, serialized)
            .with_context(|| format!("No se pudo escribir {}", path.display()))?;

        Ok(())
    }

    pub fn apply_api_token_limit(&mut self, api_input_token_limit: Option<usize>) {
        if let Some(limit) = api_input_token_limit
            && limit > 0
        {
            self.ai.limits.max_context_tokens = limit;
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.ai.provider.trim() != SUPPORTED_PROVIDER {
            bail!(
                "Proveedor de IA no soportado: '{}'. Solo se admite '{}'.",
                self.ai.provider,
                SUPPORTED_PROVIDER
            );
        }

        let model = self.ai.model.trim();
        if model.is_empty() || model.chars().any(char::is_whitespace) {
            bail!("El nombre del modelo de Gemini configurado no es válido.");
        }

        if self.ai.limits.max_context_tokens == 0 {
            bail!("ai.limits.max_context_tokens debe ser mayor que 0.");
        }

        if self.ai.limits.max_files == 0 {
            bail!("ai.limits.max_files debe ser mayor que 0.");
        }

        if self.ai.limits.max_file_size_kb == 0 {
            bail!("ai.limits.max_file_size_kb debe ser mayor que 0.");
        }

        if self.ai.limits.timeout_seconds == 0 {
            bail!("ai.limits.timeout_seconds debe ser mayor que 0.");
        }

        Ok(())
    }
}