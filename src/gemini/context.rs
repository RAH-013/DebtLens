use crate::config::AiLimitsConfig;
use crate::model::{FileAnalysis, Finding, ProjectAnalysis};
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::Path;

const IGNORED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "vendor",
    "dist",
    "build",
    "coverage",
    ".next",
    ".turbo",
    ".cache",
    "__pycache__",
    ".venv",
    "venv",
];

const SENSITIVE_FILENAMES: &[&str] = &[
    ".env",
    ".npmrc",
    ".netrc",
    ".pypirc",
    "credentials",
    "credentials.json",
    "secrets.json",
    "secrets.yaml",
    "secrets.yml",
    "kubeconfig",
    "id_rsa",
    "id_ed25519",
    "id_ecdsa",
    "id_dsa",
];

const SENSITIVE_EXTENSIONS: &[&str] = &[
    "pem", "key", "p12", "pfx", "jks", "keystore", "crt", "cer", "der", "token", "secret",
];

const GENERATED_OR_LOCK_FILES: &[&str] = &[
    "package-lock.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "cargo.lock",
    "composer.lock",
    "poetry.lock",
];

const BINARY_EXTENSIONS: &[&str] = &[
    "exe", "dll", "so", "dylib", "bin", "wasm", "o", "a", "lib", "png", "jpg", "jpeg", "gif",
    "webp", "ico", "pdf", "zip", "tar", "gz", "7z", "rar", "mp3", "mp4", "woff", "woff2", "ttf",
    "eot", "sqlite", "db",
];

#[derive(Debug, Clone)]
pub enum ContextBuildError {
    LimitExceeded(String),
    Io(String),
}

impl std::fmt::Display for ContextBuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LimitExceeded(msg) => write!(f, "{msg}"),
            Self::Io(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for ContextBuildError {}

#[derive(Debug, Clone)]
pub struct ContextFile {
    pub path: String,
    pub language: String,
    pub lines: usize,
    pub metrics: FileAnalysis,
    pub dependencies: Vec<String>,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct ProjectContext {
    pub project_name: String,
    pub directory_tree: Vec<String>,
    pub languages: Vec<String>,
    pub technologies: Vec<String>,
    pub files: Vec<ContextFile>,
    pub static_findings: Vec<Finding>,
    pub estimated_tokens: usize,
}

impl ProjectContext {
    pub fn file_line_count(&self, relative_path: &str) -> Option<usize> {
        let normalized = normalize_rel_path(relative_path);
        self.files
            .iter()
            .find(|file| file.path == normalized)
            .map(|file| file.lines)
    }
}

pub fn normalize_rel_path(path: &str) -> String {
    path.trim().trim_start_matches("./").replace('\\', "/")
}

pub fn is_ignored_or_sensitive_path(relative_path: &str) -> bool {
    let normalized = normalize_rel_path(relative_path);
    if normalized.is_empty() {
        return true;
    }

    let path = Path::new(&normalized);

    for component in path.components() {
        let part = component.as_os_str().to_string_lossy();
        let lower = part.to_ascii_lowercase();
        if IGNORED_DIRS.iter().any(|dir| *dir == lower) {
            return true;
        }
    }

    let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
        return true;
    };
    let lower_name = file_name.to_ascii_lowercase();

    if lower_name == ".env"
        || lower_name.starts_with(".env.")
        || lower_name.ends_with(".env")
        || SENSITIVE_FILENAMES.contains(&lower_name.as_str())
    {
        return true;
    }

    if lower_name.contains("secret")
        || lower_name.contains("credential")
        || lower_name.contains("password")
        || lower_name.contains("private_key")
        || lower_name.contains("private-key")
        || lower_name.starts_with("service-account")
        || lower_name.starts_with("firebase-adminsdk")
    {
        return true;
    }

    if GENERATED_OR_LOCK_FILES.contains(&lower_name.as_str())
        || lower_name.ends_with(".min.js")
        || lower_name.ends_with(".bundle.js")
        || lower_name.ends_with(".generated.js")
        || lower_name.ends_with(".generated.ts")
        || lower_name.ends_with(".d.ts")
    {
        return true;
    }

    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let lower_ext = ext.to_ascii_lowercase();
        if SENSITIVE_EXTENSIONS.contains(&lower_ext.as_str())
            || BINARY_EXTENSIONS.contains(&lower_ext.as_str())
        {
            return true;
        }
    }

    false
}

pub fn estimate_tokens(text: &str) -> usize {
    let chars = text.chars().count();
    chars.div_ceil(3)
}

