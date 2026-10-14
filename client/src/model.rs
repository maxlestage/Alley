//! L'état de l'application — en mémoire seulement. Fermer l'onglet, c'est
//! tout oublier : pseudonyme, messages, clés. L'appareil ne retient que la
//! liste des salons créés ou visités, jusqu'à leur minuit ([`crate::recent`]).

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use serde_json::{Value, json};
use web_sys::WebSocket;

use crate::call::CallRef;
use crate::crypto::{self, Identity, Key, Rejected, Replay, Signer};

/// Quelqu'un d'autre dans le salon.
#[derive(Clone, Default)]
pub struct Peer {
    /// Sa clé publique X25519 (base64), pour lui remettre la clé du salon.
    pub x: String,
    /// Sa clé publique Ed25519 (base64) : tout ce qu'il envoie doit en être
    /// signé.
    pub s: Option<String>,
    /// Les numéros de ses envois déjà reçus : un numéro déjà vu est un envoi
    /// rejoué.
    pub seen: Replay,
    /// Un envoi de lui a déjà été écarté (on ne prévient qu'une fois).
    pub warned: bool,
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
    /// Une photo à moi que le serveur vérifie encore.
    pub pending: bool,
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
    /// La clé qui signe tout ce que j'envoie, jetable elle aussi.
    pub signer: Signer,
    /// Le numéro de mon prochain envoi.
    pub sent: Cell<u64>,
    /// La clé du salon : tirée par le premier arrivant, remise aux suivants.
    pub key: Option<Key>,
    pub fingerprint: String,
    pub peers: BTreeMap<String, Peer>,
    pub msgs: Vec<Msg>,
    /// Ce qui est arrivé avant la clé.
    pub pending: Vec<Value>,
    pub socket: Option<Socket>,
    /// Le jour local d'arrivée : à minuit, tout s'efface.
    pub day: (u32, u32, u32),
    pub leaving: bool,
    /// La connexion qui administre le salon (désignée par le serveur).
    pub admin: Option<String>,
    pub retries: u32,
}

pub type RoomRef = Rc<RefCell<Room>>;

impl Room {
    /// Chiffre et signe un envoi (pour `to` seul, ou pour tout le salon).
    pub fn enc(&self, to: Option<&str>, payload: &Value) -> Option<String> {
        let key = self.key?;
        let n = self.sent.get() + 1;
        self.sent.set(n);
        Some(crypto::seal_signed(
            &key,
            &self.signer,
            &self.id,
            to,
            n,
            &payload.to_string(),
        ))
    }

    /// Ouvre un envoi de `from` : seulement s'il est signé de sa clé, pour
    /// ce salon et pour moi, et jamais vu.
    pub fn dec(&mut self, from: &str, token: &str) -> Result<Value, Rejected> {
        let key = self.key.ok_or(Rejected::Unreadable)?;
        let peer = self.peers.get(from).ok_or(Rejected::Unsigned)?;
        let (body, n) = crypto::open_signed(
            &key,
            token,
            peer.s.as_deref(),
            &self.id,
            &self.me,
            &peer.seen,
        )?;
        let value = serde_json::from_str(&body).map_err(|_| Rejected::Forged)?;
        if let Some(peer) = self.peers.get_mut(from) {
            peer.seen.mark(n);
        }
        Ok(value)
    }

    pub fn set_key(&mut self, key: Key) {
        self.fingerprint = crypto::fingerprint(&key);
        self.key = Some(key);
    }

    pub fn name_of(&self, id: &str) -> String {
        self.peers
            .get(id)
            .and_then(|p| p.name.clone())
            .unwrap_or_else(|| {
                crate::i18n::fill(crate::i18n::t().guest_name, &[&id[..id.len().min(4)]])
            })
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

    /// `from` peut-il me remettre la clé ? Seulement s'il est parmi les
    /// trois plus anciens arrivés avant moi (ceux qui la remettent).
    pub fn may_hand_key(&self, from: &str) -> bool {
        let mut older: Vec<(u64, &String)> = self
            .peers
            .iter()
            .filter(|(_, p)| p.order < self.order)
            .map(|(id, p)| (p.order, id))
            .collect();
        older.sort_unstable();
        older.iter().take(3).any(|(_, id)| id.as_str() == from)
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

    /// Suis-je l'administrateur du salon ?
    pub fn i_am_admin(&self) -> bool {
        self.admin.as_deref() == Some(self.me.as_str()) && !self.me.is_empty()
    }

    pub fn present(&self) -> usize {
        self.peers.len() + 1
    }
}

/// Un compte : pseudonyme + mot de passe. Le navigateur garde le mot de
/// passe (son gestionnaire de mots de passe) ; l'application n'en garde que
/// la marque, le temps de la visite.
#[derive(Clone)]
pub struct Account {
    pub tag: String,
    /// Le secret d'abonnement ([`crate::crypto::billing_secret`]).
    pub billing: String,
}

/// Toute l'application.
#[derive(Default)]
pub struct App {
    /// Le pseudonyme, le temps de la visite.
    pub name: String,
    /// Le compte, si l'on s'est connecté avec un mot de passe.
    pub account: Option<Account>,
    /// Les salons ouverts de ce compte.
    pub mine: Vec<Value>,
    /// Les salons dont se souvient l'appareil, encore ouverts sur ce serveur.
    pub known: Vec<Value>,
    /// Les formules d'abonnement du serveur (`GET /api/v1/plans`).
    pub plans: Option<Value>,
    /// La période payée du compte connecté (`POST /api/v1/billing/status`).
    pub pass: Option<Value>,
    /// De retour du paiement Stripe (`#periode=<session>`), et l'attente de
    /// sa confirmation déjà annoncée.
    pub paid: bool,
    pub paid_waiting: bool,
    pub paid_session: Option<String>,
    /// Le paiement Stripe de la période du compte, gardé par l'appareil.
    pub pass_id: Option<String>,
    /// La clé publique de chaque serveur, qui vérifie ce qu'il dit des salons.
    pub server_keys: std::collections::HashMap<String, String>,
    /// Bloqué par la modération : plus rien n'est possible.
    pub banned: bool,
    /// Une adresse de serveur saisie à la main (retenue par l'appareil).
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
