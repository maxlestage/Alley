//! L'API d'Alley, sous `/api/v1`.
//!
//! Le serveur en sait le moins possible :
//!
//! - **un serveur Alley** n'est qu'un identifiant, l'empreinte de son jeton
//!   d'accès, celle du secret d'admin, et un **état chiffré** (son nom, ses
//!   salons, les anciennes clés) ;
//! - **un message** n'est qu'un numéro et un jeton : le salon, l'auteur,
//!   l'heure, la signature sont **dans** le chiffré. Pour pouvoir le modifier
//!   ou le supprimer, son auteur dépose l'empreinte d'une preuve qu'il est seul
//!   à pouvoir recalculer (différente pour chaque message : le serveur ne peut
//!   pas relier deux messages au même auteur) ;
//! - **un membre** n'est qu'un identifiant aléatoire et un profil chiffré
//!   (pseudo, clés publiques X25519 et ECDSA) ;
//! - **le temps réel** ne demande que le jeton d'accès : le serveur ne sait
//!   pas qui est connecté, il relaie des jetons (présence, « j'écris »,
//!   signalisation des appels audio et vidéo).
//!
//! **Exclure un membre** : l'admin tire une nouvelle clé, l'emballe pour
//! chaque membre restant avec sa clé publique X25519 (comme l'appairage
//! Brocolis), et envoie le tout à `POST /rotate`. L'ancien jeton d'accès
//! cesse de marcher, les connexions sont coupées, l'exclu est banni.
//!
//! En-têtes : `X-Alley-Access` (jeton d'accès), `X-Alley-Member` (secret de
//! membre : profil, clés emballées), `X-Alley-Admin` (secret d'admin),
//! `X-Alley-Proof` (preuve d'auteur d'un message). Tout contenu doit être un
//! jeton Brocolis : le clair est refusé avec `422`.

use paquets::prelude::*;
use serde_json::Value;

use crate::live::Hub;
use crate::protocol::{
    self, GUILD_ID_LEN, MAX_MESSAGE_TOKEN, MAX_PROFILE_TOKEN, MAX_RELAY_TOKEN, MAX_STATE_TOKEN,
    PUBLIC_KEY_LEN, SECRET_LEN, WRAPPED_KEY_LEN, is_b64_len, is_hex, is_token,
};

/// Un serveur (une « guilde » chez Discord).
#[model(table = "guilds")]
pub struct Guild {
    pub id: i64,
    #[memoire(unique)]
    pub gid: String,
    pub access_hash: String,
    pub admin_hash: String,
    /// Nom, salons, anciennes clés : un seul jeton.
    pub state: String,
    /// Augmente à chaque modification de l'état (contre les écrasements).
    pub revision: i64,
    /// Augmente à chaque renouvellement de clé.
    pub epoch: i64,
}

/// Un membre : un identifiant aléatoire et un profil chiffré.
#[model(table = "members")]
pub struct Member {
    pub id: i64,
    #[memoire(index)]
    pub gid: String,
    pub member: String,
    pub profile: String,
}

/// Un membre exclu : il ne peut plus revenir avec ce secret.
#[model(table = "bans")]
pub struct Ban {
    pub id: i64,
    #[memoire(index)]
    pub gid: String,
    pub member: String,
}

/// La nouvelle clé d'un serveur, emballée pour un membre.
#[model(table = "shares")]
pub struct Share {
    pub id: i64,
    #[memoire(index)]
    pub gid: String,
    pub member: String,
    pub epoch: i64,
    pub blob: String,
    pub sender: String,
}

/// Un message : un jeton, et l'empreinte de la preuve de son auteur.
#[model(table = "messages")]
pub struct Message {
    pub id: i64,
    #[memoire(index)]
    pub gid: String,
    pub body: String,
    pub owner: String,
    pub edited: bool,
}

#[data]
pub struct NewGuild {
    pub gid: String,
    pub access: String,
    pub admin: String,
    pub state: String,
}

#[data]
pub struct NewState {
    pub state: String,
    pub revision: i64,
}

#[data]
pub struct Profile {
    pub profile: String,
}

#[data]
pub struct WrappedKey {
    pub member: String,
    pub blob: String,
    pub sender: String,
}

#[data]
pub struct Rotation {
    pub epoch: i64,
    pub access: String,
    pub state: String,
    #[serde(default)]
    pub remove: Vec<String>,
    pub shares: Vec<WrappedKey>,
}

#[data]
pub struct NewMessage {
    pub body: String,
    pub owner: String,
}

#[data]
pub struct Edit {
    pub body: String,
    pub proof: String,
}

