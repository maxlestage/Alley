//! Les tâches de compilation d'Alley, écrites en Rust.
//!
//! ```sh
//! rustup target add wasm32-unknown-unknown
//! cargo xtask web      # client/ → client/pkg/ (WebAssembly)
//! ```
//!
//! `web` compile la crate `alley-web` pour `wasm32-unknown-unknown`, puis
//! génère avec la bibliothèque de wasm-bindgen la colle qui permet au module
//! de parler au navigateur. Cette colle démarre le module d'elle-même : la
//! page n'a qu'à la charger, sans une ligne de JavaScript écrite à la main.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let task = std::env::args().nth(1).unwrap_or_default();
    let result = match task.as_str() {
        "web" => web(),
        _ => Err("usage : cargo xtask web".to_owned()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask : {e}");
            ExitCode::FAILURE
        }
    }
}

/// Ajouté à la fin de la colle générée.
const BOOT: &str = "
// Démarrage du module, ajouté par `cargo xtask web`.
__wbg_init({ module_or_path: new URL(\"alley_web_bg.wasm\" + new URL(import.meta.url).search, import.meta.url) });
";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("racine du dépôt")
        .to_path_buf()
}

fn web() -> Result<(), String> {
    let root = root();
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = Command::new(cargo)
        .current_dir(&root)
        .args([
            "build",
            "--package",
            "alley-web",
            "--lib",
            "--target",
            "wasm32-unknown-unknown",
            "--profile",
            "wasm-release",
        ])
        .status()
        .map_err(|e| format!("cargo : {e}"))?;
    if !status.success() {
        return Err("la compilation du navigateur a échoué".into());
    }
    let wasm = root.join("target/wasm32-unknown-unknown/wasm-release/alley_web.wasm");
    let out = root.join("client/pkg");
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let mut bindgen = wasm_bindgen_cli_support::Bindgen::new();
    bindgen
        .input_path(&wasm)
        .web(true)
        .map_err(|e| e.to_string())?
        .typescript(false);
    bindgen.generate(&out).map_err(|e| e.to_string())?;

    // La colle démarre le module elle-même : la page n'a besoin d'aucun
    // script en ligne. Le binaire est le voisin de la colle, à la même
    // version (`?v=…`, voir `src/assets.rs`).
    let glue = out.join("alley_web.js");
    let mut js = std::fs::read_to_string(&glue).map_err(|e| e.to_string())?;
    js.push_str(BOOT);
    std::fs::write(&glue, js).map_err(|e| e.to_string())?;
    for name in ["alley_web.js", "alley_web_bg.wasm"] {
        let size = std::fs::metadata(out.join(name))
            .map_err(|e| e.to_string())?
            .len();
        println!("client/pkg/{name} : {} Ko", size / 1024);
    }
    Ok(())
}
