pub mod settings;

pub use settings::{AiLimitsConfig, AppConfig, DEFAULT_GEMINI_MODEL};

#[cfg(test)]
pub use settings::{AiCacheConfig, AiConfig};