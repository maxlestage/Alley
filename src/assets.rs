//! Les fichiers du navigateur, compilés dans le programme (aucun dossier à
//! déployer à côté), et ce qui rend Alley **installable** sur tous les
//! systèmes : le manifeste d'application web et le service worker, construits
//! avec le module `pwa` d'Active.
//!
//! Le service worker garde l'application (page, WebAssembly, styles, icônes) pour
//! qu'elle s'ouvre même sans réseau, et laisse toujours passer l'API au réseau.

use paquets::active::pwa::{Manifest, ServiceWorker, version};
use paquets::prelude::*;
use paquets::vitesse::{Response, header};

use crate::Site;

/// `(chemin, type, contenu)` de chaque fichier servi sous `/assets/`.
pub const FILES: &[(&str, &str, &[u8])] = &[
    (
        "/assets/site.css",
        "text/css; charset=utf-8",
        include_bytes!("../web/site.css"),
    ),
    (
        "/assets/app.css",
        "text/css; charset=utf-8",
        include_bytes!("../web/app.css"),
    ),
    // Le navigateur, en Rust compilé en WebAssembly (`client/`), et la colle
    // générée par wasm-bindgen pour le charger.
    (
        WASM_JS,
        "text/javascript; charset=utf-8",
        include_bytes!(concat!(env!("OUT_DIR"), "/alley_web.js")),
    ),
    (
        WASM,
        "application/wasm",
        include_bytes!(concat!(env!("OUT_DIR"), "/alley_web_bg.wasm")),
    ),
    (
        "/assets/favicon.svg",
        "image/svg+xml",
        include_bytes!("../web/favicon.svg"),
    ),
    (
        "/assets/icon-192.png",
        "image/png",
        include_bytes!("../web/icons/icon-192.png"),
    ),
    (
        "/assets/icon-512.png",
        "image/png",
        include_bytes!("../web/icons/icon-512.png"),
    ),
    (
        "/assets/icon-maskable-512.png",
        "image/png",
        include_bytes!("../web/icons/icon-maskable-512.png"),
    ),
    (
        "/assets/apple-touch-icon.png",
        "image/png",
        include_bytes!("../web/icons/apple-touch-icon.png"),
    ),
];

/// La colle de wasm-bindgen et le module WebAssembly du navigateur.
pub const WASM_JS: &str = "/assets/alley_web.js";
pub const WASM: &str = "/assets/alley_web_bg.wasm";

/// Le navigateur a-t-il été compilé (`scripts/web.sh`) ?
pub fn has_wasm() -> bool {
    FILES
        .iter()
        .any(|(path, _, bytes)| *path == WASM && !bytes.is_empty())
}

pub const MANIFEST_URL: &str = "/manifest.webmanifest";
pub const WORKER_URL: &str = "/sw.js";
/// La page qu'ouvre l'application installée.
pub const START_URL: &str = "/app/";
pub const THEME_COLOR: &str = "#16131f";

/// Le manifeste : nom, icônes, couleurs, plein écran.
pub fn manifest(site: &Site) -> Manifest {
    Manifest::new("Alley — messagerie chiffrée")
        .short_name("Alley")
        .description("Salons éphémères chiffrés de bout en bout : rien n'est enregistré, tout s'efface à minuit.")
        .id(site.url(START_URL))
        .start_url(site.url(START_URL))
        .scope(site.url("/"))
        .display("standalone")
        .theme_color(THEME_COLOR)
        .background_color(THEME_COLOR)
        .lang("fr")
        .categories(["social", "communication", "productivity"])
        .icon(site.url("/assets/icon-192.png"), "192x192", "image/png")
        .icon(site.url("/assets/icon-512.png"), "512x512", "image/png")
        .maskable_icon(
            site.url("/assets/icon-maskable-512.png"),
            "512x512",
            "image/png",
        )
}

/// La version de l'application : change dès qu'un fichier change, pour que
/// le service worker remplace l'ancien cache.
pub fn build_version(site: &Site) -> String {
    let mut parts: Vec<&[u8]> = FILES.iter().map(|(_, _, bytes)| *bytes).collect();
    parts.push(env!("CARGO_PKG_VERSION").as_bytes());
    parts.push(site.base.as_bytes());
    parts.push(site.api.as_bytes());
    version(parts)
}

/// Le script du service worker : l'app s'ouvre hors ligne, l'API passe
/// toujours par le réseau.
pub fn worker(site: &Site) -> String {
    let precache = FILES
        .iter()
        .map(|(path, _, _)| site.url(path))
        .chain([site.url("/")]);
    ServiceWorker::new("alley", build_version(site))
        .precache(precache)
        .offline_page(site.url(START_URL))
        .network_only(site.url("/api/"))
        .to_js()
}

pub fn routes(app: &mut App, site: &Site) {
    for (path, kind, bytes) in FILES {
        app.get(path, move |_| async move {
            Response::new()
                .header(header::CONTENT_TYPE, *kind)
                .header(header::CACHE_CONTROL, "no-cache")
                .send(*bytes)
        });
    }
    let manifest = manifest(site).to_json();
    app.get(MANIFEST_URL, move |_| {
        let manifest = manifest.clone();
        async move {
            Response::new()
                .header(header::CONTENT_TYPE, "application/manifest+json")
                .header(header::CACHE_CONTROL, "no-cache")
                .send(manifest)
        }
    });
    let worker = worker(site);
    app.get(WORKER_URL, move |_| {
        let worker = worker.clone();
        async move {
            Response::new()
                .header(header::CONTENT_TYPE, "text/javascript; charset=utf-8")
                .header(header::CACHE_CONTROL, "no-cache")
                .send(worker)
        }
    });
    // L'icône que cherchent les navigateurs de bureau à la racine.
    app.get("/favicon.ico", |_| async {
        Redirect::permanent("/assets/favicon.svg")
    });
    app.get("/apple-touch-icon.png", |_| async {
        Redirect::permanent("/assets/apple-touch-icon.png")
    });
}
