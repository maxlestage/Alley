//! Les salons, en mémoire seulement — **rien n'est enregistré** :
//!
//! - un salon n'existe que tant que quelqu'un y est : le dernier qui part
//!   l'emporte avec lui (un salon créé mais jamais rejoint disparaît au bout
//!   d'une minute) ;
//! - le serveur ne garde aucun message : il relaie chaque jeton chiffré aux
//!   seules personnes présentes, puis l'oublie. Qui arrive ne voit rien de ce
//!   qui s'est dit avant ;
//! - chaque salon disparaît à **minuit dans le fuseau de celui qui l'a créé**
//!   (son pays) : tout le monde en est sorti, il n'en reste rien.
//!
//! Le serveur connaît le nom des salons publics (pour qu'on puisse les
//! chercher) et le nombre de présents ; il ne connaît ni les pseudonymes, ni
//! les messages, ni la clé du salon.
//!
//! Qui se connecte avec un mot de passe apporte une **marque** : une empreinte
//! tirée de son pseudonyme et de son mot de passe, dans son navigateur. Le
//! salon la retient tant qu'il vit, pour que cette personne le retrouve
//! ([`Rooms::mine`]), depuis n'importe quel appareil. Le serveur ne peut en
//! déduire ni le pseudonyme ni le mot de passe.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::protocol::random_id;

/// Personnes dans un salon, au plus (les appels vont de pair à pair).
pub const MAX_MEMBERS: usize = 64;
/// Salons ouverts en même temps, au plus.
pub const MAX_ROOMS: usize = 5_000;
/// Longueur maximale d'un nom de salon.
pub const MAX_NAME: usize = 40;
/// Un salon créé mais que personne ne rejoint disparaît au bout de ce délai.
const UNJOINED_TTL: Duration = Duration::from_secs(60);
/// Messages en attente pour une connexion lente, au plus.
const BACKLOG: usize = 512;
/// Marques retenues par un salon, au plus.
const MAX_TAGS: usize = 256;

/// Une marque : 64 caractères hexadécimaux.
pub fn is_tag(tag: &str) -> bool {
    tag.len() == 64 && tag.bytes().all(|b| b.is_ascii_hexdigit())
}

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

/// Le prochain minuit (en millisecondes UTC) pour un fuseau décalé de
/// `offset_min` minutes par rapport à UTC (`+120` à Paris l'été).
pub fn next_midnight(now: i64, offset_min: i64) -> i64 {
    let offset = offset_min.clamp(-14 * 60, 14 * 60) * 60_000;
    let local = now + offset;
    (local.div_euclid(DAY_MS) + 1) * DAY_MS - offset
}

/// Un nom de salon, pour comparer et chercher : minuscules, sans accents ni
/// espaces superflus.
pub fn normalize(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().chars().flat_map(char::to_lowercase) {
        let c = match c {
            'à' | 'â' | 'ä' | 'á' | 'ã' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' | 'í' | 'ì' => 'i',
            'ô' | 'ö' | 'ó' | 'ò' | 'õ' => 'o',
            'û' | 'ü' | 'ú' | 'ù' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            'ÿ' => 'y',
            c => c,
        };
        if c.is_whitespace() {
            if !out.ends_with(' ') {
                out.push(' ');
            }
        } else {
            out.push(c);
        }
    }
    out.trim().to_owned()
}

/// Un nom de salon acceptable : 1 à 40 caractères visibles, sans caractères
/// de contrôle.
pub fn clean_name(name: &str) -> Option<String> {
    let name: String = name.split_whitespace().collect::<Vec<_>>().join(" ");
    let ok =
        !name.is_empty() && name.chars().count() <= MAX_NAME && !name.chars().any(char::is_control);
    ok.then_some(name)
}

struct Member {
    /// Sa clé publique X25519 (base64), pour qu'on lui remette la clé du salon.
    x: String,
    tx: mpsc::Sender<String>,
    order: u64,
    /// Son adresse, pour fermer ses connexions s'il est bloqué.
    ip: Option<IpAddr>,
}

struct Room {
    id: String,
    name: String,
    key: String,
    private: bool,
    expires_at: i64,
    created: Instant,
    members: BTreeMap<String, Member>,
    next_order: u64,
    /// Les marques de ceux qui y sont venus connectés.
    tags: HashSet<String>,
}

impl Room {
    fn info(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "private": self.private,
            "members": self.members.len(),
            "expires_at": self.expires_at,
        })
    }

    fn send(&self, to: &str, event: &str) {
        if let Some(m) = self.members.get(to) {
            // Trop lent : il perd ce message plutôt que de bloquer le salon.
            let _ = m.tx.try_send(event.to_owned());
        }
    }

    fn broadcast(&self, except: &str, event: &str) {
        for (id, m) in &self.members {
            if id != except {
                let _ = m.tx.try_send(event.to_owned());
            }
        }
    }
}

