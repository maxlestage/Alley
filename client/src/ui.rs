//! L'affichage : la disposition façon Discord, les messages, les appels et
//! toutes les fenêtres. Du DOM construit en Rust, jamais de HTML injecté.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{
    Element, Event, HtmlCanvasElement, HtmlDialogElement, HtmlImageElement, HtmlInputElement,
    HtmlMediaElement, HtmlVideoElement, KeyboardEvent, MediaStream,
};

use crate::app::{self, *};
use crate::call::{self, Call};
use crate::crypto;
use crate::dom::*;
use crate::model::*;
use crate::net::needs_server;

pub fn narrow() -> bool {
    matches_media("(max-width: 860px)")
}

pub fn touch() -> bool {
    matches_media("(pointer: coarse)")
}

fn hue(id: &str) -> u32 {
    id.chars()
        .fold(0u32, |n, c| n.wrapping_mul(31).wrapping_add(c as u32))
        % 360
}

fn initials(name: &str) -> String {
    let mut words = name.split_whitespace();
    let first = words.next().and_then(|w| w.chars().next()).unwrap_or('?');
    let second = words.next().and_then(|w| w.chars().next());
    let mut out: String = first.to_uppercase().collect();
    if let Some(s) = second {
        out.extend(s.to_uppercase());
    }
    out
}

fn avatar(id: &str, name: &str, size: &str) -> El {
    h("span.avatar")
        .class(size)
        .style("--h", hue(id).to_string())
        .attr("aria-hidden", "true")
        .text(initials(name))
}

fn icon_btn(label: &str, icon: &str) -> El {
    h("button.icon-btn")
        .attr("type", "button")
        .attr("title", label)
        .attr("aria-label", label)
        .text(icon)
}

fn base_url(path: &str) -> String {
    format!("{}{path}", base())
}

// ---------------------------------------------------------------------------
// Disposition

pub fn render() {
    let Some(root) = by_id("root") else { return };
    root.set_class_name("");
    let (no_name, empty) = app(|a| (a.saved.name.is_empty(), a.guilds.is_empty()));
    if no_name || empty || needs_server() {
        scope("root", || replace(&root, vec![welcome().into()]));
        return;
    }
    if current_guild().is_none() {
        app(|a| a.current = a.guilds.first().map(|g| g.borrow().gid.clone()));
    }
    let (drawer, members) = app(|a| (a.drawer, a.show_members));
    scope("root", || {
        let layout = h("div.layout")
            .class_if("drawer-open", drawer)
            .class_if("members-open", members)
            .child(h("nav.rail#rail").attr("aria-label", "Serveurs"))
            .child(h("aside.sidebar#sidebar"))
            .child(h("main.chat#chat"))
            .child(h("aside.members#members").attr("aria-label", "Membres"))
            .child(h("div.scrim").on("click", |_| {
                toggle_drawer(false);
                toggle_members(Some(false));
            }));
        replace(&root, vec![layout.into()]);
    });
    render_rail();
    render_channels();
    render_chat();
    render_members();
}

pub fn toggle_drawer(open: bool) {
    app(|a| a.drawer = open);
    if let Some(l) = query(".layout") {
        toggle_class(&l, "drawer-open", open);
    }
    if narrow() && open {
        app(|a| a.show_members = false);
        if let Some(l) = query(".layout") {
            toggle_class(&l, "members-open", false);
        }
    }
}

pub fn toggle_members(open: Option<bool>) {
    let open = open.unwrap_or_else(|| !app(|a| a.show_members));
    app(|a| a.show_members = open);
    if let Some(l) = query(".layout") {
        toggle_class(&l, "members-open", open);
    }
    if narrow() && open {
        toggle_drawer(false);
    }
}

pub fn render_rail() {
    let Some(rail) = by_id("rail") else { return };
    let current = app(|a| a.current.clone());
    let in_call = app(|a| a.call.as_ref().map(|c| c.gid.clone()));
    let guilds = app(|a| a.guilds.clone());
    scope("rail", || {
        let mut items: Vec<Child> = vec![
            h("a.home")
                .attr("href", base_url("/"))
                .attr("title", "Accueil d'Alley")
                .attr("aria-label", "Accueil d'Alley")
                .child(
                    h("img")
                        .attr("src", base_url("/assets/favicon.svg"))
                        .attr("alt", "")
                        .attr("width", "28")
                        .attr("height", "28"),
                )
                .into(),
            h("hr").into(),
        ];
        for g in guilds {
            let (gid, name, unread) = {
                let g = g.borrow();
                (g.gid.clone(), g.name.clone(), g.unread_total())
            };
            let label = if unread > 0 {
                format!("{name}, {unread} non lus")
            } else {
                name.clone()
            };
            let target = gid.clone();
            items.push(
                h("button.guild")
                    .class_if("active", current.as_deref() == Some(gid.as_str()))
                    .attr("title", &name)
                    .attr("aria-label", label)
                    .style("--h", hue(&gid).to_string())
                    .text(initials(&name))
                    .child((unread > 0).then(|| {
                        h("span.badge").text(if unread > 99 {
                            "99+".into()
                        } else {
                            unread.to_string()
                        })
                    }))
                    .child(
                        (in_call.as_deref() == Some(gid.as_str()))
                            .then(|| h("span.in-call").text("🔊")),
                    )
                    .click(move || {
                        let t = target.clone();
                        async move { select_guild(&t, None).await }
                    })
                    .into(),
            );
        }
        items.push(
            h("button.guild.add")
                .attr("title", "Créer ou rejoindre un serveur")
                .attr("aria-label", "Créer ou rejoindre un serveur")
                .text("+")
                .click(|| async { add_guild_dialog(false) })
                .into(),
        );
        replace(&rail, items);
    });
}

fn channel_button(g: &G, c: &Channel) -> El {
    let gm = g.borrow();
    let unread = gm.unread.get(&c.id).copied().unwrap_or(0);
    let inside = if c.voice() {
        app::voice_members(&gm, &c.id)
    } else {
        Vec::new()
    };
    let my_muted = app(|a| a.call.as_ref().is_some_and(|x| x.call.borrow().muted));
    let (gw, cid) = (Rc::downgrade(g), c.id.clone());
    let edit = gm.admin.then(|| {
        let (gw, ch) = (Rc::downgrade(g), c.clone());
        h("span.gear")
            .attr("title", "Modifier le salon")
            .attr("role", "button")
            .attr("tabindex", "0")
            .text("⚙")
            .on("click", move |e| {
                e.stop_propagation();
                if let Some(g) = gw.upgrade() {
                    channel_dialog(&g, Some(ch.clone()), "text");
                }
            })
    });
    h("li")
        .child(
            h("button.channel")
                .class_if("active", gm.entry.channel.as_deref() == Some(c.id.as_str()))
                .class_if("unread", unread > 0)
                .child(h("span.hash").text(if c.voice() { "🔊" } else { "#" }))
                .child(h("span.name").text(&c.name))
                .child((unread > 0).then(|| {
                    h("span.badge").text(if unread > 99 {
                        "99+".into()
                    } else {
                        unread.to_string()
                    })
                }))
                .child(edit)
                .on("click", move |_| {
                    if let Some(g) = gw.upgrade() {
                        select_channel(&g, &cid);
                    }
                }),
        )
        .child((!inside.is_empty()).then(|| {
            h("ul.voice-members").children(inside.iter().map(|id| {
                let muted = if *id == gm.me {
                    my_muted
                } else {
                    gm.presence.get(id).is_some_and(|p| p.muted)
                };
                h("li")
                    .data("speaker", id)
                    .child(avatar(id, &gm.name_of(id), "tiny"))
                    .child(h("span").text(gm.name_of(id)))
                    .child(muted.then(|| h("span.muted-icon").text("🔇")))
            }))
        }))
}

pub fn render_channels() {
    let (Some(side), Some(g)) = (by_id("sidebar"), current_guild()) else {
        return;
    };
    let (name, admin, me, texts, voices, label) = {
        let gm = g.borrow();
        (
            gm.name.clone(),
            gm.admin,
            gm.me.clone(),
            gm.state
                .channels
                .iter()
                .filter(|c| !c.voice())
                .cloned()
                .collect::<Vec<_>>(),
            gm.state
                .channels
                .iter()
                .filter(|c| c.voice())
                .cloned()
                .collect::<Vec<_>>(),
            conn_label(&gm),
        )
    };
    let my_name = app(|a| a.saved.name.clone());
    scope("sidebar", || {
        let gw = Rc::downgrade(&g);
        let section = |title: &str, kind: &'static str, label: &str| {
            let gw = Rc::downgrade(&g);
            h("div.section-title")
                .child(h("span").text(title))
                .child(admin.then(|| {
                    icon_btn(label, "+").on("click", move |_| {
                        if let Some(g) = gw.upgrade() {
                            channel_dialog(&g, None, kind);
                        }
                    })
                }))
        };
        replace(
            &side,
            vec![
                h("header.guild-head")
                    .child(
                        h("button.guild-name")
                            .attr("aria-haspopup", "dialog")
                            .child(h("span").text(&name))
                            .child(h("span.chev").text("▾"))
                            .on("click", move |_| {
                                if let Some(g) = gw.upgrade() {
                                    guild_menu(&g);
                                }
                            }),
                    )
                    .into(),
                h("div.channels")
                    .child(section("Salons textuels", "text", "Créer un salon textuel"))
                    .child(h("ul").children(texts.iter().map(|c| channel_button(&g, c))))
                    .child(section("Salons vocaux", "voice", "Créer un salon vocal"))
                    .child(h("ul").children(voices.iter().map(|c| channel_button(&g, c))))
                    .into(),
                h("div#call-panel").into(),
                h("footer.me")
                    .child(avatar(&me, &my_name, ""))
                    .child(
                        h("div.me-text")
                            .child(h("strong").text(&my_name))
                            .child(h("small#conn").text(label)),
                    )
                    .child(icon_btn("Réglages", "⚙").on("click", |_| settings_dialog()))
                    .into(),
            ],
        );
    });
    render_call_panel();
}

