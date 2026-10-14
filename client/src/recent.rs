//! Les salons dont se souvient cet appareil : ceux qu'on a créés ou visités,
//! avec leur serveur, pour y revenir d'un geste depuis le hall. Ils sont
//! gardés dans le navigateur (`localStorage`) jusqu'à leur minuit, puis
//! oubliés d'eux-mêmes ; « Oublier » les retire avant.
//!
//! Seuls l'identifiant, le nom et le serveur d'un salon sont retenus :
//! jamais un message, une clé ou un pseudonyme. Un salon reste ouvert
//! jusqu'à minuit, même vide (le serveur le garde) : on peut donc y revenir
//! après l'avoir quitté.

use serde::{Deserialize, Serialize};
#[cfg(target_arch = "wasm32")]
use serde_json::Value;

#[cfg(target_arch = "wasm32")]
use crate::dom::window;

#[cfg(target_arch = "wasm32")]
const KEY: &str = "alley.rooms.v1";
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
    /// Créé depuis cet appareil.
    #[serde(default)]
    pub created: bool,
    /// La dernière visite, en millisecondes.
    #[serde(default)]
    pub seen: f64,
}

#[cfg(target_arch = "wasm32")]
fn storage() -> Option<web_sys::Storage> {
    window().local_storage().ok().flatten()
}

/// Les salons retenus encore ouverts, du plus récent au plus ancien.
#[cfg(target_arch = "wasm32")]
pub fn list() -> Vec<Recent> {
    let raw = storage()
        .and_then(|s| s.get_item(KEY).ok().flatten())
        .unwrap_or_default();
    alive(parse(&raw), js_sys::Date::now())
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

#[cfg(target_arch = "wasm32")]
fn save(rooms: &[Recent]) {
    if let (Some(s), Ok(raw)) = (storage(), serde_json::to_string(rooms)) {
        let _ = s.set_item(KEY, &raw);
    }
}

/// Ajoute (ou rafraîchit) un salon, d'après ce qu'en dit le serveur.
#[cfg(target_arch = "wasm32")]
pub fn remember(info: &Value, server: &str, created: bool) {
    let Some(id) = info["id"].as_str() else {
        return;
    };
    let mut rooms = list();
    let was_created = rooms
        .iter()
        .any(|r| r.id == id && r.server == server && r.created);
    rooms.retain(|r| !(r.id == id && r.server == server));
    rooms.push(Recent {
        id: id.to_owned(),
        name: info["name"].as_str().unwrap_or("Alley").to_owned(),
        private: info["private"].as_bool().unwrap_or_default(),
        server: server.to_owned(),
        expires_at: info["expires_at"].as_f64().unwrap_or_default(),
        created: created || was_created,
        seen: js_sys::Date::now(),
    });
    save(&alive(rooms, js_sys::Date::now()));
}

/// Oublie un salon.
#[cfg(target_arch = "wasm32")]
pub fn forget(id: &str, server: &str) {
    let mut rooms = list();
    rooms.retain(|r| !(r.id == id && r.server == server));
    save(&rooms);
}

/// Oublie tous les salons (et le serveur retenu).
#[cfg(target_arch = "wasm32")]
pub fn forget_all() {
    if let Some(s) = storage() {
        let _ = s.remove_item(KEY);
        let _ = s.remove_item(SERVER_KEY);
    }
}

/// Le serveur d'un salon : retire de la liste ceux de ce serveur que le
/// serveur ne connaît plus (`open` : les identifiants encore ouverts).
#[cfg(target_arch = "wasm32")]
pub fn prune(server: &str, open: &[String]) {
    let mut rooms = list();
    rooms.retain(|r| r.server != server || open.contains(&r.id));
    save(&rooms);
}

/// Le serveur saisi la dernière fois (GitHub Pages sans API configurée).
#[cfg(target_arch = "wasm32")]
pub fn saved_server() -> String {
    storage()
        .and_then(|s| s.get_item(SERVER_KEY).ok().flatten())
        .unwrap_or_default()
}

#[cfg(target_arch = "wasm32")]
pub fn save_server(server: &str) {
    if let Some(s) = storage() {
        let _ = s.set_item(SERVER_KEY, server);
    }
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
}
