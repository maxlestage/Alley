//! Ce que fait l'application : la liste des salons, entrer dans un salon,
//! recevoir sa clé, parler, appeler — et tout oublier en partant ou à minuit.

use std::rc::Rc;

use serde_json::{Value, json};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{CloseEvent, HtmlMediaElement, MessageEvent, WebSocket};

use crate::call::{self, Call};
use crate::crypto::{self, Identity};
use crate::dom::*;
use crate::model::*;
use crate::net::{needs_server, request, ws_base};
use crate::ui;

// ---------------------------------------------------------------------------
// Les salons ouverts

/// Recharge la liste des salons (filtrée par la recherche).
pub async fn load_rooms() {
    let q = app(|a| a.query.clone());
    let path = format!(
        "/api/v1/rooms?q={}",
        String::from(js_sys::encode_uri_component(&q))
    );
    match request("GET", &path, &[], None).await {
        Ok(data) => {
            let rooms = data["rooms"].as_array().cloned().unwrap_or_default();
            app(|a| a.rooms = rooms);
            ui::render_rooms();
        }
        Err(e) => ui::lobby_error(&e.message),
    }
}

/// Le décalage horaire de l'appareil, en minutes (`+120` à Paris l'été).
fn tz_offset() -> i64 {
    -(js_sys::Date::new_0().get_timezone_offset() as i64)
}

/// Crée un salon (ou rejoint le salon public de ce nom), puis y entre.
pub async fn create_room(name: &str, private: bool) {
    let body = json!({ "name": name, "private": private, "tz": tz_offset() });
    match request("POST", "/api/v1/rooms", &[], Some(&body)).await {
        Ok(room) => {
            if let Some(id) = room["id"].as_str() {
                join_room(id).await;
            }
        }
        Err(e) => toast(e.message, true),
    }
}

fn today() -> (u32, u32, u32) {
    let d = js_sys::Date::new_0();
    (d.get_full_year(), d.get_month(), d.get_date())
}

/// Entre dans un salon : une identité X25519 jetable, et la connexion.
pub async fn join_room(id: &str) {
    leave_room(None);
    let info = match request("GET", &format!("/api/v1/rooms/{id}"), &[], None).await {
        Ok(info) => info,
        Err(e) => {
            toast(
                if e.status == 404 {
                    "Ce salon n'existe plus : il a disparu quand tout le monde est parti, ou à minuit.".into()
                } else {
                    e.message
                },
                true,
            );
            return load_rooms().await;
        }
    };
    let room = Room {
        id: id.to_owned(),
        name: info["name"].as_str().unwrap_or("salon").to_owned(),
        private: info["private"].as_bool().unwrap_or_default(),
        expires_at: info["expires_at"].as_f64().unwrap_or_default(),
        me: String::new(),
        order: 0,
        identity: Identity::generate(),
        key: None,
        fingerprint: String::new(),
        peers: Default::default(),
        msgs: Vec::new(),
        pending: Vec::new(),
        socket: None,
        day: today(),
        leaving: false,
        retries: 0,
    };
    let room: RoomRef = Rc::new(std::cell::RefCell::new(room));
    app(|a| {
        a.room = Some(room.clone());
        a.reply_to = None;
        a.editing = None;
        a.show_members = !ui::narrow();
    });
    connect(&room);
    ui::render();
}

/// Quitte le salon : tout ce qu'on y a vu est oublié.
pub fn leave_room(reason: Option<&str>) {
    let Some(room) = app(|a| a.room.take()) else {
        return;
    };
    leave_call();
    let socket = {
        let mut r = room.borrow_mut();
        r.leaving = true;
        r.msgs.clear();
        r.key = None;
        r.socket.take()
    };
    if let Some(s) = socket {
        s.ws.set_onopen(None);
        s.ws.set_onmessage(None);
        s.ws.set_onclose(None);
        let _ = s.ws.close();
    }
    app(|a| a.unread = 0);
    update_badge();
    if let Some(reason) = reason {
        toast(reason, false);
    }
    ui::render();
    spawn(load_rooms());
}

