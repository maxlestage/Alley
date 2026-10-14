//! Ce que fait l'application : la liste des salons, entrer dans un salon,
//! recevoir sa clé, parler, appeler — et tout oublier en partant ou à minuit.

use std::rc::Rc;

use serde_json::{Value, json};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{CloseEvent, HtmlMediaElement, MessageEvent, WebSocket};

use alley_moderation::{Category, check_text};

use crate::call::{self, Call};
use crate::crypto::{self, Identity};
use crate::dom::*;
use crate::i18n::{self, Lang, t};
use crate::model::*;
use crate::net::{ApiError, api_base, needs_server, request, ws_base};
use crate::recent;
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
        Err(e) if e.banned() => show_banned(),
        Err(e) => ui::lobby_error(&e.text()),
    }
    load_mine().await;
    load_known().await;
}

/// Les salons dont se souvient l'appareil, sur ce serveur : ceux qui sont
/// encore ouverts (les autres sont oubliés).
pub async fn load_known() {
    let server = api_base();
    let ids: Vec<String> = recent::list()
        .into_iter()
        .filter(|r| r.server == server)
        .map(|r| r.id)
        .collect();
    if ids.is_empty() {
        app(|a| a.known.clear());
        return ui::render_mine();
    }
    let body = json!({ "ids": ids });
    if let Ok(data) = request("POST", "/api/v1/known", &[], Some(&body)).await {
        let rooms = data["rooms"].as_array().cloned().unwrap_or_default();
        let open: Vec<String> = rooms
            .iter()
            .filter_map(|r| r["id"].as_str().map(str::to_owned))
            .collect();
        recent::prune(&server, &open);
        app(|a| a.known = rooms);
        ui::render_mine();
    }
}

/// Les salons ouverts du compte connecté, retrouvés grâce à sa marque.
pub async fn load_mine() {
    let Some(account) = app(|a| a.account.clone()) else {
        return;
    };
    let body = json!({ "tag": account.tag });
    if let Ok(data) = request("POST", "/api/v1/mine", &[], Some(&body)).await {
        let rooms = data["rooms"].as_array().cloned().unwrap_or_default();
        app(|a| a.mine = rooms);
        ui::render_mine();
    }
}

/// Une erreur d'API : bloqué, ou un message.
fn api_error(e: &ApiError) {
    if e.banned() {
        show_banned();
    } else {
        toast(e.text(), true);
    }
}

/// L'adresse est bloquée par la modération : on quitte tout.
pub fn show_banned() {
    leave_room(None);
    app(|a| a.banned = true);
    ui::render();
}

/// Le message d'un refus de la modération.
fn refusal(category: Category) -> &'static str {
    match category {
        Category::Sexual => t().refused_sexual,
        Category::Drugs => t().refused_drugs,
    }
}

/// Le décalage horaire de l'appareil, en minutes (`+120` à Paris l'été).
fn tz_offset() -> i64 {
    -(js_sys::Date::new_0().get_timezone_offset() as i64)
}

/// Crée un salon (ou rejoint le salon public de ce nom), puis y entre.
pub async fn create_room(name: &str, private: bool) {
    if check_text(name).is_some() {
        return toast(t().room_name_forbidden, true);
    }
    let body = json!({ "name": name, "private": private, "tz": tz_offset() });
    match request("POST", "/api/v1/rooms", &[], Some(&body)).await {
        Ok(room) => {
            recent::remember(&room, &api_base(), true);
            if let Some(id) = room["id"].as_str() {
                join_room(id).await;
            }
        }
        Err(e) => api_error(&e),
    }
}

fn today() -> (u32, u32, u32) {
    let d = js_sys::Date::new_0();
    (d.get_full_year(), d.get_month(), d.get_date())
}

/// Entre dans un salon : une identité X25519 jetable, et la connexion.
pub async fn join_room(id: &str) {
    rejoin(id, 0).await;
}

