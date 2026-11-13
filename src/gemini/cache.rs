use crate::config::AiLimitsConfig;
use crate::gemini::context::ProjectContext;
use crate::model::AiAnalysis;
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AnalysisCache {
    dir: PathBuf,
    enabled: bool,
}

impl AnalysisCache {
    pub fn new(dir: PathBuf, enabled: bool) -> Self {
        Self { dir, enabled }
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

        let entry_path = self.entry_path(key);
        if !entry_path.is_file() {
            return Ok(None);
        }

        let raw = fs::read_to_string(&entry_path)
            .with_context(|| format!("No se pudo leer la caché en {}", entry_path.display()))?;

        let mut analysis: AiAnalysis = match serde_json::from_str(&raw) {
            Ok(data) => data,
            Err(_) => {
                let _ = fs::remove_file(&entry_path);
                return Ok(None);
            }
        };

        analysis.from_cache = true;
        Ok(Some(analysis))
    }

    pub fn put(&self, key: &str, analysis: &AiAnalysis) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        fs::create_dir_all(&self.dir).with_context(|| {
            format!(
                "No se pudo crear el directorio de caché {}",
                self.dir.display()
            )
        })?;

        let mut storable = analysis.clone();
        storable.from_cache = false;

        let serialized = serde_json::to_string_pretty(&storable)
            .context("No se pudo serializar la entrada de caché")?;

        let entry_path = self.entry_path(key);
        fs::write(&entry_path, serialized)
            .with_context(|| format!("No se pudo escribir la caché en {}", entry_path.display()))?;

        Ok(())
    }

    fn entry_path(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.json"))
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