/// Une connexion admise dans un salon.
pub struct Joined {
    pub me: String,
    pub inbox: mpsc::Receiver<String>,
}

#[derive(Default)]
pub struct Rooms {
    rooms: Mutex<HashMap<String, Room>>,
}

impl Rooms {
    /// Crée un salon — ou, pour un salon public, rejoint celui qui porte déjà
    /// ce nom. `offset_min` : le décalage horaire du créateur.
    pub fn create(&self, name: &str, private: bool, offset_min: i64) -> Option<Value> {
        let name = clean_name(name)?;
        let key = normalize(&name);
        let mut rooms = self.rooms.lock().unwrap();
        if !private && let Some(room) = rooms.values().find(|r| !r.private && r.key == key) {
            return Some(room.info());
        }
        if rooms.len() >= MAX_ROOMS {
            return None;
        }
        // Un salon privé n'est trouvable que par son lien : identifiant long.
        let id = random_id(if private { 16 } else { 6 });
        let room = Room {
            id: id.clone(),
            name,
            key,
            private,
            expires_at: next_midnight(now_ms(), offset_min),
            created: Instant::now(),
            members: BTreeMap::new(),
            next_order: 0,
            tags: HashSet::new(),
        };
        let info = room.info();
        rooms.insert(id, room);
        Some(info)
    }

    /// Les salons publics ouverts, les plus fréquentés d'abord, filtrés par
    /// `query` (sans tenir compte des accents ni des majuscules).
    pub fn list(&self, query: &str) -> Vec<Value> {
        let query = normalize(query);
        let rooms = self.rooms.lock().unwrap();
        let mut found: Vec<&Room> = rooms
            .values()
            .filter(|r| !r.private && (query.is_empty() || r.key.contains(&query)))
            .collect();
        found.sort_by(|a, b| {
            b.members
                .len()
                .cmp(&a.members.len())
                .then_with(|| a.name.cmp(&b.name))
        });
        found.into_iter().take(100).map(Room::info).collect()
    }

    pub fn info(&self, id: &str) -> Option<Value> {
        self.rooms.lock().unwrap().get(id).map(Room::info)
    }

    /// Les salons encore ouverts où est venue la personne de cette marque
    /// (privés compris).
    pub fn mine(&self, tag: &str) -> Vec<Value> {
        let rooms = self.rooms.lock().unwrap();
        let mut found: Vec<&Room> = rooms.values().filter(|r| r.tags.contains(tag)).collect();
        found.sort_by(|a, b| a.name.cmp(&b.name));
        found.into_iter().take(100).map(Room::info).collect()
    }

    /// Entre dans un salon : la connexion reçoit d'abord `welcome` (qui elle
    /// est, qui est déjà là), et les autres apprennent son arrivée.
    pub fn join(
        &self,
        id: &str,
        x: String,
        ip: Option<IpAddr>,
        tag: Option<&str>,
    ) -> Option<Joined> {
        let mut rooms = self.rooms.lock().unwrap();
        let room = rooms.get_mut(id)?;
        if room.members.len() >= MAX_MEMBERS || room.expires_at <= now_ms() {
            return None;
        }
        let me = random_id(6);
        let (tx, inbox) = mpsc::channel(BACKLOG);
        let peers: Vec<Value> = room
            .members
            .iter()
            .map(|(id, m)| json!({ "id": id, "x": m.x, "order": m.order }))
            .collect();
        let order = room.next_order;
        room.next_order += 1;
        let welcome = json!({
            "type": "welcome",
            "you": me,
            "order": order,
            "room": room.info(),
            "peers": peers,
        });
        let _ = tx.try_send(welcome.to_string());
        let joined = json!({ "type": "joined", "id": me, "x": x, "order": order }).to_string();
        room.broadcast(&me, &joined);
        room.members.insert(me.clone(), Member { x, tx, order, ip });
        if let Some(tag) = tag.filter(|t| is_tag(t))
            && room.tags.len() < MAX_TAGS
        {
            room.tags.insert(tag.to_owned());
        }
        Some(Joined { me, inbox })
    }

    /// Une connexion se ferme ; le dernier qui part emporte le salon.
    pub fn leave(&self, id: &str, me: &str) {
        let mut rooms = self.rooms.lock().unwrap();
        let Some(room) = rooms.get_mut(id) else {
            return;
        };
        if room.members.remove(me).is_none() {
            return;
        }
        if room.members.is_empty() {
            rooms.remove(id);
        } else {
            room.broadcast(me, &json!({ "type": "left", "id": me }).to_string());
        }
    }

