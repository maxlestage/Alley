//! Les pages, rendues par le serveur avec Active (SEO complet) :
//!
//! - `/` : la vitrine qui présente Alley ;
//! - `/app` : l'application elle-même (le script `app.js` la construit) ;
//! - toute autre adresse : une page 404.

use paquets::prelude::*;
use paquets::vitesse::Response;

use crate::assets::{MANIFEST_URL, THEME_COLOR};

const DESCRIPTION: &str = "Alley, l'équivalent de Discord chiffré de bout en bout : serveurs, \
salons, messages et images en temps réel, illisibles pour le serveur. Sur iPhone, Android, \
Windows, macOS et Linux.";

/// Les balises qui rendent la page installable et belle en plein écran,
/// notamment sur iPhone et iPad (barre d'état, encoche, icône d'accueil).
fn head_tags() -> String {
    format!(
        r#"<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover, interactive-widget=resizes-content"/><link rel="manifest" href="{MANIFEST_URL}"/><meta name="theme-color" content="{THEME_COLOR}"/><meta name="color-scheme" content="dark light"/><meta name="mobile-web-app-capable" content="yes"/><meta name="apple-mobile-web-app-capable" content="yes"/><meta name="apple-mobile-web-app-title" content="Alley"/><meta name="apple-mobile-web-app-status-bar-style" content="black-translucent"/><meta name="format-detection" content="telephone=no"/><link rel="icon" href="/assets/favicon.svg" type="image/svg+xml"/><link rel="apple-touch-icon" href="/assets/apple-touch-icon.png"/>"#
    )
}

fn seo(path: &str) -> Seo {
    Seo::new()
        .canonical(path.to_owned())
        .image("/assets/icon-512.png")
        .site_name("Alley")
        .locale("fr_FR")
}

