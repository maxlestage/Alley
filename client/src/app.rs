//! Ce que fait l'application : créer et rejoindre des serveurs, renouveler
//! leur clé, envoyer et recevoir des messages, le temps réel, les appels.

use std::rc::Rc;

use serde_json::{Value, json};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{CloseEvent, HtmlMediaElement, MessageEvent, WebSocket};

use crate::call::{self, Call};
use crate::crypto::{self, Key};
use crate::dom::*;
use crate::model::*;
use crate::net::{ApiError, ApiResult, needs_server, request, ws_base};
use crate::ui;

// ---------------------------------------------------------------------------
// L'API, vue d'un serveur

pub async fn api(
    g: &G,
    method: &str,
    path: &str,
    body: Option<Value>,
    extra: Vec<(&str, String)>,
) -> ApiResult {
    let mut headers = {
        let g = g.borrow();
        let mut h = vec![("x-alley-access", g.access.clone())];
        if let Some(a) = &g.entry.admin {
            h.push(("x-alley-admin", a.clone()));
        }
        h
    };
    headers.extend(extra);
    request(method, path, &headers, body.as_ref()).await
}

fn member_header(g: &G) -> Vec<(&'static str, String)> {
    vec![("x-alley-member", g.borrow().entry.member.clone())]
}

fn local_error(message: &str) -> ApiError {
    ApiError::new(0, message)
}

// ---------------------------------------------------------------------------
// Serveurs : créer, rejoindre, charger, quitter

/// Le profil chiffré : pseudo et clés publiques, lié à l'identifiant du membre.
pub async fn announce(g: &G) -> ApiResult<()> {
    let name = app(|a| a.saved.name.clone());
    let (gid, profile) = {
        let g = g.borrow();
        (g.gid.clone(), g.my_profile(&name))
    };
    let extra = member_header(g);
    api(
        g,
        "PUT",
        &format!("/api/v1/guilds/{gid}/members/me"),
        Some(json!({ "profile": profile })),
        extra,
    )
    .await?;
    let mut gm = g.borrow_mut();
    let p = Profile {
        id: gm.me.clone(),
        name,
        x: Some(gm.identity.public_b64()),
        s: Some(gm.signer.public_b64()),
    };
    gm.members.insert(p.id.clone(), p);
    gm.announced = true;
    Ok(())
}

pub async fn create_guild(name: &str) -> ApiResult<G> {
    let key = crypto::generate_key();
    let d = crypto::derive(&key);
    let admin = crypto::secret();
    let mut entry = Entry::new(&key, d.gid.clone(), Some(admin.clone()));
    entry.name = name.to_owned();
    let rid = || crypto::hex(&crypto::random_bytes::<8>());
    let state = GuildState {
        name: name.to_owned(),
        channels: vec![
            Channel {
                id: rid(),
                name: "général".into(),
                topic: String::new(),
                kind: "text".into(),
            },
            Channel {
                id: rid(),
                name: "Salon vocal".into(),
                topic: String::new(),
                kind: "voice".into(),
            },
        ],
        keys: Vec::new(),
    };
    let token = crypto::seal_token(&key, &serde_json::to_string(&state).unwrap_or_default());
    request(
        "POST",
        "/api/v1/guilds",
        &[],
        Some(&json!({ "gid": d.gid, "access": d.access, "admin": admin, "state": token })),
    )
    .await?;
    let g: G = Rc::new(std::cell::RefCell::new(
        Guild::new(entry).ok_or_else(|| local_error("identité"))?,
    ));
    app(|a| a.guilds.push(g.clone()));
    save();
    Ok(g)
}

/// Rejoint un serveur à partir d'un lien d'invitation.
pub async fn join_from_link(link: &str) -> ApiResult<G> {
    let link = link.trim();
    let fragment = link.split_once('#').map(|(_, f)| f).unwrap_or_default();
    let mut key_text = None;
    let mut gid = None;
    let mut admin = None;
    for pair in fragment.split('&') {
        match pair.split_once('=') {
            Some(("rejoindre", v)) => key_text = Some(v.to_owned()),
            Some(("g", v)) if v.len() == 32 && v.bytes().all(|b| b.is_ascii_hexdigit()) => {
                gid = Some(v.to_owned())
            }
            Some(("admin", v)) if v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit()) => {
                admin = Some(v.to_owned())
            }
            _ => {}
        }
    }
    let key_text =
        key_text.ok_or_else(|| local_error("Ce lien ne contient pas d'invitation Alley."))?;
    let key: Key = crypto::key_from_b64url(&key_text)
        .ok_or_else(|| local_error("La clé de ce lien est abîmée."))?;
    let gid = gid.unwrap_or_else(|| crypto::derive(&key).gid);
    if let Some(existing) = guild(&gid) {
        {
            let mut e = existing.borrow_mut();
            let b = crypto::b64url(&key);
            if !e.entry.keys.contains(&b) {
                let mut keys = vec![b];
                keys.extend(e.entry.keys.clone());
                e.set_keys(keys);
            }
            if admin.is_some() && e.entry.admin.is_none() {
                e.entry.admin = admin;
                toast("Droits d'administration ajoutés.", false);
            }
        }
        save();
        return Ok(existing);
    }
    let entry = Entry::new(&key, gid, admin);
    let g: G = Rc::new(std::cell::RefCell::new(
        Guild::new(entry).ok_or_else(|| local_error("identité"))?,
    ));
    g.borrow_mut().joining = true;
    let res = refresh_guild(&g).await;
    g.borrow_mut().joining = false;
    res?;
    app(|a| a.guilds.push(g.clone()));
    save();
    Ok(g)
}