pub fn build_project_context(
    analysis: &ProjectAnalysis,
    static_findings: &[Finding],
    limits: &AiLimitsConfig,
    model: &str,
) -> std::result::Result<ProjectContext, ContextBuildError> {
    let root_path = Path::new(&analysis.root);
    let project_name = root_path
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("project")
        .to_string();

    let safe_files: Vec<&FileAnalysis> = analysis
        .files
        .iter()
        .filter(|file| !is_ignored_or_sensitive_path(&file.path))
        .collect();

    if safe_files.len() > limits.max_files {
        return Err(ContextBuildError::LimitExceeded(format!(
            "El proyecto contiene {} archivos analizables, superando el límite configurado de {} archivos (ai.limits.max_files). \
             No se dividirá en múltiples solicitudes; ajusta el límite o ejecuta solo el análisis estático.",
            safe_files.len(),
            limits.max_files
        )));
    }

    let max_file_bytes = (limits.max_file_size_kb as u64).saturating_mul(1024);
    let max_fragment_chars = (limits.max_file_size_kb.saturating_mul(1024)).max(4096);

    let mut directory_set = BTreeSet::new();
    let mut language_set = BTreeSet::new();
    let mut context_files = Vec::with_capacity(safe_files.len());

    for file in safe_files {
        let rel_path = normalize_rel_path(&file.path);
        if let Some(parent) = Path::new(&rel_path).parent() {
            let parent_str = parent.to_string_lossy().replace('\\', "/");
            if !parent_str.is_empty() && parent_str != "." {
                directory_set.insert(parent_str);
            }
        }

        language_set.insert(file.language.clone());

        let disk_path = root_path.join(&rel_path);
        let raw_bytes = fs::read(&disk_path).map_err(|err| {
            ContextBuildError::Io(format!(
                "No se pudo leer el archivo fuente {}: {err}",
                disk_path.display()
            ))
        })?;

        if raw_bytes.contains(&0) {
            continue;
        }

        if raw_bytes.len() as u64 > max_file_bytes {
            return Err(ContextBuildError::LimitExceeded(format!(
                "El archivo '{}' ({} KB) supera el límite individual configurado de {} KB (ai.limits.max_file_size_kb).",
                rel_path,
                raw_bytes.len().div_ceil(1024),
                limits.max_file_size_kb
            )));
        }

        let raw_text = String::from_utf8(raw_bytes).map_err(|_| {
            ContextBuildError::Io(format!(
                "El archivo '{}' no contiene UTF-8 válido y no puede enviarse como texto.",
                rel_path
            ))
        })?;

        if raw_text.chars().count() > max_fragment_chars {
            return Err(ContextBuildError::LimitExceeded(format!(
                "El fragmento de código de '{}' excede el tamaño máximo permitido por archivo ({} caracteres).",
                rel_path, max_fragment_chars
            )));
        }

        let sanitized_content = redact_inline_secrets(&raw_text);
        let dependencies = extract_module_dependencies(&sanitized_content);

        context_files.push(ContextFile {
            path: rel_path,
            language: file.language.clone(),
            lines: file.lines,
            metrics: file.clone(),
            dependencies,
            content: sanitized_content,
        });
    }

    context_files.sort_by(|a, b| a.path.cmp(&b.path));

    let technologies = detect_technologies(root_path, &context_files);
    let filtered_findings: Vec<Finding> = static_findings
        .iter()
        .filter(|finding| !is_ignored_or_sensitive_path(&finding.file))
        .cloned()
        .collect();

    let effective_token_limit = limits.max_context_tokens;

    let estimated_tokens = estimate_context_tokens(
        &project_name,
        &directory_set,
        &language_set,
        &technologies,
        &context_files,
        &filtered_findings,
    );

    if estimated_tokens > effective_token_limit {
        return Err(ContextBuildError::LimitExceeded(format!(
            "El contexto integral estimado ({} tokens) supera el límite configurado de {} tokens para el modelo '{}'. \
             El análisis estático continúa disponible sin cambios.",
            estimated_tokens, effective_token_limit, model
        )));
    }

    Ok(ProjectContext {
        project_name,
        directory_tree: directory_set.into_iter().collect(),
        languages: language_set.into_iter().collect(),
        technologies,
        files: context_files,
        static_findings: filtered_findings,
        estimated_tokens,
    })
}

fn estimate_context_tokens(
    project_name: &str,
    directories: &BTreeSet<String>,
    languages: &BTreeSet<String>,
    technologies: &[String],
    files: &[ContextFile],
    findings: &[Finding],
) -> usize {
    let mut overhead = estimate_tokens(project_name) + 1200;

    for dir in directories {
        overhead += estimate_tokens(dir);
    }
    for lang in languages {
        overhead += estimate_tokens(lang);
    }
    for tech in technologies {
        overhead += estimate_tokens(tech);
    }
    for finding in findings {
        overhead += estimate_tokens(&finding.rule_id)
            + estimate_tokens(&finding.title)
            + estimate_tokens(&finding.description)
            + estimate_tokens(&finding.file)
            + estimate_tokens(&finding.recommendation)
            + 24;
    }

    let files_tokens: usize = files
        .iter()
        .map(|file| {
            estimate_tokens(&file.path)
                + estimate_tokens(&file.content)
                + file.metrics.functions.len() * 18
                + 32
        })
        .sum();

    overhead + files_tokens
}

