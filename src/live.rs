//! Le temps réel : un canal `broadcast` par serveur Alley, et qui est en
//! ligne. Chaque WebSocket ouverte s'abonne au canal de son serveur ; l'API
//! publie dessus (nouveau message, salon renommé…). Un seul processus sert
//! tout le monde : ni Redis ni file de messages.
//!
//! Ce qui circule ici est déjà chiffré : des jetons, des identifiants.

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

use serde_json::{Value, json};
use tokio::sync::broadcast;

/// Événements gardés pour un abonné lent avant qu'il n'en perde.
const BACKLOG: usize = 256;

#[derive(Default)]
pub struct Hub {
    rooms: Mutex<HashMap<String, Room>>,
}

struct Room {
    tx: broadcast::Sender<String>,
    /// Membre → nombre de connexions ouvertes (plusieurs onglets, appareils).
    online: BTreeMap<String, usize>,
}

impl Room {
    fn new() -> Room {
        Room {
            tx: broadcast::channel(BACKLOG).0,
            online: BTreeMap::new(),
        }
    }

    fn presence(&self) -> String {
        json!({ "type": "presence", "online": self.online.keys().collect::<Vec<_>>() }).to_string()
    }
}

impl Hub {
    /// Un membre se connecte : il reçoit désormais les événements du serveur,
    /// et tout le monde apprend qu'il est en ligne.
    pub fn join(&self, guild: &str, member: &str) -> broadcast::Receiver<String> {
        let mut rooms = self.rooms.lock().unwrap();
        let room = rooms.entry(guild.to_owned()).or_insert_with(Room::new);
        let rx = room.tx.subscribe();
        *room.online.entry(member.to_owned()).or_default() += 1;
        let _ = room.tx.send(room.presence());
        rx
    }

    /// Une connexion se ferme ; le membre n'est hors ligne qu'une fois la
    /// dernière fermée.
    pub fn leave(&self, guild: &str, member: &str) {
        let mut rooms = self.rooms.lock().unwrap();
        let Some(room) = rooms.get_mut(guild) else {
            return;
        };
        if let Some(count) = room.online.get_mut(member) {
            *count -= 1;
            if *count == 0 {
                room.online.remove(member);
            }
        }
        if room.online.is_empty() {
            rooms.remove(guild);
        } else {
            let _ = room.tx.send(room.presence());
        }
    }

    /// Publie un événement à tous les membres connectés d'un serveur.
    pub fn publish(&self, guild: &str, event: Value) {
        let rooms = self.rooms.lock().unwrap();
        if let Some(room) = rooms.get(guild) {
            let _ = room.tx.send(event.to_string());
        }
    }

    /// Les membres en ligne d'un serveur.
    pub fn online(&self, guild: &str) -> Vec<String> {
        let rooms = self.rooms.lock().unwrap();
        rooms
            .get(guild)
            .map(|room| room.online.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Le nombre de connexions ouvertes, tous serveurs confondus.
    pub fn connections(&self) -> usize {
        let rooms = self.rooms.lock().unwrap();
        rooms.values().flat_map(|r| r.online.values()).sum()
    }

    /// Ferme un serveur supprimé : ses abonnés le savent, puis le canal part.
    pub fn close(&self, guild: &str) {
        let mut rooms = self.rooms.lock().unwrap();
        if let Some(room) = rooms.remove(guild) {
            let _ = room.tx.send(json!({ "type": "guild.deleted" }).to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn presence_counts_connections_and_events_stay_in_their_guild() {
        let hub = Hub::default();
        let mut a = hub.join("g1", "ada");
        let _a2 = hub.join("g1", "ada");
        let mut other = hub.join("g2", "bob");
        assert_eq!(hub.online("g1"), vec!["ada"]);
        assert_eq!(hub.connections(), 3);

        hub.publish("g1", json!({ "type": "ping" }));
        let mut seen = Vec::new();
        while let Ok(e) = a.try_recv() {
            seen.push(e);
        }
        assert!(seen.iter().any(|e| e.contains("ping")));
        assert!(
            std::iter::from_fn(|| other.try_recv().ok()).all(|e| !e.contains("ping")),
            "un autre serveur ne voit rien"
        );

        hub.leave("g1", "ada");
        assert_eq!(hub.online("g1"), vec!["ada"], "encore un onglet ouvert");
        hub.leave("g1", "ada");
        assert!(hub.online("g1").is_empty());
    }
}