pub async fn refresh_guild(g: &G) -> ApiResult<()> {
    let gid = g.borrow().gid.clone();
    let data = match api(g, "GET", &format!("/api/v1/guilds/{gid}"), None, vec![]).await {
        Ok(d) => d,
        Err(err) => {
            // La clé a peut-être été renouvelée pendant notre absence.
            let may_adopt = {
                let g = g.borrow();
                err.status == 404 && !g.joining && !g.adopting
            };
            if may_adopt && Box::pin(adopt_share(g)).await {
                return Ok(());
            }
            return Err(err);
        }
    };
    {
        let mut gm = g.borrow_mut();
        gm.admin = data["admin"].as_bool().unwrap_or_default();
        gm.apply_state(&data);
        let profiles: Vec<Profile> = data["members"]
            .as_array()
            .map(|rows| rows.iter().filter_map(|r| gm.read_profile(r)).collect())
            .unwrap_or_default();
        gm.members = profiles.into_iter().map(|p| (p.id.clone(), p)).collect();
        gm.loaded = true;
        gm.verify_pending();
    }
    save();
    Ok(())
}

/// La clé a été renouvelée : récupère la nouvelle, emballée pour nous. On ne
/// l'adopte que si l'état qu'elle ouvre contient notre clé actuelle : seul
/// quelqu'un qui la connaissait (pas le serveur) a pu le fabriquer.
pub async fn adopt_share(g: &G) -> bool {
    if g.borrow().adopting {
        return false;
    }
    g.borrow_mut().adopting = true;
    let ok = adopt(g).await;
    g.borrow_mut().adopting = false;
    ok
}

async fn adopt(g: &G) -> bool {
    let (gid, extra) = (g.borrow().gid.clone(), member_header(g));
    let share = match request("GET", &format!("/api/v1/guilds/{gid}/keys"), &extra, None).await {
        Ok(s) => s,
        Err(err) => {
            if err.status == 403 {
                g.borrow_mut().excluded = true;
            }
            return false;
        }
    };
    let (Some(blob), Some(sender)) = (share["blob"].as_str(), share["sender"].as_str()) else {
        return false;
    };
    let Some(key) = g.borrow().identity.unwrap_group_key(blob, sender) else {
        return false;
    };
    let d = crypto::derive(&key);
    let Ok(data) = request(
        "GET",
        &format!("/api/v1/guilds/{gid}"),
        &[("x-alley-access", d.access)],
        None,
    )
    .await
    else {
        return false;
    };
    let state: Option<GuildState> = data["state"]
        .as_str()
        .and_then(|t| crypto::open_token(&key, t))
        .and_then(|t| serde_json::from_str(&t).ok());
    let current = g.borrow().entry.keys[0].clone();
    let Some(state) = state.filter(|s| s.keys.contains(&current)) else {
        toast(
            "Une clé suspecte a été refusée : elle ne vient pas d'un membre du serveur.",
            true,
        );
        return false;
    };
    {
        let mut gm = g.borrow_mut();
        let mut keys = vec![crypto::b64url(&key)];
        keys.extend(gm.entry.keys.clone());
        keys.extend(state.keys);
        gm.set_keys(keys);
    }
    save();
    if refresh_guild(g).await.is_err() {
        return false;
    }
    let (name, fp) = {
        let g = g.borrow();
        (g.name.clone(), g.fingerprint.clone())
    };
    toast(
        format!("La clé de « {name} » a été renouvelée. Nouvelle empreinte : {fp}"),
        false,
    );
    true
}

/// Admin : nouvelle clé, remise à chaque membre restant ; `remove` est exclu.
pub async fn rotate_key(g: &G, remove: Vec<String>) -> ApiResult<()> {
    refresh_guild(g).await?;
    let key = crypto::generate_key();
    let d = crypto::derive(&key);
    let (gid, epoch, body, missing) = {
        let gm = g.borrow();
        let mut state = gm.state.clone();
        for k in &gm.entry.keys {
            if !state.keys.contains(k) {
                state.keys.push(k.clone());
            }
        }
        let mut shares = Vec::new();
        let mut missing = Vec::new();
        for (id, p) in &gm.members {
            if remove.contains(id) {
                continue;
            }
            match p
                .x
                .as_deref()
                .and_then(|x| gm.identity.wrap_group_key(&key, x))
            {
                Some(blob) => shares.push(
                    json!({ "member": id, "blob": blob, "sender": gm.identity.public_b64() }),
                ),
                None => missing.push(p.name.clone()),
            }
        }
        let body = json!({
            "epoch": gm.epoch + 1,
            "access": d.access,
            "state": crypto::seal_token(&key, &serde_json::to_string(&state).unwrap_or_default()),
            "remove": remove,
            "shares": shares,
        });
        (gm.gid.clone(), gm.epoch, body, missing)
    };
    let _ = epoch;
    if app(|a| a.call.as_ref().is_some_and(|c| c.gid == gid)) {
        leave_call();
    }
    g.borrow_mut().rotating = true;
    let res = api(
        g,
        "POST",
        &format!("/api/v1/guilds/{gid}/rotate"),
        Some(body),
        vec![],
    )
    .await;
    if res.is_ok() {
        disconnect(g);
        let mut gm = g.borrow_mut();
        let mut keys = vec![crypto::b64url(&key)];
        keys.extend(gm.entry.keys.clone());
        gm.set_keys(keys);
        for id in &remove {
            gm.members.remove(id);
            gm.presence.remove(id);
        }
    }
    g.borrow_mut().rotating = false;
    res?;
    save();
    if !missing.is_empty() {
        toast(
            format!(
                "Sans clé X25519, à réinviter par lien : {}",
                missing.join(", ")
            ),
            true,
        );
    }
    refresh_guild(g).await?;
    connect(g);
    ui::render();
    Ok(())
}