pub fn render_call_panel() {
    let Some(el) = by_id("call-panel") else {
        return;
    };
    let Some((gid, channel, c)) = app(|a| {
        a.call
            .as_ref()
            .map(|c| (c.gid.clone(), c.channel.clone(), c.call.clone()))
    }) else {
        el.set_text_content(None);
        return;
    };
    let Some(g) = guild(&gid) else { return };
    let (gname, cname) = {
        let gm = g.borrow();
        (
            gm.name.clone(),
            gm.channel(&channel)
                .map(|x| x.name.clone())
                .unwrap_or_default(),
        )
    };
    let muted = c.borrow().muted;
    scope("call-panel", || {
        let (gid2, ch2) = (gid.clone(), channel.clone());
        let (c2, g2) = (c.clone(), g.clone());
        replace(
            &el,
            vec![
                h("div.call-panel")
                    .child(
                        h("button.call-text")
                            .child(h("strong").text("🔊 En appel"))
                            .child(h("small").text(format!("{cname} · {gname}")))
                            .click(move || {
                                let (g, c) = (gid2.clone(), ch2.clone());
                                async move { select_guild(&g, Some(c)).await }
                            }),
                    )
                    .child(
                        icon_btn(
                            if muted {
                                "Réactiver le micro"
                            } else {
                                "Couper le micro"
                            },
                            if muted { "🔇" } else { "🎙️" },
                        )
                        .class_if("off", muted)
                        .on("click", move |_| {
                            call::toggle_mute(&c2);
                            here(&g2, false);
                        }),
                    )
                    .child(
                        icon_btn("Raccrocher", "📞")
                            .class("hangup")
                            .on("click", |_| leave_call()),
                    )
                    .into(),
            ],
        );
    });
}

fn conn_label(g: &Guild) -> String {
    match &g.socket {
        Some(s) if s.ws.ready_state() == web_sys::WebSocket::OPEN => "🔒 Connecté, chiffré".into(),
        _ if g.retry > 0 => "Reconnexion…".into(),
        _ => "Connexion…".into(),
    }
}

pub fn render_status() {
    if let (Some(el), Some(g)) = (by_id("conn"), current_guild()) {
        el.set_text_content(Some(&conn_label(&g.borrow())));
    }
}

pub fn render_chat() {
    let (Some(chat), Some(g)) = (by_id("chat"), current_guild()) else {
        return;
    };
    let (channel, fingerprint) = {
        let gm = g.borrow();
        (gm.current_channel().cloned(), gm.fingerprint.clone())
    };
    let voice = channel.as_ref().is_some_and(Channel::voice);
    scope("chat", || {
        let gw = Rc::downgrade(&g);
        let head = h("header.chat-head")
            .child(
                icon_btn("Serveurs et salons", "☰")
                    .class("menu")
                    .on("click", |_| toggle_drawer(!app(|a| a.drawer))),
            )
            .child(h("span.hash").text(if voice { "🔊" } else { "#" }))
            .child(
                h("strong.title").text(
                    channel
                        .as_ref()
                        .map_or("aucun salon".into(), |c| c.name.clone()),
                ),
            )
            .child(
                channel
                    .as_ref()
                    .filter(|c| !c.topic.is_empty())
                    .map(|c| h("span.topic").text(&c.topic)),
            )
            .child(h("span.spacer"))
            .child(
                h("span.lock")
                    .attr(
                        "title",
                        format!("Chiffré de bout en bout · empreinte {fingerprint}"),
                    )
                    .text("🔒"),
            )
            .child(icon_btn("Inviter", "🔗").on("click", move |_| {
                if let Some(g) = gw.upgrade() {
                    invite_dialog(&g, false);
                }
            }))
            .child(icon_btn("Membres", "👥").on("click", |_| toggle_members(None)));
        if voice {
            replace(&chat, vec![head.into(), h("div.voice#voice").into()]);
            return;
        }
        let messages = h("div.messages#messages")
            .attr("role", "log")
            .attr("aria-live", "polite");
        let gw = Rc::downgrade(&g);
        let cid = channel.as_ref().map(|c| c.id.clone()).unwrap_or_default();
        let messages = messages.on("scroll", move |_| {
            let near_top = by_id("messages").is_some_and(|b| b.scroll_top() < 120);
            if near_top && let Some(g) = gw.upgrade() {
                let cid = cid.clone();
                spawn(async move { load_older(&g, &cid).await });
            }
        });
        replace(
            &chat,
            vec![
                head.into(),
                messages.into(),
                h("div.typing#typing").attr("aria-live", "polite").into(),
                composer(&g, channel.as_ref()).into(),
            ],
        );
    });
    if voice {
        render_voice();
    } else {
        render_messages(true);
        render_typing();
    }
}

thread_local! {
    /// Les `<video>` réutilisées d'un rendu à l'autre (pas de clignotement).
    static VIDEOS: RefCell<HashMap<String, HtmlVideoElement>> = RefCell::new(HashMap::new());
}

pub fn clear_videos() {
    VIDEOS.with(|v| v.borrow_mut().clear());
}

fn video_el(id: &str) -> HtmlVideoElement {
    VIDEOS.with(|v| {
        v.borrow_mut()
            .entry(id.to_owned())
            .or_insert_with(|| {
                let el: HtmlVideoElement = h("video")
                    .attr("autoplay", "")
                    .attr("playsinline", "")
                    .el()
                    .unchecked_into();
                // Le son passe par les <audio> de #call-media.
                el.set_muted(true);
                el
            })
            .clone()
    })
}

fn video_for(id: &str, stream: &MediaStream, mirror: bool) -> Element {
    let v = video_el(id);
    let media: &HtmlMediaElement = v.as_ref();
    if !media.src_object().is_some_and(|s| s == *stream) {
        media.set_src_object(Some(stream));
        play(media);
    }
    toggle_class(&v, "mirror", mirror);
    v.into()
}

/// Sa propre image : un flux qui ne change que si la piste change.
fn self_video(track: &web_sys::MediaStreamTrack, mirror: bool) -> Element {
    let v = video_el("me");
    let media: &HtmlMediaElement = v.as_ref();
    let current = media
        .src_object()
        .filter(|s| s.get_track_by_id(&track.id()).is_some());
    if current.is_none() {
        let stream = MediaStream::new().expect("flux");
        stream.add_track(track);
        media.set_src_object(Some(&stream));
        play(media);
    }
    toggle_class(&v, "mirror", mirror);
    v.into()
}

pub fn render_voice() {
    let (Some(box_), Some(g)) = (by_id("voice"), current_guild()) else {
        return;
    };
    let gm = g.borrow();
    let Some(c) = gm.current_channel().cloned().filter(Channel::voice) else {
        return;
    };
    let active = app(|a| {
        a.call
            .as_ref()
            .filter(|x| x.gid == gm.gid && x.channel == c.id)
            .map(|x| x.call.clone())
    });
    let ids = app::voice_members(&gm, &c.id);
    let tiles: Vec<Child> = ids
        .iter()
        .map(|id| {
            let mut video = None;
            if let Some(call) = &active {
                let call = call.borrow();
                if *id == gm.me {
                    if let Some(track) = call.video_track() {
                        let mirror = call.camera.is_some() && call.screen.is_none();
                        video = Some(self_video(&track, mirror));
                    }
                } else if gm.presence.get(id).is_some_and(|p| p.video)
                    && let Some(stream) = call::remote_video(&call, id)
                {
                    video = Some(video_for(id, &stream, false));
                }
            }
            let link = active
                .as_ref()
                .and_then(|c| call::link_state(&c.borrow(), id));
            let muted = if *id == gm.me {
                active.as_ref().is_some_and(|c| c.borrow().muted)
            } else {
                gm.presence.get(id).is_some_and(|p| p.muted)
            };
            let has_video = video.is_some();
            h("div.tile")
                .class_if("has-video", has_video)
                .data("speaker", id)
                .child(
                    video
                        .map(Child::from)
                        .unwrap_or_else(|| avatar(id, &gm.name_of(id), "big").into()),
                )
                .child(
                    h("span.tile-name")
                        .text(gm.name_of(id))
                        .text(if muted { " 🔇" } else { "" })
                        .text(link.map(|l| format!(" · {l}")).unwrap_or_default()),
                )
                .into()
        })
        .collect();
    let count = ids.len();
    let (gid, cid) = (gm.gid.clone(), c.id.clone());
    drop(gm);
    scope("voice", || {
        let controls: El = match &active {
            Some(call) => {
                let (muted, camera, screen) = {
                    let c = call.borrow();
                    (c.muted, c.camera.is_some(), c.screen.is_some())
                };
                let (c1, g1) = (call.clone(), g.clone());
                let (c2, g2) = (call.clone(), g.clone());
                let (c3, g3) = (call.clone(), g.clone());
                h("div.call-controls")
                    .child(
                        h("button")
                            .attr("type", "button")
                            .class_if("off", muted)
                            .attr("title", "Micro")
                            .text(if muted { "🔇" } else { "🎙️" })
                            .child(h("span").text(if muted { "Micro coupé" } else { "Micro" }))
                            .on("click", move |_| {
                                call::toggle_mute(&c1);
                                here(&g1, false);
                            }),
                    )
                    .child(
                        h("button")
                            .attr("type", "button")
                            .class_if("on", camera)
                            .attr("title", "Caméra")
                            .text("📷")
                            .child(h("span").text(if camera {
                                "Couper la caméra"
                            } else {
                                "Caméra"
                            }))
                            .click(move || {
                                let (c, g) = (c2.clone(), g2.clone());
                                async move {
                                    if call::toggle_camera(&c).await.is_err() {
                                        toast("Caméra inaccessible.", true);
                                    }
                                    here(&g, false);
                                }
                            }),
                    )
                    .child(Call::can_share_screen().then(|| {
                        h("button")
                            .attr("type", "button")
                            .class_if("on", screen)
                            .attr("title", "Partager l'écran")
                            .text("🖥️")
                            .child(h("span").text(if screen {
                                "Arrêter le partage"
                            } else {
                                "Écran"
                            }))
                            .click(move || {
                                let (c, g) = (c3.clone(), g3.clone());
                                async move {
                                    let _ = call::toggle_screen(&c).await;
                                    here(&g, false);
                                }
                            })
                    }))
                    .child(
                        h("button.hangup")
                            .attr("type", "button")
                            .attr("title", "Raccrocher")
                            .text("📞")
                            .child(h("span").text("Raccrocher"))
                            .on("click", |_| leave_call()),
                    )
            }
            None => {
                let gw = Rc::downgrade(&g);
                h("div.call-join")
                    .child(h("p").text(if count > 0 { format!("{count} personne(s) dans ce salon.") } else { "Personne pour l'instant.".into() }))
                    .child(h("button.primary").attr("type", "button").text("Rejoindre l'appel").click(move || {
                        let (gw, cid) = (gw.clone(), cid.clone());
                        async move {
                            if let Some(g) = gw.upgrade() {
                                join_call(&g, &cid).await;
                            }
                        }
                    }))
                    .child(h("p.muted.small").text(
                        "Audio et vidéo de pair à pair, chiffrés de bout en bout (DTLS-SRTP). Le serveur ne relaie que la mise en relation, chiffrée et signée elle aussi.",
                    ))
            }
        };
        let _ = gid;
        replace(
            &box_,
            vec![
                h("div.tiles")
                    .class_if("many", count > 4)
                    .children(tiles)
                    .into(),
                controls.into(),
            ],
        );
    });
}

