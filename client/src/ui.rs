//! L'affichage : le pseudonyme, le hall des salons, le salon (messages,
//! présents, appel). Du DOM construit en Rust, jamais de HTML injecté.

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
use crate::dom::*;
use crate::model::*;
use crate::net::needs_server;

// ---------------------------------------------------------------------------
// Les écrans

pub fn render() {
    let Some(root) = by_id("root") else { return };
    root.set_class_name("");
    let (name, in_room) = app(|a| (a.name.clone(), a.room.is_some()));
    if name.is_empty() {
        scope("root", || replace(&root, vec![welcome().into()]));
        return;
    }
    if !in_room {
        scope("root", || replace(&root, vec![lobby().into()]));
        render_rooms();
        return;
    }
    render_room();
}

/// L'heure locale d'une échéance (`00:00`).
fn until(t: f64) -> String {
    time_label(t)
}

fn people_label(n: u64) -> String {
    if n <= 1 {
        format!("{n} présent")
    } else {
        format!("{n} présents")
    }
}

fn welcome() -> El {
    let (server, invited) = app(|a| (a.server.clone(), a.invited.is_some()));
    let name_in = h("input")
        .attr("placeholder", "Votre pseudonyme")
        .attr("maxlength", "32")
        .attr("autocomplete", "nickname")
        .el();
    let server_in = h("input")
        .attr("placeholder", "https://mon-alley.herokuapp.com")
        .attr("type", "url")
        .el();
    set_input_value(&server_in, &server);
    let need = needs_server();
    let (n2, s2) = (name_in.clone(), server_in.clone());
    let focus_on = name_in.clone();
    spawn(async move {
        sleep(50).await;
        if !touch() {
            focus(&focus_on);
        }
    });
    h("div.welcome").child(
        h("form.welcome-card")
            .child(h("img").attr("src", base_url("/assets/icon-192.png")).attr("alt", "").attr("width", "72").attr("height", "72"))
            .child(h("h1").text(if invited { "On vous attend dans un salon" } else { "Alley" }))
            .child(h("p.muted").text(
                "Un pseudonyme, et c'est tout. Rien n'est enregistré : les salons vivent tant que quelqu'un y est et disparaissent à minuit.",
            ))
            .child(field("Votre pseudonyme", &name_in, Some("Chiffré : seuls les présents du salon le voient.")))
            .child(need.then(|| {
                h("div.notice")
                    .child(h("strong").text("Le serveur d'Alley n'est pas encore en ligne."))
                    .child(h("p.small").text(
                        "Cette page est publiée sur GitHub Pages ; le serveur qui relaie les salons n'y est pas encore relié. Si quelqu'un vous a donné l'adresse d'un serveur Alley, indiquez-la :",
                    ))
                    .child(server_in.clone())
            }))
            .child(h("button.primary").attr("type", "submit").text(if invited { "Entrer dans le salon" } else { "Entrer" }))
            .child(h("p.small.muted").text("🔒 Chiffré de bout en bout · protocole Brocolis · serveur Paquets · 100 % Rust"))
            .on("submit", move |e| {
                e.prevent_default();
                let n: String = input_value(&n2).trim().chars().take(32).collect();
                if n.is_empty() {
                    return focus(&n2);
                }
                if needs_server() {
                    let s = input_value(&s2).trim().trim_end_matches('/').to_owned();
                    let valid = s.starts_with("https://") && s.len() > 8 && !s[8..].contains('/');
                    if !valid {
                        return toast("Il faut l'adresse https:// d'un serveur Alley.", true);
                    }
                    app(|a| a.server = s);
                }
                app(|a| a.name = n);
                spawn(app::enter());
            }),
    )
}

thread_local! {
    static SEARCH: RefCell<u32> = const { RefCell::new(0) };
}