/// Admin : modifie l'état chiffré (nom, salons), en reprenant si besoin.
pub async fn put_state(g: &G, change: impl Fn(GuildState) -> GuildState) -> ApiResult<()> {
    for attempt in 0..2 {
        let (gid, body) = {
            let gm = g.borrow();
            let next = change(gm.state.clone());
            (
                gm.gid.clone(),
                json!({ "state": gm.enc(&serde_json::to_string(&next).unwrap_or_default()), "revision": gm.revision }),
            )
        };
        match api(
            g,
            "PUT",
            &format!("/api/v1/guilds/{gid}/state"),
            Some(body),
            vec![],
        )
        .await
        {
            Ok(data) => {
                g.borrow_mut().apply_state(&data);
                save();
                ui::render();
                return Ok(());
            }
            Err(err) if err.status == 409 && attempt == 0 => refresh_guild(g).await?,
            Err(err) => return Err(err),
        }
    }
    Ok(())
}

pub fn disconnect(g: &G) {
    let socket = g.borrow_mut().socket.take();
    if let Some(s) = socket {
        // Ses écouteurs partent avec elle : plus rien ne doit les appeler.
        s.ws.set_onopen(None);
        s.ws.set_onmessage(None);
        s.ws.set_onclose(None);
        let _ = s.ws.close();
    }
}

pub fn forget(g: &G, message: &str) {
    let gid = g.borrow().gid.clone();
    if app(|a| a.call.as_ref().is_some_and(|c| c.gid == gid)) {
        leave_call();
    }
    disconnect(g);
    app(|a| {
        a.guilds.retain(|x| !Rc::ptr_eq(x, g));
        if a.current.as_deref() == Some(gid.as_str()) {
            a.current = a.guilds.first().map(|x| x.borrow().gid.clone());
        }
    });
    save();
    if !message.is_empty() {
        toast(message, false);
    }
    ui::render();
}

pub fn lost(g: &G) {
    let (name, excluded) = {
        let g = g.borrow();
        (g.name.clone(), g.excluded)
    };
    if excluded {
        forget(g, &format!("Vous avez été exclu de « {name} »."));
    } else {
        forget(g, &format!("« {name} » n'est plus accessible."));
    }
}

// ---------------------------------------------------------------------------
// Messages

/// Charge une page d'historique du serveur (tous salons confondus : le salon
/// est dans le chiffré).
pub async fn load_page(g: &G, older: bool) -> ApiResult<()> {
    let (gid, before) = {
        let gm = g.borrow();
        if gm.loading_page || (older && !gm.more) {
            return Ok(());
        }
        let before = match (older, gm.oldest) {
            (true, Some(o)) => format!("&before={o}"),
            _ => String::new(),
        };
        (gm.gid.clone(), before)
    };
    g.borrow_mut().loading_page = true;
    let res = api(
        g,
        "GET",
        &format!("/api/v1/messages/{gid}?limit=200{before}"),
        None,
        vec![],
    )
    .await;
    let mut gm = g.borrow_mut();
    gm.loading_page = false;
    let data = res?;
    let rows = data["messages"].as_array().cloned().unwrap_or_default();
    for raw in &rows {
        let m = gm.decrypt_message(raw);
        gm.msgs.insert(m.id, m);
    }
    if let Some(first) = rows.first().and_then(|r| r["id"].as_i64())
        && gm.oldest.is_none_or(|o| first < o)
    {
        gm.oldest = Some(first);
    }
    if older || !gm.page_loaded {
        gm.more = data["more"].as_bool().unwrap_or_default();
    }
    gm.page_loaded = true;
    Ok(())
}

pub async fn send(g: &G, channel: &str, kind: &str, x: &str, mime: &str) -> ApiResult<()> {
    let reply = app(|a| a.reply_to).unwrap_or(0);
    let salt = crypto::secret();
    let (gid, body, owner) = {
        let gm = g.borrow();
        let env = json!({ "c": channel, "t": now(), "k": kind, "x": x, "m": mime, "r": reply, "s": salt, "e": 0 });
        (
            gm.gid.clone(),
            gm.seal(env),
            crypto::sha256_hex(&gm.proof(&salt)),
        )
    };
    let raw = api(
        g,
        "POST",
        &format!("/api/v1/messages/{gid}"),
        Some(json!({ "body": body, "owner": owner })),
        vec![],
    )
    .await?;
    // Affiché tout de suite, sans attendre l'écho du temps réel.
    let id = raw["id"].as_i64().unwrap_or_default();
    let fresh = {
        let mut gm = g.borrow_mut();
        if gm.msgs.contains_key(&id) {
            false
        } else {
            let m = gm.decrypt_message(&raw);
            gm.msgs.insert(id, m);
            true
        }
    };
    if fresh && is_current(&gid) && g.borrow().entry.channel.as_deref() == Some(channel) {
        ui::render_messages(true);
    }
    Ok(())
}