// ---------------------------------------------------------------------------
// En direct

fn connect(room: &RoomRef) {
    let (id, x) = {
        let r = room.borrow();
        (r.id.clone(), crypto::b64url(&r.identity.public_raw()))
    };
    let Ok(ws) = WebSocket::new(&format!("{}/api/v1/rooms/{id}/live?x={x}", ws_base())) else {
        return;
    };
    let mut listeners = Vec::new();

    let weak = Rc::downgrade(room);
    let on_message = Closure::<dyn FnMut(JsValue)>::new(move |e: JsValue| {
        let Some(room) = weak.upgrade() else { return };
        let e: MessageEvent = e.unchecked_into();
        let Some(text) = e.data().as_string() else {
            return;
        };
        let Ok(event) = serde_json::from_str::<Value>(&text) else {
            return;
        };
        on_event(&room, event);
    });
    ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    listeners.push(on_message);

    let weak = Rc::downgrade(room);
    let on_close = Closure::<dyn FnMut(JsValue)>::new(move |e: JsValue| {
        let Some(room) = weak.upgrade() else { return };
        let code = e
            .dyn_ref::<CloseEvent>()
            .map(CloseEvent::code)
            .unwrap_or_default();
        spawn(async move { lost(&room, code).await });
    });
    ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));
    listeners.push(on_close);

    room.borrow_mut().socket = Some(Socket {
        ws,
        _listeners: listeners,
    });
}

/// La connexion s'est coupée : le salon a disparu, ou le réseau a sauté.
async fn lost(room: &RoomRef, code: u16) {
    if room.borrow().leaving || !is_current(room) {
        return;
    }
    let old = room.borrow_mut().socket.take();
    drop(old);
    if code == 4404 {
        return leave_room(Some("Ce salon n'existe plus, ou il est complet."));
    }
    let retries = {
        let mut r = room.borrow_mut();
        r.retries += 1;
        r.retries
    };
    if retries > 3 {
        return leave_room(Some("Connexion perdue."));
    }
    sleep(1000 * retries as i32).await;
    if !is_current(room) {
        return;
    }
    // On revient comme un nouvel arrivant : rien n'a été gardé.
    let id = room.borrow().id.clone();
    toast("Reconnexion…", false);
    join_room(&id).await;
}

fn is_current(room: &RoomRef) -> bool {
    app(|a| a.room.as_ref().is_some_and(|r| Rc::ptr_eq(r, room)))
}

fn send_raw(room: &RoomRef, value: &Value) {
    let r = room.borrow();
    if let Some(s) = &r.socket
        && s.ws.ready_state() == WebSocket::OPEN
    {
        let _ = s.ws.send_with_str(&value.to_string());
    }
}

/// Envoie un message chiffré : à tous, ou à une seule personne.
pub fn send(room: &RoomRef, to: Option<&str>, payload: Value) {
    let Some(data) = room.borrow().enc(&payload) else {
        return;
    };
    let mut msg = json!({ "type": "relay", "data": data });
    if let Some(to) = to {
        msg["to"] = json!(to);
    }
    send_raw(room, &msg);
}

/// Mon pseudonyme et mon état d'appel, pour tous ou pour un nouvel arrivant.
pub fn hello(room: &RoomRef, to: Option<&str>) {
    let name = app(|a| a.name.clone());
    let (voice, muted, video) = match app(|a| a.call.clone()) {
        Some(c) => {
            let c = c.borrow();
            (true, c.muted, c.camera.is_some() || c.screen.is_some())
        }
        None => (false, false, false),
    };
    send(
        room,
        to,
        payload(
            "hello",
            json!({ "n": name, "voice": voice, "muted": muted, "video": video }),
        ),
    );
}