fn lobby() -> El {
    let (name, query, private) = app(|a| (a.name.clone(), a.query.clone(), a.private));
    let q = h("input#q")
        .attr("type", "search")
        .attr("placeholder", "Chercher un salon…")
        .attr("maxlength", "40")
        .attr("autocomplete", "off")
        .attr("enterkeyhint", "go")
        .attr("aria-label", "Nom du salon")
        .el();
    set_input_value(&q, &query);
    let q2 = q.clone();
    listen(&q, "input", move |_| {
        app(|a| a.query = input_value(&q2));
        let generation = SEARCH.with(|s| {
            *s.borrow_mut() += 1;
            *s.borrow()
        });
        render_rooms();
        spawn(async move {
            sleep(250).await;
            if SEARCH.with(|s| *s.borrow()) == generation {
                load_rooms().await;
            }
        });
    });
    let private_box = h("input")
        .attr("type", "checkbox")
        .prop("checked", private)
        .el();
    let p2 = private_box.clone();
    listen(&private_box, "change", move |_| {
        let on = p2.unchecked_ref::<HtmlInputElement>().checked();
        app(|a| a.private = on);
    });
    let q3 = q.clone();
    h("div.lobby")
        .child(
            h("header.lobby-head")
                .child(
                    h("a.brand")
                        .attr("href", base_url("/"))
                        .child(h("img").attr("src", base_url("/assets/favicon.svg")).attr("alt", "").attr("width", "28").attr("height", "28"))
                        .child(h("strong").text("Alley")),
                )
                .child(h("span.spacer"))
                .child(
                    h("button.me-chip")
                        .attr("type", "button")
                        .attr("title", "Changer de pseudonyme")
                        .child(avatar("moi", &name, "tiny"))
                        .child(h("span").text(&name))
                        .on("click", |_| {
                            app(|a| a.name.clear());
                            render();
                        }),
                ),
        )
        .child(
            h("main.lobby-main")
                .child(h("h1").text("Salons ouverts"))
                .child(h("p.muted").text(
                    "Rien n'est enregistré. Un salon vit tant que quelqu'un y est, ne montre rien de ce qui s'est dit avant votre arrivée, et disparaît à minuit.",
                ))
                .child(
                    h("form.search")
                        .child(q)
                        .child(h("button.primary").attr("type", "submit").text("Entrer"))
                        .on("submit", move |e| {
                            e.prevent_default();
                            let name = input_value(&q3).trim().to_owned();
                            if name.is_empty() {
                                return focus(&q3);
                            }
                            let private = app(|a| a.private);
                            spawn(async move { create_room(&name, private).await });
                        }),
                )
                .child(h("label.check.small").child(private_box).child(h("span").text("Salon privé : invisible dans la liste, seulement par lien")))
                .child(h("div#rooms.rooms").attr("aria-live", "polite")),
        )
}

fn simple(s: &str) -> String {
    s.trim().to_lowercase()
}

pub fn render_rooms() {
    let Some(list) = by_id("rooms") else { return };
    let (rooms, query) = app(|a| (a.rooms.clone(), a.query.trim().to_owned()));
    scope("rooms", || {
        let mut items: Vec<Child> = Vec::new();
        let exact = rooms
            .iter()
            .any(|r| simple(r["name"].as_str().unwrap_or_default()) == simple(&query));
        if !query.is_empty() && !exact {
            let name = query.clone();
            items.push(
                h("button.room-card.create")
                    .attr("type", "button")
                    .child(h("span.room-name").text(format!("＋ Créer « {query} »")))
                    .child(h("span.room-meta").text("Un nouveau salon, jusqu'à minuit"))
                    .click(move || {
                        let (name, private) = (name.clone(), app(|a| a.private));
                        async move { create_room(&name, private).await }
                    })
                    .into(),
            );
        }
        for r in &rooms {
            let id = r["id"].as_str().unwrap_or_default().to_owned();
            let n = r["members"].as_u64().unwrap_or_default();
            let expires = r["expires_at"].as_f64().unwrap_or_default();
            items.push(
                h("button.room-card")
                    .attr("type", "button")
                    .child(
                        h("span.room-name")
                            .text(format!("# {}", r["name"].as_str().unwrap_or_default())),
                    )
                    .child(h("span.room-meta").text(format!(
                        "{} · disparaît à {}",
                        people_label(n),
                        until(expires)
                    )))
                    .click(move || {
                        let id = id.clone();
                        async move { join_room(&id).await }
                    })
                    .into(),
            );
        }
        if items.is_empty() {
            items.push(
                h("p.empty")
                    .text(if query.is_empty() {
                        "Aucun salon ouvert pour l'instant. Tapez un nom pour créer le premier !"
                    } else {
                        "Aucun salon de ce nom."
                    })
                    .into(),
            );
        }
        replace(&list, items);
    });
}

