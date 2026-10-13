//! **Alley** : l'équivalent de Discord où tout est chiffré de bout en bout.
//!
//! - les messages, les noms de serveurs et de salons, les pseudos et les
//!   images sont chiffrés dans le navigateur avec le protocole de la
//!   messagerie **Brocolis 🥦** (AES-256-GCM, jetons `[[BROCOLIS:v1:…]]`) ;
//! - le serveur est une [`App`] **Paquets 📦** : Vitesse pour le web et la
//!   WebSocket, Securite pour le bouclier, Memoire pour les données, Active
//!   pour les pages et la PWA ;
//! - le client est une application web installable (PWA) : iPhone, iPad,
//!   Android, Windows, macOS, Linux, ChromeOS, depuis n'importe quel
//!   navigateur récent.
//!
//! Le serveur ne voit que du chiffré, et refuse tout le reste (voir
//! [`protocol`]).

pub mod api;
pub mod assets;
pub mod live;
pub mod pages;
pub mod protocol;

use paquets::App;
use paquets::securite::prelude::*;

use crate::live::Hub;

/// Corps de requête maximal : un message peut contenir une image chiffrée.
pub const BODY_LIMIT: usize = 8 * 1024 * 1024;

/// Le bouclier de Paquets, réglé pour une messagerie :
///
/// - 8 Mo pour l'envoi et la correction des messages (images chiffrées),
///   256 Ko partout ailleurs ;
/// - 30 requêtes par seconde et par adresse, en rafales de 150 : une
///   discussion animée passe, une inondation non ;
/// - le reste comme le préréglage recommandé (WAF, CSRF, en-têtes, robots…).
pub fn shield() -> Shield {
    paquets::shield()
        .guard(BodyLimit::new(256 * 1024).route("/api/v1/messages", BODY_LIMIT as u64))
        .guard(RateLimit::new(Quota::per_second(30).burst(150)))
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
    app.model::<api::Channel>();
    app.model::<api::Member>();
    app.model::<api::Message>();
    api::routes(&mut app);
    assets::routes(&mut app);
    pages::routes(&mut app);
    app
}
