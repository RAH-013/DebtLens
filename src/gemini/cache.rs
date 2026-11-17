use crate::config::AiLimitsConfig;
use crate::gemini::context::ProjectContext;
use crate::model::AiAnalysis;
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};

/// Número máximo de entradas retenidas en RAM (RNF04: sin crecimiento ilimitado).
const MAX_ENTRIES: usize = 32;

/// Almacén de proceso en memoria volátil.
///
/// La caché **nunca** toca el sistema de archivos: cumple el principio
/// stateless/zero-persistence (RNF04) y se destruye al terminar el proceso.
fn store() -> &'static Mutex<HashMap<String, AiAnalysis>> {
    static STORE: OnceLock<Mutex<HashMap<String, AiAnalysis>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock_store() -> MutexGuard<'static, HashMap<String, AiAnalysis>> {
    store().lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Debug, Clone, Copy)]
pub struct AnalysisCache {
    enabled: bool,
}

impl AnalysisCache {
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    pub fn compute_key(
        context: &ProjectContext,
        model: &str,
        prompt_version: &str,
        limits: &AiLimitsConfig,
    ) -> String {
        let mut hasher = Sha256::new();

        hasher.update(b"debtlens-ai-cache-v1\n");
        hasher.update(prompt_version.as_bytes());
        hasher.update(b"\nmodel:");
        hasher.update(model.trim().as_bytes());
        hasher.update(b"\nlimits:");
        hasher.update(limits.max_context_tokens.to_le_bytes());
        hasher.update(limits.max_files.to_le_bytes());
        hasher.update(limits.max_file_size_kb.to_le_bytes());

        hasher.update(b"\nfiles:\n");
        for file in &context.files {
            hasher.update(file.path.as_bytes());
            hasher.update(b"\0");
            hasher.update(file.language.as_bytes());
            hasher.update(b"\0");
            hasher.update(file.content.as_bytes());
            hasher.update(b"\n--\n");
        }

        hasher.update(b"\nfindings:\n");
        for finding in &context.static_findings {
            hasher.update(finding.id.as_deref().unwrap_or("").as_bytes());
            hasher.update(b"|");
            hasher.update(finding.rule_id.as_bytes());
            hasher.update(b"|");
            hasher.update(finding.file.as_bytes());
            hasher.update(b"|");
            hasher.update(finding.line.to_le_bytes());
            hasher.update(b"|");
            hasher.update(finding.description.as_bytes());
            hasher.update(b"\n");
        }

        let digest = hasher.finalize();
        hex_encode(&digest)
    }

    pub fn get(&self, key: &str) -> Result<Option<AiAnalysis>> {
        if !self.enabled {
            return Ok(None);
        }

        let mut cached = match lock_store().get(key).cloned() {
            Some(analysis) => analysis,
            None => return Ok(None),
        };

        cached.from_cache = true;
        Ok(Some(cached))
    }

    pub fn put(&self, key: &str, analysis: &AiAnalysis) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        let mut guard = lock_store();

        if guard.len() >= MAX_ENTRIES
            && !guard.contains_key(key)
            && let Some(evicted) = guard.keys().next().cloned()
        {
            guard.remove(&evicted);
        }

        let mut storable = analysis.clone();
        storable.from_cache = false;
        guard.insert(key.to_string(), storable);

        Ok(())
    }

    /// Vacía el almacén en memoria. Solo se usa en pruebas.
    #[cfg(test)]
    pub fn reset() {
        lock_store().clear();
    }

    /// Serializa las pruebas que comparten el almacén global de proceso.
    #[cfg(test)]
    pub fn test_guard() -> MutexGuard<'static, ()> {
        static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        TEST_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}
