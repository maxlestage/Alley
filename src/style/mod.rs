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
}