async fn load_older(g: &G, cid: &str) {
    let (more, loading) = {
        let gm = g.borrow();
        (gm.more, gm.loading_page)
    };
    if !more || loading {
        return;
    }
    let Some(box_) = by_id("messages") else {
        return;
    };
    let before = box_.scroll_height() - box_.scroll_top();
    let count = g.borrow().channel_messages(cid).len();
    // Le salon est dans le chiffré : on remonte l'historique du serveur
    // jusqu'à trouver de quoi afficher.
    for _ in 0..5 {
        if load_page(g, true).await.is_err() {
            break;
        }
        let gm = g.borrow();
        if !gm.more || gm.channel_messages(cid).len() != count {
            break;
        }
    }
    render_messages(false);
    if box_.is_connected() {
        box_.set_scroll_top(box_.scroll_height() - before);
    }
}

pub fn render_messages(stick: bool) {
    let (Some(box_), Some(g)) = (by_id("messages"), current_guild()) else {
        return;
    };
    let gm = g.borrow();
    let Some(c) = gm.current_channel().cloned().filter(|c| !c.voice()) else {
        return;
    };
    let at_bottom = box_.scroll_height() - box_.scroll_top() - box_.client_height() < 80;
    let list = gm.channel_messages(&c.id);
    let (page_loaded, more) = (gm.page_loaded, gm.more);
    drop(gm);
    scope("messages", || {
        let mut nodes: Vec<Child> = Vec::new();
        if page_loaded && !more {
            nodes.push(
                h("div.channel-start")
                    .child(h("div.big-hash").text("#"))
                    .child(h("h2").text(format!("Bienvenue dans #{} !", c.name)))
                    .child(h("p").text(
                        "C'est le début du salon. Tout ce qui s'écrit ici est chiffré de bout en bout, jusqu'à l'auteur et l'heure : le serveur ne voit que des jetons illisibles.",
                    ))
                    .into(),
            );
        } else if !page_loaded {
            nodes.push(h("div.loading").text("Déchiffrement…").into());
        } else {
            let (gw, cid) = (Rc::downgrade(&g), c.id.clone());
            nodes.push(
                h("button.load-more")
                    .text("Messages plus anciens")
                    .click(move || {
                        let (gw, cid) = (gw.clone(), cid.clone());
                        async move {
                            if let Some(g) = gw.upgrade() {
                                load_older(&g, &cid).await;
                            }
                        }
                    })
                    .into(),
            );
        }
        let mut prev: Option<&Msg> = None;
        for m in &list {
            if prev.is_none_or(|p| day_label(p.t) != day_label(m.t)) {
                nodes.push(h("div.day").child(h("span").text(day_label(m.t))).into());
            }
            let grouped = prev.is_some_and(|p| {
                p.author == m.author
                    && m.t - p.t < 300_000.0
                    && m.reply == 0
                    && day_label(p.t) == day_label(m.t)
                    && m.verified != Some(false)
            });
            nodes.push(message_node(&g, m, grouped).into());
            prev = Some(m);
        }
        replace(&box_, nodes);
    });
    if stick || at_bottom {
        box_.set_scroll_top(box_.scroll_height());
    }
}

fn message_node(g: &G, m: &Msg, grouped: bool) -> El {
    let gm = g.borrow();
    let name = gm.name_of(&m.author);
    let mine = m.author == gm.me;
    let content: El = match m.kind.as_str() {
        "image" => {
            let src = m.media.clone();
            h("button.image")
                .attr("type", "button")
                .attr("aria-label", "Agrandir la photo")
                .child(
                    h("img")
                        .attr("src", &m.media)
                        .attr("alt", "Photo")
                        .attr("loading", "lazy"),
                )
                .on("click", move |_| lightbox(&src))
        }
        "audio" => h("audio.voice-note")
            .attr("controls", "")
            .attr("preload", "metadata")
            .attr("src", &m.media),
        _ => h("div.text")
            .class_if("jumbo", only_emoji(&m.text))
            .children(format_text(&m.text)),
    };
    let reply = (m.reply != 0).then(|| {
        let target = gm.msgs.get(&m.reply).filter(|t| !t.locked);
        let what = match target {
            Some(t) if t.kind == "image" => "📷 Photo".to_owned(),
            Some(t) if t.kind == "audio" => "🎤 Vocal".to_owned(),
            Some(t) => t.text.chars().take(90).collect(),
            None => "message plus ancien".to_owned(),
        };
        let id = m.reply;
        h("div.reply-ref")
            .child(h("span.curve"))
            .child(target.map(|t| h("strong").text(format!("@{}", gm.name_of(&t.author)))))
            .child(h("span").text(what))
            .on("click", move |_| scroll_to_message(id))
    });
    let forged = m.verified == Some(false);
    let (id, text, kind) = (m.id, m.text.clone(), m.kind.clone());
    let gw = Rc::downgrade(g);
    let reply_btn = icon_btn("Répondre", "↩").on("click", move |_| start_reply(id));
    let edit_btn = (mine && kind == "text")
        .then(|| icon_btn("Modifier", "✎").on("click", move |_| start_edit(id)));
    let copy_btn = (kind == "text").then(|| {
        icon_btn("Copier le texte", "⧉").click(move || {
            let t = text.clone();
            async move { copy(&t, "Copié !").await }
        })
    });
    let delete_btn = (mine || gm.admin).then(|| {
        icon_btn("Supprimer", "🗑").class("danger").click(move || {
            let gw = gw.clone();
            async move {
                if let Some(g) = gw.upgrade() {
                    delete_message(&g, id).await;
                }
            }
        })
    });
    let meta = (!grouped).then(|| {
        h("div.meta")
            .child(h("strong.author").style("--h", hue(&m.author).to_string()).text(&name))
            .child(forged.then(|| {
                h("span.warn")
                    .attr("title", "La signature ne correspond pas : ce message n'a pas été écrit par ce membre.")
                    .text("⚠ signature invalide")
            }))
            .child(m.verified.is_none().then(|| h("span.unverified").attr("title", "Auteur inconnu : signature non vérifiable.").text("?")))
            .child(h("time").attr("title", full_date(m.t)).text(if day_label(m.t) == "Aujourd'hui" {
                time_label(m.t)
            } else {
                format!("{} {}", day_label(m.t), time_label(m.t))
            }))
    });
    let node = h("article.message")
        .class_if("grouped", grouped)
        .class_if("mine", mine)
        .class_if("forged", forged)
        .id(&format!("m{}", m.id))
        .child(reply)
        .child(if grouped {
            h("time.hover-time").text(time_label(m.t))
        } else {
            avatar(&m.author, &name, "")
        })
        .child(
            h("div.body")
                .child(meta)
                .child(content)
                .child(m.edited.then(|| h("small.edited").text("(modifié)"))),
        )
        .child(
            h("div.actions")
                .child(reply_btn)
                .child(edit_btn)
                .child(copy_btn)
                .child(delete_btn),
        );
    if touch() {
        // Appui long : les actions du message.
        let el = node.0.clone();
        let timer: Rc<RefCell<Option<i32>>> = Rc::new(RefCell::new(None));
        let (t1, t2, t3) = (timer.clone(), timer.clone(), timer);
        let el2 = el.clone();
        let show = Closure::<dyn FnMut()>::new(move || {
            let _ = el2.class_list().add_1("show-actions");
        });
        let show_fn: js_sys::Function = show.as_ref().unchecked_ref::<js_sys::Function>().clone();
        listen(&el, "touchstart", move |_| {
            *t1.borrow_mut() = window()
                .set_timeout_with_callback_and_timeout_and_arguments_0(&show_fn, 450)
                .ok();
        });
        let cancel = move |t: &Rc<RefCell<Option<i32>>>| {
            if let Some(id) = t.borrow_mut().take() {
                window().clear_timeout_with_handle(id);
            }
        };
        listen(&el, "touchend", move |_| cancel(&t2));
        listen(&el, "touchmove", move |_| {
            if let Some(id) = t3.borrow_mut().take() {
                window().clear_timeout_with_handle(id);
            }
        });
        // Libérée avec les écouteurs de la zone : on la garde vivante ici.
        keep_alive(show);
    }
    node
}