pub fn lobby_error(message: &str) {
    if let Some(list) = by_id("rooms") {
        replace(&list, vec![h("p.empty.error").text(message).into()]);
    }
}

pub fn narrow() -> bool {
    matches_media("(max-width: 860px)")
}

pub fn touch() -> bool {
    matches_media("(pointer: coarse)")
}

pub fn toggle_members(open: Option<bool>) {
    let open = open.unwrap_or_else(|| !app(|a| a.show_members));
    app(|a| a.show_members = open);
    if let Some(l) = query(".layout") {
        toggle_class(&l, "members-open", open);
    }
}

// ---------------------------------------------------------------------------
// Le salon

pub fn render_room() {
    let (Some(root), Some(room)) = (by_id("root"), room()) else {
        return;
    };
    let (name, private, fp, has_key) = {
        let r = room.borrow();
        (
            r.name.clone(),
            r.private,
            r.fingerprint.clone(),
            r.key.is_some(),
        )
    };
    let show = app(|a| a.show_members);
    scope("root", || {
        let head = h("header.chat-head")
            .child(icon_btn("Quitter le salon", "←").on("click", |_| leave_room(None)))
            .child(h("span.hash").text(if private { "🔒" } else { "#" }))
            .child(h("strong.title").text(&name))
            .child(h("span.topic#room-meta"))
            .child(h("span.spacer"))
            .child(
                h("span.lock")
                    .attr(
                        "title",
                        if has_key {
                            format!("Chiffré de bout en bout · empreinte {fp}")
                        } else {
                            "Clé du salon en route…".into()
                        },
                    )
                    .text("🔒"),
            )
            .child(icon_btn("Inviter", "🔗").click(|| async { invite().await }))
            .child(
                h("button.icon-btn#call-btn")
                    .attr("type", "button")
                    .attr("title", "Appel")
                    .text("📞")
                    .on("click", |_| {
                        if app(|a| a.call.is_some()) {
                            leave_call();
                        } else {
                            spawn(join_call());
                        }
                    }),
            )
            .child(icon_btn("Présents", "👥").on("click", |_| toggle_members(None)));
        let layout = h("div.layout.room-layout")
            .class_if("members-open", show)
            .child(
                h("main.chat#chat")
                    .child(head)
                    .child(h("div#call"))
                    .child(
                        h("div.messages#messages")
                            .attr("role", "log")
                            .attr("aria-live", "polite"),
                    )
                    .child(h("div.typing#typing").attr("aria-live", "polite"))
                    .child(composer()),
            )
            .child(h("aside.members#members").attr("aria-label", "Présents"))
            .child(h("div.scrim").on("click", |_| toggle_members(Some(false))));
        replace(&root, vec![layout.into()]);
    });
    render_people();
    render_call();
    render_messages(true);
    render_typing();
}

async fn invite() {
    let Some(room) = room() else { return };
    let (id, name) = {
        let r = room.borrow();
        (r.id.clone(), r.name.clone())
    };
    let link = format!(
        "{}{}/app/#salon={id}",
        window().location().origin().unwrap_or_default(),
        base()
    );
    let nav: JsValue = window().navigator().into();
    if touch() && has(&nav, "share") {
        let data = js_sys::Object::new();
        let _ = js_sys::Reflect::set(
            &data,
            &"title".into(),
            &format!("Viens dans #{name} sur Alley").into(),
        );
        let _ = js_sys::Reflect::set(&data, &"url".into(), &link.clone().into());
        if await_promise(call(&nav, "share", &[data.into()]))
            .await
            .is_ok()
        {
            return;
        }
    }
    copy(
        &link,
        "Lien du salon copié : il marche jusqu'à ce que le salon disparaisse.",
    )
    .await;
}

