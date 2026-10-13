//! Les pages, rendues avec Active (SEO complet) :
//!
//! - `/` : la vitrine qui présente Alley ;
//! - `/app/` : l'application elle-même (construite par le navigateur, en Rust
//!   compilé en WebAssembly) ;
//! - toute autre adresse : une page 404.
//!
//! Les mêmes pages sont servies par le serveur et exportées pour GitHub Pages
//! ([`crate::export`]) : seules changent les adresses ([`Site`]).

use paquets::prelude::*;
use paquets::vitesse::Response;

use crate::Site;
use crate::assets::{MANIFEST_URL, THEME_COLOR, WASM, WASM_JS, versioned};

const DESCRIPTION: &str = "Alley : des salons de discussion éphémères, chiffrés de bout en bout. \
Un pseudonyme, et c'est tout ; rien n'est enregistré, et tout disparaît à minuit. Messages, photos, \
vocaux et appels vidéo, sur iPhone, Android, Windows, macOS et Linux.";

/// Les balises qui rendent la page installable et belle en plein écran,
/// notamment sur iPhone et iPad (barre d'état, encoche, icône d'accueil), et
/// qui disent au module WebAssembly où sont le site et l'API.
fn head_tags(site: &Site) -> String {
    let attr = |s: &str| {
        s.replace('&', "&amp;")
            .replace('"', "&quot;")
            .replace('<', "&lt;")
    };
    format!(
        r#"<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover, interactive-widget=resizes-content"/><link rel="manifest" href="{manifest}"/><meta name="theme-color" content="{THEME_COLOR}"/><meta name="color-scheme" content="dark light"/><meta name="mobile-web-app-capable" content="yes"/><meta name="apple-mobile-web-app-capable" content="yes"/><meta name="apple-mobile-web-app-title" content="Alley"/><meta name="apple-mobile-web-app-status-bar-style" content="black-translucent"/><meta name="format-detection" content="telephone=no"/><link rel="icon" href="{favicon}" type="image/svg+xml"/><link rel="apple-touch-icon" href="{touch}"/><meta name="alley-base" content="{base}"/><meta name="alley-api" content="{api}"/>"#,
        manifest = attr(&site.url(MANIFEST_URL)),
        favicon = attr(&site.url("/assets/favicon.svg")),
        touch = attr(&site.url("/assets/apple-touch-icon.png")),
        base = attr(&site.base),
        api = attr(&site.api),
    )
}

fn seo(site: &Site, path: &str) -> Seo {
    Seo::new()
        .canonical(site.url(path))
        .image(site.url("/assets/icon-512.png"))
        .site_name("Alley")
        .locale("fr_FR")
}

/// Charge le navigateur, écrit en Rust et compilé en WebAssembly : la colle
/// générée par `cargo xtask web` démarre le module d'elle-même, la page n'a
/// aucun script en ligne. `page` dit au module s'il anime la vitrine ou
/// l'application.
fn boot(site: &Site, page: &str) -> String {
    let (js, wasm) = (versioned(site, WASM_JS), versioned(site, WASM));
    format!(
        r#"<meta name="alley-page" content="{page}"/><link rel="preload" href="{wasm}" as="fetch" type="application/wasm" crossorigin=""/><script type="module" src="{js}"></script>"#
    )
}

/// La vitrine.
pub fn home(site: &Site) -> Page {
    Page::new("Alley — salons éphémères, chiffrés de bout en bout")
        .lang("fr")
        .description(DESCRIPTION)
        .seo(seo(site, "/").json_ld(
            r#"{"@context":"https://schema.org","@type":"SoftwareApplication","name":"Alley","applicationCategory":"CommunicationApplication","operatingSystem":"iOS, iPadOS, Android, Windows, macOS, Linux, ChromeOS","offers":{"@type":"Offer","price":"0","priceCurrency":"EUR"}}"#,
        ))
        .plain()
        .head(head_tags(site))
        .stylesheet(versioned(site, "/assets/site.css"))
        .head(boot(site, "site"))
        .body(site_header(site))
        .body(hero(site))
        .body(features())
        .body(security())
        .body(platforms())
        .body(faq())
        .body(site_footer(site))
}

