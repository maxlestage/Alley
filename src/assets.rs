//! Les fichiers du navigateur, compilés dans le programme (aucun dossier à
//! déployer à côté), et ce qui rend Alley **installable** sur tous les
//! systèmes : le manifeste d'application web, construit avec le module `pwa`
//! d'Active.
//!
//! Pas de service worker : un salon n'existe que tant que quelqu'un y est
//! connecté, il n'y a rien à ouvrir hors ligne. Les adresses des fichiers
//! portent la version de l'application (`?v=…`) : un navigateur ne garde
//! jamais un ancien fichier après une mise à jour.

use std::borrow::Cow;
use std::sync::LazyLock;

use paquets::active::pwa::{Manifest, version};
use paquets::prelude::*;
use paquets::vitesse::{Response, header};

use crate::i18n::{LANGS, Lang};
use crate::{Site, style};

/// `(chemin, type, contenu)` d'un fichier servi sous `/assets/`.
pub type File = (&'static str, &'static str, Cow<'static, [u8]>);

/// Les styles, écrits en Rust (`src/style/`) et rendus une fois.
static FILES: LazyLock<Vec<File>> = LazyLock::new(|| {
    let css = "text/css; charset=utf-8";
    let mut files: Vec<File> = vec![
        (
            "/assets/site.css",
            css,
            Cow::Owned(style::site().into_bytes()),
        ),
        (
            "/assets/app.css",
            css,
            Cow::Owned(style::app().into_bytes()),
        ),
    ];
    files.extend(
        STATIC
            .iter()
            .map(|(path, kind, bytes)| (*path, *kind, Cow::Borrowed(*bytes))),
    );
    files
});

/// Chaque fichier servi sous `/assets/`.
pub fn files() -> &'static [File] {
    &FILES
}

const STATIC: &[(&str, &str, &[u8])] = &[
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

/// Le navigateur a-t-il été compilé (`cargo xtask web`) ?
pub fn has_wasm() -> bool {
    files()
        .iter()
        .any(|(path, _, bytes)| *path == WASM && !bytes.is_empty())
}

/// La version de l'application : change dès qu'un fichier change.
pub fn build_version() -> &'static str {
    static VERSION: LazyLock<String> = LazyLock::new(|| {
        let mut parts: Vec<&[u8]> = files().iter().map(|(_, _, bytes)| &bytes[..]).collect();
        parts.push(env!("CARGO_PKG_VERSION").as_bytes());
        version(parts)
    });
    &VERSION
}

/// L'adresse d'un fichier de `/assets/`, avec la version de l'application.
pub fn versioned(site: &Site, path: &str) -> String {
    format!("{}?v={}", site.url(path), build_version())
}

/// Le manifeste d'une langue : `/manifest.webmanifest`,
/// `/en/manifest.webmanifest`…
pub fn manifest_url(lang: Lang) -> String {
    lang.path("/manifest.webmanifest")
}
/// La page qu'ouvre l'application installée.
pub const START_URL: &str = "/app/";
pub const THEME_COLOR: &str = "#16131f";

/// Le manifeste d'une langue : nom, icônes, couleurs, plein écran.
pub fn manifest(site: &Site, lang: Lang) -> Manifest {
    let t = lang.texts();
    Manifest::new(t.manifest_name)
        .short_name("Alley")
        .description(t.manifest_description)
        .id(site.url(&lang.path(START_URL)))
        .start_url(site.url(&lang.path(START_URL)))
        .scope(site.url("/"))
        .display("standalone")
        .theme_color(THEME_COLOR)
        .background_color(THEME_COLOR)
        .lang(lang.code())
        .categories(["social", "communication", "productivity"])
        .icon(site.url("/assets/icon-192.png"), "192x192", "image/png")
        .icon(site.url("/assets/icon-512.png"), "512x512", "image/png")
        .maskable_icon(
            site.url("/assets/icon-maskable-512.png"),
            "512x512",
            "image/png",
        )
}

pub fn routes(app: &mut App, site: &Site) {
    for (path, kind, bytes) in files() {
        let bytes: &'static [u8] = bytes;
        app.get(path, move |_| async move {
            Response::new()
                .header(header::CONTENT_TYPE, *kind)
                .header(header::CACHE_CONTROL, "no-cache")
                .send(bytes)
        });
    }
    for lang in LANGS {
        let manifest = manifest(site, lang).to_json();
        app.get(&manifest_url(lang), move |_| {
            let manifest = manifest.clone();
            async move {
                Response::new()
                    .header(header::CONTENT_TYPE, "application/manifest+json")
                    .header(header::CACHE_CONTROL, "no-cache")
                    .send(manifest)
            }
        });
    }
    // L'icône que cherchent les navigateurs de bureau à la racine.
    app.get("/favicon.ico", |_| async {
        Redirect::permanent("/assets/favicon.svg")
    });
    app.get("/apple-touch-icon.png", |_| async {
        Redirect::permanent("/assets/apple-touch-icon.png")
    });
}
