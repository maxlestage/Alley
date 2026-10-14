//! Les salons dont se souvient cet appareil : ceux qu'on a créés ou visités,
//! avec leur serveur, pour y revenir d'un geste depuis le hall. Ils sont
//! gardés dans le navigateur jusqu'à leur minuit, puis oubliés d'eux-mêmes ;
//! « Oublier » les retire avant.
//!
//! Seuls l'identifiant, le nom et le serveur d'un salon sont retenus :
//! jamais un message, une clé ou un pseudonyme. La liste est **signée**
//! ([`crate::vault`]) par l'appareil et par le compte connecté ; chaque salon
//! porte en plus la signature du serveur sur ce qu'il en a dit
//! ([`verify_server`]). Ce qui a été modifié hors d'Alley est écarté.

use serde::{Deserialize, Serialize};
#[cfg(target_arch = "wasm32")]
use serde_json::Value;

#[cfg(target_arch = "wasm32")]
use crate::vault;

#[cfg(target_arch = "wasm32")]
const KEY: &str = "alley.rooms.v2";
#[cfg(target_arch = "wasm32")]
const SERVER_KEY: &str = "alley.server";
/// Salons retenus, au plus (les plus récents).
const MAX: usize = 30;

/// Un salon retenu.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Recent {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub private: bool,
    /// Le serveur du salon (vide : celui de la page).
    #[serde(default)]
    pub server: String,
    /// Sa fin (minuit chez son créateur), en millisecondes.
    pub expires_at: f64,
    /// La signature du serveur sur l'identifiant, le nom, le type et la fin.
    #[serde(default)]
    pub sig: String,
    /// Créé depuis cet appareil.
    #[serde(default)]
    pub created: bool,
    /// La dernière visite, en millisecondes.
    #[serde(default)]
    pub seen: f64,
}

impl Recent {
    /// Ce que le serveur a dit du salon, avec sa signature : de quoi le
    /// rouvrir s'il l'a oublié.
    pub fn statement(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "name": self.name,
            "private": self.private,
            "expires_at": self.expires_at as i64,
            "sig": self.sig,
        })
    }

    /// La signature du serveur (clé publique en base64) est-elle juste ?
    pub fn signed_by(&self, server_key: &str) -> bool {
        crate::crypto::room_signed_by(
            server_key,
            &self.id,
            &self.name,
            self.private,
            self.expires_at as i64,
            &self.sig,
        )
    }
}

#[cfg(target_arch = "wasm32")]
thread_local! {
    static ROOMS: std::cell::RefCell<Vec<Recent>> = const { std::cell::RefCell::new(Vec::new()) };
}

pub fn parse(raw: &str) -> Vec<Recent> {
    serde_json::from_str(raw).unwrap_or_default()
}

/// Sans ceux qui ont passé leur minuit, les plus récents d'abord.
pub fn alive(mut rooms: Vec<Recent>, now: f64) -> Vec<Recent> {
    rooms.retain(|r| r.expires_at > now && !r.id.is_empty());
    rooms.sort_by(|a, b| b.seen.total_cmp(&a.seen));
    rooms.truncate(MAX);
    rooms
}

/// Relit la liste gardée (au démarrage, et une fois connecté : la signature
/// du compte est alors vérifiée aussi).
#[cfg(target_arch = "wasm32")]
pub async fn load() {
    let raw = vault::read(KEY).await.unwrap_or_default();
    let rooms = alive(parse(&raw), js_sys::Date::now());
    ROOMS.with(|r| *r.borrow_mut() = rooms);
}

/// Les salons retenus encore ouverts, du plus récent au plus ancien.
#[cfg(target_arch = "wasm32")]
pub fn list() -> Vec<Recent> {
    alive(ROOMS.with(|r| r.borrow().clone()), js_sys::Date::now())
}

/// Remplace la liste, et l'enregistre signée.
#[cfg(target_arch = "wasm32")]
fn save(rooms: Vec<Recent>) {
    let rooms = alive(rooms, js_sys::Date::now());
    let raw = serde_json::to_string(&rooms).unwrap_or_else(|_| "[]".into());
    ROOMS.with(|r| *r.borrow_mut() = rooms);
    crate::dom::spawn(async move { vault::write(KEY, &raw).await });
}

