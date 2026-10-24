# debtLens

**Analizador estático de deuda técnica para proyectos de software.**

debtLens es una herramienta de línea de comandos desarrollada en Rust para inspeccionar proyectos, recopilar métricas del código fuente e identificar posibles problemas de mantenibilidad. Permite ejecutar análisis desde la terminal y generar reportes para consultar los resultados posteriormente.

## Características

- Recorrido recursivo del directorio del proyecto.
- Análisis de archivos JavaScript.
- Detección de funciones y recopilación de métricas.
- Medición de complejidad ciclomática y profundidad de anidamiento.
- Detección de errores sintácticos.
- Identificación de comentarios `TODO` y `FIXME`.
- Detección de posibles bloques de código duplicado.
- Generación de reportes JSON y HTML.
- Modo detallado para consultar información adicional.
- Exclusión de directorios que normalmente contienen dependencias o archivos generados.

## Requisitos

- Rust y Cargo instalados.
- Un proyecto que contenga archivos compatibles con el analizador.
- Acceso de lectura al directorio que se desea analizar.

## Instalación y compilación

Clona el repositorio o accede al directorio del proyecto:

```bash
cd ~/Desktop/DL
```

Compila la herramienta:

```bash
cargo build
```

Para compilar una versión optimizada:

```bash
cargo build --release
```

El ejecutable se genera en:

```text
target/debug/dl
```

La versión optimizada se encuentra en:

```text
target/release/dl
```

También es posible ejecutar la herramienta directamente mediante Cargo, sin tener que invocar el binario manualmente.

## Sintaxis general

```text
dl [OPTIONS] <PROJECT_PATH>
```

El argumento `PROJECT_PATH` es obligatorio y especifica el directorio raíz del proyecto que se analizará.

### Opciones disponibles

| Opción                     | Descripción                                                                      |
| -------------------------- | -------------------------------------------------------------------------------- |
| `-o, --output <DIRECTORY>` | Especifica el directorio de salida de los reportes.                              |
| `--html`                   | Habilita la generación del reporte HTML. Requiere `--output`.                    |
| `-v, --verbose`            | Muestra información adicional durante el análisis y en los reportes compatibles. |
| `-h, --help`               | Muestra la ayuda de la herramienta.                                              |
| `-V, --version`            | Muestra la versión instalada.                                                    |

## Modos de uso

### 1. Consultar la ayuda

Permite conocer los argumentos y opciones disponibles sin analizar un proyecto.

Con Cargo:

```bash
cargo run -- --help
```

Consultar la versión:

```bash
cargo run -- --version
```

### 2. Análisis básico por terminal

Ejecuta el análisis sin especificar un directorio de salida:

```bash
cargo run -- ../To-Do-Love-Backend
```

La herramienta recorre el proyecto, analiza los archivos compatibles y muestra los resultados en la terminal.

Este modo es útil para realizar comprobaciones rápidas sin generar archivos de reporte.

### 3. Análisis con reporte de salida

Para guardar los resultados, utiliza `--output` y especifica el directorio donde deseas almacenarlos:

```bash
cargo run -- ../To-Do-Love-Backend --output ./reports
```

El directorio de salida se utiliza para generar los reportes que admite la configuración actual de la herramienta.

Si ejecutas varios análisis, puedes utilizar directorios diferentes para separar sus resultados:

```bash
cargo run -- ../ProyectoA --output ./reports/proyecto-a
cargo run -- ../ProyectoB --output ./reports/proyecto-b
```

### 4. Generación de reporte HTML

La opción `--html` habilita la generación de un reporte HTML:

```bash
cargo run -- ../To-Do-Love-Backend --output ./reports --html
```

El reporte permite consultar los resultados mediante un navegador web.

Si la herramienta genera `reports/report.html`, puedes abrirlo en Linux con:

```bash
xdg-open ./reports/report.html
```

La ruta anterior corresponde al directorio de salida del ejemplo. Si utilizas otro directorio, ajusta la ruta del archivo.

**Importante:** `--html` requiere `--output`; no debe utilizarse de forma aislada.

### 5. Modo detallado

Utiliza `--verbose` para ampliar la información que muestra la herramienta y la que incorpora a los reportes compatibles:

```bash
cargo run -- ../To-Do-Love-Backend --verbose
```

También puedes combinarlo con la generación de reportes:

```bash
cargo run -- ../To-Do-Love-Backend \
    --output ./reports \
    --html \
    --verbose
```

Este modo es útil cuando necesitas examinar los hallazgos con mayor detalle, incluyendo fragmentos de código cuando corresponda.

### 6. Ejecución del binario compilado

Una vez compilada la herramienta, no es necesario ejecutar Cargo cada vez.

En modo de desarrollo:

```bash
./target/debug/dl ../To-Do-Love-Backend
```

Con reportes HTML:

```bash
./target/debug/dl ../To-Do-Love-Backend \
    --output ./reports \
    --html
```

Con la versión optimizada:

```bash
./target/release/dl ../To-Do-Love-Backend \
    --output ./reports \
    --html \
    --verbose
```

### 7. Analizar el directorio actual

Si deseas analizar el proyecto desde su propio directorio:

```bash
cd ../To-Do-Love-Backend
cargo run --manifest-path ../DL/Cargo.toml -- .
```