/// Messages renvoyés par page d'historique, au plus.
pub const MAX_PAGE: usize = 200;
/// Membres d'un serveur, au plus (une rotation emballe la clé pour chacun).
pub const MAX_MEMBERS: usize = 500;

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

fn gid(req: &Request) -> Result<String> {
    let gid = req.param("gid").unwrap_or_default();
    if is_hex(gid, GUILD_ID_LEN) {
        Ok(gid.to_owned())
    } else {
        Err(Error::not_found("serveur introuvable"))
    }
}

async fn find_guild(req: &Request, gid: &str) -> Result<Option<Guild>> {
    Ok(Guild::query()
        .filter(Guild::GID.eq(gid.to_owned()))
        .first(req.db())
        .await?)
}

/// Le serveur demandé, si l'appelant connaît sa clé actuelle (sinon 404 : on
/// ne dit pas à un inconnu qu'un serveur existe).
async fn guild(req: &Request) -> Result<Guild> {
    let gid = gid(req)?;
    match (find_guild(req, &gid).await?, secret(req, "x-alley-access")) {
        (Some(g), Some(access)) if protocol::same(&g.access_hash, &protocol::digest(access)) => {
            Ok(g)
        }
        _ => Err(Error::not_found("serveur introuvable")),
    }
}

fn is_admin(req: &Request, guild: &Guild) -> bool {
    secret(req, "x-alley-admin")
        .is_some_and(|s| protocol::same(&guild.admin_hash, &protocol::digest(s)))
}

fn admin(req: &Request, guild: &Guild) -> Result<()> {
    if is_admin(req, guild) {
        Ok(())
    } else {
        Err(Error::forbidden("réservé à l'administrateur du serveur"))
    }
}

fn member(req: &Request) -> Result<String> {
    secret(req, "x-alley-member")
        .map(protocol::member_id)
        .ok_or_else(|| Error::unauthorized("secret de membre manquant"))
}

async fn banned(req: &Request, gid: &str, id: &str) -> Result<bool> {
    Ok(Ban::query()
        .filter(Ban::GID.eq(gid.to_owned()) & Ban::MEMBER.eq(id.to_owned()))
        .exists(req.db())
        .await?)
}

async fn members_of(req: &Request, gid: &str) -> Result<Vec<Member>> {
    Ok(Member::query()
        .filter(Member::GID.eq(gid.to_owned()))
        .order_by(Member::ID.asc())
        .all(req.db())
        .await?)
}

fn member_json(m: &Member) -> Value {
    json!({ "id": m.member, "profile": m.profile })
}

fn message_json(m: &Message) -> Value {
    json!({ "id": m.id, "body": m.body, "edited": m.edited })
}

fn guild_json(g: &Guild) -> Value {
    json!({ "gid": g.gid, "state": g.state, "revision": g.revision, "epoch": g.epoch })
}

/// `POST /api/v1/guilds` : crée un serveur. Le créateur en devient l'admin.
async fn create_guild(req: Request) -> Result<(u16, Json<Value>)> {
    let body: NewGuild = req.json().await?;
    if !is_hex(&body.gid, GUILD_ID_LEN)
        || !is_hex(&body.access, SECRET_LEN)
        || !is_hex(&body.admin, SECRET_LEN)
    {
        return Err(Error::bad_request("identifiant ou secret invalide"));
    }
    encrypted(&body.state, MAX_STATE_TOKEN, "état du serveur")?;
    let mut guild = Guild {
        id: 0,
        gid: body.gid,
        access_hash: protocol::digest(&body.access),
        admin_hash: protocol::digest(&body.admin),
        state: body.state,
        revision: 1,
        epoch: 0,
    };
    // `gid` est unique : un doublon répond 409.
    guild.insert(req.db()).await?;
    Ok((201, Json(guild_json(&guild))))
}

/// `GET /api/v1/guilds/:gid` : l'état chiffré, les membres, et si l'appelant
/// est l'admin.
async fn show_guild(req: Request) -> Result<Json<Value>> {
    let g = guild(&req).await?;
    let members = members_of(&req, &g.gid).await?;
    let mut json = guild_json(&g);
    json["admin"] = is_admin(&req, &g).into();
    json["members"] = members.iter().map(member_json).collect();
    Ok(Json(json))
}