    /// Relaie un jeton chiffré (à une personne, ou à tous les autres) en
    /// indiquant qui l'envoie. Rien n'est gardé.
    pub fn relay(&self, id: &str, from: &str, to: Option<&str>, data: &str) {
        let rooms = self.rooms.lock().unwrap();
        let Some(room) = rooms.get(id) else { return };
        let event = json!({ "type": "relay", "from": from, "data": data }).to_string();
        match to {
            Some(to) => room.send(to, &event),
            None => room.broadcast(from, &event),
        }
    }

    /// Relaie une image vérifiée : `data` (chiffré avec la clé du salon) porte
    /// la clé de l'image, `c` l'image chiffrée avec cette clé, et `kc`
    /// l'empreinte de cette clé, pour que chacun vérifie que l'image reçue est
    /// bien celle que le serveur a vue.
    pub fn relay_image(&self, id: &str, from: &str, data: &str, c: &str, kc: &str) {
        let rooms = self.rooms.lock().unwrap();
        let Some(room) = rooms.get(id) else { return };
        let event =
            json!({ "type": "relay", "from": from, "data": data, "c": c, "kc": kc }).to_string();
        room.broadcast(from, &event);
    }

    /// Un message du serveur pour une seule connexion.
    pub fn notify(&self, id: &str, to: &str, event: &Value) {
        let rooms = self.rooms.lock().unwrap();
        if let Some(room) = rooms.get(id) {
            room.send(to, &event.to_string());
        }
    }

    /// Ferme les connexions des adresses bloquées.
    pub fn kick(&self, banned: impl Fn(IpAddr) -> bool) {
        let closed = json!({ "type": "closed", "reason": "banned" }).to_string();
        let rooms = self.rooms.lock().unwrap();
        for room in rooms.values() {
            for m in room.members.values() {
                if m.ip.is_some_and(&banned) {
                    let _ = m.tx.try_send(closed.clone());
                }
            }
        }
    }

    /// Remet la clé du salon (emballée pour lui seul) à un nouvel arrivant.
    pub fn hand_key(&self, id: &str, from: &str, to: &str, blob: &str) {
        let rooms = self.rooms.lock().unwrap();
        let Some(room) = rooms.get(id) else { return };
        let event = json!({ "type": "key", "from": from, "blob": blob }).to_string();
        room.send(to, &event);
    }

    /// Ferme les salons arrivés à minuit (tout le monde en est sorti) et ceux
    /// que personne n'a rejoints.
    pub fn sweep(&self) {
        let now = now_ms();
        let mut rooms = self.rooms.lock().unwrap();
        rooms.retain(|_, room| {
            if room.expires_at <= now {
                let closed = json!({ "type": "closed", "reason": "midnight" }).to_string();
                room.broadcast("", &closed);
                return false;
            }
            !(room.members.is_empty() && room.created.elapsed() > UNJOINED_TTL)
        });
    }

    pub fn count(&self) -> (usize, usize) {
        let rooms = self.rooms.lock().unwrap();
        (rooms.len(), rooms.values().map(|r| r.members.len()).sum())
    }