/// Ajoute (ou rafraîchit) un salon, d'après ce qu'en dit le serveur (avec sa
/// signature).
#[cfg(target_arch = "wasm32")]
pub fn remember(info: &Value, server: &str, created: bool) {
    let Some(id) = info["id"].as_str() else {
        return;
    };
    let mut rooms = list();
    let previous = rooms
        .iter()
        .position(|r| r.id == id && r.server == server)
        .map(|i| rooms.remove(i));
    rooms.push(Recent {
        id: id.to_owned(),
        name: info["name"].as_str().unwrap_or("Alley").to_owned(),
        private: info["private"].as_bool().unwrap_or_default(),
        server: server.to_owned(),
        expires_at: info["expires_at"].as_f64().unwrap_or_default(),
        sig: info["sig"].as_str().unwrap_or_default().to_owned(),
        created: created || previous.as_ref().is_some_and(|p| p.created),
        seen: js_sys::Date::now(),
    });
    save(rooms);
}

/// Oublie un salon.
#[cfg(target_arch = "wasm32")]
pub fn forget(id: &str, server: &str) {
    let mut rooms = list();
    rooms.retain(|r| !(r.id == id && r.server == server));
    save(rooms);
}

/// Oublie tous les salons (et le serveur retenu).
#[cfg(target_arch = "wasm32")]
pub fn forget_all() {
    ROOMS.with(|r| r.borrow_mut().clear());
    vault::remove(KEY);
    vault::remove(SERVER_KEY);
}

/// Oublie les salons de ce serveur qu'il ne connaît plus (`open` : ceux
/// qu'il dit encore ouverts).
#[cfg(target_arch = "wasm32")]
pub fn prune(server: &str, open: &[Value]) {
    let ids: Vec<&str> = open.iter().filter_map(|r| r["id"].as_str()).collect();
    let mut rooms = list();
    let before = rooms.len();
    rooms.retain(|r| r.server != server || ids.contains(&r.id.as_str()));
    if rooms.len() != before {
        save(rooms);
    }
}

/// Vérifie la signature du serveur sur chaque salon retenu de ce serveur :
/// un salon dont l'identifiant, le nom, le type ou la fin ont été modifiés
/// est écarté. Renvoie `true` si l'un l'a été.
#[cfg(target_arch = "wasm32")]
pub fn verify_server(server: &str, server_key: &str) -> bool {
    let mut rooms = list();
    let before = rooms.len();
    rooms.retain(|r| r.server != server || r.signed_by(server_key));
    let tampered = rooms.len() != before;
    if tampered {
        vault::mark_tampered();
        save(rooms);
    }
    tampered
}

/// Le serveur saisi la dernière fois (GitHub Pages sans API configurée),
/// s'il est intact.
#[cfg(target_arch = "wasm32")]
pub async fn saved_server() -> String {
    vault::read(SERVER_KEY).await.unwrap_or_default()
}

#[cfg(target_arch = "wasm32")]
pub fn save_server(server: &str) {
    let server = server.to_owned();
    crate::dom::spawn(async move { vault::write(SERVER_KEY, &server).await });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room(id: &str, expires_at: f64, seen: f64) -> Recent {
        Recent {
            id: id.into(),
            name: id.into(),
            private: false,
            server: String::new(),
            expires_at,
            sig: String::new(),
            created: false,
            seen,
        }
    }

    #[test]
    fn expired_rooms_are_forgotten_and_recent_ones_come_first() {
        let rooms = vec![
            room("a", 50.0, 1.0),
            room("b", 200.0, 2.0),
            room("c", 200.0, 3.0),
        ];
        let kept = alive(rooms, 100.0);
        assert_eq!(
            kept.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            ["c", "b"]
        );
        let many: Vec<Recent> = (0..40)
            .map(|i| room(&i.to_string(), 200.0, i as f64))
            .collect();
        assert_eq!(alive(many, 100.0).len(), MAX);
    }

    #[test]
    fn a_damaged_list_reads_as_empty() {
        assert!(parse("pas du json").is_empty());
        assert!(parse("").is_empty());
        let raw = serde_json::to_string(&[room("a", 1.0, 1.0)]).unwrap();
        assert_eq!(parse(&raw), [room("a", 1.0, 1.0)]);
    }

    #[test]
    fn a_room_keeps_the_server_signature() {
        use ed25519_dalek::{Signer, SigningKey};
        let key = SigningKey::from_bytes(&[5u8; 32]);
        let public = crate::crypto::b64(&key.verifying_key().to_bytes());
        let mut r = room("ab12cd", 1_791_936_000_000.0, 1.0);
        let statement =
            crate::crypto::room_statement(&r.id, &r.name, r.private, r.expires_at as i64);
        r.sig = crate::crypto::b64(&key.sign(&statement).to_bytes());
        assert!(r.signed_by(&public));
        let mut renamed = r.clone();
        renamed.name = "Hameçon".into();
        assert!(!renamed.signed_by(&public));
        let mut later = r.clone();
        later.expires_at += 86_400_000.0;
        assert!(!later.signed_by(&public));
        let mut opened = r.clone();
        opened.private = true;
        assert!(!opened.signed_by(&public));
    }
}