/// `PUT /api/v1/guilds/:gid/state` (admin) : nouveau nom, salons ajoutés,
/// renommés ou supprimés. `revision` doit être la dernière connue (sinon 409 :
/// recharger, puis réessayer).
async fn update_state(req: Request) -> Result<Json<Value>> {
    let mut g = guild(&req).await?;
    admin(&req, &g)?;
    let body: NewState = req.json().await?;
    encrypted(&body.state, MAX_STATE_TOKEN, "état du serveur")?;
    if body.revision != g.revision {
        return Err(Error::conflict("l'état du serveur a changé entre-temps"));
    }
    g.state = body.state;
    g.revision += 1;
    g.update(req.db()).await?;
    let json = guild_json(&g);
    let mut event = json.clone();
    event["type"] = "state".into();
    req.state::<Hub>().publish(&g.gid, event);
    Ok(Json(json))
}

/// `DELETE /api/v1/guilds/:gid` (admin) : efface le serveur et tout son
/// contenu.
async fn delete_guild(req: Request) -> Result<StatusCode> {
    let g = guild(&req).await?;
    admin(&req, &g)?;
    let db = req.db();
    let gid = g.gid.clone();
    Message::delete_many()
        .filter(Message::GID.eq(gid.clone()))
        .execute(db)
        .await?;
    Member::delete_many()
        .filter(Member::GID.eq(gid.clone()))
        .execute(db)
        .await?;
    Share::delete_many()
        .filter(Share::GID.eq(gid.clone()))
        .execute(db)
        .await?;
    Ban::delete_many()
        .filter(Ban::GID.eq(gid.clone()))
        .execute(db)
        .await?;
    g.delete(db).await?;
    req.state::<Hub>()
        .close(&gid, json!({ "type": "guild.deleted" }));
    Ok(StatusCode::NO_CONTENT)
}