fn logo(site: &Site) -> Element {
    a().class("logo")
        .href(site.url("/"))
        .attr("aria-label", "Alley, accueil")
        .child(
            img()
                .attr("src", site.url("/assets/favicon.svg"))
                .attr("alt", "")
                .attr("width", "28")
                .attr("height", "28"),
        )
        .child(span().text("Alley"))
}

fn site_header(site: &Site) -> Element {
    header().class("top").child(logo(site)).child(
        nav()
            .child(a().href("#fonctionnalites").text("Fonctionnalités"))
            .child(a().href("#securite").text("Sécurité"))
            .child(a().href("#installer").text("Installer"))
            .child(
                a().class("button small")
                    .href(site.url("/app/"))
                    .text("Ouvrir Alley"),
            ),
    )
}

fn hero(site: &Site) -> Element {
    section()
        .class("hero")
        .child(
            div()
                .class("hero-text")
                .child(
                    p().class("eyebrow")
                        .text("🔒 Chiffré de bout en bout · Rien n'est enregistré · Gratuit"),
                )
                .child(h1().text("On parle. À minuit, tout s'efface."))
                .child(p().class("lead").text(
                    "Alley, ce sont des salons de discussion comme sur Discord, en plus léger : un \
                     pseudonyme et c'est parti. Un salon ne vit que tant que quelqu'un y est, personne \
                     ne voit ce qui s'est dit avant son arrivée, et à minuit il ne reste rien — ni sur \
                     le serveur, ni sur votre téléphone.",
                ))
                .child(
                    div()
                        .class("cta")
                        .child(a().class("button").href(site.url("/app/")).text("Entrer dans Alley"))
                        .child(
                            a().class("button ghost")
                                .href("#securite")
                                .text("Comment ça marche"),
                        ),
                )
                .child(p().class("hint").text(
                    "Dans le navigateur, et installable comme une app sur iPhone, iPad, Android, \
                     Windows, macOS, Linux et ChromeOS.",
                )),
        )
        .child(hero_demo())
}

/// La même conversation, vue par vous et vue par le serveur.
fn hero_demo() -> Element {
    let bubble = |who: &'static str, color: &'static str, text: &'static str| {
        div()
            .class("msg")
            .child(
                span()
                    .class("avatar")
                    .attr("style", format!("--c:{color}"))
                    .text(&who[..1]),
            )
            .child(div().child(strong().text(who)).child(p().text(text)))
    };
    let cipher = |text: &'static str| div().class("msg cipher").child(code().text(text));
    div()
        .class("demo")
        .attr("aria-hidden", "true")
        .child(
            div()
                .class("pane")
                .child(
                    div()
                        .class("pane-title")
                        .text("# soirée-jeux — ce que vous voyez"),
                )
                .child(bubble(
                    "Inès",
                    "#7c5cff",
                    "On se retrouve à 20 h pour la session ?",
                ))
                .child(bubble("Malo", "#22d3a6", "Ok ! Et à minuit, plus rien 🌙"))
                .child(bubble(
                    "Inès",
                    "#7c5cff",
                    "Parfait, personne d'autre ne lira ça 😌",
                )),
        )
        .child(
            div()
                .class("pane server")
                .child(
                    div()
                        .class("pane-title")
                        .text("ce que voit le serveur (et oublie aussitôt)"),
                )
                .child(cipher("[[BROCOLIS:v1:q9Xw3J0+Lr2c…Pz8=]]"))
                .child(cipher("[[BROCOLIS:v1:7HfK1aQe9mZ0…tY4=]]"))
                .child(cipher("[[BROCOLIS:v1:Wm2Lr8sB1cXn…Qa0=]]")),
        )
}

