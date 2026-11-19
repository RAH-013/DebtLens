use crate::config::{AiCacheConfig, AiConfig, AiLimitsConfig, AppConfig};
use crate::gemini::{
    AnalysisCache, ApiKey, ContextBuildError, GeminiTransport, PROMPT_VERSION, SecretStore,
    SingleRequestClient, build_analysis_prompt, build_project_context,
    is_ignored_or_sensitive_path, parse_and_validate_response, run_semantic_analysis,
};
use crate::model::{
    AiExecutionState, FileAnalysis, Finding, FunctionMetric, ProjectAnalysis, Severity,
    calculate_technical_debt_index,
};
use anyhow::{Result, bail};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "debtlens_{prefix}_{}_{}",
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct MockSecretStore {
    key: Option<String>,
    fail: bool,
}

impl SecretStore for MockSecretStore {
    fn set_secret(&self, _secret: &ApiKey) -> Result<()> {
        Ok(())
    }

    fn get_secret(&self) -> Result<Option<ApiKey>> {
        if self.fail {
            bail!("error simulado en keyring");
        }
        match &self.key {
            Some(k) => Ok(Some(ApiKey::new(k)?)),
            None => Ok(None),
        }
    }

    fn delete_secret(&self) -> Result<bool> {
        Ok(self.key.is_some())
    }
}

struct MockTransport {
    calls: AtomicUsize,
    response: Mutex<Result<String, String>>,
}

impl MockTransport {
    fn ok(json: &str) -> Self {
        Self {
            calls: AtomicUsize::new(0),
            response: Mutex::new(Ok(json.to_string())),
        }
    }

    fn err(msg: &str) -> Self {
        Self {
            calls: AtomicUsize::new(0),
            response: Mutex::new(Err(msg.to_string())),
        }
    }

    fn call_count(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl GeminiTransport for MockTransport {
    fn generate_content(
        &self,
        _api_key: &ApiKey,
        _model: &str,
        _prompt: &str,
        _timeout: Duration,
    ) -> Result<String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        match self.response.lock().unwrap().as_ref() {
            Ok(body) => Ok(body.clone()),
            Err(err) => bail!("{err}"),
        }
    }
}

fn sample_project(root: &std::path::Path) -> (ProjectAnalysis, Vec<Finding>) {
    let src_dir = root.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    let app_file = src_dir.join("app.js");
    fs::write(
        &app_file,
        "import { repo } from './repo.js';\nfunction run(a, b) {\n  if (a && b) {\n    return repo(a);\n  }\n  return null;\n}\n",
    )
    .unwrap();

    let repo_file = src_dir.join("repo.js");
    fs::write(&repo_file, "export function repo(x) { return x * 2; }\n").unwrap();

    let analysis = ProjectAnalysis {
        root: root.display().to_string(),
        files_scanned: 2,
        total_lines: 8,
        total_bytes: 160,
        total_functions: 2,
        telemetry: None,
        files: vec![
            FileAnalysis {
                path: "src/app.js".to_string(),
                language: "javascript".to_string(),
                lines: 7,
                bytes: 110,
                syntax_errors: Vec::new(),
                functions: vec![FunctionMetric {
                    name: Some("run".to_string()),
                    start_line: 2,
                    end_line: 7,
                    lines: 6,
                    parameters: Some(2),
                    complexity: Some(3),
                    nesting_depth: 1,
                }],
                todo_markers: Vec::new(),
                code_blocks: Vec::new(),
            },
            FileAnalysis {
                path: "src/repo.js".to_string(),
                language: "javascript".to_string(),
                lines: 1,
                bytes: 50,
                syntax_errors: Vec::new(),
                functions: vec![FunctionMetric {
                    name: Some("repo".to_string()),
                    start_line: 1,
                    end_line: 1,
                    lines: 1,
                    parameters: Some(1),
                    complexity: Some(1),
                    nesting_depth: 0,
                }],
                todo_markers: Vec::new(),
                code_blocks: Vec::new(),
            },
        ],
    };

    let findings = vec![Finding {
        id: Some("F001".to_string()),
        rule_id: "FUNC002".to_string(),
        title: "Complejidad ciclomática elevada".to_string(),
        description: "Complejidad elevada en run".to_string(),
        file: "src/app.js".to_string(),
        line: 2,
        locations: Vec::new(),
        severity: Severity::High,
        recommendation: "Divide la función".to_string(),
        impact_factor: None,
        depth: None,
        roi_technical: None,
    }];

    (analysis, findings)
}

fn valid_gemini_json() -> &'static str {
    r#"{
      "project_summary": {
        "description": "Aplicación modular en JavaScript",
        "overall_assessment": "Mantenibilidad aceptable con oportunidades de desacoplamiento",
        "main_risks": ["Acoplamiento directo entre capa de entrada y repositorio"],
        "recommendations": ["Introducir interfaz de servicio"]
      },
      "existing_findings": [
        {
          "finding_id": "F001",
          "explanation": "La función combina validación y acceso a datos",
          "impact": "Dificulta pruebas unitarias aisladas",
          "recommendation": "Extraer validación previa",
          "priority": "high",
          "confidence": 0.92,
          "possible_false_positive": false
        }
      ],
      "new_findings": [
        {
          "title": "Acoplamiento directo con repositorio",
          "category": "coupling",
          "description": "app.js importa directamente repo.js sin abstracción intermedia",
          "file": "src/app.js",
          "start_line": 1,
          "end_line": 4,
          "evidence": "import { repo } from './repo.js';",
          "impact": "Cambios en persistencia afectan el flujo principal",
          "recommendation": "Inyectar dependencia de repositorio en run()",
          "priority": "medium",
          "confidence": 0.88
        }
      ],
      "executive_summary": {
        "critical_issues": [],
        "high_priority_issues": ["F001 en src/app.js"],
        "medium_priority_issues": ["Acoplamiento directo con repositorio"],
        "low_priority_issues": [],
        "recommended_actions": ["Separar validación y persistencia"]
      }
    }"#
}

