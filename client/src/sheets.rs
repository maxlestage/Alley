//! Les fenêtres de l'application : créer un salon, rejoindre par un lien,
//! les réglages (pseudonyme, thème, notifications, son, langue, installer),
//! les réglages d'un salon et le menu ⋯ d'un message. Et ce qu'elles
//! règlent : les préférences de l'appareil, gardées signées
//! ([`crate::vault`]), et l'installation de l'application.

use std::cell::RefCell;

use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{Element, HtmlDialogElement, HtmlInputElement};

use crate::app::{self, create_room, join_room, leave_room};
use crate::dom::*;
use crate::i18n::{self, fill, t};
use crate::model::*;
use crate::prefs::{self, Prefs, REACTIONS, Theme};
use crate::ui::{self, avatar, contact_email, field, icon_btn, lang_picker, site_link};
use crate::{recent, vault};

const PREFS_KEY: &str = "alley.prefs.v1";

thread_local! {
    static PREFS: RefCell<Prefs> = RefCell::new(Prefs::default());
    /// L'invitation à installer que le navigateur a proposée (Chrome, Edge,
    /// Android), gardée pour le bouton « Installer ».
    static INSTALL: RefCell<Option<JsValue>> = const { RefCell::new(None) };
}

// ---------------------------------------------------------------------------
// Les préférences

/// Relit les réglages gardés (s'ils sont intacts) et applique le thème.
pub async fn load_prefs() {
    let raw = vault::read(PREFS_KEY).await.unwrap_or_default();
    PREFS.with(|p| *p.borrow_mut() = Prefs::parse(&raw));
    apply_theme();
}

pub fn prefs() -> Prefs {
    PREFS.with(|p| p.borrow().clone())
}

/// Change un réglage, et l'enregistre signé.
fn set_prefs(f: impl FnOnce(&mut Prefs)) {
    let raw = PREFS.with(|p| {
        let mut p = p.borrow_mut();
        f(&mut p);
        serde_json::to_string(&*p).unwrap_or_default()
    });
    spawn(async move { vault::write(PREFS_KEY, &raw).await });
}

/// `<html data-theme>` : clair, sombre, ou celui de l'appareil.
pub fn apply_theme() {
    let Some(root) = document().document_element() else {
        return;
    };
    match prefs().theme.attr() {
        Some(theme) => {
            let _ = root.set_attribute("data-theme", theme);
        }
        None => {
            let _ = root.remove_attribute("data-theme");
        }
    }
}

pub fn is_muted(room: &str) -> bool {
    PREFS.with(|p| p.borrow().is_muted(room))
}

/// Un son bref et discret (aucun fichier : un oscillateur).
pub fn ding() {
    let Ok(ctx) = web_sys::AudioContext::new() else {
        return;
    };
    let ctx: JsValue = ctx.into();
    let (Ok(osc), Ok(gain)) = (
        call(&ctx, "createOscillator", &[]),
        call(&ctx, "createGain", &[]),
    ) else {
        return;
    };
    let now = get(&ctx, "currentTime").as_f64().unwrap_or_default();
    let freq = get(&osc, "frequency");
    let _ = call(&freq, "setValueAtTime", &[880.0.into(), now.into()]);
    let _ = call(
        &freq,
        "exponentialRampToValueAtTime",
        &[660.0.into(), (now + 0.15).into()],
    );
    let level = get(&gain, "gain");
    let _ = call(&level, "setValueAtTime", &[0.08.into(), now.into()]);
    let _ = call(
        &level,
        "exponentialRampToValueAtTime",
        &[0.0001.into(), (now + 0.3).into()],
    );
    let _ = call(&osc, "connect", std::slice::from_ref(&gain));
    let _ = call(&gain, "connect", &[get(&ctx, "destination")]);
    let _ = call(&osc, "start", &[now.into()]);
    let _ = call(&osc, "stop", &[(now + 0.32).into()]);
    // Le contexte se ferme une fois le son joué.
    spawn(async move {
        sleep(600).await;
        let _ = call(&ctx, "close", &[]);
    });
}

// ---------------------------------------------------------------------------
// L'installation

/// Alley tourne-t-il déjà comme une application installée ?
pub fn standalone() -> bool {
    matches_media("(display-mode: standalone)")
        || get(&window().navigator(), "standalone").as_bool() == Some(true)
}

