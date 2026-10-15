//! Les styles d'Alley, écrits en Rust.
//!
//! Chaque feuille est construite par du code : des règles (un sélecteur et ses
//! déclarations), des blocs `@media` et des animations `@keyframes`. Le CSS
//! que lit le navigateur en est le rendu, produit une fois au démarrage et
//! servi tel quel (ou écrit par `alley export`).

mod app;
mod site;

use std::fmt::Write;

pub use app::app;
pub use site::site;

/// Une feuille de style en construction.
#[derive(Default)]
pub struct Sheet {
    css: String,
}

impl Sheet {
    /// Une règle : `selecteur { propriété: valeur; … }`.
    pub fn rule(&mut self, selector: &str, declarations: &[(&str, &str)]) -> &mut Self {
        self.css.push_str(selector);
        self.css.push('{');
        for (i, (property, value)) in declarations.iter().enumerate() {
            if i > 0 {
                self.css.push(';');
            }
            let _ = write!(self.css, "{property}:{value}");
        }
        self.css.push('}');
        self
    }

    /// Des règles qui ne valent que si `query` est vraie (`@media`).
    pub fn media(&mut self, query: &str, rules: impl FnOnce(&mut Sheet)) -> &mut Self {
        self.at("media", query, rules)
    }

    /// Des règles pour le thème clair : quand le système est clair (sauf si
    /// la personne a choisi le sombre dans les réglages), ou quand elle a
    /// choisi le clair (`<html data-theme="light|dark">`).
    pub fn light(&mut self, rules: impl FnOnce(&mut Sheet)) -> &mut Self {
        let mut block = Sheet::default();
        rules(&mut block);
        let scoped = |root: &str| {
            let mut out = String::new();
            for rule in block.css.split_inclusive('}') {
                let Some((selector, body)) = rule.split_once('{') else {
                    continue;
                };
                let selector = selector
                    .split(',')
                    .map(|part| {
                        let part = part.trim();
                        match part.strip_prefix(":root") {
                            Some(rest) => format!("{root}{rest}"),
                            None => format!("{root} {part}"),
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                let _ = write!(out, "{selector}{{{body}");
            }
            out
        };
        let system = scoped(":root:not([data-theme=dark])");
        let chosen = scoped(":root[data-theme=light]");
        let _ = write!(
            self.css,
            "@media (prefers-color-scheme: light){{{system}}}{chosen}"
        );
        self
    }

    /// Une animation (`@keyframes`) : ses étapes sont des règles.
    pub fn keyframes(&mut self, name: &str, steps: impl FnOnce(&mut Sheet)) -> &mut Self {
        self.at("keyframes", name, steps)
    }

    fn at(&mut self, kind: &str, arg: &str, inner: impl FnOnce(&mut Sheet)) -> &mut Self {
        let mut block = Sheet::default();
        inner(&mut block);
        let _ = write!(self.css, "@{kind} {arg}{{{}}}", block.css);
        self
    }

    /// Le CSS que lit le navigateur.
    pub fn css(self) -> String {
        self.css
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_rules_media_and_keyframes() {
        let mut s = Sheet::default();
        s.rule("a, b", &[("color", "red"), ("margin", "0")]);
        s.media("(max-width: 10px)", |s| {
            s.rule("a", &[("color", "blue")]);
        });
        s.keyframes("pulse", |s| {
            s.rule("to", &[("opacity", "0")]);
        });
        assert_eq!(
            s.css(),
            "a, b{color:red;margin:0}@media (max-width: 10px){a{color:blue}}@keyframes pulse{to{opacity:0}}"
        );
    }

    #[test]
    fn sheets_are_complete() {
        for css in [app(), site()] {
            assert_eq!(css.matches('{').count(), css.matches('}').count());
            assert!(css.starts_with(":root{"));
        }
    }

    #[test]
    fn light_rules_follow_the_system_or_the_choice() {
        let mut s = Sheet::default();
        s.light(|s| {
            s.rule(":root", &[("--bg", "#fff")]);
            s.rule(".a, .b", &[("color", "red")]);
        });
        assert_eq!(
            s.css(),
            "@media (prefers-color-scheme: light){:root:not([data-theme=dark]){--bg:#fff}\
             :root:not([data-theme=dark]) .a,:root:not([data-theme=dark]) .b{color:red}}\
             :root[data-theme=light]{--bg:#fff}:root[data-theme=light] .a,:root[data-theme=light] .b{color:red}"
        );
    }
}
