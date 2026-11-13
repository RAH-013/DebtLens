use base64::{engine::general_purpose::STANDARD, Engine as _};
use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let assets_dir = Path::new("src/report/templates/assets");
    println!("cargo:rerun-if-changed={}", assets_dir.display());

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR no definido");
    let dest_path = Path::new(&out_dir).join("embedded_assets.rs");

    let mut entries = Vec::new();

    if let Ok(read_dir) = fs::read_dir(assets_dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("svg") {
                if let (Some(file_name), Ok(bytes)) =
                    (path.file_name().and_then(|n| n.to_str()), fs::read(&path))
                {
                    let rel_path = format!("assets/{file_name}");
                    let data_uri = format!(
                        "data:image/svg+xml;base64,{}",
                        STANDARD.encode(&bytes)
                    );
                    entries.push((rel_path, data_uri));
                }
            }
        }
    }

    // Ordenar de mayor a menor longitud para evitar reemplazos parciales
    entries.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));

    let mut code = String::from("const EMBEDDED_ASSETS: &[(&str, &str)] = &[\n");
    for (rel_path, data_uri) in entries {
        code.push_str(&format!("    (r#\"{rel_path}\"#, r#\"{data_uri}\"#),\n"));
    }
    code.push_str("];\n");

    fs::write(dest_path, code).expect("No se pudo generar embedded_assets.rs");
}