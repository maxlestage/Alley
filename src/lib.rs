//! **Alley** : l'équivalent de Discord où tout est chiffré de bout en bout.
//!
//! - les messages, photos, vocaux, noms de serveurs et de salons, pseudos
//!   — et même l'auteur, le salon et l'heure de chaque message — sont
//!   chiffrés dans le navigateur avec le protocole de la messagerie
//!   **Brocolis 🥦** (AES-256-GCM, jetons `[[BROCOLIS:v1:…]]`, appairage
//!   X25519 pour redistribuer une clé renouvelée) ;
//! - les appels audio et vidéo passent de pair à pair (WebRTC, DTLS-SRTP),
//!   avec une signalisation chiffrée : le serveur ne peut pas s'intercaler ;
//! - le serveur est une [`App`] **Paquets 📦** : Vitesse pour le web et la
//!   WebSocket, Securite pour le bouclier, Memoire pour les données, Active
//!   pour les pages et la PWA ;
//! - le client est une application web installable (PWA) : iPhone, iPad,
//!   Android, Windows, macOS, Linux, ChromeOS. Elle peut aussi être publiée
//!   seule sur GitHub Pages ([`export`]) et parler à l'API de ce serveur.

pub mod api;
pub mod assets;
pub mod export;
pub mod live;
pub mod pages;
pub mod protocol;

use paquets::App;
use paquets::securite::prelude::*;

use crate::live::Hub;

/// Corps de requête maximal : un message peut contenir une image ou un vocal
/// chiffrés.
pub const BODY_LIMIT: usize = 8 * 1024 * 1024;

/// Où vivent les pages et l'API.
#[derive(Clone, Debug, Default)]
pub struct Site {
    /// Le chemin sous lequel le site est publié : `""` sur le serveur,
    /// `"/Alley"` sur GitHub Pages.
    pub base: String,
    /// L'adresse de l'API : `""` quand c'est le même site, sinon son origine
    /// (`https://alley.herokuapp.com`).
    pub api: String,
}

impl Site {
    /// Le site servi par le serveur lui-même.
    pub fn server() -> Site {
        Site::default()
    }

    /// Une adresse du site : `/assets/app.js` → `/Alley/assets/app.js`.
    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }
}

/// Les origines autorisées à appeler l'API depuis un autre site : la version
/// GitHub Pages de l'app. `ALLEY_ALLOWED_ORIGINS` (séparées par des virgules)
/// remplace la valeur par défaut.
pub fn allowed_origins() -> Vec<String> {
    std::env::var("ALLEY_ALLOWED_ORIGINS")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "https://maxlestage.github.io".to_owned())
        .split(',')
        .map(|o| o.trim().trim_end_matches('/').to_owned())
        .filter(|o| o.starts_with("https://") || o.starts_with("http://localhost"))
        .collect()
}

/// Ce que peut utiliser une page : le micro, la caméra et le partage d'écran
/// pour les appels, rien d'autre.
pub const PERMISSIONS_POLICY: &str = "accelerometer=(), camera=(self), display-capture=(self), \
geolocation=(), gyroscope=(), magnetometer=(), microphone=(self), midi=(), payment=(), \
usb=(), xr-spatial-tracking=()";

/// Le bouclier de Paquets, réglé pour une messagerie :
///
/// - HTTP redirigé vers HTTPS derrière un proxy (Heroku) : le chiffrement du
///   navigateur (WebCrypto) n'existe qu'en HTTPS ;
/// - CORS et CSRF ouverts aux seules origines de [`allowed_origins`], avec les
///   en-têtes `X-Alley-*` ;
/// - 8 Mo pour les messages (médias chiffrés), 256 Ko partout ailleurs ;
/// - 30 requêtes par seconde et par adresse, en rafales de 150 ;
/// - micro, caméra et partage d'écran autorisés pour les appels ;
/// - le reste comme le préréglage recommandé (WAF, en-têtes, robots…).
pub fn shield() -> Shield {
    let mut cors = Cors::new()
        .allow_headers([
            "content-type",
            "x-alley-access",
            "x-alley-member",
            "x-alley-admin",
            "x-alley-proof",
        ])
        .max_age(std::time::Duration::from_secs(3600));
    let mut csrf = Csrf::new();
    for origin in allowed_origins() {
        cors = cors.allow_origin(&origin);
        csrf = csrf.trusted_origin(&origin);
    }
    paquets::shield()
        .guard(HttpsRedirect::new())
        .guard(cors)
        .guard(csrf)
        .guard(BodyLimit::new(256 * 1024).route("/api/v1/messages", BODY_LIMIT as u64))
        .guard(RateLimit::new(Quota::per_second(30).burst(150)))
        .guard(SecurityHeaders::new().permissions_policy(PERMISSIONS_POLICY))
        .build()
}

/// L'application complète : bouclier, API, pages, fichiers et PWA. Les tests
/// construisent exactement la même avec `app().test()`.
pub fn app() -> App {
    let mut app = App::new();
    app.security(shield());
    app.body_limit(BODY_LIMIT);
    app.state(Hub::default());
    app.model::<api::Guild>();
    app.model::<api::Member>();
    app.model::<api::Ban>();
    app.model::<api::Share>();
    app.model::<api::Message>();
    api::routes(&mut app);
    assets::routes(&mut app, &Site::server());
    pages::routes(&mut app, &Site::server());
    app
}