thread_local! {
    static KEEP: RefCell<Vec<Closure<dyn FnMut()>>> = const { RefCell::new(Vec::new()) };
}

/// Garde une fonction de rappel le temps de la zone « messages ».
fn keep_alive(c: Closure<dyn FnMut()>) {
    KEEP.with(|k| {
        let mut k = k.borrow_mut();
        if k.len() > 2000 {
            k.drain(..1000);
        }
        k.push(c);
    });
}

fn scroll_to_message(id: i64) {
    let Some(el) = by_id(&format!("m{id}")) else {
        return;
    };
    el.scroll_into_view_with_bool(false);
    let _ = el.class_list().add_1("flash");
    spawn(async move {
        sleep(1400).await;
        let _ = el.class_list().remove_1("flash");
    });
}

pub fn render_typing() {
    let (Some(el), Some(g)) = (by_id("typing"), current_guild()) else {
        return;
    };
    let gm = g.borrow();
    let channel = gm.entry.channel.clone().unwrap_or_default();
    let names: Vec<String> = gm
        .typing
        .iter()
        .filter(|(_, (c, until))| *until > now() && *c == channel)
        .map(|(id, _)| gm.name_of(id))
        .collect();
    let text = match names.len() {
        0 => String::new(),
        1 => format!(" {} écrit…", names[0]),
        2 | 3 => format!(" {} écrivent…", names.join(", ")),
        _ => " Plusieurs personnes écrivent…".into(),
    };
    replace(
        &el,
        vec![
            (!names.is_empty())
                .then(|| h("span.dots").child(h("i")).child(h("i")).child(h("i")))
                .into(),
            text.into(),
        ],
    );
}

pub fn render_members() {
    let (Some(el), Some(g)) = (by_id("members"), current_guild()) else {
        return;
    };
    let gm = g.borrow();
    let mut all: Vec<Profile> = gm.members.values().cloned().collect();
    all.sort_by_key(|p| p.name.to_lowercase());
    let (on, off): (Vec<Profile>, Vec<Profile>) = all.into_iter().partition(|p| gm.online(&p.id));
    let (admin, me, fp) = (gm.admin, gm.me.clone(), gm.fingerprint.clone());
    let in_voice: Vec<String> = gm
        .presence
        .iter()
        .filter(|(_, p)| p.voice.is_some())
        .map(|(id, _)| id.clone())
        .collect();
    drop(gm);
    scope("members", || {
        let row = |p: &Profile, is_on: bool| {
            let gw = Rc::downgrade(&g);
            let target = p.clone();
            h("li.member")
                .class_if("on", is_on)
                .child(
                    h("span.avatar-wrap")
                        .child(avatar(&p.id, &p.name, ""))
                        .child(h("span.dot")),
                )
                .child(
                    h("span.name")
                        .text(&p.name)
                        .child((p.id == me).then(|| h("small").text(" (vous)")))
                        .child(in_voice.contains(&p.id).then(|| h("small").text(" 🔊"))),
                )
                .child((admin && p.id != me).then(|| {
                    icon_btn(&format!("Exclure {}", p.name), "⨯")
                        .class("kick")
                        .on("click", move |_| {
                            if let Some(g) = gw.upgrade() {
                                exclude_dialog(&g, &target);
                            }
                        })
                }))
        };
        replace(
            &el,
            vec![
                h("header.members-head")
                    .child(h("strong").text("Membres"))
                    .child(icon_btn("Fermer", "✕").on("click", |_| toggle_members(Some(false))))
                    .into(),
                h("div.section-title")
                    .text(format!("En ligne — {}", on.len()))
                    .into(),
                h("ul").children(on.iter().map(|p| row(p, true))).into(),
                (!off.is_empty())
                    .then(|| h("div.section-title").text(format!("Hors ligne — {}", off.len())))
                    .into(),
                h("ul").children(off.iter().map(|p| row(p, false))).into(),
                h("div.fp")
                    .child(h("small").text("Empreinte du serveur"))
                    .child(h("code").text(&fp))
                    .child(
                        h("small.muted")
                            .text("Identique chez tous les membres = personne ne s'est intercalé."),
                    )
                    .into(),
            ],
        );
    });
}

// ---------------------------------------------------------------------------
// Écrire

fn composer(g: &G, c: Option<&Channel>) -> El {
    let cid = c.map(|c| c.id.clone()).unwrap_or_default();
    let input = h("textarea#input")
        .attr("rows", "1")
        .attr(
            "placeholder",
            c.map(|c| format!("Message chiffré dans #{}", c.name))
                .unwrap_or_default(),
        )
        .attr("enterkeyhint", if touch() { "enter" } else { "send" })
        .attr("autocomplete", "off")
        .attr("aria-label", "Votre message")
        .attr("maxlength", "20000")
        .el();
    let file = h("input")
        .attr("type", "file")
        .attr("accept", "image/*")
        .attr("hidden", "")
        .el();

    let autosize = {
        let input = input.clone();
        move || {
            if let Some(e) = input.dyn_ref::<web_sys::HtmlElement>() {
                let _ = e.style().set_property("height", "auto");
                let _ = e
                    .style()
                    .set_property("height", &format!("{}px", input.scroll_height().min(200)));
            }
        }
    };
    let submit = {
        let (input, gw, cid, autosize) = (
            input.clone(),
            Rc::downgrade(g),
            cid.clone(),
            autosize.clone(),
        );
        Rc::new(move || {
            let text = input_value(&input).trim().to_owned();
            let Some(g) = gw.upgrade() else { return };
            if text.is_empty() || cid.is_empty() {
                return;
            }
            set_input_value(&input, "");
            autosize();
            let (input, cid, autosize) = (input.clone(), cid.clone(), autosize.clone());
            spawn(async move {
                let editing = app(|a| a.editing);
                let res = match editing {
                    Some(id) => {
                        cancel_bars();
                        edit_message(&g, id, &text).await
                    }
                    None => {
                        let r = send(&g, &cid, "text", &text, "").await;
                        cancel_bars();
                        r
                    }
                };
                if let Err(err) = res {
                    set_input_value(&input, &text);
                    autosize();
                    toast(err.message, true);
                }
            });
        })
    };
    {
        let (gw, autosize, input2) = (Rc::downgrade(g), autosize.clone(), input.clone());
        listen(&input, "input", move |_| {
            autosize();
            if !input_value(&input2).is_empty()
                && let Some(g) = gw.upgrade()
            {
                send_typing(&g);
            }
        });
    }
    {
        let (submit, gw, cid, input2) =
            (submit.clone(), Rc::downgrade(g), cid.clone(), input.clone());
        listen(&input, "keydown", move |e: Event| {
            let Some(k) = e.dyn_ref::<KeyboardEvent>() else {
                return;
            };
            if k.key() == "Enter" && !k.shift_key() && !touch() && !k.is_composing() {
                e.prevent_default();
                submit();
            } else if k.key() == "Escape" {
                cancel_bars();
            } else if k.key() == "ArrowUp" && input_value(&input2).is_empty() {
                let mine = gw.upgrade().and_then(|g| {
                    let g = g.borrow();
                    g.channel_messages(&cid)
                        .into_iter()
                        .rev()
                        .find(|m| m.author == g.me && m.kind == "text")
                        .map(|m| m.id)
                });
                if let Some(id) = mine {
                    e.prevent_default();
                    start_edit(id);
                }
            }
        });
    }
    {
        let (gw, cid, file2) = (Rc::downgrade(g), cid.clone(), file.clone());
        listen(&file, "change", move |_| {
            let input: &HtmlInputElement = file2.unchecked_ref();
            let Some(f) = input.files().and_then(|l| l.get(0)) else {
                return;
            };
            input.set_value("");
            let (gw, cid) = (gw.clone(), cid.clone());
            spawn(async move {
                let Some(g) = gw.upgrade() else { return };
                toast("Chiffrement de la photo…", false);
                let res = match read_image(&f).await {
                    Ok(jpeg) => send(&g, &cid, "image", &jpeg, "")
                        .await
                        .map_err(|e| e.message),
                    Err(e) => Err(e),
                };
                match res {
                    Ok(()) => cancel_bars(),
                    Err(e) => toast(e, true),
                }
            });
        });
    }
    let can_record =
        has(&window(), "MediaRecorder") && window().navigator().media_devices().is_ok();
    let file3 = file.clone();
    let send_btn = {
        let submit = submit.clone();
        h("button.send")
            .attr("type", "button")
            .attr("title", "Envoyer")
            .attr("aria-label", "Envoyer")
            .text("➤")
            .on("click", move |_| submit())
    };
    h("div.composer").child(h("div#bar")).child(
        h("div.input-row")
            .child(
                icon_btn("Envoyer une photo (chiffrée)", "＋")
                    .class("attach")
                    .on("click", move |_| {
                        if let Some(e) = file3.dyn_ref::<web_sys::HtmlElement>() {
                            e.click();
                        }
                    }),
            )
            .child(file)
            .child(input)
            .child(can_record.then(|| {
                let (gw, cid) = (Rc::downgrade(g), cid.clone());
                icon_btn("Message vocal (chiffré)", "🎤")
                    .class("mic")
                    .id("mic")
                    .click(move || {
                        let (gw, cid) = (gw.clone(), cid.clone());
                        async move {
                            if let Some(g) = gw.upgrade() {
                                toggle_recording(&g, &cid).await;
                            }
                        }
                    })
            }))
            .child(send_btn),
    )
}