fn on_event(room: &RoomRef, event: Value) {
    match event["type"].as_str().unwrap_or_default() {
        "welcome" => {
            let alone = {
                let mut r = room.borrow_mut();
                r.me = event["you"].as_str().unwrap_or_default().to_owned();
                r.order = event["order"].as_u64().unwrap_or_default();
                r.retries = 0;
                if let Some(t) = event["room"]["expires_at"].as_f64() {
                    r.expires_at = t;
                }
                for p in event["peers"].as_array().cloned().unwrap_or_default() {
                    let id = p["id"].as_str().unwrap_or_default().to_owned();
                    r.peers.insert(
                        id,
                        Peer {
                            x: p["x"].as_str().unwrap_or_default().to_owned(),
                            order: p["order"].as_u64().unwrap_or_default(),
                            ..Peer::default()
                        },
                    );
                }
                r.peers.is_empty()
            };
            if alone {
                // Le premier arrivant tire la clé du salon.
                room.borrow_mut().set_key(crypto::generate_key());
                hello(room, None);
            } else {
                // Les présents vont nous la remettre ; sinon, on réessaie.
                let weak = Rc::downgrade(room);
                spawn(async move {
                    sleep(8000).await;
                    if let Some(room) = weak.upgrade()
                        && room.borrow().key.is_none()
                        && is_current(&room)
                    {
                        let id = room.borrow().id.clone();
                        toast(
                            "Personne n'a remis la clé du salon : nouvelle tentative…",
                            true,
                        );
                        join_room(&id).await;
                    }
                });
            }
            ui::render_room();
        }
        "joined" => {
            let id = event["id"].as_str().unwrap_or_default().to_owned();
            let x = event["x"].as_str().unwrap_or_default().to_owned();
            let give = {
                let mut r = room.borrow_mut();
                r.peers.insert(
                    id.clone(),
                    Peer {
                        x: x.clone(),
                        order: event["order"].as_u64().unwrap_or_default(),
                        ..Peer::default()
                    },
                );
                // Les trois plus anciens remettent la clé, emballée pour lui seul.
                match r.key {
                    Some(key) if r.key_holder(&id) => r.identity.wrap_group_key(&key, &x),
                    _ => None,
                }
            };
            if let Some(blob) = give {
                send_raw(room, &json!({ "type": "key", "to": id, "blob": blob }));
            }
            hello(room, Some(&id));
            ui::render_people();
        }
        "key" => {
            let from = event["from"].as_str().unwrap_or_default();
            let blob = event["blob"].as_str().unwrap_or_default();
            let got = {
                let mut r = room.borrow_mut();
                if r.key.is_some() {
                    None
                } else {
                    let x = r.peers.get(from).map(|p| p.x.clone()).unwrap_or_default();
                    match r.identity.unwrap_group_key(blob, &x) {
                        Some(key) => {
                            r.set_key(key);
                            Some(std::mem::take(&mut r.pending))
                        }
                        None => None,
                    }
                }
            };
            if let Some(pending) = got {
                hello(room, None);
                for (from, data) in pending {
                    on_relay(room, &from, &data);
                }
                ui::render_room();
            }
        }
        "left" => {
            let id = event["id"].as_str().unwrap_or_default();
            room.borrow_mut().peers.remove(id);
            sync_call();
            ui::render_people();
            ui::render_call();
        }
        "relay" => {
            let from = event["from"].as_str().unwrap_or_default().to_owned();
            let data = event["data"].as_str().unwrap_or_default().to_owned();
            if room.borrow().key.is_none() {
                room.borrow_mut().pending.push((from, data));
            } else {
                on_relay(room, &from, &data);
            }
        }
        "closed" => leave_room(Some(
            "Minuit : le salon a disparu, et tout ce qui s'y est dit avec lui.",
        )),
        _ => {}
    }
}

