//! `cargo xtask models` : télécharge les modèles qui servent au serveur à
//! refuser les images à caractère sexuel ou montrant de la drogue, et vérifie
//! leur empreinte SHA-256 (un fichier modifié est refusé).
//!
//! - `nsfw.onnx` : le classifieur d'images de GantMan (MobileNet v2, licence
//!   MIT), converti en ONNX par le projet `nsfw` ;
//! - `clip-visual.onnx` : l'encodeur d'images de CLIP RN50 (OpenAI, licence
//!   MIT), exporté en ONNX par clip-as-service. Il compare une image aux
//!   descriptions de `src/moderation/prompts.rs` (drogues, nudité, images
//!   ordinaires).

use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

pub struct Model {
    pub file: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
}

pub const MODELS: &[Model] = &[
    Model {
        file: "nsfw.onnx",
        url: "https://github.com/Fyko/nsfw/releases/download/v0.2.0/model.onnx",
        sha256: "0dca163d933af88e03fa0f9d233655857e9cc9f38bfd2bc28ba57c486d70feaf",
    },
    Model {
        file: "clip-visual.onnx",
        url: "https://clip-as-service.s3.us-east-2.amazonaws.com/models-436c69702d61732d53657276696365/onnx/RN50/visual.onnx",
        sha256: "5c1b314992fab188d56da110d1cc5342b330cd23f2ed91610c9783d4e6e1b5fc",
    },
];

fn sha256_file(path: &Path) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(hex(&hasher.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn download(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    for model in MODELS {
        let path = dir.join(model.file);
        if sha256_file(&path).as_deref() == Some(model.sha256) {
            println!("models/{} : à jour", model.file);
            continue;
        }
        println!("models/{} : téléchargement…", model.file);
        let mut body = ureq::get(model.url)
            .call()
            .map_err(|e| format!("{} : {e}", model.url))?
            .into_body();
        let bytes = body
            .with_config()
            .limit(1 << 30)
            .read_to_vec()
            .map_err(|e| format!("{} : {e}", model.url))?;
        let got = hex(&Sha256::digest(&bytes));
        if got != model.sha256 {
            return Err(format!(
                "{} : empreinte inattendue ({got}), fichier refusé",
                model.file
            ));
        }
        let partial = path.with_extension("part");
        std::fs::write(&partial, &bytes).map_err(|e| e.to_string())?;
        std::fs::rename(&partial, &path).map_err(|e| e.to_string())?;
        println!("models/{} : {} Mo", model.file, bytes.len() >> 20);
    }
    Ok(())
}