pub async fn edit_message(g: &G, id: i64, text: &str) -> ApiResult<()> {
    let (gid, body, proof) = {
        let gm = g.borrow();
        let m = gm
            .msgs
            .get(&id)
            .cloned()
            .ok_or_else(|| local_error("message introuvable"))?;
        let env = json!({ "c": m.channel, "t": m.t, "k": "text", "x": text, "m": "", "r": m.reply, "s": m.salt, "e": now() });
        (gm.gid.clone(), gm.seal(env), gm.proof(&m.salt))
    };
    api(
        g,
        "PATCH",
        &format!("/api/v1/messages/{gid}/{id}"),
        Some(json!({ "body": body, "proof": proof })),
        vec![],
    )
    .await?;
    Ok(())
}

pub async fn delete_message(g: &G, id: i64) {
    if !confirm("Supprimer ce message pour tout le monde ?") {
        return;
    }
    let (gid, extra) = {
        let gm = g.borrow();
        let extra = match gm.msgs.get(&id) {
            Some(m) if m.author == gm.me => vec![("x-alley-proof", gm.proof(&m.salt))],
            _ => vec![],
        };
        (gm.gid.clone(), extra)
    };
    if let Err(err) = api(
        g,
        "DELETE",
        &format!("/api/v1/messages/{gid}/{id}"),
        None,
        extra,
    )
    .await
    {
        toast(err.message, true);
    }
}

// ---------------------------------------------------------------------------
// Temps réel : événements du serveur, et relais chiffrés et signés entre
// membres (présence, « j'écris », signalisation des appels). Le serveur ne
// sait pas qui est connecté.

pub fn connect(g: &G) {
    let (gid, access, generation) = {
        let mut gm = g.borrow_mut();
        if gm.socket.is_some() {
            return;
        }
        gm.generation += 1;
        (gm.gid.clone(), gm.access.clone(), gm.generation)
    };
    if guild(&gid).is_none() {
        return;
    }
    let Ok(ws) = WebSocket::new(&format!(
        "{}/api/v1/guilds/{gid}/live?access={access}",
        ws_base()
    )) else {
        return;
    };
    let mut listeners = Vec::new();

    let weak = Rc::downgrade(g);
    let on_open = Closure::<dyn FnMut(JsValue)>::new(move |_| {
        let Some(g) = weak.upgrade() else { return };
        spawn(async move {
            let reload = {
                let mut gm = g.borrow_mut();
                gm.retry = 0;
                gm.page_loaded
            };
            // Ce qui a pu arriver pendant la coupure.
            if reload {
                resync(&g).await;
            }
            here(&g, true);
            ui::render_status();
            // Présence : un signe de vie toutes les 25 s.
            loop {
                sleep(25_000).await;
                let alive = g
                    .borrow()
                    .socket
                    .as_ref()
                    .is_some_and(|s| s.generation == generation);
                if !alive {
                    break;
                }
                here(&g, false);
            }
        });
    });
    ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));
    listeners.push(on_open);

    let weak = Rc::downgrade(g);
    let on_message = Closure::<dyn FnMut(JsValue)>::new(move |e: JsValue| {
        let Some(g) = weak.upgrade() else { return };
        let e: MessageEvent = e.unchecked_into();
        let Some(text) = e.data().as_string() else {
            return;
        };
        let Ok(event) = serde_json::from_str::<Value>(&text) else {
            return;
        };
        spawn(async move { on_event(&g, event).await });
    });
    ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    listeners.push(on_message);

    let weak = Rc::downgrade(g);
    let on_close = Closure::<dyn FnMut(JsValue)>::new(move |e: JsValue| {
        let Some(g) = weak.upgrade() else { return };
        let code = e
            .dyn_ref::<CloseEvent>()
            .map(CloseEvent::code)
            .unwrap_or_default();
        // Pas de `take()` ici : nous sommes dans un de ses écouteurs.
        if g.borrow()
            .socket
            .as_ref()
            .is_none_or(|s| s.generation != generation)
        {
            return;
        }
        spawn(async move {
            let old = g.borrow_mut().socket.take();
            drop(old);
            if code == 4404 {
                // La clé a changé pendant que nous étions déconnectés ?
                if adopt_share(&g).await {
                    connect(&g);
                } else {
                    lost(&g);
                }
                return;
            }
            let retry = {
                let mut gm = g.borrow_mut();
                gm.retry = (gm.retry + 1).min(6);
                gm.retry
            };
            ui::render_status();
            sleep(500 * 2_i32.pow(retry)).await;
            connect(&g);
        });
    });
    ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));
    listeners.push(on_close);

    g.borrow_mut().socket = Some(Socket {
        ws,
        generation,
        _listeners: listeners,
    });
}

pub fn relay(g: &G, payload: Value) {
    let gm = g.borrow();
    let Some(socket) = &gm.socket else { return };
    if socket.ws.ready_state() != WebSocket::OPEN {
        return;
    }
    let data = gm.relay_token(payload);
    let _ = socket
        .ws
        .send_with_str(&json!({ "type": "relay", "data": data }).to_string());
}

