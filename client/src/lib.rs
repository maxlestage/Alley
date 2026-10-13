//! Le navigateur d'Alley, en Rust compilé en WebAssembly : pas une ligne de
//! JavaScript écrite à la main.
//!
//! - [`crypto`] : le chiffrement de la messagerie Brocolis (AES-256-GCM,
//!   X25519, HKDF-SHA256) — compilé aussi hors du navigateur, pour les tests
//!   du serveur ;
//! - le reste n'existe que dans le navigateur : l'application (`app`, `ui`),
//!   les appels WebRTC (`call`), la vitrine (`site`).

pub mod crypto;

#[cfg(target_arch = "wasm32")]
mod app;
#[cfg(target_arch = "wasm32")]
mod call;
#[cfg(target_arch = "wasm32")]
mod dom;
#[cfg(target_arch = "wasm32")]
mod model;
#[cfg(target_arch = "wasm32")]
mod net;
#[cfg(target_arch = "wasm32")]
mod site;
#[cfg(target_arch = "wasm32")]
mod ui;

/// Le point d'entrée : la page dit si elle est la vitrine ou l'application
/// (`<meta name="alley-page">`).
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    match dom::meta("alley-page").as_str() {
        "app" => dom::spawn(app::start()),
        _ => site::start(),
    }
}