fn card(icon: &'static str, title: &'static str, text: &'static str) -> Element {
    article()
        .class("card")
        .child(span().class("icon").text(icon))
        .child(h3().text(title))
        .child(p().text(text))
}

fn features() -> Element {
    section()
        .id("fonctionnalites")
        .class("section")
        .child(h2().text("Simple, vivant, sans traces"))
        .child(p().class("section-lead").text(
            "Les gestes d'un Discord, sans compte, sans historique, sans rien qui traîne.",
        ))
        .child(
            div()
                .class("grid")
                .child(card("🎭", "Un pseudonyme, c'est tout", "Pas de compte, pas d'e-mail, pas de numéro. Choisissez un nom, entrez. Il est oublié dès que vous partez."))
                .child(card("🔎", "Cherchez ou créez", "La liste des salons ouverts, une barre de recherche, et si votre salon n'existe pas encore, il est créé d'un geste. Ou privé, seulement par lien."))
                .child(card("🌱", "Des salons vivants", "Un salon existe tant que quelqu'un y est. Le dernier qui part l'emporte avec lui."))
                .child(card("🙈", "Pas d'historique", "En arrivant, vous ne voyez rien de ce qui s'est dit avant vous : le serveur ne garde aucun message, il les relaie et les oublie."))
                .child(card("🌙", "Tout s'efface à minuit", "À minuit dans votre pays, l'écran se vide ; à minuit dans le pays de son créateur, le salon disparaît."))
                .child(card("🎙️", "Photos, vocaux, appels", "Envoyez des photos et des messages vocaux, appelez en audio ou en vidéo, partagez votre écran — chiffré, de pair à pair.")),
        )
}

fn step(n: &'static str, title: &'static str, text: &'static str) -> Element {
    li().class("step")
        .child(span().class("num").text(n))
        .child(div().child(h3().text(title)).child(p().text(text)))
}

fn security() -> Element {
    section()
        .id("securite")
        .class("section")
        .child(h2().text("Chiffré sur votre appareil, oublié partout"))
        .child(
            ol()
                .class("steps")
                .child(step("1", "Une clé par salon", "Le premier arrivant tire une clé AES-256 au hasard, sur son appareil. Elle n'en sort jamais en clair."))
                .child(step("2", "Remise de la main à la main", "Chaque nouvel arrivant présente une clé X25519 jetable ; les présents lui remettent la clé du salon, chiffrée pour lui seul (l'appairage de la messagerie Brocolis)."))
                .child(step("3", "Tout est chiffré avant de partir", "Messages, pseudonymes, photos, vocaux et mise en relation des appels deviennent des jetons [[BROCOLIS:v1:…]] que le serveur relaie sans pouvoir les lire."))
                .child(step("4", "Rien n'est gardé", "Le serveur ne stocke aucun message : les absents ne reçoivent rien, les retardataires ne voient rien d'avant. Rien n'est écrit sur votre appareil non plus."))
                .child(step("5", "Vérifiez l'empreinte", "Chaque salon affiche l'empreinte de sa clé (ex. 4BB0-6F8E-4E3A-7715) : identique chez tous les présents, personne ne s'est intercalé.")),
        )
        .child(
            div()
                .class("grid tech")
                .child(card("🥦", "Le protocole Brocolis", "AES-256-GCM et jetons de la messagerie Brocolis, clés remises par X25519 + HKDF-SHA256, empreintes identiques à l'app Brocolis."))
                .child(card("📦", "Un serveur Paquets", "Écrit en Rust avec Paquets : WAF, limites de débit, CSP stricte, HTTPS imposé, HSTS. Et aucune base de données : tout vit en mémoire, le temps d'un salon."))
                .child(card("🦀", "100 % Rust", "Le navigateur aussi est écrit en Rust, compilé en WebAssembly : chiffrement, salons, appels.")),
        )
}

fn platform(
    id: &'static str,
    icon: &'static str,
    name: &'static str,
    how: &'static str,
) -> Element {
    article()
        .class("platform")
        .attr("data-os", id)
        .child(span().class("icon").text(icon))
        .child(h3().text(name))
        .child(p().text(how))
}

