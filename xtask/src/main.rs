//! Les tâches d'Alley, écrites en Rust.
//!
//! ```sh
//! rustup target add wasm32-unknown-unknown
//! cargo xtask web       # client/ → client/pkg/ (WebAssembly)
//! cargo xtask models    # les modèles de modération → models/
//! cargo run -p xtask --features prompts -- prompts <vocabulaire> <textual.onnx>
//!                       # recalcule src/moderation/prompts.bin (rarement)
//! cargo xtask merge | heroku | pages-enabled | api-url   # GitHub Actions
//! ```
//!
//! `web` compile la crate `alley-web` pour `wasm32-unknown-unknown`, puis
//! génère avec la bibliothèque de wasm-bindgen la colle qui permet au module
//! de parler au navigateur. Cette colle démarre le module d'elle-même : la
//! page n'a qu'à la charger, sans une ligne de JavaScript écrite à la main.

mod ci;
mod models;
#[cfg(feature = "prompts")]
mod prompts;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const USAGE: &str =
    "usage : cargo xtask web | models | prompts <vocabulaire.txt.gz> <textual.onnx>";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("web") => web(),
        Some("models") => models::download(&root().join("models")),
        #[cfg(feature = "prompts")]
        Some("prompts") if args.len() == 3 => prompts::compute(
            Path::new(&args[1]),
            Path::new(&args[2]),
            &root().join("src/moderation/prompts.bin"),
        ),
        #[cfg(not(feature = "prompts"))]
        Some("prompts") => Err(
            "cargo run -p xtask --features prompts -- prompts <vocabulaire> <textual.onnx>".into(),
        ),
        Some("merge") => ci::merge(),
        Some("heroku") => ci::deploy_heroku(),
        Some("pages-enabled") => ci::pages_enabled(),
        Some("api-url") => ci::api_url(),
        _ => Err(USAGE.to_owned()),
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