#[test]
fn excludes_sensitive_files_and_ignored_directories() {
    assert!(is_ignored_or_sensitive_path(".env"));
    assert!(is_ignored_or_sensitive_path(".env.production"));
    assert!(is_ignored_or_sensitive_path("config/credentials.json"));
    assert!(is_ignored_or_sensitive_path("certs/private.pem"));
    assert!(is_ignored_or_sensitive_path("node_modules/pkg/index.js"));
    assert!(is_ignored_or_sensitive_path("target/debug/dl"));
    assert!(is_ignored_or_sensitive_path("vendor/lib.js"));
    assert!(is_ignored_or_sensitive_path(".git/config"));
    assert!(is_ignored_or_sensitive_path("dist/bundle.js"));
    assert!(is_ignored_or_sensitive_path("build/app.js"));
    assert!(!is_ignored_or_sensitive_path("src/app.js"));
}

#[test]
fn builds_context_preserving_relative_paths_and_respecting_limits() {
    let dir = TempDir::new("ctx");
    let (mut analysis, findings) = sample_project(&dir.path);

    analysis.files.push(FileAnalysis {
        path: ".env".to_string(),
        language: "dotenv".to_string(),
        lines: 1,
        bytes: 20,
        syntax_errors: Vec::new(),
        functions: Vec::new(),
        todo_markers: Vec::new(),
        code_blocks: Vec::new(),
    });

    let ctx = build_project_context(
        &analysis,
        &findings,
        &AiLimitsConfig::default(),
        "gemini-3.8-flash",
    )
    .expect("El contexto debe construirse correctamente");

    assert_eq!(ctx.files.len(), 2);
    assert_eq!(ctx.files[0].path, "src/app.js");
    assert_eq!(ctx.files[1].path, "src/repo.js");
    assert_eq!(ctx.files[0].dependencies, vec!["./repo.js"]);

    let strict_limits = AiLimitsConfig {
        max_files: 1,
        ..AiLimitsConfig::default()
    };
    let err = build_project_context(&analysis, &findings, &strict_limits, "gemini-3.8-flash")
        .unwrap_err();
    assert!(matches!(err, ContextBuildError::LimitExceeded(_)));
}

#[test]
fn builds_single_integral_prompt_with_metrics_findings_and_discovery_rules() {
    let dir = TempDir::new("prompt");
    let (analysis, findings) = sample_project(&dir.path);
    let ctx = build_project_context(
        &analysis,
        &findings,
        &AiLimitsConfig::default(),
        "gemini-3.8-flash",
    )
    .unwrap();

    let prompt = build_analysis_prompt(&ctx);
    assert!(prompt.contains("F001"));
    assert!(prompt.contains("FUNC002"));
    assert!(prompt.contains("src/app.js"));
    assert!(prompt.contains("new_findings"));
    assert!(prompt.contains("existing_findings"));
    assert!(prompt.contains("executive_summary"));
}