/// Une photo : réduite (1600 px), recompressée en JPEG, en base64. Lue en
/// adresse `data:` (la politique de sécurité de la page refuse `blob:`).
async fn read_image(file: &web_sys::File) -> Result<String, String> {
    if !file.type_().starts_with("image/") {
        return Err("Seules les images sont acceptées.".into());
    }
    let source = read_data_url(file).await.ok_or("Fichier illisible.")?;
    let img = HtmlImageElement::new().map_err(|_| "image")?;
    let loaded = js_sys::Promise::new(&mut |resolve, reject| {
        img.set_onload(Some(&resolve));
        img.set_onerror(Some(&reject));
    });
    img.set_src(&source);
    wasm_bindgen_futures::JsFuture::from(loaded)
        .await
        .map_err(|_| "Format d'image non pris en charge.")?;
    let (w, hgt) = (img.natural_width() as f64, img.natural_height() as f64);
    let scale = (1600.0 / w.max(hgt)).min(1.0);
    let canvas: HtmlCanvasElement = h("canvas").el().unchecked_into();
    canvas.set_width((w * scale).round().max(1.0) as u32);
    canvas.set_height((hgt * scale).round().max(1.0) as u32);
    let ctx: web_sys::CanvasRenderingContext2d = canvas
        .get_context("2d")
        .ok()
        .flatten()
        .ok_or("canvas")?
        .unchecked_into();
    ctx.set_fill_style_str("#fff");
    ctx.fill_rect(0.0, 0.0, canvas.width() as f64, canvas.height() as f64);
    let _ = ctx.draw_image_with_html_image_element_and_dw_and_dh(
        &img,
        0.0,
        0.0,
        canvas.width() as f64,
        canvas.height() as f64,
    );
    let url = canvas
        .to_data_url_with_type_and_encoder_options("image/jpeg", &JsValue::from_f64(0.82))
        .map_err(|_| "Image illisible.")?;
    Ok(url
        .split_once(',')
        .map(|(_, b)| b.to_owned())
        .unwrap_or_default())
}

async fn read_data_url(blob: &web_sys::Blob) -> Option<String> {
    let reader = web_sys::FileReader::new().ok()?;
    let done = js_sys::Promise::new(&mut |resolve, reject| {
        reader.set_onload(Some(&resolve));
        reader.set_onerror(Some(&reject));
    });
    reader.read_as_data_url(blob).ok()?;
    wasm_bindgen_futures::JsFuture::from(done).await.ok()?;
    reader.result().ok()?.as_string()
}

/// Un message vocal : enregistré, puis chiffré comme une photo (2 min max).
async fn toggle_recording(g: &G, cid: &str) {
    if let Some(rec) = app(|a| a.recording.clone()) {
        let _ = rec.stop();
        return;
    }
    let Ok(devices) = window().navigator().media_devices() else {
        return;
    };
    let constraints = web_sys::MediaStreamConstraints::new();
    constraints.set_audio(&JsValue::TRUE);
    let stream = match devices.get_user_media_with_constraints(&constraints) {
        Ok(p) => wasm_bindgen_futures::JsFuture::from(p).await,
        Err(e) => Err(e),
    };
    let Ok(stream) = stream else {
        toast("Micro inaccessible.", true);
        return;
    };
    let stream: MediaStream = stream.unchecked_into();
    let mime = [
        "audio/mp4",
        "audio/webm;codecs=opus",
        "audio/webm",
        "audio/ogg;codecs=opus",
    ]
    .into_iter()
    .find(|t| web_sys::MediaRecorder::is_type_supported(t))
    .unwrap_or("");
    let options = web_sys::MediaRecorderOptions::new();
    if !mime.is_empty() {
        options.set_mime_type(mime);
        options.set_audio_bits_per_second(32_000);
    }
    let Ok(rec) =
        web_sys::MediaRecorder::new_with_media_stream_and_media_recorder_options(&stream, &options)
    else {
        toast("Enregistrement impossible.", true);
        return;
    };
    let chunks = Rc::new(RefCell::new(js_sys::Array::new()));
    let started = now();
    {
        let chunks = chunks.clone();
        listen_forever(&rec, "dataavailable", move |e: Event| {
            let data = get(&e, "data");
            if get(&data, "size").as_f64().unwrap_or_default() > 0.0 {
                chunks.borrow().push(&data);
            }
        });
    }
    {
        let (gw, cid, rec2, stream) = (
            Rc::downgrade(g),
            cid.to_owned(),
            rec.clone(),
            Clone::clone(&stream),
        );
        listen_forever(&rec, "stop", move |_| {
            for t in stream.get_tracks().iter() {
                t.unchecked_into::<web_sys::MediaStreamTrack>().stop();
            }
            app(|a| a.recording = None);
            if let Some(mic) = by_id("mic") {
                mic.set_text_content(Some("🎤"));
                toggle_class(&mic, "recording", false);
            }
            if now() - started < 600.0 {
                return;
            }
            let kind = rec2.mime_type();
            let kind = if kind.is_empty() {
                mime.to_owned()
            } else {
                kind
            };
            let kind = kind.split(';').next().unwrap_or("audio/webm").to_owned();
            let bag = web_sys::BlobPropertyBag::new();
            bag.set_type(&kind);
            let Ok(blob) =
                web_sys::Blob::new_with_blob_sequence_and_options(&chunks.borrow(), &bag)
            else {
                return;
            };
            let (gw, cid) = (gw.clone(), cid.clone());
            spawn(async move {
                let Some(g) = gw.upgrade() else { return };
                let Some(url) = read_data_url(&blob).await else {
                    return;
                };
                let data = url
                    .split_once(',')
                    .map(|(_, b)| b.to_owned())
                    .unwrap_or_default();
                if let Err(e) = send(&g, &cid, "audio", &data, &kind).await {
                    toast(e.message, true);
                }
            });
        });
    }
    app(|a| a.recording = Some(rec.clone()));
    if let Some(mic) = by_id("mic") {
        toggle_class(&mic, "recording", true);
    }
    let _ = rec.start_with_time_slice(1000);
    toast("Enregistrement… touchez ⏺ pour envoyer.", false);
    spawn(async move {
        loop {
            sleep(250).await;
            if app(|a| a.recording.is_none()) {
                break;
            }
            let s = ((now() - started) / 1000.0) as u32;
            if let Some(mic) = by_id("mic") {
                mic.set_text_content(Some(&format!("⏺ {}:{:02}", s / 60, s % 60)));
            }
            if s >= 120 {
                let _ = rec.stop();
                break;
            }
        }
    });
}

fn render_bar() {
    let Some(bar) = by_id("bar") else { return };
    let (editing, reply) = app(|a| (a.editing, a.reply_to));
    scope("bar", || {
        let close = || icon_btn("Annuler", "✕").on("click", |_| cancel_bars());
        let content: Vec<Child> = if editing.is_some() {
            vec![
                h("div.bar")
                    .child(h("span").text("✎ Modification du message"))
                    .child(close())
                    .into(),
            ]
        } else if let Some(id) = reply {
            let who = current_guild()
                .map(|g| {
                    let g = g.borrow();
                    g.msgs
                        .get(&id)
                        .map(|m| g.name_of(&m.author))
                        .unwrap_or_default()
                })
                .unwrap_or_default();
            vec![
                h("div.bar")
                    .child(
                        h("span")
                            .text("Réponse à ")
                            .child(h("strong").text(format!("@{who}"))),
                    )
                    .child(close())
                    .into(),
            ]
        } else {
            vec![]
        };
        replace(&bar, content);
    });
}

fn start_reply(id: i64) {
    app(|a| {
        a.editing = None;
        a.reply_to = Some(id);
    });
    render_bar();
    if let Some(i) = by_id("input") {
        focus(&i);
    }
}

fn start_edit(id: i64) {
    let text = current_guild()
        .and_then(|g| g.borrow().msgs.get(&id).map(|m| m.text.clone()))
        .unwrap_or_default();
    app(|a| {
        a.reply_to = None;
        a.editing = Some(id);
    });
    if let Some(i) = by_id("input") {
        set_input_value(&i, &text);
        let _ = i.dispatch_event(&Event::new("input").expect("événement"));
        focus(&i);
    }
    render_bar();
}

fn cancel_bars() {
    let was_editing = app(|a| {
        let was = a.editing.is_some();
        a.editing = None;
        a.reply_to = None;
        was
    });
    if was_editing && let Some(i) = by_id("input") {
        set_input_value(&i, "");
    }
    render_bar();
}

// ---------------------------------------------------------------------------
// Mise en forme : **gras**, *italique*, ~~barré~~, `code`, ```bloc```,
// > citation, liens. Des nœuds DOM : aucun HTML injecté.

fn format_inline(text: &str) -> Vec<Child> {
    let mut out: Vec<Child> = Vec::new();
    let mut plain = String::new();
    let mut i = 0;
    let flush = |plain: &mut String, out: &mut Vec<Child>| {
        if !plain.is_empty() {
            out.push(std::mem::take(plain).into());
        }
    };
    while i < text.len() {
        let rest = &text[i..];
        // Un délimiteur, puis sa fermeture sur la même ligne.
        let span = |open: &str, close: &str| -> Option<(usize, &str)> {
            let inner = rest.strip_prefix(open)?;
            let end = inner.find(close)?;
            let body = &inner[..end];
            (!body.is_empty() && !body.contains('\n'))
                .then_some((open.len() + end + close.len(), body))
        };
        if let Some((len, body)) = span("`", "`") {
            flush(&mut plain, &mut out);
            out.push(h("code").text(body).into());
            i += len;
        } else if let Some((len, body)) = span("**", "**") {
            flush(&mut plain, &mut out);
            out.push(h("strong").children(format_inline(body)).into());
            i += len;
        } else if let Some((len, body)) = span("~~", "~~") {
            flush(&mut plain, &mut out);
            out.push(h("s").children(format_inline(body)).into());
            i += len;
        } else if let Some((len, body)) = span("*", "*").or_else(|| span("_", "_")) {
            flush(&mut plain, &mut out);
            out.push(h("em").children(format_inline(body)).into());
            i += len;
        } else if rest.starts_with("https://") || rest.starts_with("http://") {
            let mut end = rest
                .find(|c: char| c.is_whitespace() || "<>\"".contains(c))
                .unwrap_or(rest.len());
            while end > 0 && ".,;:!?)]'".contains(&rest[..end][end - 1..]) {
                end -= 1;
            }
            let url = &rest[..end];
            flush(&mut plain, &mut out);
            out.push(
                h("a")
                    .attr("href", url)
                    .attr("target", "_blank")
                    .attr("rel", "noopener noreferrer nofollow")
                    .text(url)
                    .into(),
            );
            i += end.max(1);
        } else {
            let c = rest.chars().next().unwrap_or(' ');
            plain.push(c);
            i += c.len_utf8();
        }
    }
    flush(&mut plain, &mut out);
    out
}

