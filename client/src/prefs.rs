//! Les réglages de l'appareil (thème, notifications, son, salons en
//! sourdine), gardés signés comme le reste ([`crate::vault`]) ; et ce qu'il
//! faut pour lire un lien de salon et compter les réactions. Rien ici ne
//! touche au navigateur : tout est testé hors de lui.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Les réactions possibles à un message.
pub const REACTIONS: [&str; 6] = ["👍", "❤️", "😂", "😮", "😢", "🙏"];

/// Le sélecteur d'emoji du champ de saisie.
pub const EMOJIS: [&str; 48] = [
    "😀", "😂", "🥲", "😊", "😍", "🥰", "😘", "😉", "😎", "🤩", "🤔", "🤨", "😐", "🙄", "😴", "😮",
    "😢", "😭", "😡", "🤯", "🥳", "😇", "🤗", "🤫", "👍", "👎", "👏", "🙌", "🙏", "💪", "👋", "🤝",
    "❤️", "🧡", "💛", "💚", "💙", "💜", "🔥", "✨", "🎉", "🎂", "☕", "🍕", "⚽", "🎵", "✅", "❌",
];

/// Le thème de l'écran.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    /// Celui de l'appareil.
    #[default]
    Auto,
    Light,
    Dark,
}

impl Theme {
    pub const ALL: [Theme; 3] = [Theme::Auto, Theme::Light, Theme::Dark];

    /// La valeur de `<html data-theme>` (aucune : celui de l'appareil).
    pub fn attr(self) -> Option<&'static str> {
        match self {
            Theme::Auto => None,
            Theme::Light => Some("light"),
            Theme::Dark => Some("dark"),
        }
    }
}

fn yes() -> bool {
    true
}

/// Les réglages de cet appareil.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Prefs {
    #[serde(default)]
    pub theme: Theme,
    /// Prévenir d'un message quand Alley est en arrière-plan.
    #[serde(default = "yes")]
    pub notify: bool,
    /// Un son discret à chaque message reçu en arrière-plan.
    #[serde(default = "yes")]
    pub sound: bool,
    /// Les salons mis en sourdine (ni notification ni son).
    #[serde(default)]
    pub muted: Vec<String>,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            theme: Theme::Auto,
            notify: true,
            sound: true,
            muted: Vec::new(),
        }
    }
}

/// Au plus, des salons en sourdine retenus (les plus récents).
const MAX_MUTED: usize = 50;

impl Prefs {
    /// Des réglages abîmés se lisent comme ceux par défaut.
    pub fn parse(raw: &str) -> Prefs {
        serde_json::from_str(raw).unwrap_or_default()
    }

    pub fn is_muted(&self, room: &str) -> bool {
        self.muted.iter().any(|m| m == room)
    }

    pub fn set_muted(&mut self, room: &str, on: bool) {
        self.muted.retain(|m| m != room);
        if on {
            self.muted.push(room.to_owned());
            if self.muted.len() > MAX_MUTED {
                self.muted.remove(0);
            }
        }
    }
}

/// L'identifiant de salon d'un lien d'invitation (`…#salon=<id>`), ou d'un
/// identifiant collé tel quel ; `None` si ce n'en est pas un.
pub fn room_from_link(text: &str) -> Option<String> {
    let text = text.trim();
    let id = match text.rfind("salon=") {
        Some(i) => &text[i + "salon=".len()..],
        None => text,
    };
    let id: String = id
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    (matches!(id.len(), 12 | 32) && id.bytes().all(|b| b.is_ascii_hexdigit())).then_some(id)
}

/// Les réactions à un message : pour chaque emoji, qui l'a mis.
pub type Reactions = BTreeMap<String, Vec<String>>;

/// `who` met (ou retire) `emoji`. Faux si l'emoji n'est pas permis.
pub fn react(reactions: &mut Reactions, emoji: &str, who: &str, on: bool) -> bool {
    if !REACTIONS.contains(&emoji) {
        return false;
    }
    let people = reactions.entry(emoji.to_owned()).or_default();
    people.retain(|p| p != who);
    if on {
        people.push(who.to_owned());
    }
    if people.is_empty() {
        reactions.remove(emoji);
    }
    true
}

/// Les réactions dans l'ordre du sélecteur, avec leur nombre.
pub fn counted(reactions: &Reactions) -> Vec<(&'static str, &[String])> {
    REACTIONS
        .iter()
        .filter_map(|e| reactions.get(*e).map(|p| (*e, p.as_slice())))
        .filter(|(_, p)| !p.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_room_is_found_in_a_link_or_alone() {
        let id = "0123456789abcdef0123456789abcdef";
        assert_eq!(
            room_from_link(&format!("https://alley.example/en#salon={id}")),
            Some(id.into())
        );
        assert_eq!(
            room_from_link("  A1B2C3D4E5F6 "),
            Some("a1b2c3d4e5f6".into())
        );
        assert_eq!(
            room_from_link("#salon=a1b2c3d4e5f6"),
            Some("a1b2c3d4e5f6".into())
        );
        assert_eq!(room_from_link("bonjour"), None);
        assert_eq!(room_from_link("salon=a1b2"), None);
        assert_eq!(room_from_link("salon=zzzzzzzzzzzz"), None);
        assert_eq!(room_from_link(""), None);
    }

    #[test]
    fn prefs_default_and_survive_damage() {
        assert_eq!(Prefs::parse("n'importe quoi"), Prefs::default());
        let p = Prefs::parse(r#"{"theme":"dark","sound":false}"#);
        assert_eq!(p.theme, Theme::Dark);
        assert!(p.notify && !p.sound);
        assert_eq!(Theme::Dark.attr(), Some("dark"));
        assert_eq!(Theme::Auto.attr(), None);
        let raw = serde_json::to_string(&p).unwrap();
        assert_eq!(Prefs::parse(&raw), p);
    }

    #[test]
    fn rooms_are_muted_and_unmuted() {
        let mut p = Prefs::default();
        p.set_muted("aa", true);
        p.set_muted("aa", true);
        assert_eq!(p.muted, ["aa"]);
        assert!(p.is_muted("aa") && !p.is_muted("bb"));
        p.set_muted("aa", false);
        assert!(!p.is_muted("aa"));
        for i in 0..60 {
            p.set_muted(&i.to_string(), true);
        }
        assert_eq!(p.muted.len(), MAX_MUTED);
        assert!(p.is_muted("59") && !p.is_muted("0"));
    }

    #[test]
    fn reactions_toggle_and_count_in_order() {
        let mut r = Reactions::new();
        assert!(react(&mut r, "😂", "ada", true));
        assert!(react(&mut r, "👍", "bob", true));
        assert!(react(&mut r, "👍", "ada", true));
        assert!(react(&mut r, "👍", "ada", true));
        assert!(!react(&mut r, "💩", "ada", true));
        let c = counted(&r);
        assert_eq!(
            c.iter().map(|(e, p)| (*e, p.len())).collect::<Vec<_>>(),
            [("👍", 2), ("😂", 1)]
        );
        react(&mut r, "😂", "ada", false);
        assert!(!r.contains_key("😂"));
    }
}