/// Les présents, le compte en tête de salon, l'état du bouton d'appel.
pub fn render_people() {
    let Some(room) = room() else { return };
    let r = room.borrow();
    let in_call = app(|a| a.call.is_some());
    let me_name = app(|a| a.name.clone());
    if let Some(meta) = by_id("room-meta") {
        meta.set_text_content(Some(&format!(
            "{} · disparaît à {}",
            people_label(r.present() as u64),
            until(r.expires_at)
        )));
    }
    if let Some(btn) = by_id("call-btn") {
        toggle_class(&btn, "on", in_call);
        let _ = btn.set_attribute(
            "title",
            if in_call {
                "Raccrocher"
            } else {
                "Rejoindre l'appel du salon"
            },
        );
    }
    let Some(el) = by_id("members") else { return };
    let mut people: Vec<(String, String, bool)> = vec![(r.me.clone(), me_name, in_call)];
    let mut others: Vec<(String, String, bool)> = r
        .peers
        .iter()
        .map(|(id, p)| (id.clone(), r.name_of(id), p.voice))
        .collect();
    others.sort_by_key(|p| p.1.to_lowercase());
    people.extend(others);
    let me = r.me.clone();
    let fp = r.fingerprint.clone();
    drop(r);
    scope("members", || {
        replace(
            &el,
            vec![
                h("header.members-head")
                    .child(h("strong").text("Présents"))
                    .child(icon_btn("Fermer", "✕").on("click", |_| toggle_members(Some(false))))
                    .into(),
                h("ul")
                    .children(people.iter().map(|(id, name, voice)| {
                        h("li.member.on")
                            .data("speaker", id)
                            .child(
                                h("span.avatar-wrap")
                                    .child(avatar(id, name, ""))
                                    .child(h("span.dot")),
                            )
                            .child(
                                h("span.name")
                                    .text(name)
                                    .child((*id == me).then(|| h("small").text(" (vous)")))
                                    .child(voice.then(|| h("small").text(" 🔊"))),
                            )
                    }))
                    .into(),
                h("div.fp")
                    .child(h("small").text("Empreinte de la clé du salon"))
                    .child(h("code").text(if fp.is_empty() { "…".into() } else { fp }))
                    .child(
                        h("small.muted").text(
                            "Identique chez tous les présents = personne ne s'est intercalé.",
                        ),
                    )
                    .into(),
            ],
        );
    });
}

pub fn render_messages(stick: bool) {
    let (Some(box_), Some(room)) = (by_id("messages"), room()) else {
        return;
    };
    let at_bottom = box_.scroll_height() - box_.scroll_top() - box_.client_height() < 80;
    let r = room.borrow();
    let (name, has_key) = (r.name.clone(), r.key.is_some());
    let msgs = r.msgs.clone();
    drop(r);
    scope("messages", || {
        let mut nodes: Vec<Child> = vec![
            h("div.channel-start")
                .child(h("div.big-hash").text("#"))
                .child(h("h2").text(format!("Bienvenue dans #{name}")))
                .child(h("p").text(
                    "Vous ne voyez que ce qui se dit à partir de maintenant : rien n'est gardé, ni avant, ni après. Tout est chiffré de bout en bout, et le salon disparaît à minuit.",
                ))
                .into(),
        ];
        if !has_key {
            nodes.push(
                h("div.loading")
                    .text("🔑 Réception de la clé du salon…")
                    .into(),
            );
        }
        let mut prev: Option<&Msg> = None;
        for m in &msgs {
            let grouped = prev
                .is_some_and(|p| p.from == m.from && m.t - p.t < 300_000.0 && m.reply.is_empty());
            nodes.push(message_node(m, &msgs, grouped).into());
            prev = Some(m);
        }
        replace(&box_, nodes);
    });
    if stick || at_bottom {
        box_.set_scroll_top(box_.scroll_height());
    }
}

