//! **Alley** : des salons de discussion éphémères, chiffrés de bout en bout.
//!
//! - **un pseudonyme suffit** ; avec un mot de passe (retenu par le
//!   navigateur, jamais envoyé), on retrouve ses salons sur tous ses
//!   appareils. Aucun message n'est enregistré, ni sur l'appareil ni sur le
//!   serveur ; l'appareil retient seulement les salons créés ou visités,
//!   jusqu'à leur minuit, pour y revenir ;
//! - le contenu sexuel et la drogue sont interdits ; une image interdite
//!   bloque l'adresse IP de son auteur ([`moderation`]) ;
//! - en français, en anglais et en espagnol ([`i18n`]) ;
//! - **un salon reste ouvert jusqu'à minuit**, même vide, dans le pays de
//!   celui qui l'a créé ([`rooms`]) ; qui arrive ne voit rien de ce qui s'est
//!   dit avant ;
//! - les messages, photos, vocaux et pseudonymes sont chiffrés dans le
//!   navigateur avec le protocole de la messagerie **Brocolis 🥦** (AES-256-GCM,
//!   jetons `[[BROCOLIS:v1:…]]`, clé du salon remise par X25519), et **signés**
//!   par leur auteur (Ed25519, numérotés) : rien ne peut être modifié, rejoué
//!   ni attribué à un autre ;
//! - les appels audio et vidéo passent de pair à pair (WebRTC, DTLS-SRTP),
//!   avec une signalisation chiffrée ;
//! - le serveur est une [`App`] **Paquets 📦** : Vitesse pour le web et la
//!   WebSocket, Securite pour le bouclier, Active pour les pages et la PWA ;
//! - le navigateur est écrit en Rust et compilé en WebAssembly (`client/`) ;
//!   il s'installe comme une app (iPhone, iPad, Android, Windows, macOS, Linux,
//!   ChromeOS) et peut être publié seul sur GitHub Pages ([`export`]).

pub mod api;
pub mod assets;
pub mod export;
pub mod i18n;
pub mod moderation;
pub mod pages;
pub mod protocol;
pub mod rooms;
pub mod style;

use paquets::App;
use paquets::securite::prelude::*;

use crate::moderation::Moderation;
use crate::rooms::Rooms;

/// Corps de requête maximal : la création d'un salon. Les messages passent
/// par la WebSocket et ne sont jamais enregistrés.
pub const BODY_LIMIT: usize = 16 * 1024;

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
/// - CORS et CSRF ouverts aux seules origines de [`allowed_origins`] ;
/// - 16 Ko de corps au plus : les messages passent par la WebSocket ;
/// - 30 requêtes par seconde et par adresse, en rafales de 150 ;
/// - micro, caméra et partage d'écran autorisés pour les appels ;
/// - le reste comme le préréglage recommandé (WAF, en-têtes, robots…).
pub fn shield() -> Shield {
    let mut cors = Cors::new()
        .allow_headers(["content-type"])
        .max_age(std::time::Duration::from_secs(3600));
    let mut csrf = Csrf::new();
    for origin in allowed_origins() {
        cors = cors.allow_origin(&origin);
        csrf = csrf.trusted_origin(&origin);
    }
    let mut shield = paquets::shield();
    if behind_proxy() {
        // L'adresse du client est la dernière de `X-Forwarded-For` : sans
        // cela, c'est le proxy qui serait bloqué à la place d'un fautif.
        shield = shield.client_ip(ClientIpSource::XForwardedFor { trusted_hops: 0 });
    }
    shield
        .guard(HttpsRedirect::new())
        .guard(cors)
        .guard(csrf)
        .guard(BodyLimit::new(16 * 1024))
        .guard(RateLimit::new(Quota::per_second(30).burst(150)))
        .guard(SecurityHeaders::new().permissions_policy(PERMISSIONS_POLICY))
        .build()
}

/// Derrière le proxy d'un hébergeur (Heroku, Render, Fly.io), ou si
/// `ALLEY_BEHIND_PROXY` est défini.
pub fn behind_proxy() -> bool {
    ["DYNO", "RENDER", "FLY_APP_NAME", "ALLEY_BEHIND_PROXY"]
        .iter()
        .any(|v| std::env::var_os(v).is_some())
}

/// Le registre des adresses bloquées : seulement ce qu'y met la modération
/// (`AutoBan` ne compte aucune autre faute).
fn blocked_addresses() -> paquets::securite::prelude::AutoBan {
    paquets::securite::prelude::AutoBan::new()
        .statuses([])
        .max_strikes(u32::MAX)
        .max_ban(std::time::Duration::from_secs(365 * 24 * 3600))
}

/// L'application complète : bouclier, API, pages, fichiers et PWA. Les tests
/// construisent exactement la même avec `app().test()`.
pub fn app() -> App {
    let mut app = App::new();
    // Derrière le routeur d'un hébergeur, l'adresse du client est la
    // dernière de `X-Forwarded-For` (comme le bouclier la lit).
    if behind_proxy() {
        app.trust_proxy(1);
    }
    let moderation = Moderation::new(blocked_addresses());
    moderation.load_in_background();
    app.security(shield());
    app.body_limit(BODY_LIMIT);
    app.state(moderation);
    // Les adresses bloquées par la modération : toutes leurs requêtes sont
    // refusées, avec une réponse que le navigateur sait lire (et traduire),
    // même depuis GitHub Pages (le bouclier y ajoute les en-têtes CORS).
    app.middleware(
        |req: paquets::vitesse::Request, next: paquets::vitesse::Next| async move {
            let banned = req
                .ip()
                .is_some_and(|ip| req.state::<Moderation>().is_banned(ip));
            if banned {
                return paquets::vitesse::Response::new()
                    .status(403)
                    .header("cache-control", "no-store")
                    .json(serde_json::json!({ "error": "banned" }));
            }
            next.run(req).await
        },
    );
    // Les salons, en mémoire seulement : aucun message n'est enregistré.
    app.state(Rooms::default());
    api::routes(&mut app);
    assets::routes(&mut app, &Site::server());
    pages::routes(&mut app, &Site::server());
    app
}