fn format_text(text: &str) -> Vec<Child> {
    let mut out: Vec<Child> = Vec::new();
    for (i, part) in text.split("```").enumerate() {
        if i % 2 == 1 {
            // Un bloc de code (la première ligne peut nommer le langage).
            let body = match part.split_once('\n') {
                Some((lang, rest)) if lang.chars().all(|c| c.is_ascii_alphanumeric()) => rest,
                _ => part,
            };
            out.push(
                h("pre")
                    .child(h("code").text(body.trim_end_matches('\n')))
                    .into(),
            );
            continue;
        }
        let lines: Vec<&str> = part.split('\n').collect();
        for (j, line) in lines.iter().enumerate() {
            if let Some(q) = line.strip_prefix("> ") {
                out.push(h("blockquote").children(format_inline(q)).into());
            } else {
                out.extend(format_inline(line));
                if j + 1 < lines.len() {
                    out.push(h("br").into());
                }
            }
        }
    }
    out
}

fn only_emoji(t: &str) -> bool {
    let emoji = |c: char| {
        let n = c as u32;
        n >= 0x1F000
            || (0x2300..=0x2BFF).contains(&n)
            || (0xFE00..=0xFE0F).contains(&n)
            || n == 0x200D
            || n == 0x20E3
    };
    let visible: Vec<char> = t.chars().filter(|c| !c.is_whitespace()).collect();
    !visible.is_empty()
        && visible.len() <= 12
        && visible.iter().all(|c| emoji(*c))
        && visible.iter().any(|c| *c as u32 >= 0x2300)
}

// ---------------------------------------------------------------------------
// Fenêtres

thread_local! {
    static DIALOGS: RefCell<u32> = const { RefCell::new(0) };
}

/// Une fenêtre modale : `build` reçoit la fenêtre (pour la fermer) et rend
/// son contenu et ses boutons ; ses écouteurs vivent autant qu'elle.
fn dialog(
    title: &str,
    build: impl FnOnce(&HtmlDialogElement) -> (El, Vec<El>),
) -> HtmlDialogElement {
    let n = DIALOGS.with(|d| {
        *d.borrow_mut() += 1;
        *d.borrow()
    });
    let name = format!("dialog-{n}");
    let d: HtmlDialogElement = h("dialog.modal").el().unchecked_into();
    scope(&name, || {
        let (body, actions) = build(&d);
        let form = h("form.modal-body")
            .attr("method", "dialog")
            .child(h("h2").text(title))
            .child(body)
            .child(h("div.modal-actions").children(actions));
        let _ = d.append_child(&form.0);
        let (d2, name2) = (d.clone(), name.clone());
        listen(&d, "close", move |_| {
            d2.remove();
            let name = name2.clone();
            spawn(async move { drop_scope(&name) });
        });
        let d3 = d.clone();
        listen(&d, "click", move |e| {
            if e.target().is_some_and(|t| t == *d3.as_ref()) {
                d3.close();
            }
        });
    });
    if let Some(body) = document().body() {
        let _ = body.append_child(&d);
    }
    let _ = d.show_modal();
    d
}

fn field(label: &str, input: &Element, hint: Option<&str>) -> El {
    h("label.field")
        .child(h("span").text(label))
        .child(input.clone())
        .child(hint.map(|t| h("small.muted").text(t)))
}

fn input(placeholder: &str, value: &str, max: u32) -> Element {
    let el = h("input")
        .attr("placeholder", placeholder)
        .attr("maxlength", max.to_string())
        .el();
    set_input_value(&el, value);
    el
}

fn close_btn(label: &str) -> El {
    h("button.ghost").attr("value", "cancel").text(label)
}

fn welcome() -> El {
    let (name, server, invite) = app(|a| {
        (
            a.saved.name.clone(),
            a.saved.server.clone(),
            a.pending_invite.is_some(),
        )
    });
    let name_in = input("Votre pseudo", &name, 32);
    let _ = name_in.set_attribute("autocomplete", "nickname");
    let server_in = input("https://mon-alley.herokuapp.com", &server, 200);
    let _ = server_in.set_attribute("type", "url");
    let need = needs_server();
    let (n2, s2) = (name_in.clone(), server_in.clone());
    h("div.welcome").child(
        h("form.welcome-card")
            .child(h("img").attr("src", base_url("/assets/icon-192.png")).attr("alt", "").attr("width", "72").attr("height", "72"))
            .child(h("h1").text(if invite { "Vous êtes invité sur Alley" } else { "Bienvenue sur Alley" }))
            .child(h("p.muted").text(
                "Des serveurs, des salons et des appels comme sur Discord, chiffrés de bout en bout. Pas de compte : vos clés restent sur cet appareil.",
            ))
            .child(field("Comment voulez-vous apparaître ?", &name_in, Some("Votre pseudo est chiffré, comme vos messages.")))
            .child(need.then(|| {
                field(
                    "Adresse du serveur Alley",
                    &server_in,
                    Some("Cette page est hébergée sur GitHub Pages : indiquez le serveur qui relaie les messages chiffrés."),
                )
            }))
            .child(h("button.primary").attr("type", "submit").text(if invite { "Rejoindre le serveur" } else { "Continuer" }))
            .child(h("p.small.muted").text("🔒 AES-256-GCM · X25519 · ECDSA · protocole Brocolis · serveur Paquets · 100 % Rust"))
            .on("submit", move |e| {
                e.prevent_default();
                let n = input_value(&n2).trim().to_owned();
                if n.is_empty() {
                    return focus(&n2);
                }
                if needs_server() {
                    let s = input_value(&s2).trim().trim_end_matches('/').to_owned();
                    if !s.starts_with("https://") || s.len() < 12 || s[8..].contains('/') {
                        return toast("Indiquez l'adresse https:// du serveur Alley.", true);
                    }
                    app(|a| a.saved.server = s);
                }
                app(|a| a.saved.name = n);
                save();
                spawn(async {
                    if app(|a| a.pending_invite.is_some()) {
                        return handle_invite().await;
                    }
                    let first = app(|a| a.current.clone().or_else(|| a.guilds.first().map(|g| g.borrow().gid.clone())));
                    match first {
                        Some(gid) => select_guild(&gid, None).await,
                        None => {
                            render();
                            add_guild_dialog(true);
                        }
                    }
                });
            }),
    )
}

pub fn add_guild_dialog(first: bool) {
    let me = app(|a| a.saved.name.clone());
    let name = input(
        "Ex. : Les copains du lycée",
        &if first {
            format!("Le serveur de {me}")
        } else {
            String::new()
        },
        60,
    );
    let link = input("https://…/app/#rejoindre=…", "", 2000);
    let focus_on = name.clone();
    dialog("Créer ou rejoindre un serveur", |d| {
        let (d1, d2, n, l) = (d.clone(), d.clone(), name.clone(), link.clone());
        let body = h("div.stack")
            .child(
                h("section.choice")
                    .child(h("h3").text("🏰 Créer un serveur"))
                    .child(field("Nom du serveur", &name, None))
                    .child(h("button.primary").attr("type", "button").text("Créer").click(move || {
                        let (d, n) = (d1.clone(), n.clone());
                        async move {
                            let v = input_value(&n).trim().to_owned();
                            if v.is_empty() {
                                return focus(&n);
                            }
                            match create_guild(&v).await {
                                Ok(g) => {
                                    d.close();
                                    let gid = g.borrow().gid.clone();
                                    select_guild(&gid, None).await;
                                    invite_dialog(&g, true);
                                }
                                Err(e) => toast(e.message, true),
                            }
                        }
                    })),
            )
            .child(
                h("section.choice")
                    .child(h("h3").text("🔗 Rejoindre avec un lien"))
                    .child(field("Lien d'invitation", &link, Some("La clé est dans le lien : demandez-le à un membre du serveur.")))
                    .child(h("button").attr("type", "button").text("Rejoindre").click(move || {
                        let (d, l) = (d2.clone(), l.clone());
                        async move {
                            match join_from_link(&input_value(&l)).await {
                                Ok(g) => {
                                    d.close();
                                    let (gid, name) = {
                                        let g = g.borrow();
                                        (g.gid.clone(), g.name.clone())
                                    };
                                    select_guild(&gid, None).await;
                                    toast(format!("Bienvenue sur « {name} » !"), false);
                                }
                                Err(e) if e.status == 404 => toast("Serveur introuvable : le lien est-il complet et à jour ?", true),
                                Err(e) => toast(e.message, true),
                            }
                        }
                    })),
            );
        (body, vec![close_btn("Fermer")])
    });
    focus(&focus_on);
}

