//! Les salons, en mémoire seulement — **rien n'est enregistré** :
//!
//! - un salon vit jusqu'à son minuit, même vide : on peut y revenir après
//!   l'avoir quitté (un salon créé mais jamais rejoint disparaît au bout
//!   d'une minute ; quand la place manque, le salon vide depuis le plus
//!   longtemps cède la sienne) ;
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
//!
//! Chaque salon a un **administrateur** : son créateur. Connecté avec un
//! compte, il le reste — revenu avec la même marque, il retrouve son rôle ;
//! sans compte, il redevient simple membre en partant. Tant qu'il est absent,
//! le plus ancien présent tient le rôle. L'administrateur peut exclure un
//! membre ([`Rooms::expel`]) et supprimer n'importe quel message.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::net::IpAddr;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use tokio::sync::mpsc;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ed25519_dalek::{Signer, SigningKey};

use crate::protocol::{random_id, room_statement};

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
/// Salons demandés d'un coup à [`Rooms::known`], au plus.
pub const MAX_KNOWN: usize = 50;
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
    /// Sa clé publique Ed25519 (base64) : tout ce qu'il envoie est signé.
    s: Option<String>,
    tx: mpsc::Sender<String>,
    order: u64,
    /// Son adresse, pour fermer ses connexions s'il est bloqué.
    ip: Option<IpAddr>,
    /// Sa marque, s'il est connecté avec un compte.
    tag: Option<String>,
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
    /// Vide depuis : le premier à céder sa place quand elle manque.
    empty_since: Option<Instant>,
    /// Un salon de plusieurs jours (abonnement) : gardé en base, il survit à
    /// un redémarrage du serveur et ne cède sa place qu'en dernier.
    long: bool,
    /// Les marques de ceux qui y sont venus connectés.
    tags: HashSet<String>,
    /// La marque du propriétaire (l'administrateur connecté avec un compte).
    owner_tag: Option<String>,
    /// La connexion qui tient le rôle d'administrateur.
    admin: Option<String>,
    /// Exclus jusqu'à la fin du salon : leur marque, ou leur adresse.
    barred_tags: HashSet<String>,
    barred_ips: HashSet<IpAddr>,
}

impl Room {
    fn new(id: String, name: String, private: bool, expires_at: i64, long: bool) -> Room {
        Room {
            id,
            key: normalize(&name),
            name,
            private,
            expires_at,
            created: Instant::now(),
            members: BTreeMap::new(),
            next_order: 0,
            // Rouvert après un redémarrage : vide, il peut céder sa place.
            empty_since: long.then(Instant::now),
            long,
            tags: HashSet::new(),
            owner_tag: None,
            admin: None,
            barred_tags: HashSet::new(),
            barred_ips: HashSet::new(),
        }
    }

    /// Garde-t-on ce salon ? Jusqu'à minuit, s'il a été rejoint au moins une
    /// fois ; une minute sinon.
    fn alive(&self, now: i64) -> bool {
        self.expires_at > now
            && (self.next_order > 0 || self.long || self.created.elapsed() < UNJOINED_TTL)
    }

    /// Désigne l'administrateur : le propriétaire s'il est là, sinon celui
    /// qui tient déjà le rôle, sinon le plus ancien présent (qui devient
    /// propriétaire si le salon n'en a pas). Renvoie `true` s'il a changé.
    fn elect(&mut self) -> bool {
        let owner_here = self.owner_tag.as_ref().and_then(|owner| {
            self.members
                .iter()
                .filter(|(_, m)| m.tag.as_ref() == Some(owner))
                .min_by_key(|(_, m)| m.order)
                .map(|(id, _)| id.clone())
        });
        let current = self.admin.clone().filter(|a| self.members.contains_key(a));
        let next = owner_here.or(current).or_else(|| {
            self.members
                .iter()
                .min_by_key(|(_, m)| m.order)
                .map(|(id, _)| id.clone())
        });
        if self.owner_tag.is_none()
            && let Some(id) = &next
        {
            self.owner_tag = self.members.get(id).and_then(|m| m.tag.clone());
        }
        let changed = next != self.admin;
        self.admin = next;
        changed
    }

