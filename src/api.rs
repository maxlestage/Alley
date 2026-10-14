//! L'API d'Alley, sous `/api/v1` :
//!
//! | Route | Rôle |
//! |---|---|
//! | `GET /rooms?q=` | les salons publics ouverts, filtrés par nom |
//! | `POST /rooms` | crée un salon (`{name, private, tz}`) ou rejoint celui de ce nom |
//! | `GET /rooms/:id` | un salon (pour un lien de salon privé) |
//! | `POST /mine` | les salons ouverts d'une marque (`{tag}`, compte connecté) |
//! | `POST /known` | parmi des salons (`{ids, rooms}`, ceux dont se souvient l'appareil), les ouverts ; rouvre ceux qu'il a oubliés, sur présentation de leur description signée |
//! | `GET /rooms/:id/live?x=&s=&t=` | le salon en direct (WebSocket) |
//! | `GET /key` | la clé publique Ed25519 qui vérifie ce que le serveur dit des salons |
//! | `GET /ice` | les serveurs STUN/TURN des appels |
//! | `GET /stats` | ce que voit le serveur : des nombres |
//!
//! En direct, le client envoie :
//!
//! - `{"type":"relay","data":"[[BROCOLIS:v1:…]]","to":?}` : un jeton chiffré
//!   pour tous les autres, ou pour une seule personne (signalisation d'appel) ;
//! - `{"type":"key","to":…,"blob":…}` : la clé du salon, emballée pour un
//!   nouvel arrivant avec sa clé publique X25519 ;
//! - `{"type":"image","id":…,"key":…,"c":…,"data":…}` : une image, que le
//!   serveur vérifie avant de la relayer ([`crate::moderation`]) ;
//! - `{"type":"expel","to":…}` : l'administrateur exclut un membre.
//!
//! Le serveur répond par `welcome` (avec `admin`), `admin` (l'administrateur
//! a changé), `joined`, `left`, `relay` (avec `from`),
//! `key`, `sent` / `moderated` (le sort d'une image) et `closed`. Il ne garde
//! rien.
//!
//! Les erreurs sont `{"error": code}` : le navigateur les traduit.

use std::time::{Duration, Instant};

use paquets::prelude::*;
use serde_json::Value;

use crate::moderation::{Moderation, Verdict, check_text};
use crate::protocol::{
    MAX_TOKEN, WRAPPED_KEY_LEN, is_b64_len, is_token, key_commitment, key_from_b64, open_token,
    public_key_from_url,
};
use crate::rooms::{self, JoinError, Rooms, is_tag};

/// Une erreur que le navigateur traduit : `{"error": code}`.
fn fail(status: u16, code: &str) -> (u16, Json<Value>) {
    (status, Json(json!({ "error": code })))
}

#[data]
pub struct NewRoom {
    pub name: String,
    #[serde(default)]
    pub private: bool,
    /// Le décalage horaire du créateur, en minutes (`+120` à Paris l'été) :
    /// le salon disparaît à minuit chez lui.
    #[serde(default)]
    pub tz: i64,
    /// Le secret d'abonnement du compte ([`crate::billing`]) et l'abonnement
    /// que connaît le navigateur : ses salons durent 3 ou 7 jours.
    #[serde(default)]
    pub billing: Option<String>,
    #[serde(default)]
    pub subscription: Option<String>,
}

/// Jetons relayés par connexion, au plus, par fenêtre de 10 s (contre
/// l'inondation ; la signalisation d'un appel en envoie une rafale).
const RELAY_BURST: u32 = 200;

/// Connexions en direct depuis une même adresse, au plus (une famille, un
/// bureau derrière la même box…).
const MAX_CONNECTIONS_PER_IP: usize = 16;

/// `GET /api/v1/rooms?q=`
async fn list(req: Request) -> Json<Value> {
    let rooms = req.state::<Rooms>();
    rooms.sweep();
    let q = req.query("q").map(|q| q.into_owned()).unwrap_or_default();
    Json(json!({ "rooms": rooms.list(&q) }))
}

#[data]
pub struct Known {
    pub ids: Vec<String>,
    /// Ce que le serveur a dit de ces salons, signé : s'il les a oubliés
    /// (redémarrage), il les rouvre ([`Rooms::revive`]).
    #[serde(default)]
    pub rooms: Vec<Signed>,
}