/// Entre (ou revient) dans un salon, en comptant les tentatives : au-delà
/// de trois, on abandonne.
async fn rejoin(id: &str, retries: u32) {
    if retries > 3 {
        return leave_room(Some(t().connection_lost));
    }
    leave_room(None);
    let info = match request("GET", &format!("/api/v1/rooms/{id}"), &[], None).await {
        Ok(info) => info,
        Err(e) => {
            api_error(&e);
            if !e.banned() {
                load_rooms().await;
            }
            return;
        }
    };
    // L'appareil s'en souvient, pour y revenir depuis le hall.
    recent::remember(&info, &api_base(), false);
    let room = Room {
        id: id.to_owned(),
        name: info["name"].as_str().unwrap_or("Alley").to_owned(),
        private: info["private"].as_bool().unwrap_or_default(),
        expires_at: info["expires_at"].as_f64().unwrap_or_default(),
        me: String::new(),
        order: 0,
        identity: Identity::generate(),
        signer: crypto::Signer::generate(),
        sent: Default::default(),
        key: None,
        fingerprint: String::new(),
        peers: Default::default(),
        msgs: Vec::new(),
        pending: Vec::new(),
        socket: None,
        day: today(),
        leaving: false,
        retries,
        admin: None,
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
    let (id, x, s) = {
        let r = room.borrow();
        (
            r.id.clone(),
            crypto::b64url(&r.identity.public_raw()),
            crypto::b64url(&r.signer.public_raw()),
        )
    };
    let tag = app(|a| a.account.as_ref().map(|acc| format!("&t={}", acc.tag))).unwrap_or_default();
    let Ok(ws) = WebSocket::new(&format!(
        "{}/api/v1/rooms/{id}/live?x={x}&s={s}{tag}",
        ws_base()
    )) else {
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
    if code == 4403 {
        return show_banned();
    }
    if code == 4404 {
        return leave_room(Some(t().room_gone_or_full));
    }
    if code == 4410 {
        return leave_room(Some(t().expelled));
    }
    // Refusé dès l'ouverture : peut-être bloqué.
    if let Err(e) = request("GET", "/api/v1/stats", &[], None).await
        && e.banned()
    {
        return show_banned();
    }
    let retries = {
        let mut r = room.borrow_mut();
        r.retries += 1;
        r.retries
    };
    if retries > 3 {
        return leave_room(Some(t().connection_lost));
    }
    sleep(1000 * retries as i32).await;
    if !is_current(room) {
        return;
    }
    // On revient comme un nouvel arrivant : rien n'a été gardé.
    let id = room.borrow().id.clone();
    toast(t().reconnecting, false);
    rejoin(&id, retries).await;
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
    let Some(data) = room.borrow().enc(to, &payload) else {
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
                r.admin = event["admin"].as_str().map(str::to_owned);
                r.order = event["order"].as_u64().unwrap_or_default();
                if let Some(t) = event["room"]["expires_at"].as_f64() {
                    r.expires_at = t;
                }
                for p in event["peers"].as_array().cloned().unwrap_or_default() {
                    let id = p["id"].as_str().unwrap_or_default().to_owned();
                    r.peers.insert(
                        id,
                        Peer {
                            x: p["x"].as_str().unwrap_or_default().to_owned(),
                            s: p["s"].as_str().map(str::to_owned),
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
                        let (id, retries) = {
                            let r = room.borrow();
                            (r.id.clone(), r.retries + 1)
                        };
                        toast(t().no_key_retry, true);
                        rejoin(&id, retries).await;
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
                        s: event["s"].as_str().map(str::to_owned),
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
                // Seuls les trois plus anciens présents remettent la clé :
                // un autre ne peut pas nous en imposer une à lui.
                if r.key.is_some() || !r.may_hand_key(from) {
                    None
                } else {
                    let x = r.peers.get(from).map(|p| p.x.clone()).unwrap_or_default();
                    match r.identity.unwrap_group_key(blob, &x) {
                        Some(key) => {
                            r.set_key(key);
                            r.retries = 0;
                            Some(std::mem::take(&mut r.pending))
                        }
                        None => None,
                    }
                }
            };
            if let Some(pending) = got {
                hello(room, None);
                for event in pending {
                    on_relay(room, &event);
                }
                // Sans redessiner la saisie : ce qu'on tapait reste.
                ui::key_arrived();
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
            if room.borrow().key.is_none() {
                room.borrow_mut().pending.push(event);
            } else {
                on_relay(room, &event);
            }
        }
        // Ma photo a passé la vérification du serveur.
        "sent" => {
            let mid = event["id"].as_str().unwrap_or_default();
            if let Some(m) = room
                .borrow_mut()
                .msgs
                .iter_mut()
                .find(|m| m.mid == mid && m.mine)
            {
                m.pending = false;
            }
            ui::render_messages(false);
        }
        // Ma photo est refusée (et je suis peut-être bloqué).
        "moderated" => {
            let mid = event["id"].as_str().unwrap_or_default();
            room.borrow_mut().msgs.retain(|m| !(m.mid == mid && m.mine));
            ui::render_messages(false);
            if event["banned"].as_bool() == Some(true) {
                return show_banned();
            }
            let t = t();
            toast(
                match event["reason"].as_str().unwrap_or_default() {
                    "sexual" => t.photo_refused_sexual,
                    "drugs" => t.photo_refused_drugs,
                    "slow_down" => t.photo_slow_down,
                    "unavailable" => t.photo_unavailable,
                    _ => t.photo_invalid,
                },
                true,
            );
        }
        // L'administrateur a changé (le précédent est parti, ou le
        // propriétaire est revenu).
        "admin" => {
            let became = {
                let mut r = room.borrow_mut();
                let was = r.i_am_admin();
                r.admin = event["id"].as_str().map(str::to_owned);
                !was && r.i_am_admin()
            };
            if became {
                toast(t().now_admin, false);
            }
            ui::render_people();
            ui::render_messages(false);
        }
        "closed" => match event["reason"].as_str() {
            Some("banned") => show_banned(),
            Some("expelled") => leave_room(Some(t().expelled)),
            _ => leave_room(Some(t().midnight_closed)),
        },
        _ => {}
    }
}

/// Un texte reçu : masqué s'il est interdit (les messages sont chiffrés, c'est
/// ici qu'on les filtre).
fn screen(text: String) -> String {
    if check_text(&text).is_some() {
        t().hidden_message.to_owned()
    } else {
        text
    }
}

/// Une photo reçue : seulement si le serveur l'a vérifiée. Sa clé (dans le
/// message chiffré) doit avoir l'empreinte que le serveur a relayée, et
/// ouvrir l'image qu'il a vue.
fn verified_image(event: &Value, ik: &str) -> Option<String> {
    let key: crypto::Key = crypto::unb64(ik)?.try_into().ok()?;
    if crypto::key_commitment(&key) != event["kc"].as_str()? {
        return None;
    }
    let jpeg = crypto::open_bytes_token(&key, event["c"].as_str()?)?;
    jpeg.starts_with(&[0xFF, 0xD8, 0xFF])
        .then(|| format!("data:image/jpeg;base64,{}", crypto::b64(&jpeg)))
}

fn on_relay(room: &RoomRef, event: &Value) {
    let from = event["from"].as_str().unwrap_or_default();
    let data = event["data"].as_str().unwrap_or_default();
    let decoded = room.borrow_mut().dec(from, data);
    let p = match decoded {
        Ok(p) => p,
        // Ni lisible ni signé de lui : écarté. Si c'était bien pour nous
        // (lisible) mais pas signé comme il faut, on prévient, une fois.
        Err(crypto::Rejected::Unreadable) => return,
        Err(_) => return rejected(room, from),
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
                let name = if check_text(&name).is_some() {
                    String::new()
                } else {
                    name
                };
                let voice = p["voice"].as_bool().unwrap_or_default();
                let muted = p["muted"].as_bool().unwrap_or_default();
                let video = p["video"].as_bool().unwrap_or_default();
                let changed = peer.voice != voice || peer.muted != muted || peer.video != video;
                peer.name = Some(if name.trim().is_empty() {
                    t().anonymous.into()
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
                // Une photo n'est montrée que si le serveur l'a vérifiée.
                "image" => match verified_image(event, &str_of("ik")) {
                    Some(src) => src,
                    None => return,
                },
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
                    text: if kind == "text" {
                        screen(x)
                    } else {
                        String::new()
                    },
                    kind,
                    media,
                    reply: str_of("r"),
                    edited: false,
                    mine: false,
                    pending: false,
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
                m.text = screen(str_of("x"));
                m.edited = true;
            }
            ui::render_messages(false);
        }
        // L'auteur, ou l'administrateur, supprime un message.
        "del" => {
            let mid = str_of("mid");
            let mut r = room.borrow_mut();
            let by_admin = r.admin.as_deref() == Some(from);
            r.msgs
                .retain(|m| !(m.mid == mid && (m.from == from || by_admin)));
            drop(r);
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

/// Un envoi de `from` est écarté (signature fausse, rejoué, détourné) : on
/// prévient une fois par membre.
fn rejected(room: &RoomRef, from: &str) {
    let first = room
        .borrow_mut()
        .peers
        .get_mut(from)
        .is_some_and(|peer| !std::mem::replace(&mut peer.warned, true));
    if first {
        let name = room.borrow().name_of(from);
        toast(i18n::fill(t().signature_rejected, &[&name]), true);
    }
}

/// Envoie une photo (JPEG, en base64) : elle est chiffrée avec sa propre clé,
/// confiée au serveur le temps qu'il la vérifie, et aux présents dans le
/// message chiffré du salon.
pub fn send_image(jpeg_b64: String) {
    let Some(room) = room() else { return };
    if room.borrow().key.is_none() {
        return toast(t().key_not_arrived, true);
    }
    let Some(jpeg) = crypto::unb64(&jpeg_b64) else {
        return toast(t().unreadable_image, true);
    };
    let key = crypto::generate_key();
    let c = crypto::seal_bytes_token(&key, &jpeg);
    let reply = app(|a| a.reply_to.take()).unwrap_or_default();
    let mid = crypto::hex(&crypto::random_bytes::<8>());
    let Some(data) = room.borrow().enc(
        None,
        &payload(
            "msg",
            json!({ "mid": mid, "kind": "image", "ik": crypto::b64(&key), "r": reply }),
        ),
    ) else {
        return;
    };
    send_raw(
        &room,
        &json!({ "type": "image", "id": mid, "key": crypto::b64(&key), "c": c, "data": data }),
    );
    let name = app(|a| a.name.clone());
    {
        let mut r = room.borrow_mut();
        let me = r.me.clone();
        r.msgs.push(Msg {
            mid,
            from: me,
            name,
            t: now(),
            text: String::new(),
            media: format!("data:image/jpeg;base64,{jpeg_b64}"),
            kind: "image".into(),
            reply,
            edited: false,
            mine: true,
            pending: true,
        });
    }
    ui::render_messages(true);
}

/// Envoie un message (texte, vocal) à tous les présents.
pub fn send_message(kind: &str, x: String, mime: &str) {
    let Some(room) = room() else { return };
    if room.borrow().key.is_none() {
        return toast(t().key_not_arrived, true);
    }
    if kind == "text"
        && let Some(category) = check_text(&x)
    {
        return toast(refusal(category), true);
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
                "audio" => format!("data:{mime};base64,{x}"),
                _ => String::new(),
            },
            kind: kind.to_owned(),
            reply,
            edited: false,
            mine: true,
            pending: false,
        });
    }
    ui::render_messages(true);
}

pub fn edit_message(mid: &str, text: String) {
    let Some(room) = room() else { return };
    if let Some(category) = check_text(&text) {
        return toast(refusal(category), true);
    }
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
    let admin = room.borrow().i_am_admin();
    room.borrow_mut()
        .msgs
        .retain(|m| !(m.mid == mid && (m.mine || admin)));
    ui::render_messages(false);
}

/// L'administrateur exclut un membre du salon.
pub fn expel(id: &str) {
    if let Some(room) = room()
        && room.borrow().i_am_admin()
    {
        send_raw(&room, &json!({ "type": "expel", "to": id }));
    }
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
        return toast(t().no_calls, true);
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
        return toast(t().mic_unavailable_call, true);
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
            "image" => t().photo_label.to_owned(),
            "audio" => t().voice_label.to_owned(),
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
    i18n::set_lang(initial_lang());
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
            .map(|h| h.replace_state_with_url(&JsValue::NULL, "", Some(&app_path(i18n::lang()))));
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
    // Sur GitHub Pages sans API configurée : le serveur saisi la dernière fois.
    if needs_server() {
        app(|a| a.server = recent::saved_server());
    }
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
                    leave_room(Some(t().midnight_local));
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

// ---------------------------------------------------------------------------
// La langue

const LANG_KEY: &str = "alley.lang";

fn storage() -> Option<web_sys::Storage> {
    window().local_storage().ok().flatten()
}

/// La langue choisie un jour sur cet appareil, sinon celle de la page.
fn initial_lang() -> Lang {
    let page = document()
        .document_element()
        .and_then(|e| e.get_attribute("lang"))
        .and_then(|l| Lang::parse(&l))
        .unwrap_or(Lang::Fr);
    storage()
        .and_then(|s| s.get_item(LANG_KEY).ok().flatten())
        .and_then(|l| Lang::parse(&l))
        .unwrap_or(page)
}

/// L'adresse de l'application dans une langue (`/app/`, `/en/app/`…).
pub fn app_path(lang: Lang) -> String {
    format!("{}{}/app/", base(), lang.prefix())
}

/// Change de langue sans rien perdre : tout est redessiné.
pub fn switch_lang(lang: Lang) {
    i18n::set_lang(lang);
    if let Some(s) = storage() {
        let _ = s.set_item(LANG_KEY, lang.code());
    }
    if let Some(root) = document().document_element() {
        let _ = root.set_attribute("lang", lang.code());
    }
    let _ = window()
        .history()
        .map(|h| h.replace_state_with_url(&JsValue::NULL, "", Some(&app_path(lang))));
    // Ce qui était déjà saisi (connexion, recherche) reste saisi.
    let fields = ["username", "password", "q"];
    let typed: Vec<Option<String>> = fields
        .iter()
        .map(|f| by_id(f).map(|el| input_value(&el)))
        .collect();
    ui::render();
    for (field, value) in fields.iter().zip(typed) {
        if let (Some(el), Some(value)) = (by_id(field), value) {
            set_input_value(&el, &value);
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