fn message_node(m: &Msg, all: &[Msg], grouped: bool) -> El {
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
    let reply = (!m.reply.is_empty()).then(|| {
        let target = all.iter().find(|t| t.mid == m.reply);
        let what = match target {
            Some(t) if t.kind == "image" => "📷 Photo".to_owned(),
            Some(t) if t.kind == "audio" => "🎤 Vocal".to_owned(),
            Some(t) => t.text.chars().take(90).collect(),
            None => "un message d'avant votre arrivée".to_owned(),
        };
        let mid = m.reply.clone();
        h("div.reply-ref")
            .child(h("span.curve"))
            .child(target.map(|t| h("strong").text(format!("@{}", t.name))))
            .child(h("span").text(what))
            .on("click", move |_| scroll_to_message(&mid))
    });
    let (mid, text) = (m.mid.clone(), m.text.clone());
    let mid2 = mid.clone();
    let mid3 = mid.clone();
    let actions = h("div.actions")
        .child(icon_btn("Répondre", "↩").on("click", move |_| start_reply(&mid)))
        .child(
            (m.mine && m.kind == "text")
                .then(|| icon_btn("Modifier", "✎").on("click", move |_| start_edit(&mid2))),
        )
        .child((m.kind == "text").then(|| {
            icon_btn("Copier le texte", "⧉").click(move || {
                let t = text.clone();
                async move { copy(&t, "Copié !").await }
            })
        }))
        .child(m.mine.then(|| {
            icon_btn("Supprimer", "🗑")
                .class("danger")
                .on("click", move |_| {
                    if confirm("Supprimer ce message chez tout le monde ?") {
                        delete_message(&mid3);
                    }
                })
        }));
    let meta = (!grouped).then(|| {
        h("div.meta")
            .child(
                h("strong.author")
                    .style("--h", hue(&m.from).to_string())
                    .text(&m.name),
            )
            .child(h("time").text(time_label(m.t)))
    });
    let node = h("article.message")
        .class_if("grouped", grouped)
        .class_if("mine", m.mine)
        .id(&format!("m{}", m.mid))
        .child(reply)
        .child(if grouped {
            h("time.hover-time").text(time_label(m.t))
        } else {
            avatar(&m.from, &m.name, "")
        })
        .child(
            h("div.body")
                .child(meta)
                .child(content)
                .child(m.edited.then(|| h("small.edited").text("(modifié)"))),
        )
        .child(actions);
    if touch() {
        // Appui long : les actions du message.
        let el = node.0.clone();
        listen(&el.clone(), "contextmenu", move |e| {
            e.prevent_default();
            let _ = el.class_list().add_1("show-actions");
        });
    }
    node
}