pub fn here(g: &G, ask: bool) {
    let gid = g.borrow().gid.clone();
    let (voice, muted, video) = app(|a| match &a.call {
        Some(c) if c.gid == gid => {
            let call = c.call.borrow();
            (
                Some(c.channel.clone()),
                call.muted,
                call.camera.is_some() || call.screen.is_some(),
            )
        }
        _ => (None, false, false),
    });
    relay(
        g,
        json!({ "type": "here", "q": u8::from(ask), "voice": voice, "muted": muted, "video": video }),
    );
}

pub fn send_typing(g: &G) {
    let t = now();
    if app(|a| t - a.last_typing < 3000.0) {
        return;
    }
    app(|a| a.last_typing = t);
    let channel = g.borrow().entry.channel.clone();
    relay(g, json!({ "type": "typing", "c": channel }));
}

async fn on_relay(g: &G, token: &str) {
    let mut opened = g.borrow().open_relay(token);
    if opened.is_err() && !g.borrow().refreshing {
        // Un nouveau membre dont le profil n'est pas encore arrivé : on
        // recharge les membres, puis on relit ce message une fois.
        g.borrow_mut().refreshing = true;
        let _ = refresh_guild(g).await;
        g.borrow_mut().refreshing = false;
        if is_current(&g.borrow().gid) {
            ui::render_members();
        }
        opened = g.borrow().open_relay(token);
    }
    let Ok(Some(p)) = opened else { return };
    let author = p["a"].as_str().unwrap_or_default().to_owned();
    let gid = g.borrow().gid.clone();
    let current = is_current(&gid);
    match p["type"].as_str() {
        Some("here") => {
            let fresh = Presence {
                seen: now(),
                voice: p["voice"].as_str().map(str::to_owned),
                muted: p["muted"].as_bool().unwrap_or_default(),
                video: p["video"].as_bool().unwrap_or_default(),
            };
            let changed = {
                let mut gm = g.borrow_mut();
                let before = gm.presence.insert(author, fresh.clone());
                before.is_none_or(|b| {
                    b.voice != fresh.voice || b.muted != fresh.muted || b.video != fresh.video
                })
            };
            if p["q"].as_u64() == Some(1) {
                here(g, false);
            }
            if changed {
                sync_call();
                if current {
                    ui::render_channels();
                    ui::render_members();
                    ui::render_voice();
                }
            }
        }
        Some("bye") => {
            g.borrow_mut().presence.remove(&author);
            sync_call();
            if current {
                ui::render_channels();
                ui::render_members();
                ui::render_voice();
            }
        }
        Some("typing") => {
            let channel = p["c"].as_str().unwrap_or_default().to_owned();
            g.borrow_mut()
                .typing
                .insert(author, (channel, now() + 6000.0));
            if current {
                ui::render_typing();
                spawn(async move {
                    sleep(6100).await;
                    if is_current(&gid) {
                        ui::render_typing();
                    }
                });
            }
        }
        Some("signal") => {
            let me = g.borrow().me.clone();
            let call = app(|a| match &a.call {
                Some(c) if c.gid == gid && p["c"].as_str() == Some(c.channel.as_str()) => {
                    Some(c.call.clone())
                }
                _ => None,
            });
            if p["to"].as_str() == Some(me.as_str())
                && let Some(call) = call
            {
                call::receive(&call, &author, p["d"].clone()).await;
                sync_call();
            }
        }
        _ => {}
    }
}

async fn resync(g: &G) {
    match refresh_guild(g).await {
        Ok(()) => {
            let _ = load_page(g, false).await;
            ui::render();
        }
        Err(err) if err.status == 404 => lost(g),
        Err(_) => {}
    }
}