#[test]
fn validates_response_and_rejects_invalid_data() {
    let dir = TempDir::new("resp");
    let (analysis, findings) = sample_project(&dir.path);
    let ctx = build_project_context(
        &analysis,
        &findings,
        &AiLimitsConfig::default(),
        "gemini-3.8-flash",
    )
    .unwrap();

    let valid = parse_and_validate_response(valid_gemini_json(), "gemini-3.8-flash", &ctx).unwrap();
    assert_eq!(valid.existing_findings.len(), 1);
    assert_eq!(valid.new_findings.len(), 1);

    assert!(parse_and_validate_response("no es json", "gemini-3.8-flash", &ctx).is_err());

    let bad_priority =
        valid_gemini_json().replace(r#""priority": "high""#, r#""priority": "urgent""#);
    assert!(parse_and_validate_response(&bad_priority, "gemini-3.8-flash", &ctx).is_err());

    let bad_confidence =
        valid_gemini_json().replace(r#""confidence": 0.92"#, r#""confidence": 1.45"#);
    assert!(parse_and_validate_response(&bad_confidence, "gemini-3.8-flash", &ctx).is_err());

    let bad_line = valid_gemini_json().replace(r#""end_line": 4"#, r#""end_line": 999"#);
    assert!(parse_and_validate_response(&bad_line, "gemini-3.8-flash", &ctx).is_err());

    let unknown_file =
        valid_gemini_json().replace(r#""file": "src/app.js""#, r#""file": "src/ghost.js""#);
    assert!(parse_and_validate_response(&unknown_file, "gemini-3.8-flash", &ctx).is_err());
}

#[test]
fn deduplicates_findings_and_computes_deterministic_debt_index() {
    let dir = TempDir::new("dedup");
    let (analysis, findings) = sample_project(&dir.path);
    let ctx = build_project_context(
        &analysis,
        &findings,
        &AiLimitsConfig::default(),
        "gemini-3.8-flash",
    )
    .unwrap();

    let dup_json = r#"{
      "project_summary": { "description": "d", "overall_assessment": "o", "main_risks": [], "recommendations": [] },
      "existing_findings": [],
      "new_findings": [
        {
          "title": "Duplicado sobre ancla estática",
          "category": "complexity",
          "description": "Misma línea que F001",
          "file": "src/app.js",
          "start_line": 2,
          "end_line": 5,
          "evidence": "if (a && b)",
          "impact": "Impacto",
          "recommendation": "Recomendación",
          "priority": "high",
          "confidence": 0.9
        },
        {
          "title": "Problema real nuevo",
          "category": "coupling",
          "description": "Import directo",
          "file": "src/app.js",
          "start_line": 1,
          "end_line": 1,
          "evidence": "import { repo }",
          "impact": "Impacto",
          "recommendation": "Recomendación",
          "priority": "medium",
          "confidence": 0.85
        },
        {
          "title": "Problema real nuevo",
          "category": "coupling",
          "description": "Repetido por Gemini",
          "file": "src/app.js",
          "start_line": 1,
          "end_line": 1,
          "evidence": "import { repo }",
          "impact": "Impacto",
          "recommendation": "Recomendación",
          "priority": "medium",
          "confidence": 0.85
        }
      ],
      "executive_summary": {
        "critical_issues": [],
        "high_priority_issues": [],
        "medium_priority_issues": [],
        "low_priority_issues": [],
        "recommended_actions": []
      }
    }"#;

    let ai = parse_and_validate_response(dup_json, "gemini-3.8-flash", &ctx).unwrap();
    assert_eq!(ai.new_findings.len(), 1);

    let index_a = calculate_technical_debt_index(&analysis, &findings, Some(&ai));
    let index_b = calculate_technical_debt_index(&analysis, &findings, Some(&ai));
    assert_eq!(index_a.score, index_b.score);
    assert_eq!(index_a.semantic_findings_count, 1);
}

#[test]
fn cache_reuses_and_invalidates_on_changes() {
    let _guard = AnalysisCache::test_guard();
    AnalysisCache::reset();
    let dir = TempDir::new("cache");
    let (mut analysis, findings) = sample_project(&dir.path);
    let limits = AiLimitsConfig::default();

    let ctx1 = build_project_context(&analysis, &findings, &limits, "gemini-3.8-flash").unwrap();
    let ai = parse_and_validate_response(valid_gemini_json(), "gemini-3.8-flash", &ctx1).unwrap();

    let cache = AnalysisCache::new(true);
    let key1 = AnalysisCache::compute_key(&ctx1, "gemini-3.8-flash", PROMPT_VERSION, &limits);
    cache.put(&key1, &ai).unwrap();

    let hit = cache.get(&key1).unwrap().unwrap();
    assert!(hit.from_cache);

    let key_model_changed =
        AnalysisCache::compute_key(&ctx1, "gemini-3.1-pro-preview", PROMPT_VERSION, &limits);
    assert_ne!(key1, key_model_changed);
    assert!(cache.get(&key_model_changed).unwrap().is_none());

    let key_prompt_changed =
        AnalysisCache::compute_key(&ctx1, "gemini-3.8-flash", "2.0.0", &limits);
    assert_ne!(key1, key_prompt_changed);

    fs::write(
        dir.path.join("src/repo.js"),
        "export function repo(x) { return x * 3; }\n",
    )
    .unwrap();
    analysis.files[1].lines = 1;
    let ctx2 = build_project_context(&analysis, &findings, &limits, "gemini-3.8-flash").unwrap();
    let key_code_changed =
        AnalysisCache::compute_key(&ctx2, "gemini-3.8-flash", PROMPT_VERSION, &limits);
    assert_ne!(key1, key_code_changed);
}

#[test]
fn enforces_single_request_cache_hit_and_fallback_behavior() {
    let _guard = AnalysisCache::test_guard();
    AnalysisCache::reset();
    let dir = TempDir::new("analyzer");
    let (analysis, findings) = sample_project(&dir.path);

    let mut cfg = AppConfig {
        ai: AiConfig {
            enabled: false,
            provider: "gemini".to_string(),
            model: "gemini-3.8-flash".to_string(),
            limits: AiLimitsConfig::default(),
            cache: AiCacheConfig { enabled: true },
        },
    };

    let store = MockSecretStore {
        key: Some("test-secret-key".to_string()),
        fail: false,
    };
    let transport = MockTransport::ok(valid_gemini_json());

    let out_disabled = run_semantic_analysis(
        &cfg,
        &analysis,
        &findings,
        &store,
        &transport,
    );
    assert_eq!(out_disabled.status.state, AiExecutionState::Disabled);
    assert_eq!(transport.call_count(), 0);

    cfg.ai.enabled = true;
    let out_live = run_semantic_analysis(
        &cfg,
        &analysis,
        &findings,
        &store,
        &transport,
    );
    assert_eq!(out_live.status.state, AiExecutionState::Completed);
    assert_eq!(transport.call_count(), 1);

    let out_cached = run_semantic_analysis(
        &cfg,
        &analysis,
        &findings,
        &store,
        &transport,
    );
    assert_eq!(out_cached.status.state, AiExecutionState::Cached);
    assert_eq!(transport.call_count(), 1);

    let guard = SingleRequestClient::new(&transport);
    let key = ApiKey::new("test-secret-key").unwrap();
    assert!(
        guard
            .analyze_once(&key, "gemini-3.8-flash", "p1", Duration::from_secs(5))
            .is_ok()
    );
    assert!(
        guard
            .analyze_once(&key, "gemini-3.8-flash", "p2", Duration::from_secs(5))
            .is_err()
    );

    let failing_transport = MockTransport::err("timeout agotado");
    cfg.ai.cache.enabled = false;
    let out_fail = run_semantic_analysis(
        &cfg,
        &analysis,
        &findings,
        &store,
        &failing_transport,
    );
    assert_eq!(out_fail.status.state, AiExecutionState::FailedFallback);
    assert!(out_fail.analysis.is_none());
}

#[test]
fn api_key_masking_and_debug_never_reveal_key_material() {
    const SECRET: &str = "AIzaSyFakeKeyForDebtLensAudit1234567890";
    let key = ApiKey::new(SECRET).expect("una clave no vacía debe construirse sin error");

    let masked = key.masked();
    assert!(
        !masked.contains(SECRET) && !masked.contains("AIza") && !masked.contains(&SECRET[..10]),
        "masked() no debe revelar ningún fragmento de la clave (RNF03)"
    );
    assert!(
        masked.contains(&SECRET.chars().count().to_string()),
        "el indicador debe reflejar la longitud real de la clave"
    );

    let debug = format!("{key:?}");
    assert_eq!(debug, "ApiKey([REDACTED])");
    assert!(!debug.contains("AIza"), "Debug no debe filtrar material de la clave");
}