fn ios() -> bool {
    let ua = window().navigator().user_agent().unwrap_or_default();
    ua.contains("iPhone") || ua.contains("iPad") || ua.contains("iPod")
}

/// Le bouton « Installer » a-t-il quelque chose à faire ici ?
pub fn can_install() -> bool {
    !standalone() && (ios() || INSTALL.with(|i| i.borrow().is_some()))
}

/// Retient l'invitation du navigateur à installer Alley, et montre les
/// boutons « Installer » quand elle arrive.
pub fn watch_install() {
    listen_forever(&window(), "beforeinstallprompt", |e| {
        e.prevent_default();
        INSTALL.with(|i| *i.borrow_mut() = Some(e.into()));
        for b in query_all(".install-btn") {
            let _ = b.remove_attribute("hidden");
        }
    });
    listen_forever(&window(), "appinstalled", |_| {
        INSTALL.with(|i| i.borrow_mut().take());
        for b in query_all(".install-btn") {
            let _ = b.set_attribute("hidden", "");
        }
        toast(t().installed_now, false);
    });
}

pub fn install() {
    if standalone() {
        return toast(t().installed_already, false);
    }
    match INSTALL.with(|i| i.borrow_mut().take()) {
        Some(prompt) => {
            let _ = call(&prompt, "prompt", &[]);
        }
        None if ios() => toast(t().install_ios, false),
        None => toast(t().install_browser, false),
    }
}

/// Un bouton « Installer l'app », caché s'il n'y a rien à installer.
pub fn install_button(spec: &str) -> El {
    h(spec)
        .class("install-btn")
        .attr("type", "button")
        .attr_if("hidden", !can_install())
        .child(h("span").attr("aria-hidden", "true").text("⬇"))
        .child(h("span").text(t().install_app))
        .on("click", |_| install())
}

trait AttrIf {
    fn attr_if(self, k: &str, on: bool) -> Self;
}

impl AttrIf for El {
    fn attr_if(self, k: &str, on: bool) -> Self {
        if on { self.attr(k, "") } else { self }
    }
}

// ---------------------------------------------------------------------------
// Les fenêtres

/// Une fenêtre (un `<dialog>` modal) : un titre, un contenu, « Fermer ».
/// Elle se ferme d'un clic à côté ou avec Échap, et disparaît fermée.
fn sheet(name: &str, title: &str, body: Vec<Child>) -> HtmlDialogElement {
    let d: HtmlDialogElement = h("dialog.sheet").data("sheet", name).el().unchecked_into();
    scope(&format!("sheet-{name}"), || {
        let d1 = d.clone();
        let content = h("div.sheet-box")
            .child(
                h("header.sheet-head").child(h("h2").text(title)).child(
                    icon_btn(t().close, "✕")
                        .class("close")
                        .on("click", move |_| d1.close()),
                ),
            )
            .child(h("div.sheet-body").children(body));
        let _ = d.append_child(&content.el());
        // Un clic sur le fond (hors de la boîte) ferme la fenêtre.
        let d2 = d.clone();
        listen(&d, "click", move |e| {
            if e.target()
                .and_then(|t| t.dyn_into::<Element>().ok())
                .is_some_and(|t| t.is_same_node(Some(&d2)))
            {
                d2.close();
            }
        });
        let d3 = d.clone();
        listen(&d, "close", move |_| d3.remove());
    });
    // Une seule à la fois.
    for open in query_all("dialog.sheet") {
        open.remove();
    }
    if let Some(body) = document().body() {
        let _ = body.append_child(&d);
    }
    let _ = d.show_modal();
    d
}

fn close_sheets() {
    for open in query_all("dialog.sheet") {
        if let Ok(d) = open.dyn_into::<HtmlDialogElement>() {
            d.close();
        }
    }
}

fn checkbox(checked: bool) -> Element {
    h("input")
        .attr("type", "checkbox")
        .attr("role", "switch")
        .prop("checked", checked)
        .el()
}

fn checked(el: &Element) -> bool {
    el.unchecked_ref::<HtmlInputElement>().checked()
}

/// Une ligne de réglage : un intitulé, une explication, un interrupteur.
fn switch_row(label: &str, hint: &str, input: Element) -> El {
    h("label.switch-row")
        .child(
            h("span.switch-text")
                .child(h("strong").text(label))
                .child(h("small").text(hint)),
        )
        .child(input)
}