pub fn invite_dialog(g: &G, just_created: bool) {
    let origin = window().location().origin().unwrap_or_default();
    let (link, admin_link, name, fp, is_admin) = {
        let gm = g.borrow();
        (
            gm.invite_link(&origin, &base(), false),
            gm.invite_link(&origin, &base(), true),
            gm.name.clone(),
            gm.fingerprint.clone(),
            gm.entry.admin.is_some(),
        )
    };
    let title = if just_created {
        "Votre serveur est prêt 🎉".to_owned()
    } else {
        format!("Inviter sur « {name} »")
    };
    dialog(&title, |_| {
        let field_el = input("", &link, 4000);
        let _ = field_el.set_attribute("readonly", "");
        let _ = field_el.class_list().add_1("link");
        let l1 = link.clone();
        let mut actions = vec![
            h("button.primary")
                .attr("type", "button")
                .text("Copier le lien")
                .click(move || {
                    let l = l1.clone();
                    async move { copy(&l, "Lien copié !").await }
                }),
        ];
        let nav: JsValue = window().navigator().into();
        if has(&nav, "share") {
            let (l2, n2) = (link.clone(), name.clone());
            actions.push(
                h("button")
                    .attr("type", "button")
                    .text("Partager…")
                    .click(move || {
                        let data = js_sys::Object::new();
                        let _ = js_sys::Reflect::set(
                            &data,
                            &"title".into(),
                            &format!("Rejoins « {n2} » sur Alley").into(),
                        );
                        let _ = js_sys::Reflect::set(
                            &data,
                            &"text".into(),
                            &format!("Rejoins « {n2} » sur Alley (chiffré de bout en bout) :")
                                .into(),
                        );
                        let _ = js_sys::Reflect::set(&data, &"url".into(), &l2.clone().into());
                        let nav: JsValue = window().navigator().into();
                        async move {
                            let _ = await_promise(call(&nav, "share", &[data.into()])).await;
                        }
                    }),
            );
        }
        actions.push(close_btn("Fermer"));
        let al = admin_link.clone();
        let body = h("div.stack")
            .child(h("p").text(
                "Toute personne qui a ce lien peut rejoindre le serveur. Envoyez-le par un canal de confiance — et si un lien fuit, l'admin renouvelle la clé : l'ancien lien ne mène plus nulle part.",
            ))
            .child(field_el)
            .child(h("p.muted.small").text("La clé est après le « # » : cette partie du lien n'est jamais envoyée au serveur."))
            .child(h("div.fp-inline").child(h("span").text("Empreinte : ")).child(h("code").text(&fp)))
            .child(is_admin.then(|| {
                h("details")
                    .child(h("summary").text("Lien d'administration (à garder pour vous)"))
                    .child(h("p.muted.small").text(
                        "Il donne aussi les droits d'administrateur : ouvrez-le sur vos autres appareils, ne le partagez pas.",
                    ))
                    .child(h("button").attr("type", "button").text("Copier le lien d'administration").click(move || {
                        let l = al.clone();
                        async move { copy(&l, "Lien d'administration copié").await }
                    }))
            }));
        (body, actions)
    });
}

fn guild_menu(g: &G) {
    let (name, admin) = {
        let gm = g.borrow();
        (gm.name.clone(), gm.admin)
    };
    dialog(&name, |d| {
        let item = |label: &str, f: Box<dyn Fn()>| {
            let d = d.clone();
            h("button")
                .attr("type", "button")
                .text(label)
                .on("click", move |e| {
                    e.prevent_default();
                    d.close();
                    f();
                })
        };
        let (g1, g2, g3, g4, g5) = (g.clone(), g.clone(), g.clone(), g.clone(), g.clone());
        let n = name.clone();
        let body = h("div.menu")
            .child(item("🔗 Inviter des gens", Box::new(move || invite_dialog(&g1, false))))
            .child(admin.then(|| item("➕ Créer un salon", Box::new(move || channel_dialog(&g2, None, "text")))))
            .child(admin.then(|| item("⚙ Paramètres du serveur", Box::new(move || guild_settings(&g3)))))
            .child(item("🔒 Chiffrement et empreinte", Box::new(move || security_dialog(&g4))))
            .child(item(
                "🚪 Quitter le serveur",
                Box::new(move || {
                    if !confirm(&format!(
                        "Quitter « {n} » ? La clé sera effacée de cet appareil : il vous faudra un nouveau lien pour revenir."
                    )) {
                        return;
                    }
                    let (g, n) = (g5.clone(), n.clone());
                    spawn(async move {
                        let (gid, extra) = {
                            let gm = g.borrow();
                            (gm.gid.clone(), vec![("x-alley-member", gm.entry.member.clone())])
                        };
                        let _ = app::api(&g, "DELETE", &format!("/api/v1/guilds/{gid}/members/me"), None, extra).await;
                        forget(&g, &format!("Vous avez quitté « {n} »."));
                    });
                }),
            )
            .class("danger"));
        (body, vec![close_btn("Fermer")])
    });
}

fn security_dialog(g: &G) {
    let (fp, admin, key) = {
        let gm = g.borrow();
        (gm.fingerprint.clone(), gm.admin, gm.key)
    };
    dialog("Chiffrement de bout en bout", |d| {
        let (d1, g1) = (d.clone(), g.clone());
        let body = h("div.stack")
            .child(h("p").text(
                "Messages, photos, vocaux, noms et pseudos — et l'auteur, le salon et l'heure de chaque message — sont chiffrés sur votre appareil (AES-256-GCM) et signés (ECDSA) avant de partir.",
            ))
            .child(h("div.fp-big").child(h("small").text("Empreinte du serveur")).child(h("code").text(&fp)))
            .child(h("p.muted.small").text(
                "Comparez-la de vive voix avec les autres membres : identique pour tout le monde, personne n'a remplacé la clé.",
            ))
            .child(admin.then(|| {
                h("section.choice")
                    .child(h("h3").text("♻️ Renouveler la clé"))
                    .child(h("p.muted.small").text(
                        "Si un lien a fui : une nouvelle clé est remise à chaque membre actuel, chiffrée pour lui seul (X25519). Les anciens liens ne mènent plus nulle part.",
                    ))
                    .child(h("button").attr("type", "button").text("Renouveler maintenant").click(move || {
                        let (d, g) = (d1.clone(), g1.clone());
                        async move {
                            match rotate_key(&g, vec![]).await {
                                Ok(()) => {
                                    d.close();
                                    let fp = g.borrow().fingerprint.clone();
                                    toast(format!("Clé renouvelée. Nouvelle empreinte : {fp}"), false);
                                }
                                Err(e) => toast(e.message, true),
                            }
                        }
                    }))
            }))
            .child(
                h("details")
                    .child(h("summary").text("Clé au format Brocolis 🥦"))
                    .child(h("p.muted.small").text(
                        "Les jetons d'Alley sont ceux de la messagerie Brocolis (AES-256-GCM, [[BROCOLIS:v1:…]]). Cette clé (base64) donne accès à tout le serveur : gardez-la secrète.",
                    ))
                    .child(h("button").attr("type", "button").text("Copier la clé").click(move || async move {
                        copy(&crypto::b64(&key), "Clé copiée").await
                    })),
            );
        (body, vec![close_btn("Fermer")])
    });
}

fn exclude_dialog(g: &G, p: &Profile) {
    let (name, id) = (p.name.clone(), p.id.clone());
    dialog(&format!("Exclure {name} ?"), |d| {
        let (d1, g1, n1) = (d.clone(), g.clone(), name.clone());
        let body = h("div.stack")
            .child(h("p").text(format!(
                "{name} ne pourra plus rien lire ni écrire : la clé du serveur est renouvelée et remise, chiffrée, à chacun des autres membres. Ses anciens liens ne marchent plus et son identifiant est banni."
            )))
            .child(h("p.muted.small").text("Ce qui est déjà sur son appareil y reste : aucune messagerie ne peut l'effacer à distance."));
        let go = h("button.primary.danger-fill")
            .attr("type", "button")
            .text("Exclure")
            .click(move || {
                let (d, g, n, id) = (d1.clone(), g1.clone(), n1.clone(), id.clone());
                async move {
                    match rotate_key(&g, vec![id]).await {
                        Ok(()) => {
                            d.close();
                            let fp = g.borrow().fingerprint.clone();
                            toast(format!("{n} a été exclu. Nouvelle empreinte : {fp}"), false);
                        }
                        Err(e) => toast(e.message, true),
                    }
                }
            });
        (body, vec![close_btn("Annuler"), go])
    });
}

fn guild_settings(g: &G) {
    let name = g.borrow().name.clone();
    let name_in = input("", &name, 60);
    dialog("Paramètres du serveur", |d| {
        let (d1, d2, g1, g2, n1) = (d.clone(), d.clone(), g.clone(), g.clone(), name_in.clone());
        let nm = name.clone();
        let body = h("div.stack")
            .child(field("Nom du serveur", &name_in, Some("Chiffré : seuls les membres le voient.")))
            .child(
                h("details.danger-zone")
                    .child(h("summary").text("Zone dangereuse"))
                    .child(h("p.muted.small").text(
                        "Supprimer le serveur efface définitivement tous ses salons et messages, pour tout le monde.",
                    ))
                    .child(h("button.danger").attr("type", "button").text("Supprimer le serveur").click(move || {
                        let (d, g, nm) = (d1.clone(), g1.clone(), nm.clone());
                        async move {
                            if prompt(&format!("Pour confirmer, tapez le nom du serveur : {nm}")).as_deref() != Some(nm.as_str()) {
                                return;
                            }
                            let gid = g.borrow().gid.clone();
                            match app::api(&g, "DELETE", &format!("/api/v1/guilds/{gid}"), None, vec![]).await {
                                Ok(_) => {
                                    d.close();
                                    forget(&g, "Serveur supprimé.");
                                }
                                Err(e) => toast(e.message, true),
                            }
                        }
                    })),
            );
        let save_btn = h("button.primary")
            .attr("type", "button")
            .text("Enregistrer")
            .click(move || {
                let (d, g, n) = (d2.clone(), g2.clone(), n1.clone());
                async move {
                    let v = input_value(&n).trim().to_owned();
                    if v.is_empty() {
                        return;
                    }
                    match put_state(&g, |mut s| {
                        s.name = v.clone();
                        s
                    })
                    .await
                    {
                        Ok(()) => d.close(),
                        Err(e) => toast(e.message, true),
                    }
                }
            });
        (body, vec![close_btn("Annuler"), save_btn])
    });
}