/// `/` : la vitrine.
async fn home(_req: Request) -> Page {
    Page::new("Alley — l'équivalent de Discord, chiffré de bout en bout")
        .lang("fr")
        .description(DESCRIPTION)
        .seo(seo("/").json_ld(
            r#"{"@context":"https://schema.org","@type":"SoftwareApplication","name":"Alley","applicationCategory":"CommunicationApplication","operatingSystem":"iOS, iPadOS, Android, Windows, macOS, Linux, ChromeOS","offers":{"@type":"Offer","price":"0","priceCurrency":"EUR"}}"#,
        ))
        .plain()
        .head(head_tags())
        .stylesheet("/assets/site.css")
        .head(r#"<script type="module" src="/assets/site.js"></script>"#)
        .body(site_header())
        .body(hero())
        .body(features())
        .body(security())
        .body(platforms())
        .body(faq())
        .body(site_footer())
}

fn logo() -> Element {
    a().class("logo")
        .href("/")
        .attr("aria-label", "Alley, accueil")
        .child(
            img()
                .attr("src", "/assets/favicon.svg")
                .attr("alt", "")
                .attr("width", "28")
                .attr("height", "28"),
        )
        .child(span().text("Alley"))
}

fn site_header() -> Element {
    header().class("top").child(logo()).child(
        nav()
            .child(a().href("#fonctionnalites").text("Fonctionnalités"))
            .child(a().href("#securite").text("Sécurité"))
            .child(a().href("#installer").text("Installer"))
            .child(a().class("button small").href("/app").text("Ouvrir Alley")),
    )
}

fn hero() -> Element {
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
                    "Alley, ce sont des serveurs, des salons et des discussions en temps réel, \
                     comme sur Discord. Sauf que chaque message, chaque nom de salon, chaque \
                     pseudo et chaque image est chiffré sur votre appareil avant de partir : \
                     le serveur ne voit que des jetons illisibles.",
                ))
                .child(
                    div()
                        .class("cta")
                        .child(
                            a().class("button")
                                .href("/app#nouveau")
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
                .child(bubble("Malo", "#22d3a6", "Ok ! Je ramène la carte 🗺️"))
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
        .child(h2().text("Tout ce qu'on attend d'un Discord"))
        .child(p().class("section-lead").text(
            "Les gestes que vous connaissez déjà, sans rien céder sur la confidentialité.",
        ))
        .child(
            div()
                .class("grid")
                .child(card("🏰", "Serveurs et salons", "Créez autant de serveurs que vous voulez, organisez-les en salons, renommez-les, donnez-leur un sujet."))
                .child(card("⚡", "Temps réel", "Les messages arrivent instantanément, avec « en train d'écrire… », la présence en ligne et les badges de non-lus."))
                .child(card("🖼️", "Images chiffrées", "Envoyez des photos : elles sont réduites puis chiffrées sur votre appareil, comme le texte."))
                .child(card("💬", "Réponses, modifications", "Répondez à un message, corrigez-le, supprimez-le. Mise en forme **gras**, *italique*, `code` et liens."))
                .child(card("🔗", "Invitation par lien ou QR", "Un lien suffit pour rejoindre. La clé voyage dans le lien, jamais sur le serveur."))
                .child(card("🛡️", "Administration", "Le créateur gère les salons et peut modérer. Les autres ne modifient que leurs propres messages.")),
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
                .child(step("3", "Tout est chiffré avant de partir", "Messages, images, noms de salons et pseudos deviennent des jetons [[BROCOLIS:v1:…]]. Le serveur refuse tout ce qui n'est pas chiffré."))
                .child(step("4", "Vérifiez l'empreinte", "Chaque serveur affiche une empreinte (ex. 4BB0-6F8E-4E3A-7715). Comparez-la de vive voix : identique pour tous, personne ne s'est intercalé.")),
        )
        .child(
            div()
                .class("grid tech")
                .child(card("🥦", "Le protocole Brocolis", "AES-256-GCM au format de la messagerie Brocolis : un jeton Alley se lit dans l'app Brocolis avec la même clé, et inversement."))
                .child(card("📦", "Un serveur Paquets", "Écrit en Rust avec Paquets : WAF, limites de débit, CSP stricte, HSTS et anti-robots devant chaque requête."))
                .child(card("🙈", "Ce que voit le serveur", "Des identifiants, des horaires et des jetons. Ni la clé, ni vos messages, ni vos noms. Le jeton d'accès n'est gardé qu'en empreinte SHA-256.")),
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
                .child(platform("ios", "📱", "iPhone et iPad", "Dans Safari, touchez Partager puis « Sur l'écran d'accueil ». Alley s'ouvre en plein écran, comme une app."))
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
        .child(qa("Que se passe-t-il si je perds le lien d'invitation ?", "Demandez-en un nouveau à un membre du serveur : n'importe quel membre peut le partager depuis l'app. Sans la clé, personne, pas même le serveur, ne peut relire les messages."))
        .child(qa("Comment changer d'appareil ?", "Ouvrez le lien d'invitation du serveur sur le nouvel appareil. Pour garder vos droits d'administrateur, utilisez le lien d'administration, à garder pour vous."))
        .child(qa("Mes métadonnées sont-elles protégées ?", "Le serveur ne connaît ni les noms ni le contenu, mais il voit quand un message est envoyé, dans quel salon et par quel identifiant de membre (aléatoire). C'est le prix du temps réel."))
        .child(qa("Ça marche hors ligne ?", "L'app s'ouvre sans réseau et affiche vos serveurs ; les messages partent et arrivent dès que la connexion revient."))
}

fn site_footer() -> Element {
    footer()
        .class("bottom")
        .child(logo())
        .child(p().text("Écrit avec Paquets 📦 · chiffré avec Brocolis 🥦"))
        .child(
            nav()
                .child(a().href("/app").text("Ouvrir l'app"))
                .child(a().href("/api/v1/stats").text("Ce que voit le serveur")),
        )
}

/// `/app` : l'application. Le serveur n'envoie que la coquille ; le script
/// déchiffre et affiche tout dans le navigateur.
async fn app_page(req: Request) -> Response {
    let page = Page::new("Alley")
        .lang("fr")
        .description(DESCRIPTION)
        .seo(seo("/app"))
        .plain()
        .head(head_tags())
        .stylesheet("/assets/app.css")
        .head(r#"<script type="module" src="/assets/app.js"></script>"#)
        .body(
            div()
                .id("root")
                .class("boot")
                .child(
                    img()
                        .attr("src", "/assets/icon-192.png")
                        .attr("alt", "")
                        .attr("width", "72")
                        .attr("height", "72"),
                )
                .child(p().text("Ouverture d'Alley…")),
        )
        .body(
            noscript().text("Alley chiffre vos messages dans le navigateur : activez JavaScript."),
        );
    with_websocket(page.into_response(), &req)
}

/// La politique de la page autorise `connect-src 'self'`, que les navigateurs
/// anciens (Safari avant 15.4) n'étendent pas aux WebSocket : on ajoute
/// explicitement l'adresse `wss://` (et `ws://` en développement) de l'hôte.
fn with_websocket(res: Response, req: &Request) -> Response {
    let host = req
        .host()
        .filter(|h| {
            h.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-.:[]".contains(&b))
        })
        .map(str::to_owned);
    let Some(host) = host else {
        return res;
    };
    let Some(csp) = res.get_header("content-security-policy") else {
        return res;
    };
    let csp = csp.replace(
        "connect-src 'self'",
        &format!("connect-src 'self' wss://{host} ws://{host}"),
    );
    res.header("content-security-policy", csp)
}

async fn not_found(req: Request) -> Response {
    if req.path().starts_with("/api/") {
        return Response::new()
            .status(404)
            .json(json!({ "message": "adresse inconnue" }));
    }
    Page::new("Introuvable — Alley")
        .lang("fr")
        .plain()
        .head(head_tags())
        .stylesheet("/assets/site.css")
        .seo(Seo::new().noindex(true))
        .status(404)
        .body(site_header())
        .body(
            section()
                .class("hero notfound")
                .child(h1().text("Cette ruelle ne mène nulle part."))
                .child(p().class("lead").text("La page demandée n'existe pas."))
                .child(a().class("button").href("/").text("Retour à l'accueil")),
        )
        .into_response()
}

pub fn routes(app: &mut App) {
    app.get("/", home);
    app.get("/app", app_page);
    app.fallback(not_found);
}