fn section(title: &str) -> El {
    h("section.sheet-section").child(h("h3").text(title))
}

/// Créer un salon.
pub fn create_sheet() {
    let t = t();
    let input = h("input#new-room-name")
        .attr("type", "text")
        .attr("maxlength", "40")
        .attr("autocomplete", "off")
        .attr("enterkeyhint", "go")
        .attr("placeholder", t.new_room_placeholder)
        .attr("required", "")
        .el();
    let private = checkbox(app(|a| a.private));
    let (i2, p2) = (input.clone(), private.clone());
    let form = h("form.sheet-form")
        .child(field(t.room_name, &input, Some(t.new_room_hint)))
        .child(switch_row(t.private_short, t.private_room, private))
        .child(h("p.muted.small").text(t.new_room_paid_hint))
        .child(
            h("button.primary.wide")
                .attr("type", "submit")
                .text(t.new_room),
        )
        .on("submit", move |e| {
            e.prevent_default();
            let name = input_value(&i2).trim().to_owned();
            if name.is_empty() {
                return focus(&i2);
            }
            let private = checked(&p2);
            app(|a| a.private = private);
            close_sheets();
            spawn(async move { create_room(&name, private).await });
        });
    sheet("create", t.new_room, vec![form.into()]);
    focus(&input);
}

/// Rejoindre un salon par son lien d'invitation.
pub fn join_sheet() {
    let t = t();
    let input = h("input#join-link")
        .attr("type", "text")
        .attr("autocomplete", "off")
        .attr("autocapitalize", "off")
        .attr("spellcheck", "false")
        .attr("enterkeyhint", "go")
        .attr("placeholder", t.join_placeholder)
        .attr("required", "")
        .el();
    let i2 = input.clone();
    let form = h("form.sheet-form")
        .child(field(t.join_label, &input, Some(t.join_hint)))
        .child(
            h("button.primary.wide")
                .attr("type", "submit")
                .text(t.join_room),
        )
        .on("submit", move |e| {
            e.prevent_default();
            match prefs::room_from_link(&input_value(&i2)) {
                Some(id) => {
                    close_sheets();
                    spawn(async move { join_room(&id).await });
                }
                None => {
                    toast(i18n::t().join_invalid, true);
                    focus(&i2);
                }
            }
        });
    sheet("join", t.join_room, vec![form.into()]);
    // Un lien déjà copié : il est proposé tout de suite.
    let i3 = input.clone();
    spawn(async move {
        let clip = get(&window().navigator(), "clipboard");
        if clip.is_undefined() || !has(&clip, "readText") {
            focus(&i3);
            return;
        }
        focus(&i3);
        if let Ok(text) = await_promise(call(&clip, "readText", &[])).await
            && let Some(text) = text.as_string()
            && prefs::room_from_link(&text).is_some()
            && input_value(&i3).is_empty()
        {
            set_input_value(&i3, text.trim());
        }
    });
}

