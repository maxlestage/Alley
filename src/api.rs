//! L'API d'Alley, sous `/api/v1` : serveurs, salons, membres, messages, et
//! le flux temps réel en WebSocket.
//!
//! Chaque requête sur un serveur porte le **jeton d'accès** (en-tête
//! `X-Alley-Access`), qui prouve qu'on connaît sa clé. S'y ajoutent, selon
//! l'action, le **secret de membre** (`X-Alley-Member` : écrire, modifier ses
//! messages) et le **secret d'admin** (`X-Alley-Admin` : salons, nom du
//! serveur, modération). Tout le contenu reçu doit être un jeton Brocolis :
//! le clair est refusé avec `422`.

use std::time::{SystemTime, UNIX_EPOCH};

use paquets::prelude::*;
use serde_json::Value;

use crate::live::Hub;
use crate::protocol::{
    self, GUILD_ID_LEN, MAX_MESSAGE_TOKEN, MAX_NAME_TOKEN, SECRET_LEN, is_hex, is_token,
};

/// Un serveur (une « guilde » chez Discord). Son nom est chiffré.
#[model(table = "guilds")]
pub struct Guild {
    pub id: i64,
    #[memoire(unique)]
    pub gid: String,
    pub access_hash: String,
    pub admin_hash: String,
    pub name: String,
    pub created_at: i64,
}

/// Un salon textuel. Son nom et son sujet sont chiffrés.
#[model(table = "channels")]
pub struct Channel {
    pub id: i64,
    #[memoire(index)]
    pub gid: String,
    pub name: String,
    pub topic: String,
    pub position: i64,
    pub created_at: i64,
}

/// Un membre : son identifiant public et son pseudo chiffré.
#[model(table = "members")]
pub struct Member {
    pub id: i64,
    #[memoire(index)]
    pub gid: String,
    pub member: String,
    pub name: String,
    pub joined_at: i64,
}

/// Un message. `body` est un jeton : du texte, ou une image chiffrée.
#[model(table = "messages")]
pub struct Message {
    pub id: i64,
    #[memoire(index)]
    pub gid: String,
    #[memoire(index)]
    pub channel_id: i64,
    pub author: String,
    pub body: String,
    pub reply_to: i64,
    pub created_at: i64,
    pub edited_at: i64,
}

#[data]
pub struct NewGuild {
    pub gid: String,
    pub access: String,
    pub admin: String,
    pub name: String,
    /// Le nom du premier salon (« général »), chiffré.
    pub channel: String,
}

#[data]
pub struct Rename {
    pub name: String,
}

#[data]
pub struct NewChannel {
    pub name: String,
    #[serde(default)]
    pub topic: String,
}

#[data]
pub struct NewMessage {
    pub body: String,
    #[serde(default)]
    pub reply_to: i64,
}

#[data]
pub struct Edit {
    pub body: String,
}

/// Un serveur ne garde pas plus de salons que ça.
pub const MAX_CHANNELS: usize = 100;
/// Messages renvoyés par page d'historique, au plus.
pub const MAX_PAGE: usize = 100;

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

fn encrypted(text: &str, max: usize, what: &str) -> Result<()> {
    if is_token(text, max) {
        Ok(())
    } else {
        Err(Error::unprocessable(format!(
            "{what} : seul du contenu chiffré (jeton Brocolis) est accepté"
        )))
    }
}

/// Un secret lu dans un en-tête, s'il a la bonne forme.
fn secret<'a>(req: &'a Request, header: &str) -> Option<&'a str> {
    req.header(header).filter(|s| is_hex(s, SECRET_LEN))
}

/// Le serveur demandé, si l'appelant connaît sa clé (sinon 404 : on ne dit
/// pas à un inconnu qu'un serveur existe).
async fn guild(req: &Request) -> Result<Guild> {
    let gid = req.param("gid").unwrap_or_default();
    if !is_hex(gid, GUILD_ID_LEN) {
        return Err(Error::not_found("serveur introuvable"));
    }
    let found = Guild::query()
        .filter(Guild::GID.eq(gid.to_owned()))
        .first(req.db())
        .await?;
    match (found, secret(req, "x-alley-access")) {
        (Some(g), Some(access)) if protocol::same(&g.access_hash, &protocol::digest(access)) => {
            Ok(g)
        }
        _ => Err(Error::not_found("serveur introuvable")),
    }
}