fn on_relay(room: &RoomRef, from: &str, data: &str) {
    let Some(p) = room.borrow().dec(data) else {
        return;
    };
    let str_of = |k: &str| p[k].as_str().unwrap_or_default().to_owned();
    match p["k"].as_str().unwrap_or_default() {
        "hello" => {
            let changed = {
                let mut r = room.borrow_mut();
                let Some(peer) = r.peers.get_mut(from) else {
                    return;
                };
                let name: String = str_of("n").chars().take(32).collect();
                let voice = p["voice"].as_bool().unwrap_or_default();
                let muted = p["muted"].as_bool().unwrap_or_default();
                let video = p["video"].as_bool().unwrap_or_default();
                let changed = peer.voice != voice || peer.muted != muted || peer.video != video;
                peer.name = Some(if name.trim().is_empty() {
                    "Anonyme".into()
                } else {
                    name
                });
                (peer.voice, peer.muted, peer.video) = (voice, muted, video);
                changed
            };
            if changed {
                sync_call();
            }
            ui::render_people();
            ui::render_call();
            ui::render_messages(false);
        }
        "msg" => {
            let kind = match p["kind"].as_str() {
                Some(k @ ("image" | "audio")) => k.to_owned(),
                _ => "text".to_owned(),
            };
            let x = str_of("x");
            let mime = str_of("m");
            let media = match kind.as_str() {
                "image" => format!("data:image/jpeg;base64,{x}"),
                "audio" => {
                    let ok = mime.starts_with("audio/")
                        && mime[6..]
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || ".+-".contains(c));
                    format!(
                        "data:{};base64,{x}",
                        if ok { mime.as_str() } else { "audio/mp4" }
                    )
                }
                _ => String::new(),
            };
            let msg = {
                let r = room.borrow();
                Msg {
                    mid: str_of("mid"),
                    from: from.to_owned(),
                    name: r.name_of(from),
                    t: now(),
                    text: if kind == "text" { x } else { String::new() },
                    kind,
                    media,
                    reply: str_of("r"),
                    edited: false,
                    mine: false,
                }
            };
            notify(room, &msg);
            {
                let mut r = room.borrow_mut();
                if let Some(peer) = r.peers.get_mut(from) {
                    peer.typing_until = 0.0;
                }
                r.msgs.push(msg);
                // Seulement ce qui est à l'écran : rien ne s'accumule.
                let len = r.msgs.len();
                if len > 500 {
                    r.msgs.drain(..len - 500);
                }
            }
            ui::render_messages(false);
            ui::render_typing();
        }
        // Seul l'auteur d'un message peut le modifier ou le supprimer : le
        // serveur indique qui l'envoie, impossible de se faire passer pour lui.
        "edit" => {
            let mid = str_of("mid");
            if let Some(m) = room
                .borrow_mut()
                .msgs
                .iter_mut()
                .find(|m| m.mid == mid && m.from == from)
            {
                m.text = str_of("x");
                m.edited = true;
            }
            ui::render_messages(false);
        }
        "del" => {
            let mid = str_of("mid");
            room.borrow_mut()
                .msgs
                .retain(|m| !(m.mid == mid && m.from == from));
            ui::render_messages(false);
        }
        "typing" => {
            if let Some(peer) = room.borrow_mut().peers.get_mut(from) {
                peer.typing_until = now() + 6000.0;
            }
            ui::render_typing();
        }
        "signal" => {
            if let Some(c) = app(|a| a.call.clone()) {
                let (from, d) = (from.to_owned(), p["d"].clone());
                spawn(async move {
                    call::receive(&c, &from, d).await;
                    sync_call();
                });
            }
        }
        _ => {}
    }
}

/// Envoie un message (texte, photo, vocal) à tous les présents.
pub fn send_message(kind: &str, x: String, mime: &str) {
    let Some(room) = room() else { return };
    if room.borrow().key.is_none() {
        return toast("La clé du salon n'est pas encore arrivée.", true);
    }
    let reply = app(|a| a.reply_to.take()).unwrap_or_default();
    let mid = crypto::hex(&crypto::random_bytes::<8>());
    let name = app(|a| a.name.clone());
    send(
        &room,
        None,
        payload(
            "msg",
            json!({ "mid": mid, "kind": kind, "x": x, "m": mime, "r": reply }),
        ),
    );
    {
        let mut r = room.borrow_mut();
        let me = r.me.clone();
        r.msgs.push(Msg {
            mid,
            from: me,
            name,
            t: now(),
            text: if kind == "text" {
                x.clone()
            } else {
                String::new()
            },
            media: match kind {
                "image" => format!("data:image/jpeg;base64,{x}"),
                "audio" => format!("data:{mime};base64,{x}"),
                _ => String::new(),
            },
            kind: kind.to_owned(),
            reply,
            edited: false,
            mine: true,
        });
    }
    ui::render_messages(true);
}

