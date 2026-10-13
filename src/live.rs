//! Le temps réel : un canal `broadcast` par serveur Alley. Chaque WebSocket
//! ouverte s'abonne au canal de son serveur ; l'API publie dessus (nouveau
//! message, état du serveur…) et les membres y relaient leurs propres
//! messages chiffrés (présence, « j'écris », signalisation des appels).
//!
//! Le serveur ne sait pas **qui** est connecté : une connexion ne présente
//! que le jeton d'accès du serveur, pas d'identifiant de membre. Il ne compte
//! que des connexions.

use std::collections::HashMap;
use std::sync::Mutex;

use serde_json::Value;
use tokio::sync::broadcast;

/// Événements gardés pour un abonné lent avant qu'il n'en perde.
const BACKLOG: usize = 512;

#[derive(Default)]
pub struct Hub {
    rooms: Mutex<HashMap<String, Room>>,
}

struct Room {
    tx: broadcast::Sender<String>,
    connections: usize,
}

impl Hub {
    /// Une connexion s'ouvre : elle reçoit désormais les événements du serveur.
    pub fn join(&self, guild: &str) -> broadcast::Receiver<String> {
        let mut rooms = self.rooms.lock().unwrap();
        let room = rooms.entry(guild.to_owned()).or_insert_with(|| Room {
            tx: broadcast::channel(BACKLOG).0,
            connections: 0,
        });
        room.connections += 1;
        room.tx.subscribe()
    }

    /// Une connexion se ferme.
    pub fn leave(&self, guild: &str) {
        let mut rooms = self.rooms.lock().unwrap();
        if let Some(room) = rooms.get_mut(guild) {
            room.connections = room.connections.saturating_sub(1);
            if room.connections == 0 {
                rooms.remove(guild);
            }
        }
    }

    /// Publie un événement à toutes les connexions d'un serveur.
    pub fn publish(&self, guild: &str, event: Value) {
        self.publish_raw(guild, event.to_string());
    }

    pub fn publish_raw(&self, guild: &str, event: String) {
        let rooms = self.rooms.lock().unwrap();
        if let Some(room) = rooms.get(guild) {
            let _ = room.tx.send(event);
        }
    }

    /// Le nombre de connexions ouvertes, tous serveurs confondus.
    pub fn connections(&self) -> usize {
        let rooms = self.rooms.lock().unwrap();
        rooms.values().map(|r| r.connections).sum()
    }

    /// Envoie un dernier événement puis coupe toutes les connexions d'un
    /// serveur : serveur supprimé, ou clé renouvelée (un membre exclu ne doit
    /// plus rien recevoir ; les autres se reconnectent avec la nouvelle clé).
    pub fn close(&self, guild: &str, event: Value) {
        let mut rooms = self.rooms.lock().unwrap();
        if let Some(room) = rooms.remove(guild) {
            let _ = room.tx.send(event.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn events_stay_in_their_guild_and_closing_ends_the_stream() {
        let hub = Hub::default();
        let mut a = hub.join("g1");
        let mut other = hub.join("g2");
        assert_eq!(hub.connections(), 2);

        hub.publish("g1", json!({ "type": "ping" }));
        assert!(a.recv().await.unwrap().contains("ping"));
        assert!(other.try_recv().is_err(), "un autre serveur ne voit rien");

        hub.close("g1", json!({ "type": "rotated" }));
        assert!(a.recv().await.unwrap().contains("rotated"));
        assert!(a.recv().await.is_err(), "le flux est coupé");
        hub.leave("g2");
        assert_eq!(hub.connections(), 0);
    }
}