fn admin(req: &Request, guild: &Guild) -> Result<()> {
    match secret(req, "x-alley-admin") {
        Some(s) if protocol::same(&guild.admin_hash, &protocol::digest(s)) => Ok(()),
        _ => Err(Error::forbidden("réservé à l'administrateur du serveur")),
    }
}

fn member(req: &Request) -> Result<String> {
    secret(req, "x-alley-member")
        .map(protocol::member_id)
        .ok_or_else(|| Error::unauthorized("secret de membre manquant"))
}

fn channel_json(c: &Channel) -> Value {
    json!({ "id": c.id, "name": c.name, "topic": c.topic, "position": c.position })
}

fn member_json(m: &Member) -> Value {
    json!({ "id": m.member, "name": m.name, "joined_at": m.joined_at })
}

fn message_json(m: &Message) -> Value {
    json!({
        "id": m.id,
        "channel": m.channel_id,
        "author": m.author,
        "body": m.body,
        "reply_to": m.reply_to,
        "created_at": m.created_at,
        "edited_at": m.edited_at,
    })
}

async fn channel_of(req: &Request, gid: &str) -> Result<Channel> {
    let cid: i64 = req.param_as("cid")?;
    Channel::query()
        .filter(Channel::ID.eq(cid) & Channel::GID.eq(gid.to_owned()))
        .first(req.db())
        .await?
        .ok_or_else(|| Error::not_found("salon introuvable"))
}

/// `POST /api/v1/guilds` : crée un serveur et son premier salon. Le créateur
/// en devient l'administrateur.
async fn create_guild(req: Request) -> Result<(u16, Json<Value>)> {
    let body: NewGuild = req.json().await?;
    if !is_hex(&body.gid, GUILD_ID_LEN)
        || !is_hex(&body.access, SECRET_LEN)
        || !is_hex(&body.admin, SECRET_LEN)
    {
        return Err(Error::bad_request("identifiant ou secret invalide"));
    }
    encrypted(&body.name, MAX_NAME_TOKEN, "nom du serveur")?;
    encrypted(&body.channel, MAX_NAME_TOKEN, "nom du salon")?;
    let db = req.db();
    let now = now();
    let mut guild = Guild {
        id: 0,
        gid: body.gid.clone(),
        access_hash: protocol::digest(&body.access),
        admin_hash: protocol::digest(&body.admin),
        name: body.name,
        created_at: now,
    };
    // `gid` est unique : un doublon répond 409.
    guild.insert(db).await?;
    let mut channel = Channel {
        id: 0,
        gid: body.gid.clone(),
        name: body.channel,
        topic: String::new(),
        position: 0,
        created_at: now,
    };
    channel.insert(db).await?;
    Ok((
        201,
        Json(json!({ "gid": guild.gid, "channels": [channel_json(&channel)] })),
    ))
}

/// `GET /api/v1/guilds/:gid` : le serveur, ses salons, ses membres, qui est
/// en ligne, et si l'appelant en est l'administrateur.
async fn show_guild(req: Request) -> Result<Json<Value>> {
    let g = guild(&req).await?;
    let db = req.db();
    let channels = Channel::query()
        .filter(Channel::GID.eq(g.gid.clone()))
        .order_by(Channel::POSITION.asc())
        .all(db)
        .await?;
    let members = Member::query()
        .filter(Member::GID.eq(g.gid.clone()))
        .order_by(Member::ID.asc())
        .all(db)
        .await?;
    let is_admin = admin(&req, &g).is_ok();
    let online = req.state::<Hub>().online(&g.gid);
    Ok(Json(json!({
        "gid": g.gid,
        "name": g.name,
        "admin": is_admin,
        "channels": channels.iter().map(channel_json).collect::<Vec<_>>(),
        "members": members.iter().map(member_json).collect::<Vec<_>>(),
        "online": online,
    })))
}

/// `PATCH /api/v1/guilds/:gid` (admin) : renomme le serveur.
async fn rename_guild(req: Request) -> Result<Json<Value>> {
    let mut g = guild(&req).await?;
    admin(&req, &g)?;
    let body: Rename = req.json().await?;
    encrypted(&body.name, MAX_NAME_TOKEN, "nom du serveur")?;
    g.name = body.name;
    g.update(req.db()).await?;
    let event = json!({ "type": "guild.updated", "name": g.name });
    req.state::<Hub>().publish(&g.gid, event.clone());
    Ok(Json(event))
}