pub fn edit_message(mid: &str, text: String) {
    let Some(room) = room() else { return };
    send(
        &room,
        None,
        payload("edit", json!({ "mid": mid, "x": text })),
    );
    if let Some(m) = room
        .borrow_mut()
        .msgs
        .iter_mut()
        .find(|m| m.mid == mid && m.mine)
    {
        m.text = text;
        m.edited = true;
    }
    ui::render_messages(false);
}

pub fn delete_message(mid: &str) {
    let Some(room) = room() else { return };
    send(&room, None, payload("del", json!({ "mid": mid })));
    room.borrow_mut().msgs.retain(|m| !(m.mid == mid && m.mine));
    ui::render_messages(false);
}

pub fn send_typing() {
    let t = now();
    if app(|a| t - a.last_typing < 3000.0) {
        return;
    }
    app(|a| a.last_typing = t);
    if let Some(room) = room() {
        send(&room, None, payload("typing", json!({})));
    }
}

// ---------------------------------------------------------------------------
// Appels audio et vidéo

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

pub async fn join_call() {
    let Some(room) = room() else { return };
    if app(|a| a.call.is_some()) || room.borrow().key.is_none() {
        return;
    }
    let ok = window().navigator().media_devices().is_ok()
        && js_sys::Reflect::has(&window(), &"RTCPeerConnection".into()).unwrap_or(false);
    if !ok {
        return toast("Ce navigateur ne permet pas les appels.", true);
    }
    let ice = ice_servers().await;
    let me = room.borrow().me.clone();
    let weak = Rc::downgrade(&room);
    let c = Call::new(
        me,
        &ice,
        move |to, d| {
            if let Some(room) = weak.upgrade() {
                send(&room, Some(&to), payload("signal", json!({ "d": d })));
            }
        },
        ui::render_call,
    );
    if call::start(&c).await.is_err() {
        return toast(
            "Micro inaccessible : autorisez-le dans les réglages du navigateur.",
            true,
        );
    }
    app(|a| a.call = Some(c));
    hello(&room, None);
    sync_call();
    ui::render_call();
    ui::render_people();
}

pub fn leave_call() {
    let Some(c) = app(|a| a.call.take()) else {
        return;
    };
    call::stop(&c);
    ui::clear_videos();
    if let Some(b) = by_id("call-media") {
        b.set_text_content(None);
    }
    if let Some(room) = room() {
        hello(&room, None);
    }
    ui::render_call();
    ui::render_people();
}

