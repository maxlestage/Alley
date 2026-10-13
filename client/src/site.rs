//! La vitrine : reconnaît l'appareil pour mettre en avant la bonne façon
//! d'installer Alley, et propose le bouton « Installer » là où le navigateur
//! le permet (Chrome, Edge, Android, Windows, ChromeOS, Linux).

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::JsValue;

use crate::dom::*;

fn detect_os() -> Option<&'static str> {
    let nav = window().navigator();
    let ua = nav.user_agent().unwrap_or_default();
    let data = get(&nav, "userAgentData");
    let platform = get(&data, "platform")
        .as_string()
        .or_else(|| nav.platform().ok())
        .unwrap_or_default();
    let touch = get(&nav, "maxTouchPoints").as_f64().unwrap_or_default() > 1.0;
    // Les iPad récents se présentent comme des Mac : on les reconnaît au tactile.
    if ["iPhone", "iPad", "iPod"].iter().any(|d| ua.contains(d))
        || (platform.contains("Mac") && touch)
    {
        Some("ios")
    } else if ua.contains("Android") {
        Some("android")
    } else if ua.contains("CrOS") {
        Some("chromeos")
    } else if platform.contains("Win") {
        Some("windows")
    } else if platform.contains("Mac") {
        Some("macos")
    } else if platform.contains("Linux") {
        Some("linux")
    } else {
        None
    }
}

pub fn start() {
    let base = meta("alley-base");
    crate::app::register_worker(&base);
    let os = detect_os();
    if let Some(os) = os
        && let Some(card) = query(&format!(".platform[data-os=\"{os}\"]"))
    {
        let _ = card.class_list().add_1("current");
        if let Some(parent) = card.parent_element() {
            let _ = parent.prepend_with_node_1(&card);
        }
    }
    let (Some(hint), Some(button)) = (by_id("install-hint"), by_id("install")) else {
        return;
    };
    let standalone = matches_media("(display-mode: standalone)")
        || get(&window().navigator(), "standalone").as_bool() == Some(true);
    if standalone {
        hint.set_text_content(Some("Alley est déjà installé sur cet appareil ✨"));
    } else if os == Some("ios") {
        hint.set_text_content(Some(
            "Sur iPhone et iPad : ouvrez cette page dans Safari, touchez Partager ⎋ puis « Sur l'écran d'accueil ».",
        ));
    }
    let deferred: Rc<RefCell<Option<JsValue>>> = Rc::new(RefCell::new(None));
    {
        let (deferred, button, hint) = (deferred.clone(), button.clone(), hint.clone());
        listen_forever(&window(), "beforeinstallprompt", move |e| {
            e.prevent_default();
            *deferred.borrow_mut() = Some(e.into());
            let _ = button.remove_attribute("hidden");
            hint.set_text_content(Some("Un clic, et Alley s'ouvre dans sa propre fenêtre."));
        });
    }
    {
        let button2 = button.clone();
        listen_forever(&button, "click", move |_| {
            let Some(prompt) = deferred.borrow_mut().take() else {
                return;
            };
            let _ = call(&prompt, "prompt", &[]);
            let _ = button2.set_attribute("hidden", "");
        });
    }
    listen_forever(&window(), "appinstalled", move |_| {
        hint.set_text_content(Some(
            "Alley est installé ✨ Retrouvez-le avec vos autres applications.",
        ));
    });
}