fn extract_module_dependencies(source: &str) -> Vec<String> {
    let mut deps = BTreeSet::new();

    for line in source.lines() {
        let trimmed = line.trim();

        if let Some(rest) = trimmed.strip_prefix("import ")
            && let Some(from_idx) = rest.find(" from ")
        {
            let target = trim_quotes(&rest[from_idx + 6..]);
            if !target.is_empty() {
                deps.insert(target.to_string());
            }
        } else if let Some(rest) = trimmed.strip_prefix("import ") {
            let target = trim_quotes(rest);
            if !target.is_empty() && !target.contains('{') && !target.contains('*') {
                deps.insert(target.to_string());
            }
        }

        if let Some(req_idx) = trimmed.find("require(") {
            let after = &trimmed[req_idx + 8..];
            if let Some(end_idx) = after.find(')') {
                let target = trim_quotes(&after[..end_idx]);
                if !target.is_empty() {
                    deps.insert(target.to_string());
                }
            }
        }
    }

    deps.into_iter().collect()
}

fn trim_quotes(raw: &str) -> &str {
    raw.trim()
        .trim_end_matches(';')
        .trim()
        .trim_matches(|ch| ch == '"' || ch == '\'' || ch == '`')
}

fn detect_technologies(root: &Path, files: &[ContextFile]) -> Vec<String> {
    let mut techs = BTreeSet::new();

    for file in files {
        match file.language.as_str() {
            "javascript" => {
                techs.insert("JavaScript".to_string());
            }
            other if !other.is_empty() => {
                techs.insert(other.to_string());
            }
            _ => {}
        }

        if file.path.ends_with(".jsx") {
            techs.insert("React / JSX".to_string());
        } else if file.path.ends_with(".mjs") || file.path.ends_with(".cjs") {
            techs.insert("Node.js Modules".to_string());
        }

        for dep in &file.dependencies {
            let lower = dep.to_ascii_lowercase();
            if lower == "express" {
                techs.insert("Express".to_string());
            } else if lower == "react" || lower.starts_with("react-") {
                techs.insert("React".to_string());
            } else if lower == "next" || lower.starts_with("next/") {
                techs.insert("Next.js".to_string());
            } else if lower == "fastify" {
                techs.insert("Fastify".to_string());
            } else if lower == "koa" {
                techs.insert("Koa".to_string());
            } else if lower == "mongoose" || lower == "sequelize" || lower == "prisma" {
                techs.insert("ORM / Database Layer".to_string());
            } else if lower == "axios" {
                techs.insert("Axios".to_string());
            }
        }
    }

    let pkg_json = root.join("package.json");
    if pkg_json.is_file()
        && let Ok(content) = fs::read_to_string(&pkg_json)
        && let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content)
    {
        techs.insert("Node.js (package.json)".to_string());
        let mut all_deps = HashMap::new();
        for section in ["dependencies", "devDependencies"] {
            if let Some(obj) = parsed.get(section).and_then(|v| v.as_object()) {
                for key in obj.keys() {
                    all_deps.insert(key.to_ascii_lowercase(), ());
                }
            }
        }
        for (known, label) in [
            ("express", "Express"),
            ("react", "React"),
            ("vue", "Vue"),
            ("svelte", "Svelte"),
            ("jest", "Jest"),
            ("vitest", "Vitest"),
            ("mocha", "Mocha"),
            ("eslint", "ESLint"),
        ] {
            if all_deps.contains_key(known) {
                techs.insert(label.to_string());
            }
        }
    }

    techs.into_iter().collect()
}

fn redact_inline_secrets(source: &str) -> String {
    if source.contains("PRIVATE KEY-----") {
        return "[REDACTED_PRIVATE_KEY_MATERIAL]".to_string();
    }

    source
        .lines()
        .map(redact_line_secret)
        .collect::<Vec<_>>()
        .join("\n")
}

fn redact_line_secret(line: &str) -> String {
    let lower = line.to_ascii_lowercase();
    let has_secret_identifier = [
        "api_key",
        "apikey",
        "secret_key",
        "private_key",
        "access_token",
        "auth_token",
        "db_password",
        "database_url",
    ]
    .iter()
    .any(|kw| lower.contains(kw));

    if has_secret_identifier
        && line.contains('=')
        && (line.contains('"') || line.contains('\''))
        && let Some((lhs, _)) = line.split_once('=')
    {
        return format!("{lhs}= \"[REDACTED_SECRET]\"");
    }

    for prefix in ["AIza", "sk-", "ghp_", "AKIA"] {
        if let Some(pos) = line.find(prefix) {
            let suffix_len = line[pos..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                .count();
            if suffix_len >= 20 {
                let mut sanitized = String::with_capacity(line.len());
                sanitized.push_str(&line[..pos]);
                sanitized.push_str("[REDACTED_TOKEN]");
                sanitized.push_str(&line[pos + suffix_len..]);
                return sanitized;
            }
        }
    }

    line.to_string()
}