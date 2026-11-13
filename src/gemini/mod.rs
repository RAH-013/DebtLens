pub mod analyzer;
pub mod cache;
pub mod context;
#[allow(clippy::module_inception)]
pub mod gemini;
pub mod prompt;
pub mod response;

#[cfg(test)]
mod tests;

pub use analyzer::run_semantic_analysis_with_progress;
pub use gemini::{ApiKey, HttpGeminiClient, SecretStore, SystemKeyringStore};

#[cfg(test)]
pub use analyzer::run_semantic_analysis;
#[cfg(test)]
pub use cache::AnalysisCache;
#[cfg(test)]
pub use context::{ContextBuildError, build_project_context, is_ignored_or_sensitive_path};
#[cfg(test)]
pub use gemini::{GeminiTransport, SingleRequestClient};
#[cfg(test)]
pub use prompt::{PROMPT_VERSION, build_analysis_prompt};
#[cfg(test)]
pub use response::parse_and_validate_response;
