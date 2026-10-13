//! L'état de l'application — en mémoire seulement : **rien n'est enregistré**
//! sur l'appareil. Fermer l'onglet, c'est tout oublier : pseudonyme, salon,
//! messages, clés.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use serde_json::{Value, json};
use web_sys::WebSocket;

use crate::call::CallRef;
use crate::crypto::{self, Identity, Key};

/// Quelqu'un d'autre dans le salon.
#[derive(Clone, Default)]
pub struct Peer {
    /// Sa clé publique X25519 (base64), pour lui remettre la clé du salon.
    pub x: String,
    /// Son rang d'arrivée : les trois plus anciens remettent la clé.
    pub order: u64,
    /// Son pseudonyme, qui arrive chiffré.
    pub name: Option<String>,
    pub voice: bool,
    pub muted: bool,
    pub video: bool,
    pub typing_until: f64,
}

/// Un message reçu depuis qu'on est arrivé (les précédents n'existent plus
/// nulle part).
#[derive(Clone, Default)]
pub struct Msg {
    pub mid: String,
    pub from: String,
    pub name: String,
    pub t: f64,
    pub kind: String,
    pub text: String,
    pub media: String,
    pub reply: String,
    pub edited: bool,
    pub mine: bool,
}

/// La connexion temps réel au salon, avec ses écouteurs.
pub struct Socket {
    pub ws: WebSocket,
    pub _listeners: Vec<wasm_bindgen::closure::Closure<dyn FnMut(wasm_bindgen::JsValue)>>,
}

/// Le salon où l'on est.
pub struct Room {
    pub id: String,
    pub name: String,
    pub private: bool,
    pub expires_at: f64,
    /// Mon identifiant de connexion (donné par le serveur), et mon rang.
    pub me: String,
    pub order: u64,
    /// Une identité X25519 jetable, le temps du salon.
    pub identity: Identity,
    /// La clé du salon : tirée par le premier arrivant, remise aux suivants.
    pub key: Option<Key>,
    pub fingerprint: String,
    pub peers: BTreeMap<String, Peer>,
    pub msgs: Vec<Msg>,
    /// Ce qui est arrivé avant la clé.
    pub pending: Vec<(String, String)>,
    pub socket: Option<Socket>,
    /// Le jour local d'arrivée : à minuit, tout s'efface.
    pub day: (u32, u32, u32),
    pub leaving: bool,
    pub retries: u32,
}

pub type RoomRef = Rc<RefCell<Room>>;

impl Room {
    pub fn enc(&self, payload: &Value) -> Option<String> {
        self.key
            .map(|k| crypto::seal_token(&k, &payload.to_string()))
    }

    pub fn dec(&self, token: &str) -> Option<Value> {
        let key = self.key?;
        serde_json::from_str(&crypto::open_token(&key, token)?).ok()
    }

    pub fn set_key(&mut self, key: Key) {
        self.fingerprint = crypto::fingerprint(&key);
        self.key = Some(key);
    }

    pub fn name_of(&self, id: &str) -> String {
        self.peers
            .get(id)
            .and_then(|p| p.name.clone())
            .unwrap_or_else(|| format!("Invité {}", &id[..id.len().min(4)]))
    }

    /// Suis-je parmi les trois plus anciens (ceux qui remettent la clé) ?
    pub fn key_holder(&self, except: &str) -> bool {
        let mut orders: Vec<u64> = self
            .peers
            .iter()
            .filter(|(id, _)| id.as_str() != except)
            .map(|(_, p)| p.order)
            .collect();
        orders.push(self.order);
        orders.sort_unstable();
        orders.iter().take(3).any(|o| *o == self.order)
    }

    pub fn voice_members(&self, me_in_call: bool) -> Vec<String> {
        let mut ids: Vec<String> = Vec::new();
        if me_in_call {
            ids.push(self.me.clone());
        }
        ids.extend(
            self.peers
                .iter()
                .filter(|(_, p)| p.voice)
                .map(|(id, _)| id.clone()),
        );
        ids
    }

    pub fn present(&self) -> usize {
        self.peers.len() + 1
    }
}

/// Toute l'application.
#[derive(Default)]
pub struct App {
    /// Le pseudonyme, le temps de la visite.
    pub name: String,
    /// Une adresse de serveur saisie à la main (jamais enregistrée).
    pub server: String,
    pub rooms: Vec<Value>,
    pub query: String,
    pub private: bool,
    pub room: Option<RoomRef>,
    pub call: Option<CallRef>,
    pub reply_to: Option<String>,
    pub editing: Option<String>,
    pub show_members: bool,
    pub ice: Option<Value>,
    pub recording: Option<web_sys::MediaRecorder>,
    /// Le salon d'un lien (`#salon=…`), à rejoindre une fois le pseudonyme choisi.
    pub invited: Option<String>,
    pub base: String,
    pub last_typing: f64,
    pub unread: u32,
}

thread_local! {
    static APP: RefCell<App> = RefCell::new(App::default());
}

/// Accède à l'état (jamais à travers un `.await`).
pub fn app<R>(f: impl FnOnce(&mut App) -> R) -> R {
    APP.with(|a| f(&mut a.borrow_mut()))
}

pub fn room() -> Option<RoomRef> {
    app(|a| a.room.clone())
}

pub fn base() -> String {
    app(|a| a.base.clone())
}

/// Un message chiffré à envoyer (`k` : son genre).
pub fn payload(kind: &str, mut fields: Value) -> Value {
    fields["k"] = json!(kind);
    fields
}