/// Les réglages : pseudonyme et compte, affichage, notifications,
/// application, aide.
pub fn settings_sheet() {
    let t = t();
    let p = prefs();
    let (name, signed_in) = app(|a| (a.name.clone(), a.account.is_some()));

    let account = section(t.settings_account)
        .child(
            h("div.me-row").child(avatar("moi", &name, "")).child(
                h("div")
                    .child(h("strong").text(&name))
                    .child(h("small.muted").text(if signed_in {
                        t.signed_in_note
                    } else {
                        t.guest_note
                    })),
            ),
        )
        .child(
            h("div.sheet-actions")
                .child(
                    h("button.secondary#change-name")
                        .attr("type", "button")
                        .text(t.change_pseudonym)
                        .on("click", |_| {
                            close_sheets();
                            app::logout();
                        }),
                )
                .child(signed_in.then(|| {
                    h("button.secondary.danger#logout")
                        .attr("type", "button")
                        .text(t.logout)
                        .on("click", |_| {
                            close_sheets();
                            app::logout();
                        })
                })),
        );

    let themes = h("div.segmented")
        .attr("role", "radiogroup")
        .attr("aria-label", t.theme)
        .children(Theme::ALL.map(|theme| {
            h("button")
                .attr("type", "button")
                .attr("role", "radio")
                .attr("aria-checked", (p.theme == theme).to_string())
                .data("theme", theme.attr().unwrap_or("auto"))
                .text(match theme {
                    Theme::Auto => t.theme_auto,
                    Theme::Light => t.theme_light,
                    Theme::Dark => t.theme_dark,
                })
                .on("click", move |_| {
                    set_prefs(|p| p.theme = theme);
                    apply_theme();
                    for b in query_all(".segmented [data-theme]") {
                        let on = b.get_attribute("data-theme").as_deref()
                            == Some(theme.attr().unwrap_or("auto"));
                        let _ = b.set_attribute("aria-checked", &on.to_string());
                    }
                })
        }));
    let display = section(t.settings_display)
        .child(
            h("div.setting")
                .child(h("span").text(t.theme))
                .child(themes),
        )
        .child(
            h("div.setting")
                .child(h("span").text(t.language))
                .child(lang_picker()),
        );

    let notify = checkbox(p.notify && notifications_granted());
    let n2 = notify.clone();
    listen(&notify, "change", move |_| {
        let on = checked(&n2);
        set_prefs(|p| p.notify = on);
        if on && !notifications_granted() {
            let n3 = n2.clone();
            spawn(async move {
                let granted = match web_sys::Notification::request_permission() {
                    Ok(p) => {
                        await_promise(Ok(p.into()))
                            .await
                            .ok()
                            .and_then(|v| v.as_string())
                            .as_deref()
                            == Some("granted")
                    }
                    Err(_) => false,
                };
                if !granted {
                    n3.unchecked_ref::<HtmlInputElement>().set_checked(false);
                    set_prefs(|p| p.notify = false);
                    toast(i18n::t().notify_denied, true);
                }
            });
        }
    });
    let sound = checkbox(p.sound);
    let s2 = sound.clone();
    listen(&sound, "change", move |_| {
        let on = checked(&s2);
        set_prefs(|p| p.sound = on);
        if on {
            ding();
        }
    });
    let alerts = section(t.settings_alerts)
        .child(switch_row(t.notify_label, t.notify_hint, notify))
        .child(switch_row(t.sound_label, t.sound_hint, sound));

    let device = section(t.settings_device)
        .child(
            h("div.sheet-actions")
                .child(install_button("button.secondary"))
                .child(
                    h("button.secondary.danger")
                        .attr("type", "button")
                        .text(t.forget_all)
                        .on("click", |_| {
                            if confirm(i18n::t().confirm_forget_all) {
                                recent::forget_all();
                                app(|a| a.known.clear());
                                toast(i18n::t().forgotten_all, false);
                                ui::render_mine();
                            }
                        }),
                ),
        )
        .child(h("p.muted.small").text(t.device_note));

    let email = contact_email();
    let help = section(t.settings_help).child(
        h("nav.help-links")
            .child(site_link("faq", t.help_link))
            .child(site_link("rules", t.rules_title))
            .child(site_link("terms", t.terms_title))
            .child(site_link("privacy", t.privacy_link))
            .child(site_link("legal", t.legal_link))
            .child((!email.is_empty()).then(|| {
                h("a")
                    .attr("href", format!("mailto:{email}"))
                    .text(t.contact_link)
            })),
    );

    sheet(
        "settings",
        t.settings,
        vec![
            account.into(),
            display.into(),
            alerts.into(),
            device.into(),
            help.into(),
        ],
    );
}

fn notifications_granted() -> bool {
    has(&window(), "Notification")
        && web_sys::Notification::permission() == web_sys::NotificationPermission::Granted
}

/// Le lien d'invitation du salon où l'on est.
pub fn room_link(id: &str) -> String {
    format!(
        "{}{}#salon={id}",
        window().location().origin().unwrap_or_default(),
        app::app_path(i18n::lang())
    )
}

/// Un e-mail de signalement à l'éditeur (le salon, l'heure, et le message
/// signalé s'il y en a un — ce que l'on choisit d'envoyer).
pub fn report_href(room_name: &str, message: Option<(&str, &str)>) -> Option<String> {
    let email = contact_email();
    if email.is_empty() {
        return None;
    }
    let t = t();
    let id = room().map(|r| r.borrow().id.clone()).unwrap_or_default();
    let when = String::from(js_sys::Date::new_0().to_locale_string("default", &JsValue::UNDEFINED));
    let subject = fill(t.report_subject, &[room_name]);
    let mut body = fill(t.report_body, &[room_name, &id, &when]);
    if let Some((author, text)) = message {
        let quoted: String = text.chars().take(500).collect();
        body = fill(t.report_message_body, &[author, &quoted]) + &body;
    }
    let enc = |s: &str| String::from(js_sys::encode_uri_component(s));
    Some(format!(
        "mailto:{email}?subject={}&body={}",
        enc(&subject),
        enc(&body)
    ))
}