/// `PUT /api/v1/guilds/:gid/members/me` : rejoint le serveur, ou met à jour
/// son profil chiffré. Un membre exclu ne revient pas.
async fn upsert_member(req: Request) -> Result<Json<Value>> {
    let g = guild(&req).await?;
    let id = member(&req)?;
    let body: Profile = req.json().await?;
    encrypted(&body.profile, MAX_PROFILE_TOKEN, "profil")?;
    if banned(&req, &g.gid, &id).await? {
        return Err(Error::forbidden("vous avez été exclu de ce serveur"));
    }
    let db = req.db();
    let found = Member::query()
        .filter(Member::GID.eq(g.gid.clone()) & Member::MEMBER.eq(id.clone()))
        .first(db)
        .await?;
    let m = match found {
        Some(mut m) => {
            m.profile = body.profile;
            m.update(db).await?;
            m
        }
        None => {
            if Member::query()
                .filter(Member::GID.eq(g.gid.clone()))
                .count(db)
                .await?
                >= MAX_MEMBERS as u64
            {
                return Err(Error::conflict("ce serveur est complet"));
            }
            let mut m = Member {
                id: 0,
                gid: g.gid.clone(),
                member: id,
                profile: body.profile,
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

/// `DELETE /api/v1/guilds/:gid/members/me` : quitte le serveur.
async fn leave_guild(req: Request) -> Result<StatusCode> {
    let g = guild(&req).await?;
    let id = member(&req)?;
    Member::delete_many()
        .filter(Member::GID.eq(g.gid.clone()) & Member::MEMBER.eq(id.clone()))
        .execute(req.db())
        .await?;
    req.state::<Hub>()
        .publish(&g.gid, json!({ "type": "member.removed", "id": id }));
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/v1/guilds/:gid/rotate` (admin, avec la clé actuelle) : passe à
/// une nouvelle clé, en excluant éventuellement des membres.
async fn rotate(req: Request) -> Result<Json<Value>> {
    let mut g = guild(&req).await?;
    admin(&req, &g)?;
    let body: Rotation = req.json().await?;
    if body.epoch != g.epoch + 1 {
        return Err(Error::conflict("la clé a déjà été renouvelée entre-temps"));
    }
    if !is_hex(&body.access, SECRET_LEN) {
        return Err(Error::bad_request("jeton d'accès invalide"));
    }
    encrypted(&body.state, MAX_STATE_TOKEN, "état du serveur")?;
    if body.shares.len() > MAX_MEMBERS
        || body.remove.len() > MAX_MEMBERS
        || body.remove.iter().any(|id| !is_hex(id, 16))
    {
        return Err(Error::bad_request("liste de membres invalide"));
    }
    let members = members_of(&req, &g.gid).await?;
    for share in &body.shares {
        if !is_b64_len(&share.blob, WRAPPED_KEY_LEN)
            || !is_b64_len(&share.sender, PUBLIC_KEY_LEN)
            || !members.iter().any(|m| m.member == share.member)
            || body.remove.contains(&share.member)
        {
            return Err(Error::bad_request("clé emballée invalide"));
        }
    }
    let db = req.db();
    let gid = g.gid.clone();
    for id in &body.remove {
        Member::delete_many()
            .filter(Member::GID.eq(gid.clone()) & Member::MEMBER.eq(id.clone()))
            .execute(db)
            .await?;
        if !banned(&req, &gid, id).await? {
            let mut ban = Ban {
                id: 0,
                gid: gid.clone(),
                member: id.clone(),
            };
            ban.insert(db).await?;
        }
    }
    Share::delete_many()
        .filter(Share::GID.eq(gid.clone()))
        .execute(db)
        .await?;
    for share in body.shares {
        let mut row = Share {
            id: 0,
            gid: gid.clone(),
            member: share.member,
            epoch: body.epoch,
            blob: share.blob,
            sender: share.sender,
        };
        row.insert(db).await?;
    }
    g.access_hash = protocol::digest(&body.access);
    g.state = body.state;
    g.epoch = body.epoch;
    g.revision += 1;
    g.update(db).await?;
    req.state::<Hub>()
        .close(&gid, json!({ "type": "rotated", "epoch": g.epoch }));
    Ok(Json(guild_json(&g)))
}

/// `GET /api/v1/guilds/:gid/keys` (secret de membre, sans jeton d'accès : il
/// a justement changé) : la nouvelle clé, emballée pour ce membre. Un exclu
/// n'obtient rien.
async fn my_key(req: Request) -> Result<Json<Value>> {
    let gid = gid(&req)?;
    let id = member(&req)?;
    let not_found = || Error::not_found("aucune clé pour vous sur ce serveur");
    let g = find_guild(&req, &gid).await?.ok_or_else(not_found)?;
    if banned(&req, &gid, &id).await? {
        return Err(Error::forbidden("vous avez été exclu de ce serveur"));
    }
    let share = Share::query()
        .filter(Share::GID.eq(gid.clone()) & Share::MEMBER.eq(id) & Share::EPOCH.eq(g.epoch))
        .first(req.db())
        .await?
        .ok_or_else(not_found)?;
    Ok(Json(json!({
        "epoch": share.epoch,
        "blob": share.blob,
        "sender": share.sender,
    })))
}

/// `GET /api/v1/messages/:gid?before=&limit=` : une page d'historique de tout
/// le serveur, du plus ancien au plus récent (le salon est dans le chiffré).
async fn list_messages(req: Request) -> Result<Json<Value>> {
    let g = guild(&req).await?;
    let limit = req
        .query("limit")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(100)
        .clamp(1, MAX_PAGE);
    let mut filter = Message::GID.eq(g.gid.clone());
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

/// `POST /api/v1/messages/:gid` : un message chiffré, diffusé aussitôt.
async fn send_message(req: Request) -> Result<(u16, Json<Value>)> {
    let g = guild(&req).await?;
    let body: NewMessage = req.json().await?;
    encrypted(&body.body, MAX_MESSAGE_TOKEN, "message")?;
    if !is_hex(&body.owner, SECRET_LEN) {
        return Err(Error::bad_request("preuve d'auteur invalide"));
    }
    let mut message = Message {
        id: 0,
        gid: g.gid.clone(),
        body: body.body,
        owner: body.owner,
        edited: false,
    };
    message.insert(req.db()).await?;
    let json = message_json(&message);
    let mut event = json.clone();
    event["type"] = "message".into();
    req.state::<Hub>().publish(&g.gid, event);
    Ok((201, Json(json)))
}

async fn message_of(req: &Request, gid: &str) -> Result<Message> {
    let mid: i64 = req.param_as("mid")?;
    Message::query()
        .filter(Message::ID.eq(mid) & Message::GID.eq(gid.to_owned()))
        .first(req.db())
        .await?
        .ok_or_else(|| Error::not_found("message introuvable"))
}

/// La preuve d'auteur : son empreinte doit être celle déposée avec le message.
fn author(proof: Option<&str>, message: &Message) -> bool {
    proof
        .filter(|p| is_hex(p, SECRET_LEN))
        .is_some_and(|p| protocol::same(&message.owner, &protocol::digest(p)))
}

/// `PATCH /api/v1/messages/:gid/:mid` : son auteur le corrige.
async fn edit_message(req: Request) -> Result<Json<Value>> {
    let g = guild(&req).await?;
    let mut message = message_of(&req, &g.gid).await?;
    let body: Edit = req.json().await?;
    if !author(Some(&body.proof), &message) {
        return Err(Error::forbidden("seul l'auteur peut modifier ce message"));
    }
    encrypted(&body.body, MAX_MESSAGE_TOKEN, "message")?;
    message.body = body.body;
    message.edited = true;
    message.update(req.db()).await?;
    let json = message_json(&message);
    let mut event = json.clone();
    event["type"] = "message.updated".into();
    req.state::<Hub>().publish(&g.gid, event);
    Ok(Json(json))
}

/// `DELETE /api/v1/messages/:gid/:mid` : son auteur (`X-Alley-Proof`), ou
/// l'admin.
async fn delete_message(req: Request) -> Result<StatusCode> {
    let g = guild(&req).await?;
    let message = message_of(&req, &g.gid).await?;
    if !author(req.header("x-alley-proof"), &message) {
        admin(&req, &g)?;
    }
    let id = message.id;
    message.delete(req.db()).await?;
    req.state::<Hub>()
        .publish(&g.gid, json!({ "type": "message.deleted", "id": id }));
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
        "authors_known": 0,
    })))
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

/// Le flux temps réel d'un serveur : `GET /api/v1/guilds/:gid/live?access=…`
/// (en WebSocket ; les navigateurs ne savent pas y mettre d'en-têtes).
///
/// Le serveur y pousse ses événements ; les membres y envoient
/// `{"type":"relay","data":"[[BROCOLIS:v1:…]]"}`, relayé tel quel à tous.
async fn live(req: Request, socket: ws::WebSocket) {
    let gid = req.param("gid").unwrap_or_default().to_owned();
    let access = req
        .query("access")
        .map(|v| v.into_owned())
        .unwrap_or_default();
    let mut socket = socket;
    let allowed = is_hex(&gid, GUILD_ID_LEN)
        && is_hex(&access, SECRET_LEN)
        && matches!(
            find_guild(&req, &gid).await,
            Ok(Some(g)) if protocol::same(&g.access_hash, &protocol::digest(&access))
        );
    if !allowed {
        let _ = socket.close(4404, "serveur introuvable").await;
        return;
    }
    let hub = req.state::<Hub>();
    let mut inbox = hub.join(&gid);
    let (mut tx, mut rx) = socket.split();

    let forward = tokio::spawn(async move {
        // Un ping toutes les 25 s : le routeur de Heroku coupe une connexion
        // muette au bout de 55 s.
        let mut ping = tokio::time::interval(std::time::Duration::from_secs(25));
        loop {
            tokio::select! {
                event = inbox.recv() => match event {
                    Ok(event) => {
                        if tx.send(event).await.is_err() {
                            break;
                        }
                    }
                    // Trop lent : il a raté des événements, il rechargera.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        if tx.send(json!({ "type": "resync" }).to_string()).await.is_err() {
                            break;
                        }
                    }
                    // Serveur supprimé ou clé renouvelée : on raccroche.
                    Err(_) => {
                        let _ = tx.close(4000, "fermé").await;
                        break;
                    }
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
        if forward.is_finished() {
            break;
        }
        match message {
            ws::Message::Text(text) => {
                if text.len() > MAX_RELAY_TOKEN + 64 {
                    continue;
                }
                let Ok(value) = serde_json::from_str::<Value>(&text) else {
                    continue;
                };
                if value["type"] == "relay"
                    && let Some(data) = value["data"].as_str()
                    && is_token(data, MAX_RELAY_TOKEN)
                {
                    hub.publish(&gid, json!({ "type": "relay", "data": data }));
                }
            }
            ws::Message::Close(_) => break,
            _ => {}
        }
    }
    forward.abort();
    hub.leave(&gid);
}

/// Toutes les routes de l'API.
pub fn routes(app: &mut App) {
    app.post("/api/v1/guilds", create_guild);
    app.get("/api/v1/stats", stats);
    app.get("/api/v1/ice", ice);
    app.get("/api/v1/guilds/:gid", show_guild);
    app.delete("/api/v1/guilds/:gid", delete_guild);
    app.put("/api/v1/guilds/:gid/state", update_state);
    app.put("/api/v1/guilds/:gid/members/me", upsert_member);
    app.delete("/api/v1/guilds/:gid/members/me", leave_guild);
    app.post("/api/v1/guilds/:gid/rotate", rotate);
    app.get("/api/v1/guilds/:gid/keys", my_key);
    app.ws("/api/v1/guilds/:gid/live", live);
    // Sous leur propre préfixe : le bouclier n'accepte les gros corps (images
    // et vocaux chiffrés) que là.
    app.get("/api/v1/messages/:gid", list_messages);
    app.post("/api/v1/messages/:gid", send_message);
    app.patch("/api/v1/messages/:gid/:mid", edit_message);
    app.delete("/api/v1/messages/:gid/:mid", delete_message);
}
