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
use crate::assets::{MANIFEST_URL, THEME_COLOR, WASM, WASM_JS};

const DESCRIPTION: &str = "Alley, l'équivalent de Discord chiffré de bout en bout : serveurs, \
salons, messages, photos, vocaux et appels vidéo, illisibles pour le serveur. Sur iPhone, \
Android, Windows, macOS et Linux.";

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

/// Démarre le navigateur, écrit en Rust et compilé en WebAssembly : la seule
/// ligne de JavaScript qu'exige un navigateur pour charger un module
/// WebAssembly, générée ici comme le fait Paquets pour ses îlots (autorisée
/// par son empreinte dans la politique de sécurité de la page). `page` dit au
/// module s'il anime la vitrine ou l'application.
fn boot(site: &Site, page: &str) -> String {
    let (js, wasm) = (site.url(WASM_JS), site.url(WASM));
    format!(
        r#"<meta name="alley-page" content="{page}"/><link rel="modulepreload" href="{js}"/><link rel="preload" href="{wasm}" as="fetch" type="application/wasm" crossorigin=""/><script type="module">import init from "{js}";init({{module_or_path:"{wasm}"}});</script>"#
    )
}

/// La vitrine.
pub fn home(site: &Site) -> Page {
    Page::new("Alley — l'équivalent de Discord, chiffré de bout en bout")
        .lang("fr")
        .description(DESCRIPTION)
        .seo(seo(site, "/").json_ld(
            r#"{"@context":"https://schema.org","@type":"SoftwareApplication","name":"Alley","applicationCategory":"CommunicationApplication","operatingSystem":"iOS, iPadOS, Android, Windows, macOS, Linux, ChromeOS","offers":{"@type":"Offer","price":"0","priceCurrency":"EUR"}}"#,
        ))
        .plain()
        .head(head_tags(site))
        .stylesheet(site.url("/assets/site.css"))
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
                        .text("🔒 Chiffré de bout en bout · Gratuit · Sans compte"),
                )
                .child(h1().text("Vos communautés. Vos messages. Personne d'autre."))
                .child(p().class("lead").text(
                    "Alley, ce sont des serveurs, des salons, des discussions et des appels en \
                     temps réel, comme sur Discord. Sauf que tout est chiffré sur votre appareil \
                     avant de partir — même qui écrit, où et quand : le serveur ne voit que des \
                     jetons illisibles.",
                ))
                .child(
                    div()
                        .class("cta")
                        .child(
                            a().class("button")
                                .href(site.url("/app/#nouveau"))
                                .text("Créer mon serveur"),
                        )
                        .child(
                            a().class("button ghost")
                                .href("#securite")
                                .text("Comment ça marche"),
                        ),
                )
                .child(p().class("hint").text(
                    "Fonctionne dans le navigateur et s'installe comme une app sur iPhone, iPad, \
                     Android, Windows, macOS, Linux et ChromeOS.",
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
                        .text("# général — ce que vous voyez"),
                )
                .child(bubble(
                    "Inès",
                    "#7c5cff",
                    "On se retrouve à 20 h pour la session ?",
                ))
                .child(bubble("Malo", "#22d3a6", "Ok ! Je lance l'appel vidéo 🎥"))
                .child(bubble(
                    "Inès",
                    "#7c5cff",
                    "Parfait, personne d'autre ne lira ça 😌",
                )),
        )
        .child(
            div()
                .class("pane server")
                .child(div().class("pane-title").text("ce que voit le serveur"))
                .child(cipher("#4182 · [[BROCOLIS:v1:q9Xw3J0+Lr2c…Pz8=]]"))
                .child(cipher("#4183 · [[BROCOLIS:v1:7HfK1aQe9mZ0…tY4=]]"))
                .child(cipher("#4184 · [[BROCOLIS:v1:Wm2Lr8sB1cXn…Qa0=]]")),
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
        .child(h2().text("Tout ce qu'on attend d'un Discord"))
        .child(p().class("section-lead").text(
            "Les gestes que vous connaissez déjà, sans rien céder sur la confidentialité.",
        ))
        .child(
            div()
                .class("grid")
                .child(card("🏰", "Serveurs et salons", "Créez autant de serveurs que vous voulez, organisez-les en salons textuels et vocaux, renommez-les, donnez-leur un sujet."))
                .child(card("🎙️", "Appels audio et vidéo", "Rejoignez un salon vocal : voix, caméra et partage d'écran, de pair à pair et chiffrés de bout en bout."))
                .child(card("⚡", "Temps réel", "Les messages arrivent instantanément, avec « en train d'écrire… », la présence en ligne et les badges de non-lus."))
                .child(card("🖼️", "Photos et vocaux", "Envoyez des photos et des messages vocaux : ils sont chiffrés sur votre appareil, comme le texte."))
                .child(card("💬", "Réponses, modifications", "Répondez, corrigez, supprimez. Mise en forme en gras, italique, code, citations et liens."))
                .child(card("🛡️", "Modération réelle", "L'admin peut exclure un membre : la clé du serveur est renouvelée et redistribuée aux autres, l'exclu ne lit plus rien.")),
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
        .child(h2().text("Chiffré sur votre appareil, illisible partout ailleurs"))
        .child(
            ol()
                .class("steps")
                .child(step("1", "Une clé par serveur", "En créant un serveur, votre appareil tire une clé AES-256 au hasard. Elle ne quitte jamais votre appareil autrement que dans un lien d'invitation."))
                .child(step("2", "Le lien porte la clé", "La clé est placée après le « # » du lien : cette partie n'est jamais envoyée au serveur par le navigateur. Partagez le lien par un canal de confiance."))
                .child(step("3", "Tout est chiffré, même les métadonnées", "Texte, photos, vocaux, noms, pseudos, mais aussi l'auteur, le salon et l'heure de chaque message : tout part dans un jeton [[BROCOLIS:v1:…]], signé par son auteur."))
                .child(step("4", "Exclure, c'est changer de clé", "Quand l'admin exclut quelqu'un, une nouvelle clé est tirée et remise à chaque membre restant, chiffrée pour lui seul (X25519, comme l'appairage Brocolis)."))
                .child(step("5", "Vérifiez l'empreinte", "Chaque serveur affiche une empreinte (ex. 4BB0-6F8E-4E3A-7715). Comparez-la de vive voix : identique pour tous, personne ne s'est intercalé.")),
        )
        .child(
            div()
                .class("grid tech")
                .child(card("🥦", "Le protocole Brocolis", "AES-256-GCM et jetons de la messagerie Brocolis, clés redistribuées par X25519 + HKDF-SHA256, empreintes identiques à l'app Brocolis."))
                .child(card("📦", "Un serveur Paquets", "Écrit en Rust avec Paquets : WAF, limites de débit, CSP stricte, HTTPS imposé, HSTS et anti-robots devant chaque requête."))
                .child(card("🙈", "Ce que voit le serveur", "Des numéros de messages et des jetons. Ni la clé, ni le contenu, ni les noms, ni qui a écrit quoi, dans quel salon ou à quelle heure, ni qui est en ligne.")),
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
        .child(qa("Faut-il créer un compte ?", "Non. Choisissez un pseudo (chiffré, lui aussi) et c'est parti. Vos serveurs et leurs clés sont gardés sur votre appareil."))
        .child(qa("Quelqu'un a eu le lien sans y être invité : que faire ?", "L'admin l'exclut, ou renouvelle simplement la clé : chaque membre connu reçoit la nouvelle clé, chiffrée pour lui, et l'ancien lien ne mène plus nulle part."))
        .child(qa("Que voit exactement le serveur ?", "Qu'un serveur Alley existe, combien il a de membres et de messages, et des jetons chiffrés. Pas les noms, pas le contenu, pas l'auteur, le salon ni l'heure d'un message, pas qui est en ligne. Il voit, comme tout serveur web, les adresses IP qui se connectent : passez par un VPN ou Tor si cela compte pour vous."))
        .child(qa("Les appels sont-ils chiffrés ?", "Oui : ils passent de pair à pair en WebRTC (DTLS-SRTP), et leur mise en relation est chiffrée avec la clé du serveur, ce qui empêche le serveur de s'intercaler."))
        .child(qa("Comment changer d'appareil ?", "Ouvrez le lien d'invitation du serveur sur le nouvel appareil. Pour garder vos droits d'administrateur, utilisez le lien d'administration, à garder pour vous."))
        .child(qa("Ça marche hors ligne ?", "L'app s'ouvre sans réseau et affiche vos serveurs ; les messages partent et arrivent dès que la connexion revient."))
}

fn site_footer(site: &Site) -> Element {
    footer()
        .class("bottom")
        .child(logo(site))
        .child(p().text("Écrit avec Paquets 📦 · chiffré avec Brocolis 🥦"))
        .child(
            nav()
                .child(a().href(site.url("/app/")).text("Ouvrir l'app"))
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
        .stylesheet(site.url("/assets/app.css"))
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
        .stylesheet(site.url("/assets/site.css"))
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