    fn announce_admin(&self) {
        let event = json!({ "type": "admin", "id": self.admin }).to_string();
        self.broadcast("", &event);
    }

    /// Ce qu'on dit d'un salon, signé par le serveur ([`room_statement`]) :
    /// l'appareil qui le retient vérifie que personne ne l'a modifié depuis.
    fn info(&self, key: &SigningKey) -> Value {
        let statement = room_statement(&self.id, &self.name, self.private, self.expires_at);
        json!({
            "id": self.id,
            "name": self.name,
            "private": self.private,
            "members": self.members.len(),
            "expires_at": self.expires_at,
            "sig": STANDARD.encode(key.sign(&statement).to_bytes()),
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

/// Pourquoi une connexion n'entre pas.
#[derive(Debug, PartialEq, Eq)]
pub enum JoinError {
    /// Le salon n'existe plus (ou il est complet, ou c'est minuit).
    Gone,
    /// Exclu par l'administrateur.
    Barred,
}

/// Une connexion admise dans un salon.
pub struct Joined {
    pub me: String,
    pub inbox: mpsc::Receiver<String>,
}

/// Les salons. Une copie partage le même état (la base le recharge au
/// démarrage, avant la première requête).
#[derive(Clone)]
pub struct Rooms {
    rooms: Arc<Mutex<HashMap<String, Room>>>,
    /// La clé qui signe ce que le serveur dit des salons : tirée au premier
    /// démarrage, puis gardée en base ([`crate::billing::restore`]) pour que
    /// les salons de plusieurs jours restent vérifiables après un redémarrage.
    key: Arc<RwLock<SigningKey>>,
}

impl Default for Rooms {
    fn default() -> Self {
        let mut seed = [0u8; 32];
        getrandom::getrandom(&mut seed).expect("générateur aléatoire du système");
        Rooms {
            rooms: Arc::default(),
            key: Arc::new(RwLock::new(SigningKey::from_bytes(&seed))),
        }
    }
}

/// Ce que la base garde d'un salon de plusieurs jours : de quoi le rouvrir,
/// rien de plus (ni membres, ni messages).
#[derive(Clone, Debug, PartialEq)]
pub struct Kept {
    pub id: String,
    pub name: String,
    pub private: bool,
    pub expires_at: i64,
}

impl Rooms {
    /// Crée un salon — ou, pour un salon public, rejoint celui qui porte déjà
    /// ce nom. `offset_min` : le décalage horaire du créateur ; `days` : 1
    /// (jusqu'à minuit), ou 3 et 7 avec un abonnement (jusqu'au minuit qui
    /// clôt le troisième ou le septième jour).
    pub fn create(&self, name: &str, private: bool, offset_min: i64, days: u32) -> Option<Value> {
        let name = clean_name(name)?;
        let key = normalize(&name);
        let mut rooms = self.rooms.lock().unwrap();
        if !private && let Some(room) = rooms.values().find(|r| !r.private && r.key == key) {
            return Some(room.info(&self.signing_key()));
        }
        if rooms.len() >= MAX_ROOMS {
            // La place manque : le salon vide depuis le plus longtemps la cède
            // (un salon de plusieurs jours en dernier).
            let oldest = rooms
                .iter()
                .filter_map(|(id, r)| r.empty_since.map(|t| (r.long, t, id.clone())))
                .min()?;
            rooms.remove(&oldest.2);
        }
        // Un salon privé n'est trouvable que par son lien : identifiant long.
        let id = random_id(if private { 16 } else { 6 });
        let days = days.clamp(1, 7);
        let expires_at = next_midnight(now_ms(), offset_min) + i64::from(days - 1) * DAY_MS;
        let room = Room::new(id.clone(), name, private, expires_at, days > 1);
        let info = room.info(&self.signing_key());
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
        found
            .into_iter()
            .take(100)
            .map(|r| r.info(&self.signing_key()))
            .collect()
    }

    fn signing_key(&self) -> SigningKey {
        self.key.read().unwrap().clone()
    }

    /// La clé publique qui vérifie ce que le serveur dit des salons (base64).
    pub fn public_key(&self) -> String {
        STANDARD.encode(self.signing_key().verifying_key().to_bytes())
    }

    /// La clé de signature gardée en base (sa graine, 32 octets).
    pub fn set_signing_key(&self, seed: [u8; 32]) {
        *self.key.write().unwrap() = SigningKey::from_bytes(&seed);
    }

    /// La graine de la clé de signature, pour la garder en base.
    pub fn signing_seed(&self) -> [u8; 32] {
        self.signing_key().to_bytes()
    }

    /// Rouvre les salons de plusieurs jours gardés en base (au démarrage).
    pub fn restore(&self, kept: Vec<Kept>) {
        let now = now_ms();
        let mut rooms = self.rooms.lock().unwrap();
        for k in kept {
            let Some(name) = clean_name(&k.name) else {
                continue;
            };
            if k.expires_at <= now || rooms.contains_key(&k.id) || rooms.len() >= MAX_ROOMS {
                continue;
            }
            let room = Room::new(k.id.clone(), name, k.private, k.expires_at, true);
            rooms.insert(k.id, room);
        }
    }

    /// Un salon de plusieurs jours, à garder en base ; `None` pour un salon
    /// qui finit à minuit.
    pub fn kept(&self, id: &str) -> Option<Kept> {
        let rooms = self.rooms.lock().unwrap();
        let r = rooms.get(id).filter(|r| r.long)?;
        Some(Kept {
            id: r.id.clone(),
            name: r.name.clone(),
            private: r.private,
            expires_at: r.expires_at,
        })
    }

    pub fn info(&self, id: &str) -> Option<Value> {
        self.rooms
            .lock()
            .unwrap()
            .get(id)
            .map(|r| r.info(&self.signing_key()))
    }

    /// Parmi ces identifiants (les salons dont se souvient un appareil), ceux
    /// qui sont encore ouverts, dans le même ordre.
    pub fn known(&self, ids: &[String]) -> Vec<Value> {
        let rooms = self.rooms.lock().unwrap();
        ids.iter()
            .take(MAX_KNOWN)
            .filter_map(|id| rooms.get(id).map(|r| r.info(&self.signing_key())))
            .collect()
    }

    /// Les salons encore ouverts où est venue la personne de cette marque
    /// (privés compris).
    pub fn mine(&self, tag: &str) -> Vec<Value> {
        let rooms = self.rooms.lock().unwrap();
        let mut found: Vec<&Room> = rooms.values().filter(|r| r.tags.contains(tag)).collect();
        found.sort_by(|a, b| a.name.cmp(&b.name));
        found
            .into_iter()
            .take(100)
            .map(|r| r.info(&self.signing_key()))
            .collect()
    }

    /// Entre dans un salon : la connexion reçoit d'abord `welcome` (qui elle
    /// est, qui est déjà là), et les autres apprennent son arrivée.
    pub fn join(
        &self,
        id: &str,
        x: String,
        s: Option<String>,
        ip: Option<IpAddr>,
        tag: Option<&str>,
    ) -> Result<Joined, JoinError> {
        let mut rooms = self.rooms.lock().unwrap();
        let room = rooms.get_mut(id).ok_or(JoinError::Gone)?;
        if room.members.len() >= MAX_MEMBERS || room.expires_at <= now_ms() {
            return Err(JoinError::Gone);
        }
        let tag = tag.filter(|t| is_tag(t)).map(str::to_owned);
        if tag.as_ref().is_some_and(|t| room.barred_tags.contains(t))
            || ip.is_some_and(|ip| room.barred_ips.contains(&ip))
        {
            return Err(JoinError::Barred);
        }
        let me = random_id(6);
        let (tx, inbox) = mpsc::channel(BACKLOG);
        let peers: Vec<Value> = room
            .members
            .iter()
            .map(|(id, m)| json!({ "id": id, "x": m.x, "s": m.s, "order": m.order }))
            .collect();
        let order = room.next_order;
        room.next_order += 1;
        if let Some(tag) = &tag
            && room.tags.len() < MAX_TAGS
        {
            room.tags.insert(tag.clone());
        }
        let joined =
            json!({ "type": "joined", "id": me, "x": x, "s": s, "order": order }).to_string();
        room.broadcast(&me, &joined);
        room.members.insert(
            me.clone(),
            Member {
                x,
                s,
                tx: tx.clone(),
                order,
                ip,
                tag,
            },
        );
        room.empty_since = None;
        let changed = room.elect();
        let welcome = json!({
            "type": "welcome",
            "you": me,
            "order": order,
            "admin": room.admin,
            "room": room.info(&self.signing_key()),
            "peers": peers,
        });
        let _ = tx.try_send(welcome.to_string());
        if changed {
            // Les autres apprennent le nouveau rôle (le nouvel arrivant le lit
            // dans `welcome`).
            let event = json!({ "type": "admin", "id": room.admin }).to_string();
            room.broadcast(&me, &event);
        }
        Ok(Joined { me, inbox })
    }

    /// Une connexion se ferme. Le salon reste, même vide, jusqu'à minuit :
    /// on peut y revenir (avec une nouvelle clé : l'ancienne est partie avec
    /// le dernier présent).
    pub fn leave(&self, id: &str, me: &str) {
        let mut rooms = self.rooms.lock().unwrap();
        let Some(room) = rooms.get_mut(id) else {
            return;
        };
        if room.members.remove(me).is_none() {
            return;
        }
        if room.members.is_empty() {
            room.admin = None;
            room.empty_since = Some(Instant::now());
        } else {
            room.broadcast(me, &json!({ "type": "left", "id": me }).to_string());
            if room.elect() {
                room.announce_admin();
            }
        }
    }

    /// L'administrateur exclut un membre : il est déconnecté, et ne peut plus
    /// revenir (sa marque, ou son adresse s'il n'a pas de compte) tant que le
    /// salon existe. Renvoie `false` si `by` n'est pas l'administrateur.
    pub fn expel(&self, id: &str, by: &str, target: &str) -> bool {
        let mut rooms = self.rooms.lock().unwrap();
        let Some(room) = rooms.get_mut(id) else {
            return false;
        };
        if room.admin.as_deref() != Some(by) || by == target {
            return false;
        }
        let Some(member) = room.members.remove(target) else {
            return false;
        };
        match (&member.tag, member.ip) {
            (Some(tag), _) => {
                room.barred_tags.insert(tag.clone());
            }
            (None, Some(ip)) => {
                room.barred_ips.insert(ip);
            }
            (None, None) => {}
        }
        let closed = json!({ "type": "closed", "reason": "expelled" }).to_string();
        let _ = member.tx.try_send(closed);
        room.broadcast(target, &json!({ "type": "left", "id": target }).to_string());
        true
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

    /// Ferme les connexions des adresses bloquées : elles sont prévenues
    /// (si leur file n'est pas pleine) et retirées de leur salon, ce qui
    /// coupe leur relais même si elles ne lisent plus rien.
    pub fn kick(&self, banned: impl Fn(IpAddr) -> bool) {
        let closed = json!({ "type": "closed", "reason": "banned" }).to_string();
        let mut rooms = self.rooms.lock().unwrap();
        for room in rooms.values_mut() {
            let gone: Vec<String> = room
                .members
                .iter()
                .filter(|(_, m)| m.ip.is_some_and(&banned))
                .map(|(id, _)| id.clone())
                .collect();
            for id in gone {
                if let Some(m) = room.members.remove(&id) {
                    let _ = m.tx.try_send(closed.clone());
                }
                room.broadcast(&id, &json!({ "type": "left", "id": id }).to_string());
            }
            if room.members.is_empty() {
                if room.empty_since.is_none() {
                    room.admin = None;
                    room.empty_since = Some(Instant::now());
                }
            } else if room.elect() {
                room.announce_admin();
            }
        }
    }

    /// Connexions ouvertes depuis une adresse, tous salons confondus.
    pub fn connections_from(&self, ip: IpAddr) -> usize {
        let rooms = self.rooms.lock().unwrap();
        rooms
            .values()
            .flat_map(|r| r.members.values())
            .filter(|m| m.ip == Some(ip))
            .count()
    }

    /// Remet la clé du salon (emballée pour lui seul) à un nouvel arrivant.
    pub fn hand_key(&self, id: &str, from: &str, to: &str, blob: &str) {
        let rooms = self.rooms.lock().unwrap();
        let Some(room) = rooms.get(id) else { return };
        let event = json!({ "type": "key", "from": from, "blob": blob }).to_string();
        room.send(to, &event);
    }

    /// Ferme les salons arrivés à minuit (tout le monde en est sorti) et ceux
    /// que personne n'a jamais rejoints.
    pub fn sweep(&self) {
        let now = now_ms();
        let mut rooms = self.rooms.lock().unwrap();
        rooms.retain(|_, room| {
            if room.expires_at <= now {
                let closed = json!({ "type": "closed", "reason": "midnight" }).to_string();
                room.broadcast("", &closed);
                return false;
            }
            room.alive(now)
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
    fn a_room_lives_until_midnight_even_empty() {
        let rooms = Rooms::default();
        let room = rooms.create("Café", false, 120, 1).unwrap();
        let id = room["id"].as_str().unwrap().to_owned();
        // Le même nom public mène au même salon.
        assert_eq!(rooms.create("cafe", false, 0, 1).unwrap()["id"], id);
        assert_eq!(rooms.list("CAFÉ").len(), 1);
        assert!(rooms.list("thé").is_empty());

        let mut ada = rooms.join(&id, "xa".into(), None, None, None).unwrap();
        let welcome = next(&mut ada);
        assert_eq!(welcome["type"], "welcome");
        assert!(welcome["peers"].as_array().unwrap().is_empty());

        let mut bob = rooms.join(&id, "xb".into(), None, None, None).unwrap();
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
        let mut eve = rooms.join(&id, "xe".into(), None, None, None).unwrap();
        assert_eq!(next(&mut eve)["type"], "welcome");
        assert!(eve.inbox.try_recv().is_err());

        rooms.leave(&id, &eve.me);
        rooms.leave(&id, &bob.me);
        assert_eq!(next(&mut ada)["type"], "joined");
        assert_eq!(next(&mut ada)["type"], "left");
        assert_eq!(rooms.count(), (1, 1));
        // Le dernier parti, le salon reste jusqu'à minuit : on y revient.
        rooms.leave(&id, &ada.me);
        assert_eq!(rooms.count(), (1, 0));
        rooms.sweep();
        assert_eq!(rooms.known(&[id.clone(), "inconnu".into()]).len(), 1);
        let mut back = rooms.join(&id, "x".into(), None, None, None).unwrap();
        let welcome = next(&mut back);
        assert!(welcome["peers"].as_array().unwrap().is_empty());
        assert_eq!(welcome["admin"], back.me.as_str());
        // À minuit, il disparaît.
        rooms.expire_now(&id);
        rooms.sweep();
        assert_eq!(rooms.count(), (0, 0));
        assert!(rooms.known(std::slice::from_ref(&id)).is_empty());
    }

    #[test]
    fn signing_keys_are_relayed_with_each_member() {
        let rooms = Rooms::default();
        let id = room_id(&rooms);
        let mut ada = rooms
            .join(&id, "xa".into(), Some("sa".into()), None, None)
            .unwrap();
        next(&mut ada);
        let mut bob = rooms
            .join(&id, "xb".into(), Some("sb".into()), None, None)
            .unwrap();
        assert_eq!(next(&mut bob)["peers"][0]["s"], "sa");
        assert_eq!(next(&mut ada)["s"], "sb");
    }

    #[test]
    fn an_empty_room_gives_its_place_when_the_server_is_full() {
        let rooms = Rooms::default();
        let first = room_id(&rooms);
        let a = rooms.join(&first, "x".into(), None, None, None).unwrap();
        rooms.leave(&first, &a.me);
        // Jamais rejoint : un salon ne dure qu'une minute.
        let unjoined = rooms.create("Personne", true, 0, 1).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
        rooms
            .rooms
            .lock()
            .unwrap()
            .get_mut(&unjoined)
            .unwrap()
            .created -= UNJOINED_TTL;
        rooms.sweep();
        assert!(rooms.info(&unjoined).is_none());
        for i in 1..MAX_ROOMS {
            rooms.create(&format!("Salon {i}"), true, 0, 1).unwrap();
        }
        assert_eq!(rooms.count().0, MAX_ROOMS);
        // Plein : le salon vide cède sa place ; ensuite, plus rien.
        assert!(rooms.create("Encore un", true, 0, 1).is_some());
        assert!(rooms.info(&first).is_none());
        assert!(rooms.create("De trop", true, 0, 1).is_none());
    }

    #[test]
    fn a_tag_finds_its_rooms_again() {
        let rooms = Rooms::default();
        let tag = "ab".repeat(32);
        let secret = rooms.create("Secret", true, 0, 1).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let public = rooms.create("Place", false, 0, 1).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let _a = rooms
            .join(&secret, "xa".into(), None, None, Some(&tag))
            .unwrap();
        let _b = rooms.join(&public, "xb".into(), None, None, None).unwrap();
        let mine = rooms.mine(&tag);
        assert_eq!(mine.len(), 1);
        assert_eq!(mine[0]["id"], secret.as_str());
        assert!(rooms.mine(&"cd".repeat(32)).is_empty());
        assert!(!is_tag("pas une marque"));
    }

    /// Vide la boîte d'une connexion et renvoie le dernier administrateur
    /// annoncé (`welcome` ou `admin`).
    fn admin_seen(j: &mut Joined) -> Option<String> {
        let mut admin = None;
        while let Ok(raw) = j.inbox.try_recv() {
            let e: Value = serde_json::from_str(&raw).unwrap();
            if e["type"] == "welcome" || e["type"] == "admin" {
                admin = e[if e["type"] == "welcome" {
                    "admin"
                } else {
                    "id"
                }]
                .as_str()
                .map(str::to_owned);
            }
        }
        admin
    }

    fn room_id(rooms: &Rooms) -> String {
        rooms.create("Club", false, 0, 1).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    #[test]
    fn an_owner_with_an_account_stays_admin() {
        let rooms = Rooms::default();
        let id = room_id(&rooms);
        let ada_tag = "a".repeat(64);
        let mut ada = rooms
            .join(&id, "xa".into(), None, None, Some(&ada_tag))
            .unwrap();
        assert_eq!(admin_seen(&mut ada), Some(ada.me.clone()), "le créateur");
        let mut bob = rooms.join(&id, "xb".into(), None, None, None).unwrap();
        let mut eve = rooms.join(&id, "xe".into(), None, None, None).unwrap();
        assert_eq!(admin_seen(&mut bob), Some(ada.me.clone()));
        // Ada part : le plus ancien présent (Bob) tient le rôle.
        rooms.leave(&id, &ada.me);
        assert_eq!(admin_seen(&mut eve), Some(bob.me.clone()));
        // Ada revient avec son compte : elle redevient administratrice.
        let mut ada2 = rooms
            .join(&id, "xa2".into(), None, None, Some(&ada_tag))
            .unwrap();
        assert_eq!(admin_seen(&mut ada2), Some(ada2.me.clone()));
        assert_eq!(admin_seen(&mut bob), Some(ada2.me.clone()));
    }

    #[test]
    fn an_owner_without_an_account_comes_back_as_a_member() {
        let rooms = Rooms::default();
        let id = room_id(&rooms);
        let ada = rooms.join(&id, "xa".into(), None, None, None).unwrap();
        let mut bob = rooms.join(&id, "xb".into(), None, None, None).unwrap();
        rooms.leave(&id, &ada.me);
        assert_eq!(admin_seen(&mut bob), Some(bob.me.clone()), "le plus ancien");
        let mut ada2 = rooms.join(&id, "xa2".into(), None, None, None).unwrap();
        assert_eq!(
            admin_seen(&mut ada2),
            Some(bob.me.clone()),
            "Ada est membre"
        );
    }

    #[test]
    fn the_admin_expels_and_the_expelled_cannot_return() {
        let rooms = Rooms::default();
        let id = room_id(&rooms);
        let ip: IpAddr = "198.51.100.9".parse().unwrap();
        let ada = rooms
            .join(&id, "xa".into(), None, None, Some(&"a".repeat(64)))
            .unwrap();
        let mut bob = rooms.join(&id, "xb".into(), None, Some(ip), None).unwrap();
        let eve_tag = "e".repeat(64);
        let eve = rooms
            .join(&id, "xe".into(), None, None, Some(&eve_tag))
            .unwrap();
        // Seul l'administrateur exclut.
        assert!(!rooms.expel(&id, &bob.me, &eve.me));
        assert!(rooms.expel(&id, &ada.me, &bob.me));
        let mut last = Value::Null;
        while let Ok(raw) = bob.inbox.try_recv() {
            last = serde_json::from_str(&raw).unwrap();
        }
        assert_eq!(last["reason"], "expelled");
        assert_eq!(
            rooms.join(&id, "xb2".into(), None, Some(ip), None).err(),
            Some(JoinError::Barred)
        );
        assert!(rooms.expel(&id, &ada.me, &eve.me));
        assert_eq!(
            rooms
                .join(&id, "xe2".into(), None, None, Some(&eve_tag))
                .err(),
            Some(JoinError::Barred)
        );
        assert_eq!(rooms.count(), (1, 1));
    }

    #[test]
    fn a_banned_address_is_disconnected() {
        let rooms = Rooms::default();
        let id = rooms.create("Place", false, 0, 1).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let bad: IpAddr = "203.0.113.7".parse().unwrap();
        let mut eve = rooms.join(&id, "xe".into(), None, Some(bad), None).unwrap();
        let mut ada = rooms
            .join(
                &id,
                "xa".into(),
                None,
                Some("198.51.100.1".parse().unwrap()),
                None,
            )
            .unwrap();
        next(&mut eve);
        next(&mut eve);
        next(&mut ada);
        rooms.kick(|ip| ip == bad);
        assert_eq!(next(&mut eve)["reason"], "banned");
        // Ève est retirée du salon : Ada la voit partir.
        let left = next(&mut ada);
        assert_eq!(
            (left["type"].as_str(), left["id"].as_str()),
            (Some("left"), Some(eve.me.as_str()))
        );
        assert_eq!(rooms.count(), (1, 1));
    }

    #[test]
    fn midnight_closes_the_room_for_everyone() {
        let rooms = Rooms::default();
        let id = rooms.create("Nuit", true, 0, 1).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(id.len(), 32, "un salon privé a un identifiant long");
        assert!(
            rooms.list("nuit").is_empty(),
            "un salon privé n'est pas listé"
        );
        let mut ada = rooms.join(&id, "xa".into(), None, None, None).unwrap();
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