    #[cfg(test)]
    fn expire_now(&self, id: &str) {
        if let Some(r) = self.rooms.lock().unwrap().get_mut(id) {
            r.expires_at = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn next(j: &mut Joined) -> Value {
        serde_json::from_str(&j.inbox.try_recv().expect("un événement")).unwrap()
    }

    #[test]
    fn midnight_is_local_to_the_creator() {
        // 2026-10-13 21:30 UTC.
        let now = 1_791_927_000_000;
        let paris = next_midnight(now, 120);
        // À Paris il est 23:30 : minuit dans 30 minutes.
        assert_eq!(paris - now, 30 * 60_000);
        // À New York (UTC−4) il est 17:30 : minuit dans 6 h 30.
        assert_eq!(next_midnight(now, -240) - now, 390 * 60_000);
        // À minuit pile, c'est le minuit suivant.
        assert_eq!(next_midnight(paris, 120) - paris, DAY_MS);
    }

    #[test]
    fn names_are_compared_without_accents_or_case() {
        assert_eq!(normalize("  Café   Été "), "cafe ete");
        assert_eq!(clean_name("  Les   copains "), Some("Les copains".into()));
        assert_eq!(clean_name("   "), None);
        assert_eq!(clean_name(&"x".repeat(41)), None);
    }

    #[test]
    fn a_room_lives_only_while_someone_is_in_it() {
        let rooms = Rooms::default();
        let room = rooms.create("Café", false, 120).unwrap();
        let id = room["id"].as_str().unwrap().to_owned();
        // Le même nom public mène au même salon.
        assert_eq!(rooms.create("cafe", false, 0).unwrap()["id"], id);
        assert_eq!(rooms.list("CAFÉ").len(), 1);
        assert!(rooms.list("thé").is_empty());

        let mut ada = rooms.join(&id, "xa".into(), None, None).unwrap();
        let welcome = next(&mut ada);
        assert_eq!(welcome["type"], "welcome");
        assert!(welcome["peers"].as_array().unwrap().is_empty());

        let mut bob = rooms.join(&id, "xb".into(), None, None).unwrap();
        let welcome = next(&mut bob);
        assert_eq!(welcome["peers"][0]["id"], ada.me);
        assert_eq!(welcome["peers"][0]["x"], "xa");
        let joined = next(&mut ada);
        assert_eq!(joined["type"], "joined");
        assert_eq!(joined["x"], "xb");

        // Un message va aux autres, signé de son expéditeur ; rien n'est gardé.
        rooms.relay(&id, &ada.me, None, "[[jeton]]");
        let got = next(&mut bob);
        assert_eq!(
            (got["from"].as_str(), got["data"].as_str()),
            (Some(ada.me.as_str()), Some("[[jeton]]"))
        );
        assert!(ada.inbox.try_recv().is_err(), "pas d'écho");
        // Une clé emballée ne va qu'à son destinataire.
        rooms.hand_key(&id, &ada.me, &bob.me, "blob");
        assert_eq!(next(&mut bob)["type"], "key");

        // Qui arrive ensuite ne voit rien de ce qui précède.
        let mut eve = rooms.join(&id, "xe".into(), None, None).unwrap();
        assert_eq!(next(&mut eve)["type"], "welcome");
        assert!(eve.inbox.try_recv().is_err());

        rooms.leave(&id, &eve.me);
        rooms.leave(&id, &bob.me);
        assert_eq!(next(&mut ada)["type"], "joined");
        assert_eq!(next(&mut ada)["type"], "left");
        assert_eq!(rooms.count(), (1, 1));
        // Le dernier qui part emporte le salon.
        rooms.leave(&id, &ada.me);
        assert_eq!(rooms.count(), (0, 0));
        assert!(rooms.join(&id, "x".into(), None, None).is_none());
    }

    #[test]
    fn a_tag_finds_its_rooms_again() {
        let rooms = Rooms::default();
        let tag = "ab".repeat(32);
        let secret = rooms.create("Secret", true, 0).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let public = rooms.create("Place", false, 0).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let _a = rooms.join(&secret, "xa".into(), None, Some(&tag)).unwrap();
        let _b = rooms.join(&public, "xb".into(), None, None).unwrap();
        let mine = rooms.mine(&tag);
        assert_eq!(mine.len(), 1);
        assert_eq!(mine[0]["id"], secret.as_str());
        assert!(rooms.mine(&"cd".repeat(32)).is_empty());
        assert!(!is_tag("pas une marque"));
    }

    #[test]
    fn a_banned_address_is_disconnected() {
        let rooms = Rooms::default();
        let id = rooms.create("Place", false, 0).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let bad: IpAddr = "203.0.113.7".parse().unwrap();
        let mut eve = rooms.join(&id, "xe".into(), Some(bad), None).unwrap();
        let mut ada = rooms
            .join(
                &id,
                "xa".into(),
                Some("198.51.100.1".parse().unwrap()),
                None,
            )
            .unwrap();
        next(&mut eve);
        next(&mut eve);
        next(&mut ada);
        rooms.kick(|ip| ip == bad);
        assert_eq!(next(&mut eve)["reason"], "banned");
        assert!(ada.inbox.try_recv().is_err());
    }

    #[test]
    fn midnight_closes_the_room_for_everyone() {
        let rooms = Rooms::default();
        let id = rooms.create("Nuit", true, 0).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(id.len(), 32, "un salon privé a un identifiant long");
        assert!(
            rooms.list("nuit").is_empty(),
            "un salon privé n'est pas listé"
        );
        let mut ada = rooms.join(&id, "xa".into(), None, None).unwrap();
        next(&mut ada);
        rooms.expire_now(&id);
        rooms.sweep();
        let closed = next(&mut ada);
        assert_eq!(
            (closed["type"].as_str(), closed["reason"].as_str()),
            (Some("closed"), Some("midnight"))
        );
        assert_eq!(rooms.count(), (0, 0));
    }
}