/// `DELETE /api/v1/guilds/:gid` (admin) : efface le serveur et tout son
/// contenu.
async fn delete_guild(req: Request) -> Result<StatusCode> {
    let g = guild(&req).await?;
    admin(&req, &g)?;
    let db = req.db();
    Message::delete_many()
        .filter(Message::GID.eq(g.gid.clone()))
        .execute(db)
        .await?;
    Channel::delete_many()
        .filter(Channel::GID.eq(g.gid.clone()))
        .execute(db)
        .await?;
    Member::delete_many()
        .filter(Member::GID.eq(g.gid.clone()))
        .execute(db)
        .await?;
    g.delete(db).await?;
    req.state::<Hub>().close(&g.gid);
    Ok(StatusCode::NO_CONTENT)
}

/// `PUT /api/v1/guilds/:gid/members/me` : rejoint le serveur, ou change de
/// pseudo (chiffré).
async fn upsert_member(req: Request) -> Result<Json<Value>> {
    let g = guild(&req).await?;
    let id = member(&req)?;
    let body: Rename = req.json().await?;
    encrypted(&body.name, MAX_NAME_TOKEN, "pseudo")?;
    let db = req.db();
    let found = Member::query()
        .filter(Member::GID.eq(g.gid.clone()) & Member::MEMBER.eq(id.clone()))
        .first(db)
        .await?;
    let m = match found {
        Some(mut m) => {
            m.name = body.name;
            m.update(db).await?;
            m
        }
        None => {
            let mut m = Member {
                id: 0,
                gid: g.gid.clone(),
                member: id,
                name: body.name,
                joined_at: now(),
            };
            m.insert(db).await?;
            m
        }
    };
    let json = member_json(&m);
    req.state::<Hub>()
        .publish(&g.gid, json!({ "type": "member", "member": json }));
    Ok(Json(json))
}

/// `POST /api/v1/guilds/:gid/channels` (admin) : un nouveau salon.
async fn create_channel(req: Request) -> Result<(u16, Json<Value>)> {
    let g = guild(&req).await?;
    admin(&req, &g)?;
    let body: NewChannel = req.json().await?;
    encrypted(&body.name, MAX_NAME_TOKEN, "nom du salon")?;
    if !body.topic.is_empty() {
        encrypted(&body.topic, MAX_NAME_TOKEN, "sujet du salon")?;
    }
    let db = req.db();
    let existing = Channel::query()
        .filter(Channel::GID.eq(g.gid.clone()))
        .all(db)
        .await?;
    if existing.len() >= MAX_CHANNELS {
        return Err(Error::conflict(
            "ce serveur a déjà le nombre maximal de salons",
        ));
    }
    let position = existing.iter().map(|c| c.position).max().unwrap_or(-1) + 1;
    let mut channel = Channel {
        id: 0,
        gid: g.gid.clone(),
        name: body.name,
        topic: body.topic,
        position,
        created_at: now(),
    };
    channel.insert(db).await?;
    let json = channel_json(&channel);
    req.state::<Hub>()
        .publish(&g.gid, json!({ "type": "channel", "channel": json }));
    Ok((201, Json(json)))
}

/// `PATCH /api/v1/guilds/:gid/channels/:cid` (admin) : renomme un salon, ou
/// change son sujet.
async fn update_channel(req: Request) -> Result<Json<Value>> {
    let g = guild(&req).await?;
    admin(&req, &g)?;
    let mut channel = channel_of(&req, &g.gid).await?;
    let body: NewChannel = req.json().await?;
    encrypted(&body.name, MAX_NAME_TOKEN, "nom du salon")?;
    if !body.topic.is_empty() {
        encrypted(&body.topic, MAX_NAME_TOKEN, "sujet du salon")?;
    }
    channel.name = body.name;
    channel.topic = body.topic;
    channel.update(req.db()).await?;
    let json = channel_json(&channel);
    req.state::<Hub>()
        .publish(&g.gid, json!({ "type": "channel", "channel": json }));
    Ok(Json(json))
}