fn channel_dialog(g: &G, c: Option<Channel>, kind: &str) {
    let kind = c
        .as_ref()
        .map(|c| c.kind.clone())
        .unwrap_or_else(|| kind.to_owned());
    let voice = kind == "voice";
    let name_in = input(
        if voice {
            "Salon vocal"
        } else {
            "nouveau-salon"
        },
        c.as_ref().map_or("", |c| &c.name),
        60,
    );
    let topic_in = input(
        "De quoi parle-t-on ici ?",
        c.as_ref().map_or("", |c| &c.topic),
        200,
    );
    let count = g.borrow().state.channels.len();
    let title = match &c {
        Some(c) => format!("Modifier {}{}", if voice { "🔊 " } else { "#" }, c.name),
        None if voice => "Créer un salon vocal".into(),
        None => "Créer un salon textuel".into(),
    };
    let focus_on = name_in.clone();
    dialog(&title, |d| {
        let delete = c.as_ref().filter(|_| count > 1).map(|c| {
            let (d, g, id, n) = (d.clone(), g.clone(), c.id.clone(), c.name.clone());
            h("button.danger")
                .attr("type", "button")
                .text("Supprimer le salon")
                .click(move || {
                    let (d, g, id, n) = (d.clone(), g.clone(), id.clone(), n.clone());
                    async move {
                        if !confirm(&format!("Supprimer « {n} » ?")) {
                            return;
                        }
                        match put_state(&g, |mut s| {
                            s.channels.retain(|x| x.id != id);
                            s
                        })
                        .await
                        {
                            Ok(()) => d.close(),
                            Err(e) => toast(e.message, true),
                        }
                    }
                })
        });
        let body = h("div.stack")
            .child(field("Nom du salon", &name_in, None))
            .child((!voice).then(|| {
                field(
                    "Sujet",
                    &topic_in,
                    Some("Facultatif. Chiffré, comme le nom."),
                )
            }))
            .child(delete);
        let (d2, g2, n2, t2, existing, kind2) = (
            d.clone(),
            g.clone(),
            name_in.clone(),
            topic_in.clone(),
            c.clone(),
            kind.clone(),
        );
        let ok = h("button.primary")
            .attr("type", "button")
            .text(if c.is_some() { "Enregistrer" } else { "Créer" })
            .click(move || {
                let (d, g, n, t, existing, kind) = (
                    d2.clone(),
                    g2.clone(),
                    n2.clone(),
                    t2.clone(),
                    existing.clone(),
                    kind2.clone(),
                );
                async move {
                    let raw = input_value(&n).trim().to_owned();
                    let name = if kind == "voice" {
                        raw
                    } else {
                        raw.to_lowercase()
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .join("-")
                    };
                    if name.is_empty() {
                        return focus(&n);
                    }
                    let topic = input_value(&t).trim().to_owned();
                    let res = match &existing {
                        Some(c) => {
                            let id = c.id.clone();
                            put_state(&g, |mut s| {
                                for x in &mut s.channels {
                                    if x.id == id {
                                        x.name = name.clone();
                                        x.topic = topic.clone();
                                    }
                                }
                                s
                            })
                            .await
                            .map(|_| None)
                        }
                        None => {
                            if g.borrow().state.channels.len() >= 100 {
                                return toast("Ce serveur a déjà 100 salons.", true);
                            }
                            let id = crypto::hex(&crypto::random_bytes::<8>());
                            put_state(&g, |mut s| {
                                s.channels.push(Channel {
                                    id: id.clone(),
                                    name: name.clone(),
                                    topic: topic.clone(),
                                    kind: kind.clone(),
                                });
                                s
                            })
                            .await
                            .map(|_| Some(id.clone()))
                        }
                    };
                    match res {
                        Ok(created) => {
                            d.close();
                            if let Some(id) = created {
                                select_channel(&g, &id);
                            }
                        }
                        Err(e) => toast(e.message, true),
                    }
                }
            });
        (body, vec![close_btn("Annuler"), ok])
    });
    focus(&focus_on);
}

fn settings_dialog() {
    let (name, server, notify, count) = app(|a| {
        (
            a.saved.name.clone(),
            a.saved.server.clone(),
            a.saved.notify,
            a.guilds.len(),
        )
    });
    let name_in = input("", &name, 32);
    let server_in = input(
        &if meta("alley-api").is_empty() {
            "même site".into()
        } else {
            meta("alley-api")
        },
        &server,
        200,
    );
    let can_notify = has(&window(), "Notification");
    let granted = can_notify
        && web_sys::Notification::permission() == web_sys::NotificationPermission::Granted;
    let notif = h("input")
        .attr("type", "checkbox")
        .attr_if("disabled", !can_notify)
        .prop("checked", notify && granted)
        .el();
    dialog("Réglages", |d| {
        let body = h("div.stack")
            .child(field("Pseudo", &name_in, Some("Mis à jour (chiffré) sur tous vos serveurs.")))
            .child(h("label.check").child(notif.clone()).child(h("span").text("Notifications des nouveaux messages")))
            .child((!can_notify).then(|| {
                h("p.muted.small").text(
                    "Sur iPhone et iPad, installez d'abord Alley sur l'écran d'accueil (Partager → Sur l'écran d'accueil) pour recevoir des notifications.",
                )
            }))
            .child(
                h("details")
                    .child(h("summary").text("Avancé"))
                    .child(field("Serveur Alley", &server_in, Some("Laissez vide pour utiliser celui de cette page."))),
            )
            .child(h("p.muted.small").text(format!("Alley garde {count} serveur(s) et leurs clés sur cet appareil uniquement.")));
        let (d2, n2, s2, c2) = (d.clone(), name_in.clone(), server_in.clone(), notif.clone());
        let ok = h("button.primary")
            .attr("type", "button")
            .text("Enregistrer")
            .click(move || {
                let (d, n, s, c) = (d2.clone(), n2.clone(), s2.clone(), c2.clone());
                async move {
                    let name = input_value(&n).trim().to_owned();
                    if name.is_empty() {
                        return focus(&n);
                    }
                    let server = input_value(&s).trim().trim_end_matches('/').to_owned();
                    let valid = server.is_empty()
                        || (server.starts_with("https://") && !server[8..].contains('/'))
                        || server.starts_with("http://localhost");
                    if !valid {
                        return toast("Le serveur doit être en https://", true);
                    }
                    let checked = c.unchecked_ref::<HtmlInputElement>().checked();
                    let mut notify = checked;
                    if checked
                        && web_sys::Notification::permission()
                            != web_sys::NotificationPermission::Granted
                    {
                        let res = match web_sys::Notification::request_permission() {
                            Ok(p) => wasm_bindgen_futures::JsFuture::from(p)
                                .await
                                .ok()
                                .and_then(|v| v.as_string()),
                            Err(_) => None,
                        };
                        notify = res.as_deref() == Some("granted");
                        if !notify {
                            toast("Notifications refusées par le navigateur.", true);
                        }
                    }
                    let (changed, moved) = app(|a| {
                        let changed = a.saved.name != name;
                        let moved = a.saved.server != server;
                        a.saved.name = name.clone();
                        a.saved.server = server.clone();
                        a.saved.notify = notify;
                        (changed, moved)
                    });
                    save();
                    d.close();
                    if moved {
                        let _ = window().location().reload();
                        return;
                    }
                    if changed {
                        for g in app(|a| a.guilds.clone()) {
                            if g.borrow().loaded {
                                let _ = announce(&g).await;
                            }
                        }
                        render();
                    }
                }
            });
        (body, vec![close_btn("Annuler"), ok])
    });
}

fn lightbox(src: &str) {
    let d: HtmlDialogElement = h("dialog.lightbox").el().unchecked_into();
    scope("lightbox", || {
        let _ = d.append_child(&h("img").attr("src", src).attr("alt", "Photo").el());
        let d1 = d.clone();
        let _ = d.append_child(
            &icon_btn("Fermer", "✕")
                .class("close")
                .on("click", move |_| d1.close())
                .el(),
        );
        let d2 = d.clone();
        listen(&d, "click", move |_| d2.close());
        let d3 = d.clone();
        listen(&d, "close", move |_| d3.remove());
    });
    if let Some(body) = document().body() {
        let _ = body.append_child(&d);
    }
    let _ = d.show_modal();
}

/// iOS : garde la zone de saisie au-dessus du clavier.
pub fn track_viewport() {
    let set = || {
        let height = window()
            .visual_viewport()
            .map(|v| v.height())
            .or_else(|| window().inner_height().ok().and_then(|h| h.as_f64()))
            .unwrap_or_default();
        if let Some(root) = document().document_element()
            && let Some(e) = root.dyn_ref::<web_sys::HtmlElement>()
        {
            let _ = e
                .style()
                .set_property("--app-height", &format!("{height}px"));
        }
    };
    set();
    if let Some(vv) = window().visual_viewport() {
        listen_forever(&vv, "resize", move |_| set());
    }
    listen_forever(&window(), "resize", move |_| set());
}

/// Le chiffrement et les appels n'existent qu'en HTTPS : on y renvoie.
pub fn insecure() {
    let Some(root) = by_id("root") else { return };
    let https = window()
        .location()
        .href()
        .unwrap_or_default()
        .replacen("http:", "https:", 1);
    scope("root", || {
        replace(
            &root,
            vec![
                h("div.welcome")
                    .child(
                        h("div.welcome-card")
                            .child(h("h1").text("Connexion non sécurisée"))
                            .child(h("p").text("Alley chiffre tout dans votre navigateur et passe des appels, ce qui n'est possible qu'en HTTPS."))
                            .child(h("a.button-link").attr("href", https).text("Ouvrir la version sécurisée")),
                    )
                    .into(),
            ],
        );
    });
}
