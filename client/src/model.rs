//! L'état de l'application et ce que garde l'appareil.
//!
//! localStorage « alley.v3 » : le pseudo, et pour chaque serveur son
//! trousseau de clés (la plus récente d'abord), le secret de membre,
//! l'identité X25519 (pour recevoir une clé renouvelée), la clé de signature
//! ECDSA et, pour son créateur, le secret d'admin.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use web_sys::WebSocket;

use crate::call::Call;
use crate::crypto::{self, Identity, Key, Signer};
use crate::dom::{storage_get, storage_set, toast};

const STORE: &str = "alley.v3";

/// Ce qu'on garde d'un serveur sur l'appareil.
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Entry {
    pub gid: String,
    /// Le trousseau, en base64url : la clé actuelle d'abord.
    pub keys: Vec<String>,
    pub member: String,
    pub admin: Option<String>,
    /// Secret X25519 (base64).
    pub idx: String,
    /// Secret ECDSA P-256 (base64).
    pub sig: String,
    pub channel: Option<String>,
    pub name: String,
}

impl Entry {
    pub fn new(key: &Key, gid: String, admin: Option<String>) -> Entry {
        Entry {
            gid,
            keys: vec![crypto::b64url(key)],
            member: crypto::secret(),
            admin,
            idx: Identity::generate().secret_b64(),
            sig: Signer::generate().secret_b64(),
            channel: None,
            name: String::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Saved {
    pub name: String,
    pub guilds: Vec<Entry>,
    pub notify: bool,
    pub last: Option<String>,
    pub server: String,
}

#[derive(Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct Channel {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub topic: String,
    #[serde(default = "text_kind")]
    pub kind: String,
}

fn text_kind() -> String {
    "text".into()
}

impl Channel {
    pub fn voice(&self) -> bool {
        self.kind == "voice"
    }
}

/// L'état chiffré d'un serveur : son nom, ses salons, ses anciennes clés.
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct GuildState {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub channels: Vec<Channel>,
    #[serde(default)]
    pub keys: Vec<String>,
}

/// Un profil de membre, déchiffré : pseudo et clés publiques.
#[derive(Clone)]
pub struct Profile {
    pub id: String,
    pub name: String,
    /// X25519 (base64), pour lui remettre une clé renouvelée.
    pub x: Option<String>,
    /// ECDSA (base64), pour vérifier ses signatures.
    pub s: Option<String>,
}

#[derive(Clone, Default)]
pub struct Presence {
    pub seen: f64,
    pub voice: Option<String>,
    pub muted: bool,
    pub video: bool,
}

/// Un message déchiffré.
#[derive(Clone, Default)]
pub struct Msg {
    pub id: i64,
    pub locked: bool,
    pub channel: String,
    pub author: String,
    pub t: f64,
    pub kind: String,
    pub text: String,
    pub media: String,
    pub reply: i64,
    pub salt: String,
    pub edited: bool,
    pub b: String,
    pub sig: String,
    /// `Some(true)` signé par son auteur, `Some(false)` falsifié, `None`
    /// auteur inconnu.
    pub verified: Option<bool>,
}

/// Une connexion temps réel ouverte, avec ses écouteurs.
pub struct Socket {
    pub ws: WebSocket,
    pub generation: u32,
    pub _listeners: Vec<wasm_bindgen::closure::Closure<dyn FnMut(wasm_bindgen::JsValue)>>,
}

/// Un serveur : ce qui est gardé, plus ce qui est chargé et déchiffré.
pub struct Guild {
    pub entry: Entry,
    pub gid: String,
    pub me: String,
    pub name: String,
    pub admin: bool,
    pub state: GuildState,
    pub revision: i64,
    pub epoch: i64,
    pub members: BTreeMap<String, Profile>,
    pub presence: HashMap<String, Presence>,
    pub msgs: BTreeMap<i64, Msg>,
    pub oldest: Option<i64>,
    pub more: bool,
    pub page_loaded: bool,
    pub loading_page: bool,
    pub loaded: bool,
    pub announced: bool,
    pub unread: HashMap<String, u32>,
    pub typing: HashMap<String, (String, f64)>,
    pub socket: Option<Socket>,
    pub generation: u32,
    pub retry: u32,
    pub adopting: bool,
    pub rotating: bool,
    pub refreshing: bool,
    pub joining: bool,
    pub excluded: bool,
    // Le trousseau, déjà décodé.
    pub key: Key,
    pub ring: Vec<Key>,
    pub access: String,
    pub fingerprint: String,
    pub identity: Identity,
    pub signer: Signer,
}

pub type G = Rc<RefCell<Guild>>;

impl Guild {
    pub fn new(entry: Entry) -> Option<Guild> {
        let identity = Identity::from_b64(&entry.idx)?;
        let signer = Signer::from_b64(&entry.sig)?;
        let mut g = Guild {
            gid: entry.gid.clone(),
            me: crypto::member_id(&entry.member),
            name: if entry.name.is_empty() {
                "Serveur".into()
            } else {
                entry.name.clone()
            },
            admin: false,
            state: GuildState::default(),
            revision: 0,
            epoch: 0,
            members: BTreeMap::new(),
            presence: HashMap::new(),
            msgs: BTreeMap::new(),
            oldest: None,
            more: true,
            page_loaded: false,
            loading_page: false,
            loaded: false,
            announced: false,
            unread: HashMap::new(),
            typing: HashMap::new(),
            socket: None,
            generation: 0,
            retry: 0,
            adopting: false,
            rotating: false,
            refreshing: false,
            joining: false,
            excluded: false,
            key: [0; 32],
            ring: Vec::new(),
            access: String::new(),
            fingerprint: String::new(),
            identity,
            signer,
            entry,
        };
        let keys = g.entry.keys.clone();
        g.set_keys(keys).then_some(g)
    }

    /// Le trousseau : la clé actuelle (chiffre), et toutes (déchiffrent).
    pub fn set_keys(&mut self, keys: Vec<String>) -> bool {
        let mut seen = Vec::new();
        for k in keys {
            if !seen.contains(&k) && crypto::key_from_b64url(&k).is_some() {
                seen.push(k);
            }
        }
        let Some(first) = seen.first().and_then(|k| crypto::key_from_b64url(k)) else {
            return false;
        };
        self.ring = seen
            .iter()
            .filter_map(|k| crypto::key_from_b64url(k))
            .collect();
        self.entry.keys = seen;
        self.key = first;
        let d = crypto::derive(&first);
        self.access = d.access;
        self.fingerprint = d.fingerprint;
        true
    }

    /// Ajoute les anciennes clés que porte l'état du serveur (pour l'historique).
    pub fn learn_keys(&mut self, keys: &[String]) -> bool {
        let fresh: Vec<String> = keys
            .iter()
            .filter(|k| !self.entry.keys.contains(k))
            .cloned()
            .collect();
        if fresh.is_empty() {
            return false;
        }
        let mut all = self.entry.keys.clone();
        all.extend(fresh);
        self.set_keys(all)
    }

    pub fn enc(&self, text: &str) -> String {
        crypto::seal_token(&self.key, text)
    }

    pub fn dec(&self, token: &str) -> Option<String> {
        crypto::open_with_any(&self.ring, token)
    }

    pub fn dec_json(&self, token: &str) -> Option<Value> {
        serde_json::from_str(&self.dec(token)?).ok()
    }

    /// Applique l'état chiffré renvoyé par le serveur.
    pub fn apply_state(&mut self, data: &Value) {
        let state: Option<GuildState> = data["state"]
            .as_str()
            .and_then(|t| self.dec(t))
            .and_then(|t| serde_json::from_str(&t).ok());
        match state {
            Some(mut st) => {
                st.channels.retain(|c| !c.id.is_empty());
                if st.name.is_empty() {
                    st.name = "Serveur".into();
                }
                let keys = st.keys.clone();
                self.state = st;
                self.learn_keys(&keys);
            }
            None => {
                self.state = GuildState {
                    name: "Serveur illisible".into(),
                    ..GuildState::default()
                };
            }
        }
        self.revision = data["revision"].as_i64().unwrap_or(self.revision);
        self.epoch = data["epoch"].as_i64().unwrap_or(self.epoch);
        self.name = self.state.name.clone();
        self.entry.name = self.name.clone();
        let valid = self
            .entry
            .channel
            .as_ref()
            .is_some_and(|c| self.state.channels.iter().any(|x| &x.id == c));
        if !valid {
            self.entry.channel = self
                .state
                .channels
                .iter()
                .find(|c| !c.voice())
                .or(self.state.channels.first())
                .map(|c| c.id.clone());
        }
    }

    /// Un profil doit parler de lui-même : sinon, c'est un collage du serveur.
    pub fn read_profile(&self, row: &Value) -> Option<Profile> {
        let id = row["id"].as_str()?;
        let p = self.dec_json(row["profile"].as_str()?)?;
        if p["id"].as_str() != Some(id) {
            return None;
        }
        Some(Profile {
            id: id.to_owned(),
            name: p["n"].as_str()?.chars().take(40).collect(),
            x: p["x"].as_str().map(str::to_owned),
            s: p["s"].as_str().map(str::to_owned),
        })
    }

    pub fn my_profile(&self, name: &str) -> String {
        let p = json!({
            "id": self.me,
            "n": name,
            "x": self.identity.public_b64(),
            "s": self.signer.public_b64(),
        });
        self.enc(&p.to_string())
    }

    /// Une enveloppe chiffrée et signée. Clair = `{"b": enveloppe, "g":
    /// signature, "p": remplissage}` ; la taille est arrondie au
    /// demi-kilo-octet pour ne pas trahir la longueur du message.
    pub fn seal(&self, mut env: Value) -> String {
        env["gid"] = self.gid.clone().into();
        env["a"] = self.me.clone().into();
        let b = env.to_string();
        let g = self.signer.sign(&b);
        let bare = json!({ "b": b, "g": g, "p": "" }).to_string().len();
        let pad = (512 - bare % 512) % 512;
        self.enc(&json!({ "b": b, "g": g, "p": " ".repeat(pad) }).to_string())
    }

    fn check(&self, author: &str, sig: &str, b: &str) -> Option<bool> {
        let key = self.members.get(author)?.s.as_ref()?;
        Some(crypto::verify(key, sig, b))
    }

    pub fn decrypt_message(&self, raw: &Value) -> Msg {
        let id = raw["id"].as_i64().unwrap_or_default();
        let locked = Msg {
            id,
            locked: true,
            ..Msg::default()
        };
        let Some(outer) = raw["body"].as_str().and_then(|t| self.dec_json(t)) else {
            return locked;
        };
        let (Some(b), Some(sig)) = (outer["b"].as_str(), outer["g"].as_str()) else {
            return locked;
        };
        let Ok(env) = serde_json::from_str::<Value>(b) else {
            return locked;
        };
        if env["gid"].as_str() != Some(self.gid.as_str()) {
            return locked;
        }
        let kind = match env["k"].as_str() {
            Some(k @ ("image" | "audio")) => k.to_owned(),
            _ => "text".to_owned(),
        };
        let x = env["x"].as_str().unwrap_or_default();
        let mime = env["m"].as_str().unwrap_or_default();
        let media = match kind.as_str() {
            "image" => format!("data:image/jpeg;base64,{x}"),
            "audio" => {
                let ok = mime.starts_with("audio/")
                    && mime[6..]
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || ".+-".contains(c));
                format!("data:{};base64,{x}", if ok { mime } else { "audio/mp4" })
            }
            _ => String::new(),
        };
        let author = env["a"].as_str().unwrap_or_default().to_owned();
        let mut m = Msg {
            id,
            locked: false,
            channel: env["c"].as_str().unwrap_or_default().to_owned(),
            t: env["t"].as_f64().unwrap_or_default(),
            text: if kind == "text" {
                x.to_owned()
            } else {
                String::new()
            },
            kind,
            media,
            reply: env["r"].as_i64().unwrap_or_default(),
            salt: env["s"].as_str().unwrap_or_default().to_owned(),
            edited: raw["edited"].as_bool().unwrap_or_default()
                || env["e"].as_f64().unwrap_or_default() > 0.0,
            b: b.to_owned(),
            sig: sig.to_owned(),
            verified: None,
            author,
        };
        m.verified = self.check(&m.author, &m.sig, &m.b);
        m
    }

    /// Revérifie les messages dont l'auteur vient d'être connu.
    pub fn verify_pending(&mut self) {
        let todo: Vec<(i64, String, String, String)> = self
            .msgs
            .values()
            .filter(|m| m.verified.is_none() && !m.locked)
            .map(|m| (m.id, m.author.clone(), m.sig.clone(), m.b.clone()))
            .collect();
        for (id, author, sig, b) in todo {
            let v = self.check(&author, &sig, &b);
            if let Some(m) = self.msgs.get_mut(&id) {
                m.verified = v;
            }
        }
    }

    /// Un message relayé (présence, « j'écris », signal), s'il est signé par
    /// un membre connu. `Err(())` : auteur inconnu (son profil arrive peut-être).
    pub fn open_relay(&self, token: &str) -> Result<Option<Value>, ()> {
        let Some(outer) = self.dec_json(token) else {
            return Ok(None);
        };
        let (Some(b), Some(sig)) = (outer["b"].as_str(), outer["g"].as_str()) else {
            return Ok(None);
        };
        let Ok(p) = serde_json::from_str::<Value>(b) else {
            return Ok(None);
        };
        let author = p["a"].as_str().unwrap_or_default();
        if author == self.me
            || (crate::dom::now() - p["t"].as_f64().unwrap_or_default()).abs() > 120_000.0
        {
            return Ok(None);
        }
        match self.check(author, sig, b) {
            None => Err(()),
            Some(true) => Ok(Some(p)),
            Some(false) => Ok(None),
        }
    }

    pub fn relay_token(&self, mut payload: Value) -> String {
        payload["a"] = self.me.clone().into();
        payload["t"] = crate::dom::now().into();
        let b = payload.to_string();
        let g = self.signer.sign(&b);
        self.enc(&json!({ "b": b, "g": g }).to_string())
    }

    pub fn proof(&self, salt: &str) -> String {
        crypto::sha256_hex(&format!("{}:{salt}", self.entry.member))
    }

    pub fn channel(&self, id: &str) -> Option<&Channel> {
        self.state.channels.iter().find(|c| c.id == id)
    }

    pub fn current_channel(&self) -> Option<&Channel> {
        self.channel(self.entry.channel.as_deref()?)
    }

    pub fn channel_messages(&self, cid: &str) -> Vec<Msg> {
        self.msgs
            .values()
            .filter(|m| m.channel == cid && !m.locked)
            .cloned()
            .collect()
    }

    pub fn name_of(&self, id: &str) -> String {
        self.members
            .get(id)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| format!("Membre {}", &id[..id.len().min(4)]))
    }

