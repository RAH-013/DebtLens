# DebtLens (`dl`)

**Herramienta CLI de auditoría híbrida de deuda técnica que combina análisis estático AST con evaluación semántica impulsada por Google Gemini.**

DebtLens (`dl`) inspecciona repositorios de código fuente desde la terminal para cuantificar su mantenibilidad, calcular un índice de deuda técnica y detectar riesgos estructurales o de diseño. Integra un motor determinista basado en árboles de sintaxis abstracta (Tree-sitter) con una capa de auditoría semántica mediante Google Gemini, exportando los resultados en JSON o en un único reporte HTML portátil y autocontenido.

---

## Características principales

### Motor de análisis estático (AST)

- **Recorrido inteligente de proyectos:** Exploración recursiva con exclusión automática de dependencias, cachés y artefactos de compilación.
- **Métricas de funciones:** Cálculo de líneas de código, cantidad de parámetros, complejidad ciclomática y profundidad máxima de anidamiento.
- **Diagnóstico sintáctico:** Detección precisa de errores de sintaxis por archivo y línea mediante Tree-sitter.
- **Marcadores de deuda:** Identificación y catalogación de comentarios `TODO` y `FIXME`.
- **Detección de código duplicado (`CODE001`):** Localización de secuencias repetidas mediante normalización de tokens del árbol sintáctico, adjuntando ubicaciones relacionadas y fragmentos de contexto.
- **Índice de deuda técnica:** Puntuación normalizada (`0`–`100`), clasificación por nivel de severidad (`Baja`, `Moderada`, `Alta`, `Crítica`) y densidad de hallazgos por cada 1,000 líneas de código.

### Auditoría semántica con Google Gemini

- **Configuración integrada (`dl ai setup`):** Asistente en línea de comandos para vincular y gestionar el acceso a Google Gemini sin depender de configuraciones externas.
- **Resumen ejecutivo:** Evaluación global de mantenibilidad, descripción del proyecto, riesgos principales, problemas críticos o de alta prioridad y acciones recomendadas.
- **Triaje inteligente de hallazgos estáticos:** Reevaluación contextual de cada regla detectada con asignación de prioridad real, porcentaje de confianza, explicación de impacto, propuesta de solución y alertas de **posible falso positivo**.
- **Hallazgos semánticos adicionales:** Detección de problemas de diseño, acoplamiento o lógica que trascienden las reglas sintácticas tradicionales, con evidencia, rango de líneas y contexto de código fuente.
- **Caché en memoria y telemetría de tokens:** Reutilización de análisis previos dentro de la misma sesión desde una caché **volátil en RAM** (sin escritura en disco) para optimizar consumo y reporte detallado de tokens utilizados (entrada, salida y cuota restante de la API en tiempo real).

### Reportes portátiles y visualización

- **Salida dual:** Generación de reportes estructurados en JSON y paneles interactivos en HTML.
- **Archivo HTML único y autocontenido (`report.html`):** Todos los estilos (`layout.css` + `style.css`), los 17 iconos SVG (convertidos a _Data URIs_ Base64) y la lógica JavaScript (codificada en Base64) se incrustan y minifican en un solo archivo portátil sin carpetas ni dependencias auxiliares.
- **Dashboard interactivo:** Indicador visual tipo termómetro que cambia progresivamente de verde a rojo según el puntaje de deuda (`0`–`100`), tarjetas KPI con semántica visual por estado, filtros en tiempo real por origen (`Regla estática` / `Hallazgo semántico`), severidad o texto libre, selector de tema (`Sistema`, `Claro`, `Oscuro`) y diseño responsive desde `320px`.

---

## Requisitos

- **Rust y Cargo** instalados en el sistema (`rustc` y `cargo`).
- Permisos de lectura sobre el directorio del proyecto que se desea auditar.
- _(Opcional)_ Una clave de API de Google Gemini configurada mediante `dl ai setup` para habilitar el análisis semántico.

---

## Instalación y actualización

1. Accede al directorio raíz del código fuente de DebtLens:

```bash
cd <RUTA_DEL_REPOSITORIO_DEBTLENS>
```

2. Compila e instala el binario `dl` de forma global en tu entorno:

```bash
cargo build --release
cargo install --path .
```

3. Para actualizar el binario tras incorporar cambios en el código fuente:

```bash
cargo build --release && cargo install --path . --force
```

4. Verifica que el ejecutable esté disponible en tu `PATH`:

```bash
dl --version
```

---

## Configuración de IA (Google Gemini)

Para habilitar la capa de auditoría semántica, ejecuta el asistente de configuración integrado:

```bash
dl ai setup
```

Para consultar las opciones disponibles dentro del subcomando de inteligencia artificial:

```bash
dl ai --help
```

### Comportamiento durante el análisis

- **Con IA configurada:** `dl` ejecuta el escaneo estático y envía el contexto estructurado a Google Gemini para generar el resumen ejecutivo, reevaluar los hallazgos existentes y detectar nuevos hallazgos semánticos. Si existe un resultado previo válido en la **caché en memoria** de la sesión, lo reutiliza sin volver a consultar la API.
- **Sin IA configurada o sin conexión:** `dl` continúa operando en modo de **análisis estático** sin interrumpir la ejecución, informando el estado del motor semántico en el panel correspondiente del reporte.

---

## Sintaxis y opciones

```text
dl [OPCIONES] <RUTA_DEL_PROYECTO>
```

El argumento `<RUTA_DEL_PROYECTO>` es obligatorio e indica el directorio raíz del proyecto que será analizado.