async fn on_event(g: &G, event: Value) {
    let gid = g.borrow().gid.clone();
    let current = is_current(&gid);
    match event["type"].as_str().unwrap_or_default() {
        "relay" => {
            if let Some(data) = event["data"].as_str() {
                on_relay(g, data).await;
            }
        }
        "message" => {
            let m = g.borrow().decrypt_message(&event);
            g.borrow_mut().msgs.insert(m.id, m.clone());
            if m.locked {
                return;
            }
            let (mine, here_now) = {
                let mut gm = g.borrow_mut();
                gm.typing.remove(&m.author);
                (
                    m.author == gm.me,
                    gm.entry.channel.as_deref() == Some(m.channel.as_str()),
                )
            };
            let visible = current
                && here_now
                && document().visibility_state() == web_sys::VisibilityState::Visible;
            if !mine && !visible {
                *g.borrow_mut().unread.entry(m.channel.clone()).or_default() += 1;
                notify(g, &m);
            }
            if current && here_now {
                ui::render_messages(true);
                ui::render_typing();
            }
            ui::render_rail();
            if current {
                ui::render_channels();
            }
            update_badge();
        }
        "message.updated" => {
            let m = g.borrow().decrypt_message(&event);
            let here_now = g.borrow().entry.channel.as_deref() == Some(m.channel.as_str());
            g.borrow_mut().msgs.insert(m.id, m);
            if current && here_now {
                ui::render_messages(false);
            }
        }
        "message.deleted" => {
            if let Some(id) = event["id"].as_i64() {
                g.borrow_mut().msgs.remove(&id);
            }
            if current {
                ui::render_messages(false);
            }
        }
        "state" => {
            g.borrow_mut().apply_state(&event);
            save();
            ui::render();
        }
        "member" => {
            {
                let mut gm = g.borrow_mut();
                if let Some(p) = gm.read_profile(&event["member"]) {
                    gm.members.insert(p.id.clone(), p);
                    gm.verify_pending();
                }
            }
            if current {
                ui::render();
            }
        }
        "member.removed" => {
            if let Some(id) = event["id"].as_str() {
                let mut gm = g.borrow_mut();
                gm.members.remove(id);
                gm.presence.remove(id);
            }
            if current {
                ui::render();
            }
        }
        "rotated" => {
            // L'admin a changé de clé : on va chercher la nôtre (sauf si c'est lui).
            let skip = {
                let gm = g.borrow();
                gm.rotating
                    || gm.adopting
                    || event["epoch"].as_i64().unwrap_or_default() <= gm.epoch
            };
            if skip {
                return;
            }
            disconnect(g);
            if app(|a| a.call.as_ref().is_some_and(|c| c.gid == gid)) {
                leave_call();
            }
            if adopt_share(g).await {
                connect(g);
                ui::render();
            } else {
                lost(g);
            }
        }
        "guild.deleted" => {
            let name = g.borrow().name.clone();
            forget(g, &format!("Le serveur « {name} » a été supprimé."));
        }
        "resync" => resync(g).await,
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Appels audio et vidéo

pub fn voice_members(g: &Guild, channel: &str) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    if app(|a| {
        a.call
            .as_ref()
            .is_some_and(|c| c.gid == g.gid && c.channel == channel)
    }) {
        ids.push(g.me.clone());
    }
    let mut others: Vec<String> = g
        .presence
        .iter()
        .filter(|(id, p)| p.voice.as_deref() == Some(channel) && g.online(id))
        .map(|(id, _)| id.clone())
        .collect();
    others.sort();
    for id in others {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

async fn ice_servers() -> Value {
    if let Some(ice) = app(|a| a.ice.clone()) {
        return ice;
    }
    let ice = request("GET", "/api/v1/ice", &[], None)
        .await
        .ok()
        .map(|d| d["iceServers"].clone())
        .filter(Value::is_array)
        .unwrap_or_else(|| json!([{ "urls": ["stun:stun.cloudflare.com:3478", "stun:stun.l.google.com:19302"] }]));
    app(|a| a.ice = Some(ice.clone()));
    ice
}

pub async fn join_call(g: &G, channel: &str) {
    if app(|a| a.call.is_some()) {
        leave_call();
    }
    let ok = window().navigator().media_devices().is_ok()
        && js_sys::Reflect::has(&window(), &"RTCPeerConnection".into()).unwrap_or(false);
    if !ok {
        toast("Ce navigateur ne permet pas les appels.", true);
        return;
    }
    let ice = ice_servers().await;
    let (me, gid) = {
        let gm = g.borrow();
        (gm.me.clone(), gm.gid.clone())
    };
    let weak = Rc::downgrade(g);
    let ch = channel.to_owned();
    let c = Call::new(
        me,
        &ice,
        move |to, d| {
            if let Some(g) = weak.upgrade() {
                relay(&g, json!({ "type": "signal", "to": to, "c": ch, "d": d }));
            }
        },
        || {
            ui::render_voice();
            ui::render_call_panel();
        },
    );
    if call::start(&c).await.is_err() {
        toast(
            "Micro inaccessible : autorisez-le dans les réglages du navigateur.",
            true,
        );
        return;
    }
    app(|a| {
        a.call = Some(ActiveCall {
            gid,
            channel: channel.to_owned(),
            call: c,
        })
    });
    here(g, false);
    sync_call();
    ui::render();
}

pub fn leave_call() {
    let Some(c) = app(|a| a.call.take()) else {
        return;
    };
    call::stop(&c.call);
    ui::clear_videos();
    if let Some(box_) = by_id("call-media") {
        box_.set_text_content(None);
    }
    if let Some(g) = guild(&c.gid) {
        here(&g, false);
    }
    ui::render();
}

/// Relie l'appel aux participants annoncés, et branche leur son.
pub fn sync_call() {
    let Some((gid, channel, c)) = app(|a| {
        a.call
            .as_ref()
            .map(|c| (c.gid.clone(), c.channel.clone(), c.call.clone()))
    }) else {
        return;
    };
    let Some(g) = guild(&gid) else { return };
    let ids = voice_members(&g.borrow(), &channel);
    call::sync(&c, &ids);
    // Le son des autres : un <audio> par participant, hors du rendu.
    let Some(box_) = by_id("call-media") else {
        return;
    };
    let peers: Vec<(String, web_sys::MediaStream)> = c
        .borrow()
        .peers
        .iter()
        .map(|(id, p)| (id.clone(), Clone::clone(&p.stream)))
        .collect();
    for (id, stream) in &peers {
        let el = match query(&format!("#call-media audio[data-id=\"{id}\"]")) {
            Some(el) => el,
            None => {
                let el = h("audio")
                    .attr("autoplay", "")
                    .attr("playsinline", "")
                    .data("id", id)
                    .el();
                let _ = box_.append_child(&el);
                el
            }
        };
        let media: &HtmlMediaElement = el.unchecked_ref();
        let same = media.src_object().is_some_and(|s| s == *stream);
        if !same {
            media.set_src_object(Some(stream));
        }
        play(media);
    }
    for el in query_all("#call-media audio") {
        let id = el.get_attribute("data-id").unwrap_or_default();
        if !peers.iter().any(|(p, _)| *p == id) {
            el.remove();
        }
    }
}

// ---------------------------------------------------------------------------
// Notifications et badge

fn notify(g: &G, m: &Msg) {
    if !app(|a| a.saved.notify)
        || document().visibility_state() == web_sys::VisibilityState::Visible
    {
        return;
    }
    if web_sys::Notification::permission() != web_sys::NotificationPermission::Granted {
        return;
    }
    let (title, body, tag) = {
        let gm = g.borrow();
        let channel = gm
            .channel(&m.channel)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        let body = match m.kind.as_str() {
            "image" => "📷 Photo".to_owned(),
            "audio" => "🎤 Message vocal".to_owned(),
            _ => m.text.chars().take(140).collect(),
        };
        (
            format!("{} · #{channel} — {}", gm.name_of(&m.author), gm.name),
            body,
            format!("{}:{}", gm.gid, m.channel),
        )
    };
    let options = web_sys::NotificationOptions::new();
    options.set_body(&body);
    options.set_tag(&tag);
    options.set_icon(&format!("{}/assets/icon-192.png", base()));
    match web_sys::Notification::new_with_options(&title, &options) {
        Ok(n) => {
            let (gid, channel) = (g.borrow().gid.clone(), m.channel.clone());
            let click = Closure::<dyn FnMut(JsValue)>::new(move |_| {
                let _ = window().focus();
                let (gid, channel) = (gid.clone(), channel.clone());
                spawn(async move { select_guild(&gid, Some(channel)).await });
            });
            n.set_onclick(Some(click.as_ref().unchecked_ref()));
            click.forget();
        }
        Err(_) => {
            // Android et les apps installées : par le service worker.
            if let Ok(sw) = js_sys::Reflect::get(&window().navigator(), &"serviceWorker".into())
                && !sw.is_undefined()
            {
                let ready = get(&sw, "ready");
                spawn(async move {
                    if let Ok(reg) = await_promise(Ok(ready)).await {
                        let _ = call(&reg, "showNotification", &[title.into(), options.into()]);
                    }
                });
            }
        }
    }
}

pub fn update_badge() {
    let n: u32 = app(|a| a.guilds.iter().map(|g| g.borrow().unread_total()).sum());
    document().set_title(&if n > 0 {
        format!("({n}) Alley")
    } else {
        "Alley".into()
    });
    let nav: JsValue = window().navigator().into();
    if has(&nav, "setAppBadge") {
        let _ = if n > 0 {
            call(&nav, "setAppBadge", &[JsValue::from(n)])
        } else {
            call(&nav, "clearAppBadge", &[])
        };
    }
}

// ---------------------------------------------------------------------------
// Navigation

pub async fn select_guild(gid: &str, channel: Option<String>) {
    let Some(g) = guild(gid) else { return };
    app(|a| {
        a.current = Some(gid.to_owned());
        a.reply_to = None;
        a.editing = None;
    });
    if let Some(c) = channel {
        g.borrow_mut().entry.channel = Some(c);
    }
    save();
    let (loaded, announced) = {
        let gm = g.borrow();
        (gm.loaded, gm.announced)
    };
    if !loaded || !announced {
        let res = async {
            if !loaded {
                refresh_guild(&g).await?;
            }
            if !g.borrow().announced {
                announce(&g).await?;
            }
            Ok::<_, ApiError>(())
        }
        .await;
        if let Err(err) = res {
            if err.status == 404 || err.status == 403 {
                return lost(&g);
            }
            toast(err.message, true);
        }
    }
    connect(&g);
    ui::render();
    if !g.borrow().page_loaded
        && let Err(err) = load_page(&g, false).await
    {
        toast(err.message, true);
    }
    let channel = g.borrow().entry.channel.clone();
    if let Some(c) = channel {
        open_channel(&g, &c);
    }
}

pub fn select_channel(g: &G, cid: &str) {
    g.borrow_mut().entry.channel = Some(cid.to_owned());
    app(|a| {
        a.reply_to = None;
        a.editing = None;
    });
    save();
    if ui::narrow() {
        ui::toggle_drawer(false);
    }
    ui::render_channels();
    ui::render_chat();
    open_channel(g, cid);
}

pub fn open_channel(g: &G, cid: &str) {
    g.borrow_mut().unread.remove(cid);
    update_badge();
    ui::render_rail();
    ui::render_channels();
    ui::render_messages(true);
    if !ui::touch()
        && let Some(i) = by_id("input")
    {
        focus(&i);
    }
}

pub async fn handle_invite() {
    let Some(link) = app(|a| a.pending_invite.take()) else {
        return;
    };
    match join_from_link(&link).await {
        Ok(g) => {
            let (gid, name) = {
                let g = g.borrow();
                (g.gid.clone(), g.name.clone())
            };
            select_guild(&gid, None).await;
            toast(format!("Bienvenue sur « {name} » !"), false);
        }
        Err(err) => {
            toast(
                if err.status == 404 {
                    "Serveur introuvable : le lien a peut-être été renouvelé.".into()
                } else {
                    err.message
                },
                true,
            );
            ui::render();
        }
    }
}

/// Démarre l'application.
pub async fn start() {
    let body = document().body().expect("body");
    let _ = body.append_child(&h("div#toasts").attr("aria-live", "polite").el());
    let _ = body.append_child(&h("div#call-media").attr("hidden", "").el());
    ui::track_viewport();
    if !window().is_secure_context() {
        return ui::insecure();
    }
    let base = meta("alley-base");
    register_worker(&base);
    let saved = load();
    app(|a| {
        a.base = base.clone();
        a.current = saved.last.clone();
        a.show_members = !ui::narrow();
        a.saved = saved.clone();
    });
    for entry in saved.guilds {
        if let Some(g) = Guild::new(entry) {
            app(|a| a.guilds.push(Rc::new(std::cell::RefCell::new(g))));
        }
    }
    // Le lien d'invitation : la clé est dans le fragment, qu'on efface
    // aussitôt de la barre d'adresse et de l'historique.
    let location = window().location();
    let hash = location.hash().unwrap_or_default();
    let clean = |_: ()| {
        let _ = window()
            .history()
            .map(|h| h.replace_state_with_url(&JsValue::NULL, "", Some(&format!("{base}/app/"))));
    };
    if hash.contains("rejoindre=") {
        let href = location.href().unwrap_or_default();
        app(|a| a.pending_invite = Some(href));
        clean(());
    }
    let wants_new = hash == "#nouveau";
    if wants_new {
        clean(());
    }

    // Les écouteurs permanents.
    listen_forever(&window(), "pagehide", |_| {
        for g in app(|a| a.guilds.clone()) {
            relay(&g, json!({ "type": "bye" }));
        }
    });
    listen_forever(&window(), "online", |_| {
        for g in app(|a| a.guilds.clone()) {
            let ready = {
                let gm = g.borrow();
                gm.socket.is_none() && gm.loaded
            };
            if ready {
                connect(&g);
            }
        }
    });
    listen_forever(&document(), "visibilitychange", |_| {
        if document().visibility_state() != web_sys::VisibilityState::Visible {
            return;
        }
        if let Some(g) = current_guild() {
            let cleared = {
                let mut gm = g.borrow_mut();
                let c = gm.entry.channel.clone().unwrap_or_default();
                gm.unread.remove(&c).is_some()
            };
            if cleared {
                ui::render_rail();
                ui::render_channels();
                update_badge();
            }
        }
    });
    if let Ok(Some(mq)) = window().match_media("(max-width: 860px)") {
        listen_forever(&mq, "change", |_| ui::render());
    }
    spawn(ticker());

    let (name_missing, has_invite, count, current) = app(|a| {
        (
            a.saved.name.is_empty(),
            a.pending_invite.is_some(),
            a.guilds.len(),
            a.current.clone(),
        )
    });
    if name_missing || needs_server() {
        return ui::render();
    }
    if has_invite {
        return handle_invite().await;
    }
    if count == 0 {
        ui::render();
        if wants_new {
            ui::add_guild_dialog(true);
        }
        return;
    }
    let first = app(|a| a.guilds[0].borrow().gid.clone());
    let target = current.filter(|c| guild(c).is_some()).unwrap_or(first);
    select_guild(&target, None).await;
    if wants_new {
        ui::add_guild_dialog(false);
    }
    // Les autres serveurs : en ligne aussi, pour les badges de non-lus.
    for g in app(|a| a.guilds.clone()) {
        if g.borrow().gid == target {
            continue;
        }
        spawn(async move {
            let res = async {
                refresh_guild(&g).await?;
                announce(&g).await?;
                load_page(&g, false).await?;
                Ok::<_, ApiError>(())
            }
            .await;
            match res {
                Ok(()) => {
                    connect(&g);
                    ui::render_rail();
                }
                Err(err) if err.status == 404 || err.status == 403 => lost(&g),
                Err(_) => {}
            }
        });
    }
}

/// Toutes les 300 ms : qui parle, et qui n'est plus là.
async fn ticker() {
    loop {
        sleep(300).await;
        let call = app(|a| a.call.as_ref().map(|c| (c.gid.clone(), c.call.clone())));
        if let Some((_, c)) = &call {
            let c = c.borrow();
            for el in query_all("[data-speaker]") {
                let id = el.get_attribute("data-speaker").unwrap_or_default();
                toggle_class(&el, "speaking", c.speaking(&id));
            }
        }
        for g in app(|a| a.guilds.clone()) {
            let changed = {
                let mut gm = g.borrow_mut();
                let before = gm.presence.len();
                gm.presence.retain(|_, p| now() - p.seen <= 70_000.0);
                gm.presence.len() != before
            };
            if !changed {
                continue;
            }
            let gid = g.borrow().gid.clone();
            if call.as_ref().is_some_and(|(cg, _)| *cg == gid) {
                sync_call();
            }
            if is_current(&gid) {
                ui::render_members();
                ui::render_channels();
                ui::render_voice();
            }
        }
    }
}

/// Le service worker de l'app (généré par Active) : ouverture hors ligne.
pub fn register_worker(base: &str) {
    let nav = window().navigator();
    if !has(&nav, "serviceWorker") {
        return;
    }
    let options = web_sys::RegistrationOptions::new();
    options.set_scope(&format!("{base}/"));
    let _ = nav
        .service_worker()
        .register_with_options(&format!("{base}/sw.js"), &options);
}