fn platforms() -> Element {
    section()
        .id("installer")
        .class("section")
        .child(h2().text("Sur tous vos appareils"))
        .child(p().class("section-lead").text(
            "Alley est une application web installable : la même app, à jour partout, sans \
             passer par un magasin d'applications.",
        ))
        .child(
            div()
                .class("install-now")
                .child(button().id("install").class("button").attr("hidden", "").text("Installer Alley"))
                .child(p().id("install-hint").class("hint").text("")),
        )
        .child(
            div()
                .class("grid platforms")
                .child(platform("ios", "📱", "iPhone et iPad", "Dans Safari, touchez Partager puis « Sur l'écran d'accueil ». Alley s'ouvre en plein écran, comme une app, avec ses notifications."))
                .child(platform("android", "🤖", "Android", "Dans Chrome, touchez « Installer l'application » (ou ⋮ → Ajouter à l'écran d'accueil)."))
                .child(platform("windows", "🪟", "Windows", "Dans Edge ou Chrome, cliquez sur l'icône d'installation dans la barre d'adresse. Alley rejoint le menu Démarrer."))
                .child(platform("macos", "🍎", "macOS", "Dans Safari : Fichier → Ajouter au Dock. Dans Chrome ou Edge : l'icône d'installation de la barre d'adresse."))
                .child(platform("linux", "🐧", "Linux", "Dans Chrome, Chromium ou Edge : l'icône d'installation. Dans Firefox, Alley marche directement dans un onglet."))
                .child(platform("chromeos", "💻", "ChromeOS", "Cliquez sur l'icône d'installation dans la barre d'adresse : Alley rejoint le lanceur d'applications.")),
        )
}

fn qa(q: &'static str, a_: &'static str) -> Element {
    details().child(summary().text(q)).child(p().text(a_))
}

fn faq() -> Element {
    section()
        .id("faq")
        .class("section narrow")
        .child(h2().text("Questions fréquentes"))
        .child(qa("Faut-il créer un compte ?", "Non. Un pseudonyme, le temps de la visite. Il n'est enregistré nulle part : fermez la page et il est oublié."))
        .child(qa("Pourquoi je ne vois pas les anciens messages ?", "Parce qu'ils n'existent plus nulle part. Le serveur relaie chaque message aux personnes présentes, puis l'oublie. C'est voulu : ce qui se dit dans Alley reste entre ceux qui étaient là."))
        .child(qa("Quand un salon disparaît-il ?", "Dès que la dernière personne en sort, et au plus tard à minuit dans le pays de celui qui l'a créé. De votre côté, l'écran se vide aussi à minuit chez vous."))
        .child(qa("Que voit le serveur ?", "Le nom des salons publics (pour qu'on puisse les chercher) et combien de personnes y sont. Ni les pseudonymes, ni les messages, ni la clé. Comme tout serveur web, il voit les adresses IP qui se connectent."))
        .child(qa("Les appels sont-ils chiffrés ?", "Oui : ils passent de pair à pair en WebRTC (DTLS-SRTP), et leur mise en relation est chiffrée avec la clé du salon."))
        .child(qa("Comment inviter quelqu'un dans un salon privé ?", "Touchez 🔗 dans le salon : le lien marche tant que le salon existe, c'est-à-dire tant que quelqu'un y est, et jusqu'à minuit."))
}

fn site_footer(site: &Site) -> Element {
    footer()
        .class("bottom")
        .child(logo(site))
        .child(p().text("Écrit avec Paquets 📦 · chiffré avec Brocolis 🥦"))
        .child(
            nav()
                .child(a().href(site.url("/app/")).text("Entrer dans Alley"))
                .child(
                    a().href(format!("{}/api/v1/stats", site.api))
                        .text("Ce que voit le serveur"),
                ),
        )
}