/// Relie l'appel à ceux qui y sont, et branche leur son.
pub fn sync_call() {
    let (Some(c), Some(room)) = (app(|a| a.call.clone()), room()) else {
        return;
    };
    let ids = room.borrow().voice_members(true);
    call::sync(&c, &ids);
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
        if !media.src_object().is_some_and(|s| s == *stream) {
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
// Pastille de non-lus (seulement pendant qu'on est ailleurs)

fn notify(room: &RoomRef, msg: &Msg) {
    if document().visibility_state() == web_sys::VisibilityState::Visible {
        return;
    }
    app(|a| a.unread += 1);
    update_badge();
    if web_sys::Notification::permission() == web_sys::NotificationPermission::Granted {
        let options = web_sys::NotificationOptions::new();
        options.set_body(&match msg.kind.as_str() {
            "image" => "📷 Photo".to_owned(),
            "audio" => "🎤 Message vocal".to_owned(),
            _ => msg.text.chars().take(140).collect(),
        });
        options.set_tag(&room.borrow().id);
        let _ = web_sys::Notification::new_with_options(
            &format!("{} · #{}", msg.name, room.borrow().name),
            &options,
        );
    }
}

pub fn update_badge() {
    let n = app(|a| a.unread);
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
// Démarrage

/// Après le pseudonyme : le salon d'un lien, ou la liste des salons.
pub async fn enter() {
    if let Some(id) = app(|a| a.invited.take()) {
        join_room(&id).await;
    } else {
        ui::render();
        load_rooms().await;
    }
}

pub async fn start() {
    let body = document().body().expect("body");
    let _ = body.append_child(&h("div#toasts").attr("aria-live", "polite").el());
    let _ = body.append_child(&h("div#call-media").attr("hidden", "").el());
    ui::track_viewport();
    if !window().is_secure_context() {
        return ui::insecure();
    }
    let base = meta("alley-base");
    retire_worker();
    app(|a| a.base = base.clone());
    // Un lien de salon : `#salon=<id>`, effacé aussitôt de la barre d'adresse.
    let hash = window().location().hash().unwrap_or_default();
    if let Some(id) = hash
        .strip_prefix("#salon=")
        .filter(|id| id.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        app(|a| a.invited = Some(id.to_owned()));
    }
    if !hash.is_empty() {
        let _ = window()
            .history()
            .map(|h| h.replace_state_with_url(&JsValue::NULL, "", Some(&format!("{base}/app/"))));
    }

    listen_forever(&document(), "visibilitychange", |_| {
        if document().visibility_state() == web_sys::VisibilityState::Visible {
            app(|a| a.unread = 0);
            update_badge();
        }
    });
    // En partant, on prévient tout de suite les autres.
    listen_forever(&window(), "pagehide", |_| {
        if let Some(room) = room()
            && let Some(s) = &room.borrow().socket
        {
            let _ = s.ws.close();
        }
    });
    if let Ok(Some(mq)) = window().match_media("(max-width: 860px)") {
        listen_forever(&mq, "change", |_| ui::render());
    }
    spawn(ticker());
    let _ = needs_server();
    ui::render();
}

/// Toutes les 300 ms : qui parle, qui écrit, et minuit.
async fn ticker() {
    let mut n = 0u32;
    loop {
        sleep(300).await;
        n += 1;
        if let Some(c) = app(|a| a.call.clone()) {
            let c = c.borrow();
            for el in query_all("[data-speaker]") {
                let id = el.get_attribute("data-speaker").unwrap_or_default();
                toggle_class(&el, "speaking", c.speaking(&id));
            }
        }
        match room() {
            Some(room) => {
                // À minuit chez soi, tout s'efface.
                if room.borrow().day != today() {
                    leave_room(Some("Minuit : tout s'efface."));
                    continue;
                }
                if n.is_multiple_of(4) {
                    ui::render_typing();
                }
            }
            // Dans le hall : la liste se met à jour toute seule.
            None if n.is_multiple_of(50) && !app(|a| a.name.is_empty()) => spawn(load_rooms()),
            None => {}
        }
    }
}

/// Alley n'a plus de service worker (rien à ouvrir hors ligne) : retire
/// celui qu'une ancienne version aurait installé, et son cache.
pub fn retire_worker() {
    let window = window();
    let nav = window.navigator();
    if has(&nav, "serviceWorker") {
        let container = nav.service_worker();
        spawn(async move {
            let Ok(list) =
                wasm_bindgen_futures::JsFuture::from(container.get_registrations()).await
            else {
                return;
            };
            for registration in js_sys::Array::from(&list).iter() {
                let registration: web_sys::ServiceWorkerRegistration =
                    registration.unchecked_into();
                if let Ok(done) = registration.unregister() {
                    let _ = wasm_bindgen_futures::JsFuture::from(done).await;
                }
            }
        });
    }
    if let Ok(caches) = window.caches() {
        spawn(async move {
            let Ok(keys) = wasm_bindgen_futures::JsFuture::from(caches.keys()).await else {
                return;
            };
            for key in js_sys::Array::from(&keys).iter() {
                if let Some(name) = key.as_string()
                    && name.starts_with("alley")
                {
                    let _ = wasm_bindgen_futures::JsFuture::from(caches.delete(&name)).await;
                }
            }
        });
    }
}