    pub fn online(&self, id: &str) -> bool {
        id == self.me
            || self
                .presence
                .get(id)
                .is_some_and(|p| crate::dom::now() - p.seen < 70_000.0)
    }

    pub fn unread_total(&self) -> u32 {
        self.unread.values().sum()
    }

    pub fn invite_link(&self, origin: &str, base: &str, with_admin: bool) -> String {
        let mut link = format!(
            "{origin}{base}/app/#rejoindre={}&g={}",
            self.entry.keys[0], self.gid
        );
        if with_admin && let Some(a) = &self.entry.admin {
            link.push_str(&format!("&admin={a}"));
        }
        link
    }
}

/// L'appel en cours.
pub struct ActiveCall {
    pub gid: String,
    pub channel: String,
    pub call: Rc<RefCell<Call>>,
}

/// Toute l'application.
#[derive(Default)]
pub struct App {
    pub saved: Saved,
    pub current: Option<String>,
    pub guilds: Vec<G>,
    pub reply_to: Option<i64>,
    pub editing: Option<i64>,
    pub show_members: bool,
    pub drawer: bool,
    pub call: Option<ActiveCall>,
    pub ice: Option<Value>,
    pub recording: Option<web_sys::MediaRecorder>,
    pub pending_invite: Option<String>,
    pub base: String,
    pub last_typing: f64,
}

thread_local! {
    static APP: RefCell<App> = RefCell::new(App::default());
}

/// Accède à l'état (jamais à travers un `.await`).
pub fn app<R>(f: impl FnOnce(&mut App) -> R) -> R {
    APP.with(|a| f(&mut a.borrow_mut()))
}

pub fn load() -> Saved {
    storage_get(STORE)
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save() {
    let data = app(|a| {
        let mut saved = a.saved.clone();
        saved.guilds = a.guilds.iter().map(|g| g.borrow().entry.clone()).collect();
        saved.last = a.current.clone();
        serde_json::to_string(&saved).unwrap_or_default()
    });
    if !storage_set(STORE, &data) {
        toast(
            "Impossible d'enregistrer sur cet appareil (navigation privée ?)",
            true,
        );
    }
}

pub fn guild(gid: &str) -> Option<G> {
    app(|a| a.guilds.iter().find(|g| g.borrow().gid == gid).cloned())
}

pub fn current_guild() -> Option<G> {
    let gid = app(|a| a.current.clone())?;
    guild(&gid)
}

pub fn is_current(gid: &str) -> bool {
    app(|a| a.current.as_deref() == Some(gid))
}

pub fn base() -> String {
    app(|a| a.base.clone())
}