fn scroll_to_message(mid: &str) {
    let Some(el) = by_id(&format!("m{mid}")) else {
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
    let (Some(el), Some(room)) = (by_id("typing"), room()) else {
        return;
    };
    let r = room.borrow();
    let names: Vec<String> = r
        .peers
        .iter()
        .filter(|(_, p)| p.typing_until > now())
        .map(|(id, _)| r.name_of(id))
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

// ---------------------------------------------------------------------------
// L'appel du salon

pub fn render_call() {
    let (Some(box_), Some(room)) = (by_id("call"), room()) else {
        return;
    };
    let active = app(|a| a.call.clone());
    let r = room.borrow();
    let ids = r.voice_members(active.is_some());
    let me = r.me.clone();
    if let Some(btn) = by_id("call-btn") {
        toggle_class(&btn, "on", active.is_some());
    }
    let Some(c) = active else {
        let others = ids.len();
        drop(r);
        scope("call", || {
            let content: Vec<Child> = if others == 0 {
                vec![]
            } else {
                vec![
                    h("div.call-invite")
                        .child(h("span").text(format!(
                            "🔊 {} en appel",
                            people_label(others as u64).replace("présent", "personne")
                        )))
                        .child(
                            h("button.primary")
                                .attr("type", "button")
                                .text("Rejoindre")
                                .click(|| async { join_call().await }),
                        )
                        .into(),
                ]
            };
            replace(&box_, content);
        });
        return;
    };
    let tiles: Vec<Child> = ids
        .iter()
        .map(|id| {
            let call_ = c.borrow();
            let mut video = None;
            if *id == me {
                if let Some(track) = call_.video_track() {
                    video = Some(self_video(
                        &track,
                        call_.camera.is_some() && call_.screen.is_none(),
                    ));
                }
            } else if r.peers.get(id).is_some_and(|p| p.video)
                && let Some(stream) = call::remote_video(&call_, id)
            {
                video = Some(video_for(id, &stream, false));
            }
            let link = if *id == me {
                None
            } else {
                call::link_state(&call_, id)
            };
            let muted = if *id == me {
                call_.muted
            } else {
                r.peers.get(id).is_some_and(|p| p.muted)
            };
            let name = if *id == me {
                app(|a| a.name.clone())
            } else {
                r.name_of(id)
            };
            let has_video = video.is_some();
            h("div.tile")
                .class_if("has-video", has_video)
                .data("speaker", id)
                .child(
                    video
                        .map(Child::from)
                        .unwrap_or_else(|| avatar(id, &name, "big").into()),
                )
                .child(
                    h("span.tile-name")
                        .text(&name)
                        .text(if muted { " 🔇" } else { "" })
                        .text(link.map(|l| format!(" · {l}")).unwrap_or_default()),
                )
                .into()
        })
        .collect();
    let count = ids.len();
    drop(r);
    let (muted, camera, screen) = {
        let c = c.borrow();
        (c.muted, c.camera.is_some(), c.screen.is_some())
    };
    scope("call", || {
        let (c1, c2, c3) = (c.clone(), c.clone(), c.clone());
        let controls = h("div.call-controls")
            .child(
                h("button")
                    .attr("type", "button")
                    .class_if("off", muted)
                    .text(if muted { "🔇" } else { "🎙️" })
                    .child(h("span").text(if muted { "Micro coupé" } else { "Micro" }))
                    .on("click", move |_| {
                        call::toggle_mute(&c1);
                        if let Some(room) = crate::model::room() {
                            hello(&room, None);
                        }
                    }),
            )
            .child(
                h("button")
                    .attr("type", "button")
                    .class_if("on", camera)
                    .text("📷")
                    .child(h("span").text(if camera { "Couper" } else { "Caméra" }))
                    .click(move || {
                        let c = c2.clone();
                        async move {
                            if call::toggle_camera(&c).await.is_err() {
                                toast("Caméra inaccessible.", true);
                            }
                            if let Some(room) = crate::model::room() {
                                hello(&room, None);
                            }
                        }
                    }),
            )
            .child(Call::can_share_screen().then(|| {
                h("button")
                    .attr("type", "button")
                    .class_if("on", screen)
                    .text("🖥️")
                    .child(h("span").text(if screen { "Arrêter" } else { "Écran" }))
                    .click(move || {
                        let c = c3.clone();
                        async move {
                            let _ = call::toggle_screen(&c).await;
                            if let Some(room) = crate::model::room() {
                                hello(&room, None);
                            }
                        }
                    })
            }))
            .child(
                h("button.hangup")
                    .attr("type", "button")
                    .text("📞")
                    .child(h("span").text("Raccrocher"))
                    .on("click", |_| leave_call()),
            );
        replace(
            &box_,
            vec![
                h("div.call-area")
                    .child(h("div.tiles").class_if("many", count > 4).children(tiles))
                    .child(controls)
                    .into(),
            ],
        );
    });
}

// ---------------------------------------------------------------------------
// Écrire

fn composer() -> El {
    let input = h("textarea#input")
        .attr("rows", "1")
        .attr("placeholder", "Message chiffré…")
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
        let (input, autosize) = (input.clone(), autosize.clone());
        Rc::new(move || {
            let text = input_value(&input).trim().to_owned();
            if text.is_empty() {
                return;
            }
            set_input_value(&input, "");
            autosize();
            match app(|a| a.editing.take()) {
                Some(mid) => edit_message(&mid, text),
                None => send_message("text", text, ""),
            }
            render_bar();
        })
    };
    {
        let (autosize, input2) = (autosize.clone(), input.clone());
        listen(&input, "input", move |_| {
            autosize();
            if !input_value(&input2).is_empty() {
                send_typing();
            }
        });
    }
    {
        let (submit, input2) = (submit.clone(), input.clone());
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
                let mine = room().and_then(|r| {
                    r.borrow()
                        .msgs
                        .iter()
                        .rev()
                        .find(|m| m.mine && m.kind == "text")
                        .map(|m| m.mid.clone())
                });
                if let Some(mid) = mine {
                    e.prevent_default();
                    start_edit(&mid);
                }
            }
        });
    }
    {
        let file2 = file.clone();
        listen(&file, "change", move |_| {
            let input: &HtmlInputElement = file2.unchecked_ref();
            let Some(f) = input.files().and_then(|l| l.get(0)) else {
                return;
            };
            input.set_value("");
            spawn(async move {
                toast("Chiffrement de la photo…", false);
                match read_image(&f).await {
                    Ok(jpeg) => send_message("image", jpeg, ""),
                    Err(e) => toast(e, true),
                }
                render_bar();
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
                icon_btn("Message vocal (chiffré)", "🎤")
                    .class("mic")
                    .id("mic")
                    .click(|| async { toggle_recording().await })
            }))
            .child(send_btn),
    )
}