/// Les réglages du salon : inviter, sourdine, renommer (l'administrateur),
/// signaler, quitter.
pub fn room_sheet() {
    let Some(room) = room() else { return };
    let t = t();
    let (id, name, private, admin, expires_at, fp) = {
        let r = room.borrow();
        (
            r.id.clone(),
            r.name.clone(),
            r.private,
            r.i_am_admin(),
            r.expires_at,
            r.fingerprint.clone(),
        )
    };
    let link = room_link(&id);
    let link_input = h("input.link-input")
        .attr("type", "text")
        .attr("readonly", "")
        .attr("aria-label", t.invite)
        .el();
    set_input_value(&link_input, &link);
    let invite = section(t.room_invite)
        .child(
            h("div.copy-row").child(link_input).child(
                h("button.primary")
                    .attr("type", "button")
                    .text(t.copy_link)
                    .click(|| async { ui::invite().await }),
            ),
        )
        .child(h("p.muted.small").text(if private {
            t.room_private_note
        } else {
            t.room_public_note
        }));

    let mute = checkbox(is_muted(&id));
    let (m2, id2) = (mute.clone(), id.clone());
    listen(&mute, "change", move |_| {
        let on = checked(&m2);
        set_prefs(|p| p.set_muted(&id2, on));
        toast(
            if on {
                i18n::t().muted_now
            } else {
                i18n::t().unmuted_now
            },
            false,
        );
    });
    let alerts = section(t.settings_alerts).child(switch_row(t.mute_label, t.mute_hint, mute));

    let rename = admin.then(|| {
        let input = h("input#rename-input")
            .attr("type", "text")
            .attr("maxlength", "40")
            .attr("autocomplete", "off")
            .attr("required", "")
            .el();
        set_input_value(&input, &name);
        let i2 = input.clone();
        section(t.rename_title).child(
            h("form.copy-row")
                .child(input)
                .child(h("button.primary").attr("type", "submit").text(t.rename))
                .on("submit", move |e| {
                    e.prevent_default();
                    let name = input_value(&i2).trim().to_owned();
                    if name.is_empty() {
                        return focus(&i2);
                    }
                    if app::rename_room(&name) {
                        close_sheets();
                    }
                }),
        )
    });

    let about = section(t.room_about).child(
        h("dl.facts")
            .child(h("dt").text(t.room_name))
            .child(h("dd").text(&name))
            .child(h("dt").text(t.room_closes))
            .child(h("dd").text(closes(expires_at)))
            .child(h("dt").text(t.room_key))
            .child(h("dd.mono").text(if fp.is_empty() {
                t.key_on_its_way.to_owned()
            } else {
                fp
            }))
            .child(h("dt").text(t.role))
            .child(h("dd").text(if admin { t.role_admin } else { t.role_member })),
    );

    let report = report_href(&name, None).map(|href| {
        h("a.secondary.button-like.report")
            .attr("href", href)
            .text(format!("⚑ {}", t.report))
    });
    let actions = h("div.sheet-actions").child(report).child(
        h("button.secondary.danger#leave-room")
            .attr("type", "button")
            .text(t.leave_room)
            .on("click", |_| {
                close_sheets();
                leave_room(None);
            }),
    );

    sheet(
        "room",
        &fill(t.room_settings_title, &[&name]),
        vec![
            invite.into(),
            alerts.into(),
            rename.into(),
            about.into(),
            actions.into(),
        ],
    );
}

fn closes(expires_at: f64) -> String {
    let date = js_sys::Date::new(&JsValue::from(expires_at));
    let options = js_sys::Object::new();
    for (k, v) in [
        ("weekday", "long"),
        ("day", "numeric"),
        ("month", "long"),
        ("hour", "2-digit"),
        ("minute", "2-digit"),
    ] {
        let _ = js_sys::Reflect::set(&options, &k.into(), &v.into());
    }
    String::from(date.to_locale_string(i18n::lang().code(), &options))
}