/// L'application : seulement la coquille ; le WebAssembly déchiffre et affiche tout
/// dans le navigateur.
pub fn app_shell(site: &Site) -> Page {
    Page::new("Alley")
        .lang("fr")
        .description(DESCRIPTION)
        .seo(seo(site, "/app/"))
        .plain()
        .head(head_tags(site))
        .stylesheet(versioned(site, "/assets/app.css"))
        .head(boot(site, "app"))
        .body(
            div()
                .id("root")
                .class("boot")
                .child(
                    img()
                        .attr("src", site.url("/assets/icon-192.png"))
                        .attr("alt", "")
                        .attr("width", "72")
                        .attr("height", "72"),
                )
                .child(p().text("Ouverture d'Alley…")),
        )
        .body(
            noscript().text("Alley chiffre vos messages dans le navigateur : activez JavaScript."),
        )
}

pub fn not_found(site: &Site) -> Page {
    Page::new("Introuvable — Alley")
        .lang("fr")
        .plain()
        .head(head_tags(site))
        .stylesheet(versioned(site, "/assets/site.css"))
        .seo(Seo::new().noindex(true))
        .status(404)
        .body(site_header(site))
        .body(
            section()
                .class("hero notfound")
                .child(h1().text("Cette ruelle ne mène nulle part."))
                .child(p().class("lead").text("La page demandée n'existe pas."))
                .child(
                    a().class("button")
                        .href(site.url("/"))
                        .text("Retour à l'accueil"),
                ),
        )
}

/// La politique de sécurité d'une page, élargie à ce dont l'app a besoin :
///
/// - `connect-src` : l'API (une autre origine sur GitHub Pages) et sa
///   WebSocket (`wss://`) ; certains navigateurs n'étendent pas `'self'` aux
///   WebSocket ;
/// - `media-src` : les vocaux déchiffrés (`data:`) et les flux des appels.
pub fn policy(csp: &str, site: &Site, host: Option<&str>) -> String {
    let mut connect = vec!["'self'".to_owned()];
    if let Some(host) = host {
        connect.push(format!("wss://{host}"));
        connect.push(format!("ws://{host}"));
    }
    if let Some(rest) = site.api.strip_prefix("https://") {
        connect.push(site.api.clone());
        connect.push(format!("wss://{rest}"));
    } else if let Some(rest) = site.api.strip_prefix("http://") {
        connect.push(site.api.clone());
        connect.push(format!("ws://{rest}"));
    }
    csp.replace(
        "connect-src 'self'",
        &format!("connect-src {}", connect.join(" ")),
    )
    .replace(
        "img-src 'self' data: https:",
        "img-src 'self' data: blob: https:; media-src 'self' data: blob:",
    )
}

fn respond(page: Page, site: &Site, req: &Request) -> Response {
    // Sur le serveur, l'API est ici : sa WebSocket aussi.
    let host = req
        .host()
        .filter(|_| site.api.is_empty())
        .filter(|h| {
            h.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-.:[]".contains(&b))
        })
        .map(str::to_owned);
    let res = page.into_response();
    let Some(csp) = res.get_header("content-security-policy") else {
        return res;
    };
    let csp = policy(csp, site, host.as_deref());
    res.header("content-security-policy", csp)
}

/// Les pages, servies pour `site`.
pub fn routes(app: &mut App, site: &Site) {
    let s = site.clone();
    app.get("/", move |req: Request| {
        let s = s.clone();
        async move { respond(home(&s), &s, &req) }
    });
    // `/app` y mène aussi (Vitesse ignore la barre finale).
    let s = site.clone();
    app.get("/app/", move |req: Request| {
        let s = s.clone();
        async move { respond(app_shell(&s), &s, &req) }
    });
    let s = site.clone();
    app.fallback(move |req: Request| {
        let s = s.clone();
        async move {
            if req.path().starts_with("/api/") {
                return Response::new()
                    .status(404)
                    .json(json!({ "message": "adresse inconnue" }));
            }
            respond(not_found(&s), &s, &req)
        }
    });
}