/// Un message vocal : enregistré, puis chiffré comme une photo (2 min max).
async fn toggle_recording() {
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
        return toast("Micro inaccessible.", true);
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
        return toast("Enregistrement impossible.", true);
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
        let (rec2, stream) = (rec.clone(), Clone::clone(&stream));
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
            spawn(async move {
                let Some(url) = read_data_url(&blob).await else {
                    return;
                };
                let data = url
                    .split_once(',')
                    .map(|(_, b)| b.to_owned())
                    .unwrap_or_default();
                send_message("audio", data, &kind);
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
    let (editing, reply) = app(|a| (a.editing.clone(), a.reply_to.clone()));
    scope("bar", || {
        let close = || icon_btn("Annuler", "✕").on("click", |_| cancel_bars());
        let content: Vec<Child> = if editing.is_some() {
            vec![
                h("div.bar")
                    .child(h("span").text("✎ Modification du message"))
                    .child(close())
                    .into(),
            ]
        } else if let Some(mid) = reply {
            let who = room()
                .and_then(|r| {
                    r.borrow()
                        .msgs
                        .iter()
                        .find(|m| m.mid == mid)
                        .map(|m| m.name.clone())
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

fn start_reply(mid: &str) {
    app(|a| {
        a.editing = None;
        a.reply_to = Some(mid.to_owned());
    });
    render_bar();
    if let Some(i) = by_id("input") {
        focus(&i);
    }
}

fn start_edit(mid: &str) {
    let text = room()
        .and_then(|r| {
            r.borrow()
                .msgs
                .iter()
                .find(|m| m.mid == mid)
                .map(|m| m.text.clone())
        })
        .unwrap_or_default();
    app(|a| {
        a.reply_to = None;
        a.editing = Some(mid.to_owned());
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

fn field(label: &str, input: &Element, hint: Option<&str>) -> El {
    h("label.field")
        .child(h("span").text(label))
        .child(input.clone())
        .child(hint.map(|t| h("small.muted").text(t)))
}

// ---------------------------------------------------------------------------
// Outils d'affichage

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