En este ejemplo, `../DL/Cargo.toml` debe apuntar al archivo `Cargo.toml` de debtLens. Ajusta la ruta si tu organización de directorios es diferente.

## Exclusión de directorios

Para evitar analizar dependencias y archivos generados que pueden añadir ruido al resultado, el recorrido omite los siguientes directorios:

- `.git`
- `node_modules`
- `dist`
- `build`
- `coverage`
- `.next`
- `.turbo`
- `.cache`
- `vendor`

La exclusión se aplica durante el recorrido del proyecto.

## Métricas recopiladas

### Funciones

El analizador registra información como:

- Nombre de la función, cuando puede identificarse.
- Línea inicial y final.
- Número de líneas.
- Cantidad de parámetros.
- Complejidad ciclomática.
- Profundidad máxima de anidamiento.

Estas métricas permiten identificar funciones que podrían requerir una revisión manual por su tamaño o complejidad.

### Errores sintácticos

Tree-sitter permite detectar errores sintácticos y registrar las líneas donde aparecen. Esta información ayuda a localizar posibles problemas de estructura en el código.

La ausencia de errores sintácticos no demuestra que el programa funcione correctamente.

### Marcadores de trabajo pendiente

Se identifican los marcadores `TODO` y `FIXME` presentes en comentarios.

Por ejemplo:

```javascript
// TODO: implementar validación adicional

/*
 * FIXME: corregir el tratamiento de errores
 */
```

Los marcadores encontrados incluyen su tipo y la línea correspondiente.

### Código duplicado

La regla `CODE001` busca secuencias repetidas de instrucciones que pueden representar oportunidades de refactorización.

La implementación utiliza el árbol sintáctico de JavaScript para identificar instrucciones y normalizar sus tokens. Aplica filtros para reducir falsos positivos causados por coincidencias triviales de sintaxis.

Un hallazgo es una señal para investigar, no una prueba automática de que exista deuda técnica.

## Interpretación de los resultados

Los hallazgos incluyen información como la regla que los generó, su descripción, la ubicación y una recomendación de revisión. Según el tipo de hallazgo, también pueden incluir una clasificación de severidad y ubicaciones adicionales.

Las severidades deben interpretarse como indicadores de prioridad, no como una medición objetiva del impacto real en producción.

Se recomienda revisar cada hallazgo considerando:

- Si el comportamiento repetido realmente es equivalente.
- Si extraer una función compartida simplificaría el mantenimiento.
- Si las diferencias entre bloques son intencionales.
- Si una función extensa está dividida en responsabilidades coherentes.
- Si las condiciones y efectos secundarios se conservarían después de una refactorización.

No es recomendable modificar código únicamente para reducir el número de hallazgos.

## Limitaciones conocidas

### Lenguajes compatibles

La configuración actual del scanner acepta estas extensiones:

- `.js`
- `.jsx`
- `.mjs`
- `.cjs`

Aunque JSX se admite mediante la gramática JavaScript, el análisis estático no sustituye la compilación ni la validación del framework correspondiente.

TypeScript, TSX, HTML y CSS no están habilitados por el scanner actual. Incorporarlos requiere registrar sus gramáticas y desarrollar analizadores compatibles con el modelo común de resultados.

### Detección de duplicados

La detección utiliza coincidencias de tokens normalizados dentro de secuencias de instrucciones. Por lo tanto:

- Puede detectar bloques repetidos con diferencias de formato o comentarios.
- No garantiza detectar código equivalente con nombres de variables diferentes.
- No demuestra que dos bloques tengan el mismo comportamiento.
- Puede omitir duplicaciones que no superen los filtros establecidos.
- Puede producir falsos positivos en coincidencias sintácticas suficientemente extensas.

La detección debe seguir mejorándose mediante pruebas de regresión con proyectos reales.

### Análisis estático

debtLens no ejecuta el programa analizado ni comprueba su comportamiento durante la ejecución. Sus métricas tampoco demuestran por sí solas que una función sea difícil de mantener o que un fragmento repetido deba refactorizarse.

## Flujo de trabajo recomendado

Para utilizar la herramienta de manera práctica:

1. Ejecuta un análisis básico para comprobar que el proyecto se recorre correctamente.
2. Genera un reporte cuando necesites conservar los resultados.
3. Activa el modo detallado para investigar hallazgos concretos.
4. Revisa las ubicaciones y compara el código fuente original.
5. Prioriza los problemas confirmados y aplica cambios de forma controlada.
6. Ejecuta las pruebas del proyecto después de cada modificación.
7. Repite el análisis para comparar los resultados antes y después.

## Conclusión

debtLens automatiza una primera revisión estática de proyectos JavaScript mediante métricas de funciones, complejidad, errores sintácticos, marcadores de trabajo pendiente y posibles duplicaciones. Su interfaz de línea de comandos permite ejecutar análisis rápidos y generar reportes para documentar los resultados.

Sin embargo, **no sustituye una revisión de código, las pruebas automatizadas ni la validación del comportamiento del programa**. No demuestra que una implementación sea correcta, no identifica necesariamente toda la deuda técnica y no puede determinar de forma infalible cuándo una duplicación debe refactorizarse. Sus resultados deben utilizarse como orientación para investigar y mejorar la mantenibilidad del software, no como decisiones automáticas.