/// Le menu ⋯ d'un message : réagir, répondre, copier, modifier, supprimer,
/// signaler.
pub fn message_menu(mid: &str) {
    let Some(room) = room() else { return };
    let t = t();
    let (msg, admin, me, room_name) = {
        let r = room.borrow();
        (
            r.msgs.iter().find(|m| m.mid == mid).cloned(),
            r.i_am_admin(),
            r.me.clone(),
            r.name.clone(),
        )
    };
    let Some(m) = msg else { return };
    let reactions = h("div.react-row")
        .attr("role", "group")
        .attr("aria-label", t.react)
        .children(REACTIONS.map(|e| {
            let mine = m.reactions.get(e).is_some_and(|p| p.contains(&me));
            let mid = m.mid.clone();
            h("button.react-pick")
                .attr("type", "button")
                .attr("aria-pressed", mine.to_string())
                .attr("title", e)
                .text(e)
                .on("click", move |_| {
                    close_sheets();
                    app::react(&mid, e);
                })
        }));
    let item = |icon: &str, label: &str| {
        h("button.menu-item")
            .attr("type", "button")
            .child(h("span.menu-icon").attr("aria-hidden", "true").text(icon))
            .child(h("span").text(label))
    };
    let text = m.kind == "text";
    let mut items: Vec<Child> = vec![reactions.into()];
    let mid1 = m.mid.clone();
    items.push(
        item("↩", t.reply)
            .on("click", move |_| {
                close_sheets();
                ui::start_reply(&mid1);
            })
            .into(),
    );
    if text {
        let body = m.text.clone();
        items.push(
            item("⧉", t.copy_text)
                .click(move || {
                    close_sheets();
                    let body = body.clone();
                    async move { copy(&body, i18n::t().copied).await }
                })
                .into(),
        );
    }
    if m.mine && text {
        let mid2 = m.mid.clone();
        items.push(
            item("✎", t.edit)
                .on("click", move |_| {
                    close_sheets();
                    ui::start_edit(&mid2);
                })
                .into(),
        );
    }
    if m.mine || admin {
        let mid3 = m.mid.clone();
        items.push(
            item("🗑", t.delete)
                .class("danger")
                .on("click", move |_| {
                    if confirm(i18n::t().confirm_delete) {
                        close_sheets();
                        app::delete_message(&mid3);
                    }
                })
                .into(),
        );
    }
    if !m.mine {
        let excerpt = match m.kind.as_str() {
            "image" => t.photo_label.to_owned(),
            "audio" => t.voice_label.to_owned(),
            _ => m.text.clone(),
        };
        if let Some(href) = report_href(&room_name, Some((&m.name, &excerpt))) {
            items.push(
                h("a.menu-item.danger")
                    .attr("href", href)
                    .child(h("span.menu-icon").attr("aria-hidden", "true").text("⚑"))
                    .child(h("span").text(t.report_message))
                    .on("click", |_| close_sheets())
                    .into(),
            );
        }
    }
    let title = fill(t.message_of, &[&m.name]);
    let d = sheet(
        "message",
        &title,
        vec![h("div.menu").children(items).into()],
    );
    let _ = d.class_list().add_1("menu-sheet");
}

/// Le sélecteur d'emoji du champ de saisie : un emoji choisi s'insère au
/// curseur.
pub fn emoji_picker(input: &Element) -> Element {
    let pop = h("div.emoji-pop")
        .attr("role", "dialog")
        .attr("aria-label", t().emoji)
        .el();
    for e in prefs::EMOJIS {
        let (input, pop2) = (input.clone(), pop.clone());
        let b = h("button.emoji")
            .attr("type", "button")
            .text(e)
            .on("click", move |_| {
                ui::insert_at_caret(&input, e);
                let _ = pop2.set_attribute("hidden", "");
            });
        let _ = pop.append_child(&b.el());
    }
    let _ = pop.set_attribute("hidden", "");
    // Un clic ailleurs le referme (un seul écouteur, pour toute la page).
    thread_local! {
        static WATCHING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    if !WATCHING.with(|w| w.replace(true)) {
        listen_forever(&document(), "pointerdown", |e| {
            let inside = e
                .target()
                .and_then(|t| t.dyn_into::<Element>().ok())
                .is_some_and(|t| t.closest(".emoji-pop, .emoji-btn").ok().flatten().is_some());
            if !inside {
                for pop in query_all(".emoji-pop") {
                    let _ = pop.set_attribute("hidden", "");
                }
            }
        });
    }
    pop
}
