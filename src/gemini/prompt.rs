use crate::gemini::context::ProjectContext;
use serde::Serialize;
use std::fmt::Write;

pub const PROMPT_VERSION: &str = "1.0.0";

#[derive(Serialize)]
struct PromptStaticFinding<'a> {
    finding_id: &'a str,
    rule_id: &'a str,
    title: &'a str,
    description: &'a str,
    file: &'a str,
    line: usize,
    severity: &'a crate::model::Severity,
    recommendation: &'a str,
}

#[derive(Serialize)]
struct PromptFileMetric<'a> {
    path: &'a str,
    language: &'a str,
    lines: usize,
    syntax_errors: &'a [usize],
    functions_count: usize,
    max_complexity: usize,
    max_nesting_depth: usize,
    todo_markers_count: usize,
    dependencies: &'a [String],
}

pub fn build_analysis_prompt(context: &ProjectContext) -> String {
    let mut prompt = String::with_capacity(context.estimated_tokens * 3 + 4096);

    prompt.push_str(
        r#"Eres un analista experto en calidad de software, arquitectura, mantenibilidad y deuda técnica.
Tu misión es realizar un análisis semántico y arquitectónico integral del proyecto proporcionado, evaluando tanto los problemas locales dentro de cada archivo como los problemas derivados de las relaciones entre módulos y componentes.

REGLAS ESTRICTAS DE RESPUESTA:
1. Devuelve EXCLUSIVAMENTE un objeto JSON válido, sin bloques Markdown (```json), sin encabezados y sin texto fuera del JSON.
2. Conserva exactamente los `finding_id` originales al evaluar los hallazgos estáticos.
3. No inventes problemas ni reportes defectos hipotéticos como confirmados. Cada nuevo hallazgo debe estar respaldado por evidencia real y verificable en el código fuente proporcionado.
4. No actúes como agente de refactorización automática ni propongas reescrituras innecesarias; formula recomendaciones concretas adaptadas a la arquitectura existente.
5. Las prioridades permitidas son únicamente: "critical", "high", "medium", "low".
6. Las categorías permitidas para nuevos hallazgos son: "architecture", "coupling", "cohesion", "design_antipattern", "duplication", "maintainability", "scalability", "complexity", "error_handling", "security", "other".
7. El campo `confidence` debe ser un número decimal entre 0.0 y 1.0.
8. IDIOMA OBLIGATORIO: Todos los textos descriptivos, títulos, riesgos, impactos, evidencias y recomendaciones de la respuesta JSON deben redactarse exclusivamente en español, incluso si el código fuente o los comentarios del proyecto están en inglés.

TAREAS DE ANÁLISIS:
- Tarea 1 (Evaluación de hallazgos estáticos en `existing_findings`):
  Para cada hallazgo estático listado en el contexto:
  a) Explica el problema técnico subyacente.
  b) Identifica sus consecuencias reales y su impacto sobre la mantenibilidad.
  c) Determina por qué merece atención (o si es un posible falso positivo).
  d) Propón una solución concreta y aplicable.
  e) No dupliques hallazgos estáticos dentro de `new_findings`.

- Tarea 2 (Descubrimiento de nuevos hallazgos en `new_findings`):
  Examina el código fuente completo y las relaciones entre módulos para descubrir problemas que las reglas deterministas no detectan, tales como:
  acoplamiento excesivo, baja cohesión, responsabilidades mal distribuidas, antipatrones de diseño, problemas arquitectónicos, abstracciones innecesarias, duplicación lógica, código difícil de extender o modificar, dependencias internas problemáticas, complejidad semántica, violaciones de separación de responsabilidades, inconsistencias de diseño o riesgos de escalabilidad por diseño.
  Si no existe evidencia suficiente en el código proporcionado, no generes el hallazgo.

- Tarea 3 (Recomendaciones):
  Asegura que cada recomendación explique qué componente o archivo modificar y por qué, evitando consejos genéricos.

- Tarea 4 (Resumen ejecutivo en `project_summary` y `executive_summary`):
  Resume el estado general de mantenibilidad del sistema, los riesgos principales, los problemas clasificados por prioridad (`critical_issues`, `high_priority_issues`, `medium_priority_issues`, `low_priority_issues`) y las acciones recomendadas (`recommended_actions`).

ESQUEMA JSON OBLIGATORIO:
{
  "project_summary": {
    "description": "Descripción técnica breve del sistema",
    "overall_assessment": "Evaluación general de mantenibilidad y arquitectura",
    "main_risks": ["Riesgo principal 1"],
    "recommendations": ["Recomendación estratégica 1"]
  },
  "existing_findings": [
    {
      "finding_id": "F001",
      "explanation": "Explicación detallada del problema",
      "impact": "Impacto concreto en la mantenibilidad",
      "recommendation": "Solución concreta propuesta",
      "priority": "high",
      "confidence": 0.95,
      "possible_false_positive": false
    }
  ],
  "new_findings": [
    {
      "title": "Título conciso del problema",
      "category": "architecture",
      "description": "Descripción técnica del problema detectado",
      "file": "ruta/relativa/archivo.js",
      "start_line": 10,
      "end_line": 35,
      "evidence": "Fragmento o referencia verificable en el archivo",
      "impact": "Consecuencias sobre el mantenimiento y evolución",
      "recommendation": "Solución concreta propuesta",
      "priority": "medium",
      "confidence": 0.90
    }
  ],
  "executive_summary": {
    "critical_issues": [],
    "high_priority_issues": [],
    "medium_priority_issues": [],
    "low_priority_issues": [],
    "recommended_actions": []
  }
}

=== CONTEXTO INTEGRAL DEL PROYECTO ===
"#,
    );

    let _ = writeln!(prompt, "Proyecto: {}", context.project_name);
    let _ = writeln!(prompt, "Lenguajes detectados: {:?}", context.languages);
    let _ = writeln!(
        prompt,
        "Tecnologías identificadas: {:?}",
        context.technologies
    );
    let _ = writeln!(
        prompt,
        "Estructura de directorios: {:?}",
        context.directory_tree
    );

    let file_metrics: Vec<PromptFileMetric<'_>> = context
        .files
        .iter()
        .map(|f| PromptFileMetric {
            path: &f.path,
            language: &f.language,
            lines: f.lines,
            syntax_errors: &f.metrics.syntax_errors,
            functions_count: f.metrics.functions.len(),
            max_complexity: f
                .metrics
                .functions
                .iter()
                .filter_map(|func| func.complexity)
                .max()
                .unwrap_or(0),
            max_nesting_depth: f
                .metrics
                .functions
                .iter()
                .map(|func| func.nesting_depth)
                .max()
                .unwrap_or(0),
            todo_markers_count: f.metrics.todo_markers.len(),
            dependencies: &f.dependencies,
        })
        .collect();

    prompt.push_str("\n--- MÉTRICAS Y RELACIONES DE ARCHIVOS (TREE-SITTER) ---\n");
    if let Ok(metrics_json) = serde_json::to_string_pretty(&file_metrics) {
        prompt.push_str(&metrics_json);
        prompt.push('\n');
    }

    let prompt_findings: Vec<PromptStaticFinding<'_>> = context
        .static_findings
        .iter()
        .enumerate()
        .map(|(idx, f)| PromptStaticFinding {
            finding_id: f
                .id
                .as_deref()
                .unwrap_or(if idx == 0 { "F001" } else { "UNKNOWN" }),
            rule_id: &f.rule_id,
            title: &f.title,
            description: &f.description,
            file: &f.file,
            line: f.line,
            severity: &f.severity,
            recommendation: &f.recommendation,
        })
        .collect();

    prompt.push_str("\n--- HALLAZGOS ESTÁTICOS DETECTADOS ---\n");
    if let Ok(findings_json) = serde_json::to_string_pretty(&prompt_findings) {
        prompt.push_str(&findings_json);
        prompt.push('\n');
    }

    prompt.push_str("\n--- CÓDIGO FUENTE DEL PROYECTO (RUTAS RELATIVAS) ---\n");
    for file in &context.files {
        let _ = writeln!(prompt, "\n>>> FILE: {} ({} líneas)", file.path, file.lines);
        for (line_idx, line_content) in file.content.lines().enumerate() {
            let _ = writeln!(prompt, "{:4} | {}", line_idx + 1, line_content);
        }
    }

    prompt
}