/// `DELETE /api/v1/guilds/:gid/channels/:cid` (admin) : supprime un salon et
/// ses messages. Le dernier salon reste.
async fn delete_channel(req: Request) -> Result<StatusCode> {
    let g = guild(&req).await?;
    admin(&req, &g)?;
    let channel = channel_of(&req, &g.gid).await?;
    let db = req.db();
    let count = Channel::query()
        .filter(Channel::GID.eq(g.gid.clone()))
        .count(db)
        .await?;
    if count <= 1 {
        return Err(Error::conflict("un serveur garde au moins un salon"));
    }
    Message::delete_many()
        .filter(Message::CHANNEL_ID.eq(channel.id))
        .execute(db)
        .await?;
    let id = channel.id;
    channel.delete(db).await?;
    req.state::<Hub>()
        .publish(&g.gid, json!({ "type": "channel.deleted", "id": id }));
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/v1/messages/:gid/:cid?before=&limit=` : une page
/// d'historique, du plus ancien au plus récent.
async fn list_messages(req: Request) -> Result<Json<Value>> {
    let g = guild(&req).await?;
    let channel = channel_of(&req, &g.gid).await?;
    let limit = req
        .query("limit")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(50)
        .clamp(1, MAX_PAGE);
    let mut filter = Message::CHANNEL_ID.eq(channel.id);
    if let Some(before) = req.query("before").and_then(|v| v.parse::<i64>().ok()) {
        filter = filter & Message::ID.lt(before);
    }
    let mut page = Message::query()
        .filter(filter)
        .order_by(Message::ID.desc())
        .limit(limit as u64)
        .all(req.db())
        .await?;
    page.reverse();
    Ok(Json(json!({
        "messages": page.iter().map(message_json).collect::<Vec<_>>(),
        "more": page.len() == limit,
    })))
}

/// `POST /api/v1/messages/:gid/:cid` : envoie un message
/// chiffré, diffusé aussitôt aux membres connectés.
async fn send_message(req: Request) -> Result<(u16, Json<Value>)> {
    let g = guild(&req).await?;
    let author = member(&req)?;
    let channel = channel_of(&req, &g.gid).await?;
    let body: NewMessage = req.json().await?;
    encrypted(&body.body, MAX_MESSAGE_TOKEN, "message")?;
    let mut message = Message {
        id: 0,
        gid: g.gid.clone(),
        channel_id: channel.id,
        author,
        body: body.body,
        reply_to: body.reply_to.max(0),
        created_at: now(),
        edited_at: 0,
    };
    message.insert(req.db()).await?;
    let json = message_json(&message);
    req.state::<Hub>()
        .publish(&g.gid, json!({ "type": "message", "message": json }));
    Ok((201, Json(json)))
}

async fn message_of(req: &Request, gid: &str) -> Result<Message> {
    let mid: i64 = req.param_as("mid")?;
    let cid: i64 = req.param_as("cid")?;
    Message::query()
        .filter(Message::ID.eq(mid) & Message::GID.eq(gid.to_owned()) & Message::CHANNEL_ID.eq(cid))
        .first(req.db())
        .await?
        .ok_or_else(|| Error::not_found("message introuvable"))
}

/// `PATCH /api/v1/messages/:gid/:cid/:mid` : son auteur le corrige.
async fn edit_message(req: Request) -> Result<Json<Value>> {
    let g = guild(&req).await?;
    let author = member(&req)?;
    let mut message = message_of(&req, &g.gid).await?;
    if message.author != author {
        return Err(Error::forbidden("seul l'auteur peut modifier ce message"));
    }
    let body: Edit = req.json().await?;
    encrypted(&body.body, MAX_MESSAGE_TOKEN, "message")?;
    message.body = body.body;
    message.edited_at = now();
    message.update(req.db()).await?;
    let json = message_json(&message);
    req.state::<Hub>().publish(
        &g.gid,
        json!({ "type": "message.updated", "message": json }),
    );
    Ok(Json(json))
}

/// `DELETE /api/v1/messages/:gid/:cid/:mid` : son auteur, ou l'admin.
async fn delete_message(req: Request) -> Result<StatusCode> {
    let g = guild(&req).await?;
    let message = message_of(&req, &g.gid).await?;
    let is_author = member(&req).is_ok_and(|id| id == message.author);
    if !is_author {
        admin(&req, &g)?;
    }
    let (id, channel) = (message.id, message.channel_id);
    message.delete(req.db()).await?;
    req.state::<Hub>().publish(
        &g.gid,
        json!({ "type": "message.deleted", "id": id, "channel": channel }),
    );
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/v1/stats` : ce que le serveur voit. Que des nombres.
async fn stats(req: Request) -> Result<Json<Value>> {
    let db = req.db();
    Ok(Json(json!({
        "guilds": Guild::count(db).await?,
        "messages": Message::count(db).await?,
        "connections": req.state::<Hub>().connections(),
        "plaintext_stored": 0,
    })))
}

/// Le flux temps réel d'un serveur :
/// `GET /api/v1/guilds/:gid/live?access=…&member=…` (en WebSocket ; les
/// navigateurs ne savent pas y mettre d'en-têtes, d'où la query string).
///
/// Le serveur y pousse les événements (messages, salons, membres, présence) ;
/// le client y envoie `{"type":"typing","channel":…}` quand il écrit.
async fn live(req: Request, socket: ws::WebSocket) {
    let gid = req.param("gid").unwrap_or_default().to_owned();
    let access = req
        .query("access")
        .map(|v| v.into_owned())
        .unwrap_or_default();
    let secret = req
        .query("member")
        .map(|v| v.into_owned())
        .unwrap_or_default();
    let mut socket = socket;
    let allowed = is_hex(&gid, GUILD_ID_LEN)
        && is_hex(&access, SECRET_LEN)
        && is_hex(&secret, SECRET_LEN)
        && matches!(
            Guild::query().filter(Guild::GID.eq(gid.clone())).first(req.db()).await,
            Ok(Some(g)) if protocol::same(&g.access_hash, &protocol::digest(&access))
        );
    if !allowed {
        let _ = socket.close(4404, "serveur introuvable").await;
        return;
    }
    let me = protocol::member_id(&secret);
    let hub = req.state::<Hub>();
    let mut inbox = hub.join(&gid, &me);
    let (mut tx, mut rx) = socket.split();

    let forward = tokio::spawn(async move {
        // Un ping toutes les 25 s : le routeur de Heroku coupe une connexion
        // muette au bout de 55 s.
        let mut ping = tokio::time::interval(std::time::Duration::from_secs(25));
        loop {
            tokio::select! {
                event = inbox.recv() => match event {
                    Ok(event) => {
                        let closing = event.contains("\"guild.deleted\"");
                        if tx.send(event).await.is_err() || closing {
                            break;
                        }
                    }
                    // Trop lent : il a raté des événements, il rechargera.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        if tx.send(json!({ "type": "resync" }).to_string()).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                },
                _ = ping.tick() => {
                    if tx.send(ws::Message::Ping(Default::default())).await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    while let Some(Ok(message)) = rx.recv().await {
        match message {
            ws::Message::Text(text) => {
                // Seul « j'écris » est relayé, et seulement avec un salon.
                if text.len() > 256 {
                    continue;
                }
                let Ok(value) = serde_json::from_str::<Value>(&text) else {
                    continue;
                };
                if value["type"] == "typing"
                    && let Some(channel) = value["channel"].as_i64()
                {
                    hub.publish(
                        &gid,
                        json!({ "type": "typing", "member": me, "channel": channel }),
                    );
                }
            }
            ws::Message::Close(_) => break,
            _ => {}
        }
    }
    forward.abort();
    hub.leave(&gid, &me);
}

/// Toutes les routes de l'API.
pub fn routes(app: &mut App) {
    app.post("/api/v1/guilds", create_guild);
    app.get("/api/v1/stats", stats);
    app.get("/api/v1/guilds/:gid", show_guild);
    app.patch("/api/v1/guilds/:gid", rename_guild);
    app.delete("/api/v1/guilds/:gid", delete_guild);
    app.put("/api/v1/guilds/:gid/members/me", upsert_member);
    app.post("/api/v1/guilds/:gid/channels", create_channel);
    app.patch("/api/v1/guilds/:gid/channels/:cid", update_channel);
    app.delete("/api/v1/guilds/:gid/channels/:cid", delete_channel);
    // Sous leur propre préfixe : le bouclier n'accepte les gros corps (images
    // chiffrées) que là.
    app.get("/api/v1/messages/:gid/:cid", list_messages);
    app.post("/api/v1/messages/:gid/:cid", send_message);
    app.patch("/api/v1/messages/:gid/:cid/:mid", edit_message);
    app.delete("/api/v1/messages/:gid/:cid/:mid", delete_message);
    app.ws("/api/v1/guilds/:gid/live", live);
}