/// Ce que le serveur a dit d'un salon, avec sa signature.
#[data]
pub struct Signed {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub private: bool,
    pub expires_at: i64,
    pub sig: String,
}

#[data]
pub struct Mine {
    pub tag: String,
}

/// `POST /api/v1/rooms`
async fn create(req: Request) -> (u16, Json<Value>) {
    let Ok(body) = req.json::<NewRoom>().await else {
        return fail(400, "bad_request");
    };
    let rooms = req.state::<Rooms>();
    rooms.sweep();
    if rooms::clean_name(&body.name).is_none() {
        return fail(400, "bad_name");
    }
    // Le nom d'un salon est public : il ne peut pas être sexuel ni parler
    // de drogue.
    if let Some(category) = check_text(&body.name) {
        return (
            422,
            Json(json!({ "error": "forbidden_name", "reason": category.code() })),
        );
    }
    let days = req
        .state::<crate::billing::Billing>()
        .days_for(body.billing.as_deref(), body.subscription.as_deref())
        .await;
    match rooms.create(&body.name, body.private, body.tz, days) {
        Some(room) => {
            // Un salon de plusieurs jours survit aux redémarrages.
            if let Some(kept) = room["id"].as_str().and_then(|id| rooms.kept(id))
                && let Err(e) = crate::billing::keep(req.db(), kept).await
            {
                tracing::warn!("salon de plusieurs jours non gardé : {e}");
            }
            (201, Json(room))
        }
        None => fail(503, "too_many_rooms"),
    }
}

/// `POST /api/v1/mine` : les salons ouverts d'une marque.
async fn mine(req: Request) -> (u16, Json<Value>) {
    match req.json::<Mine>().await {
        Ok(body) if is_tag(&body.tag) => {
            let rooms = req.state::<Rooms>();
            rooms.sweep();
            (200, Json(json!({ "rooms": rooms.mine(&body.tag) })))
        }
        _ => fail(400, "bad_request"),
    }
}

/// `POST /api/v1/known` : parmi les salons dont se souvient un appareil,
/// ceux qui sont encore ouverts.
async fn known(req: Request) -> (u16, Json<Value>) {
    match req.json::<Known>().await {
        Ok(body) if body.ids.len() <= rooms::MAX_KNOWN && body.rooms.len() <= rooms::MAX_KNOWN => {
            let rooms = req.state::<Rooms>();
            rooms.sweep();
            for r in &body.rooms {
                let kept = rooms::Kept {
                    id: r.id.clone(),
                    name: r.name.clone(),
                    private: r.private,
                    expires_at: r.expires_at,
                };
                rooms.revive(&kept, &r.sig);
            }
            (200, Json(json!({ "rooms": rooms.known(&body.ids) })))
        }
        _ => fail(400, "bad_request"),
    }
}

/// `GET /api/v1/rooms/:id`
async fn show(req: Request) -> (u16, Json<Value>) {
    let id = req.param("id").unwrap_or_default();
    let rooms = req.state::<Rooms>();
    rooms.sweep();
    match rooms.info(id) {
        Some(room) => (200, Json(room)),
        None => fail(404, "room_gone"),
    }
}

/// `GET /api/v1/key` : la clé qui vérifie la signature des salons (`sig`).
async fn key(req: Request) -> Json<Value> {
    Json(json!({ "alg": "Ed25519", "key": req.state::<Rooms>().public_key() }))
}

/// `GET /api/v1/stats` : ce que le serveur voit.
async fn stats(req: Request) -> Json<Value> {
    let (rooms, people) = req.state::<Rooms>().count();
    Json(json!({
        "rooms": rooms,
        "connections": people,
        "messages_stored": 0,
        "pseudonyms_known": 0,
    }))
}

/// `GET /api/v1/ice` : les serveurs STUN/TURN des appels. Par défaut des
/// STUN publics ; `ALLEY_ICE_SERVERS` (JSON, format `RTCIceServer[]`) en
/// ajoute, typiquement un TURN pour les réseaux qui bloquent le pair-à-pair.
/// Un TURN ne relaie que du média chiffré (DTLS-SRTP) : il ne l'écoute pas.
async fn ice(_req: Request) -> Json<Value> {
    let mut servers = vec![json!({
        "urls": ["stun:stun.cloudflare.com:3478", "stun:stun.l.google.com:19302"]
    })];
    if let Some(extra) = std::env::var("ALLEY_ICE_SERVERS")
        .ok()
        .and_then(|v| serde_json::from_str::<Vec<Value>>(&v).ok())
    {
        servers.extend(extra);
    }
    Json(json!({ "iceServers": servers }))
}