| Opción                      | Descripción                                                                                 |
| --------------------------- | ------------------------------------------------------------------------------------------- |
| `-o, --output <DIRECTORIO>` | Define el directorio de destino donde se guardarán los reportes generados.                  |
| `--html`                    | Genera el reporte interactivo autocontenido `report.html`. Requiere especificar `--output`. |
| `-v, --verbose`             | Amplía el nivel de detalle en la salida de consola y conserva metadatos completos en JSON.  |
| `-h, --help`                | Muestra la ayuda general y la referencia de comandos y opciones disponibles.                |
| `-V, --version`             | Muestra la versión instalada de `dl`.                                                       |

---

## Guía de uso

### 1. Consulta de ayuda y versión

```bash
dl --help
dl --version
```

### 2. Auditoría rápida en terminal

Ejecuta el análisis directamente sobre la ruta del proyecto objetivo para obtener un resumen inmediato sin escribir archivos en disco:

```bash
dl <RUTA_DEL_PROYECTO>
```

### 3. Exportación de reportes en directorio de salida

Utiliza `--output` (`-o`) para almacenar los resultados del análisis en una ubicación específica:

```bash
dl <RUTA_DEL_PROYECTO> --output <DIRECTORIO_DE_SALIDA>
```

### 4. Generación de reporte HTML portátil

Combina `--output` y `--html` para producir un único archivo `report.html` listo para abrir en cualquier navegador o compartir sin archivos adicionales:

```bash
dl <RUTA_DEL_PROYECTO> --output <DIRECTORIO_DE_SALIDA> --html
```

Para abrir el reporte generado desde Linux:

```bash
xdg-open <DIRECTORIO_DE_SALIDA>/report.html
```

> **Nota:** La bandera `--html` depende de `--output` y no debe invocarse de forma aislada.

### 5. Análisis detallado (`--verbose`)

Activa `--verbose` (`-v`) para inspeccionar información extendida durante la ejecución y conservar el detalle completo por archivo en el reporte JSON:

```bash
dl <RUTA_DEL_PROYECTO> \
    --output <DIRECTORIO_DE_SALIDA> \
    --html \
    --verbose
```

### 6. Ejecución sobre el directorio de trabajo actual

Con `dl` instalado en el sistema, puedes situarte dentro del repositorio a auditar y pasar `.` como ruta objetivo:

```bash
cd <RUTA_DEL_PROYECTO>
dl . --output <DIRECTORIO_DE_SALIDA> --html
```

---

## Exclusiones predeterminadas

Durante el recorrido del árbol de directorios, `dl` omite automáticamente las siguientes carpetas para evitar ruido proveniente de dependencias o código generado:

- `.git`
- `node_modules`
- `dist`
- `build`
- `coverage`
- `.next`
- `.turbo`
- `.cache`
- `vendor`

---

## Métricas y criterios de evaluación

| Categoría                   | Descripción técnica                                                                                                |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| **Índice de deuda**         | Puntuación compuesta (`0`–`100`) ponderada por severidad, densidad de hallazgos y métricas estructurales.          |
| **Complejidad de función**  | Evalúa longitud en líneas, número de parámetros, complejidad ciclomática y profundidad de anidamiento por función. |
| **Errores sintácticos**     | Identifica fallos de gramática mediante Tree-sitter indicando el archivo y la línea exacta del problema.           |
| **Marcadores pendientes**   | Extrae comentarios `TODO` y `FIXME` para visibilizar compromisos técnicos documentados en el código.               |
| **Duplicación (`CODE001`)** | Detecta bloques de instrucciones estructuralmente equivalentes mediante normalización de tokens AST.               |

### Interpretación de resultados

Las severidades, puntuaciones y hallazgos deben utilizarse como **indicadores de priorización técnica** y no como métricas absolutas de defectos en producción. Antes de aplicar una refactorización, evalúa:

1. Si los bloques marcados como duplicados comparten realmente la misma regla de negocio o solo coinciden estructuralmente.
2. Si extraer una abstracción común reduce la complejidad sin introducir acoplamiento innecesario entre módulos.
3. Si las sugerencias generadas por Gemini IA preservan los efectos secundarios y contratos existentes del sistema.

---

## Alcance y limitaciones conocidas

- **Lenguajes soportados:** El escáner estático procesa archivos con extensión `.js`, `.jsx`, `.mjs` y `.cjs`. El soporte para TypeScript, TSX, HTML y CSS requiere incorporar sus respectivas gramáticas al analizador.
- **Análisis estático vs. ejecución:** DebtLens no compila ni ejecuta el código auditado; sus hallazgos no sustituyen las pruebas unitarias, de integración ni la revisión de código por pares.
- **Detección heurística de duplicados:** La regla `CODE001` identifica coincidencias sobre tokens normalizados, por lo que puede omitir duplicaciones con estructuras sintácticas distintas o señalar similitudes accidentales (filtrables mediante el triaje de confianza de Gemini IA).

---

## Flujo de trabajo recomendado

1. **Configuración inicial:** Ejecuta `dl ai setup` si deseas habilitar el análisis semántico con Google Gemini.
2. **Exploración rápida:** Ejecuta `dl <RUTA_DEL_PROYECTO>` para validar el alcance del escaneo y revisar las métricas generales en consola.
3. **Auditoría visual:** Genera el reporte portátil con `dl <RUTA_DEL_PROYECTO> -o <DIRECTORIO_DE_SALIDA> --html` y revisa el índice de deuda junto con el resumen ejecutivo.
4. **Priorización:** Filtra por severidad crítica o alta y revisa los hallazgos confirmados por Gemini IA, descartando posibles falsos positivos.
5. **Refactorización controlada:** Inspecciona los fragmentos de código resaltados, aplica mejoras incrementales y ejecuta la suite de pruebas del proyecto.
6. **Verificación de mejora:** Vuelve a ejecutar `dl` para contrastar la reducción en el índice de deuda y la densidad de hallazgos.
