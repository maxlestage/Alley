//! L'API d'Alley, sous `/api/v1` :
//!
//! | Route | Rôle |
//! |---|---|
//! | `GET /rooms?q=` | les salons publics ouverts, filtrés par nom |
//! | `POST /rooms` | crée un salon (`{name, private, tz}`) ou rejoint celui de ce nom |
//! | `GET /rooms/:id` | un salon (pour un lien de salon privé) |
//! | `GET /rooms/:id/live?x=` | le salon en direct (WebSocket) |
//! | `GET /ice` | les serveurs STUN/TURN des appels |
//! | `GET /stats` | ce que voit le serveur : des nombres |
//!
//! En direct, le client envoie :
//!
//! - `{"type":"relay","data":"[[BROCOLIS:v1:…]]","to":?}` : un jeton chiffré
//!   pour tous les autres, ou pour une seule personne (signalisation d'appel) ;
//! - `{"type":"key","to":…,"blob":…}` : la clé du salon, emballée pour un
//!   nouvel arrivant avec sa clé publique X25519.
//!
//! Le serveur répond par `welcome`, `joined`, `left`, `relay` (avec `from`),
//! `key` et `closed`. Il ne garde rien.

use std::time::{Duration, Instant};

use paquets::prelude::*;
use serde_json::Value;

use crate::protocol::{MAX_TOKEN, WRAPPED_KEY_LEN, is_b64_len, is_token, public_key_from_url};
use crate::rooms::{self, Rooms};

#[data]
pub struct NewRoom {
    pub name: String,
    #[serde(default)]
    pub private: bool,
    /// Le décalage horaire du créateur, en minutes (`+120` à Paris l'été) :
    /// le salon disparaît à minuit chez lui.
    #[serde(default)]
    pub tz: i64,
}

/// Jetons relayés par connexion, au plus, par fenêtre de 10 s (contre
/// l'inondation ; la signalisation d'un appel en envoie une rafale).
const RELAY_BURST: u32 = 200;

/// `GET /api/v1/rooms?q=`
async fn list(req: Request) -> Json<Value> {
    let rooms = req.state::<Rooms>();
    rooms.sweep();
    let q = req.query("q").map(|q| q.into_owned()).unwrap_or_default();
    Json(json!({ "rooms": rooms.list(&q) }))
}

/// `POST /api/v1/rooms`
async fn create(req: Request) -> Result<(u16, Json<Value>)> {
    let body: NewRoom = req.json().await?;
    let rooms = req.state::<Rooms>();
    rooms.sweep();
    if rooms::clean_name(&body.name).is_none() {
        return Err(Error::bad_request(format!(
            "le nom d'un salon fait de 1 à {} caractères",
            rooms::MAX_NAME
        )));
    }
    let room = rooms
        .create(&body.name, body.private, body.tz)
        .ok_or_else(|| Error::new(503, "trop de salons ouverts, réessayez plus tard"))?;
    Ok((201, Json(room)))
}

/// `GET /api/v1/rooms/:id`
async fn show(req: Request) -> Result<Json<Value>> {
    let id = req.param("id").unwrap_or_default();
    req.state::<Rooms>()
        .info(id)
        .map(Json)
        .ok_or_else(|| Error::not_found("ce salon n'existe plus"))
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
/// base64url>`.
async fn live(req: Request, socket: ws::WebSocket) {
    let id = req.param("id").unwrap_or_default().to_owned();
    let x = req.query("x").and_then(|x| public_key_from_url(&x));
    let mut socket = socket;
    let rooms = req.state::<Rooms>();
    rooms.sweep();
    let Some(joined) = x.and_then(|x| rooms.join(&id, x)) else {
        let _ = socket.close(4404, "salon introuvable ou complet").await;
        return;
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
                        if tx.send(event).await.is_err() || closing {
                            let _ = tx.close(4000, "salon fermé").await;
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
    while let Some(Ok(message)) = rx.recv().await {
        if forward.is_finished() {
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
        if sent > RELAY_BURST || text.len() > MAX_TOKEN + 256 {
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
            _ => {}
        }
    }
    forward.abort();
    rooms.leave(&id, &me);
}

pub fn routes(app: &mut App) {
    app.get("/api/v1/rooms", list);
    app.post("/api/v1/rooms", create);
    app.get("/api/v1/rooms/:id", show);
    app.ws("/api/v1/rooms/:id/live", live);
    app.get("/api/v1/ice", ice);
    app.get("/api/v1/stats", stats);
}