/// Le salon en direct : `GET /api/v1/rooms/:id/live?x=<clé publique X25519,
/// base64url>&s=<clé publique Ed25519, base64url>&t=<marque, si connecté>`.
async fn live(req: Request, socket: ws::WebSocket) {
    let id = req.param("id").unwrap_or_default().to_owned();
    let x = req.query("x").and_then(|x| public_key_from_url(&x));
    // La clé qui signe tout ce qu'il enverra : relayée aux autres membres.
    let s = req.query("s").and_then(|s| public_key_from_url(&s));
    let tag = req.query("t").map(|t| t.into_owned());
    let ip = req.ip();
    let mut socket = socket;
    let moderation = req.state::<Moderation>().clone();
    if ip.is_some_and(|ip| moderation.is_banned(ip)) {
        let _ = socket.close(4403, "banned").await;
        return;
    }
    let rooms = req.state::<Rooms>();
    rooms.sweep();
    // Quelques connexions par adresse, pas des centaines.
    if ip.is_some_and(|ip| rooms.connections_from(ip) >= MAX_CONNECTIONS_PER_IP) {
        let _ = socket.close(4429, "trop de connexions").await;
        return;
    }
    let Some(x) = x else {
        let _ = socket.close(4404, "salon introuvable ou complet").await;
        return;
    };
    let joined = match rooms.join(&id, x, s, ip, tag.as_deref()) {
        Ok(joined) => joined,
        Err(JoinError::Barred) => {
            let _ = socket.close(4410, "exclu").await;
            return;
        }
        Err(JoinError::Gone) => {
            let _ = socket.close(4404, "salon introuvable ou complet").await;
            return;
        }
    };
    let me = joined.me.clone();
    let mut inbox = joined.inbox;
    let (mut tx, mut rx) = socket.split();

    let forward = tokio::spawn(async move {
        // Un ping toutes les 25 s : le routeur de Heroku coupe une connexion
        // muette au bout de 55 s. C'est aussi l'heure de fermer les salons
        // arrivés à minuit.
        let mut ping = tokio::time::interval(Duration::from_secs(25));
        loop {
            tokio::select! {
                event = inbox.recv() => match event {
                    Some(event) => {
                        let closing = event.contains("\"type\":\"closed\"");
                        let banned = closing && event.contains("\"reason\":\"banned\"");
                        let expelled = closing && event.contains("\"reason\":\"expelled\"");
                        if tx.send(event).await.is_err() || closing {
                            let code = if banned {
                                4403
                            } else if expelled {
                                4410
                            } else {
                                4000
                            };
                            let _ = tx.close(code, "salon fermé").await;
                            break;
                        }
                    }
                    None => {
                        let _ = tx.close(4000, "salon fermé").await;
                        break;
                    }
                },
                _ = ping.tick() => {
                    rooms.sweep();
                    if tx.send(ws::Message::Ping(Default::default())).await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    let mut window = Instant::now();
    let mut sent = 0u32;
    let mut last_image: Option<Instant> = None;
    let mut banned = false;
    while let Some(Ok(message)) = rx.recv().await {
        // Fermé, ou bloqué entre-temps (depuis une autre connexion).
        if forward.is_finished() || ip.is_some_and(|ip| moderation.is_banned(ip)) {
            break;
        }
        let text = match message {
            ws::Message::Text(text) => text,
            ws::Message::Close(_) => break,
            _ => continue,
        };
        if window.elapsed() > Duration::from_secs(10) {
            window = Instant::now();
            sent = 0;
        }
        sent += 1;
        let too_much = sent > RELAY_BURST || text.len() > MAX_TOKEN + 256;
        if too_much {
            // Une image écartée reçoit quand même une réponse : sinon elle
            // resterait « en vérification » chez son expéditeur.
            if text.contains("\"type\":\"image\"")
                && let Some(mid) = image_id(&text)
            {
                rooms.notify(
                    &id,
                    &me,
                    &json!({ "type": "moderated", "id": mid, "reason": "slow_down" }),
                );
            }
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        let to = value["to"].as_str();
        match value["type"].as_str() {
            Some("relay") => {
                if let Some(data) = value["data"].as_str().filter(|d| is_token(d, MAX_TOKEN)) {
                    rooms.relay(&id, &me, to, data);
                }
            }
            Some("key") => {
                if let (Some(to), Some(blob)) = (to, value["blob"].as_str())
                    && is_b64_len(blob, WRAPPED_KEY_LEN)
                {
                    rooms.hand_key(&id, &me, to, blob);
                }
            }
            // L'administrateur exclut un membre.
            Some("expel") => {
                if let Some(target) = to {
                    rooms.expel(&id, &me, target);
                }
            }
            Some("image") => {
                // Une image toutes les 2 secondes au plus : la vérifier
                // occupe le serveur.
                if last_image.is_some_and(|t| t.elapsed() < Duration::from_secs(2)) {
                    let mid = value["id"].as_str().unwrap_or_default();
                    rooms.notify(
                        &id,
                        &me,
                        &json!({ "type": "moderated", "id": mid, "reason": "slow_down" }),
                    );
                    continue;
                }
                last_image = Some(Instant::now());
                if image(&value, &id, &me, ip, rooms, &moderation).await {
                    banned = true;
                    break;
                }
            }
            _ => {}
        }
    }
    if banned {
        // Le refus, puis la fermeture (4403) : on laisse partir les deux.
        let _ = tokio::time::timeout(Duration::from_secs(3), forward).await;
    } else {
        forward.abort();
    }
    rooms.leave(&id, &me);
}

/// L'identifiant d'une image dans un message trop gros pour être lu en
/// entier : `"id":"…"` (16 caractères hexadécimaux au plus).
fn image_id(text: &str) -> Option<String> {
    let start = text.find("\"id\":\"")? + 6;
    let id: String = text[start..]
        .chars()
        .take_while(|c| c.is_ascii_hexdigit())
        .take(32)
        .collect();
    (!id.is_empty()).then_some(id)
}

/// Vérifie une image puis la relaie, ou la refuse ; une image à caractère
/// sexuel ou montrant de la drogue bloque l'adresse de l'expéditeur.
/// Renvoie `true` si la connexion doit être fermée.
async fn image(
    value: &Value,
    id: &str,
    me: &str,
    ip: Option<std::net::IpAddr>,
    rooms: &Rooms,
    moderation: &Moderation,
) -> bool {
    let mid = value["id"].as_str().unwrap_or_default();
    let refuse = |reason: &str, banned: bool| {
        rooms.notify(
            id,
            me,
            &json!({ "type": "moderated", "id": mid, "reason": reason, "banned": banned }),
        );
    };
    let (Some(key), Some(c), Some(data)) = (
        value["key"].as_str().and_then(key_from_b64),
        value["c"].as_str().filter(|c| is_token(c, MAX_TOKEN)),
        value["data"].as_str().filter(|d| is_token(d, 64 * 1024)),
    ) else {
        refuse("invalid", false);
        return false;
    };
    let Some(jpeg) = open_token(&key, c) else {
        refuse("invalid", false);
        return false;
    };
    // La file d'attente est bornée : pleine, l'image est refusée tout de
    // suite plutôt que de s'accumuler en mémoire.
    let Some(_slot) = moderation.enqueue() else {
        refuse("slow_down", false);
        return false;
    };
    match moderation.judge(jpeg).await {
        Verdict::Clean => {
            rooms.relay_image(id, me, data, c, &key_commitment(&key));
            rooms.notify(id, me, &json!({ "type": "sent", "id": mid }));
            false
        }
        Verdict::Refused(category) => {
            refuse(category.code(), false);
            false
        }
        Verdict::Banned(category) => {
            refuse(category.code(), true);
            if let Some(ip) = ip {
                moderation.ban(ip);
                rooms.kick(|other| moderation.is_banned(other));
            }
            true
        }
        Verdict::Invalid => {
            refuse("invalid", false);
            false
        }
        Verdict::Unavailable => {
            refuse("unavailable", false);
            false
        }
    }
}

pub fn routes(app: &mut App) {
    app.get("/api/v1/rooms", list);
    app.post("/api/v1/rooms", create);
    app.post("/api/v1/mine", mine);
    app.post("/api/v1/known", known);
    app.get("/api/v1/rooms/:id", show);
    app.ws("/api/v1/rooms/:id/live", live);
    app.get("/api/v1/key", key);
    app.get("/api/v1/ice", ice);
    app.get("/api/v1/stats", stats);
}
