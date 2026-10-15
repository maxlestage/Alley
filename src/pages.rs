//! Les pages, rendues avec Active (SEO complet), en trois langues
//! ([`crate::i18n`]) :
//!
//! - `/`, `/en/`, `/es/` : la vitrine qui présente Alley ;
//! - `/app/`, `/en/app/`, `/es/app/` : l'application elle-même (construite par
//!   le navigateur, en Rust compilé en WebAssembly) ;
//! - `/legal/`, `/terms/`, `/privacy/`, `/rules/` (et leurs versions `/en/`,
//!   `/es/`) : mentions légales, conditions, confidentialité, règles de la
//!   communauté ([`crate::legal`]) ;
//! - toute autre adresse : une page 404, dans les trois langues.
//!
//! Les mêmes pages sont servies par le serveur et exportées pour GitHub Pages
//! ([`crate::export`]) : seules changent les adresses ([`Site`]).

use paquets::prelude::*;
use paquets::vitesse::Response;

use crate::Site;
use crate::assets::{THEME_COLOR, WASM, WASM_JS, manifest_url, versioned};
use crate::i18n::{LANGS, Lang};
use crate::legal::{self, PUBLISHER};

fn attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
}

/// Les balises qui rendent la page installable et belle en plein écran,
/// notamment sur iPhone et iPad (barre d'état, encoche, icône d'accueil), qui
/// relient les trois langues, et qui disent au module WebAssembly où sont le
/// site et l'API.
fn head_tags(site: &Site, lang: Lang, path: &str) -> String {
    let mut alternates = String::new();
    for other in LANGS {
        alternates.push_str(&format!(
            r#"<link rel="alternate" hreflang="{}" href="{}"/>"#,
            other.code(),
            attr(&site.url(&other.path(path)))
        ));
    }
    alternates.push_str(&format!(
        r#"<link rel="alternate" hreflang="x-default" href="{}"/>"#,
        attr(&site.url(path))
    ));
    format!(
        r#"<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover, interactive-widget=resizes-content"/><link rel="manifest" href="{manifest}"/><meta name="theme-color" content="{THEME_COLOR}"/><meta name="color-scheme" content="dark light"/><meta name="mobile-web-app-capable" content="yes"/><meta name="apple-mobile-web-app-capable" content="yes"/><meta name="apple-mobile-web-app-title" content="Alley"/><meta name="apple-mobile-web-app-status-bar-style" content="black-translucent"/><meta name="format-detection" content="telephone=no"/><link rel="icon" href="{favicon}" type="image/svg+xml"/><link rel="apple-touch-icon" href="{touch}"/>{alternates}<meta name="alley-base" content="{base}"/><meta name="alley-api" content="{api}"/><meta name="alley-contact" content="{contact}"/>"#,
        manifest = attr(&site.url(&manifest_url(lang))),
        favicon = attr(&site.url("/assets/favicon.svg")),
        touch = attr(&site.url("/assets/apple-touch-icon.png")),
        base = attr(&site.base),
        api = attr(&site.api),
        contact = attr(&PUBLISHER.email),
    )
}

fn seo(site: &Site, lang: Lang, path: &str) -> Seo {
    Seo::new()
        .canonical(site.url(&lang.path(path)))
        .image(site.url("/assets/icon-512.png"))
        .site_name("Alley")
        .locale(lang.texts().locale)
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

/// La vitrine, dans une langue.
pub fn home(site: &Site, lang: Lang) -> Page {
    let t = lang.texts();
    Page::new(t.title)
        .lang(lang.code())
        .description(t.description)
        .seo(seo(site, lang, "/").json_ld(
            r#"{"@context":"https://schema.org","@type":"SoftwareApplication","name":"Alley","applicationCategory":"CommunicationApplication","operatingSystem":"iOS, iPadOS, Android, Windows, macOS, Linux, ChromeOS","offers":{"@type":"Offer","price":"0","priceCurrency":"EUR"}}"#,
        ))
        .plain()
        .head(head_tags(site, lang, "/"))
        .stylesheet(versioned(site, "/assets/site.css"))
        .head(boot(site, "site"))
        .body(site_header(site, lang))
        .body(hero(site, lang))
        .body(features(lang))
        .body(security(lang))
        .body(platforms(lang))
        .body(pricing(site, lang))
        .body(faq(lang))
        .body(site_footer(site, lang))
}

/// Les tarifs : gratuit, ou une période payée une fois.
fn pricing(site: &Site, lang: Lang) -> Element {
    let l = legal::labels(lang);
    let price = |cents: u32| {
        let sep = if lang == Lang::En { "." } else { "," };
        let amount = if cents == 0 {
            "0".to_owned()
        } else {
            format!("{}{sep}{:02}", cents / 100, cents % 100)
        };
        if lang == Lang::En {
            format!("€{amount}")
        } else {
            format!("{amount} €")
        }
    };
    let mut grid = div().class("grid prices").child(
        article()
            .class("card price-card")
            .child(h3().text(l.pricing_free))
            .child(p().class("amount").text(price(0)))
            .child(p().text(l.pricing_free_text)),
    );
    for plan in crate::billing::PLANS {
        let length = match plan.months {
            0 => l.pricing_days.replace("{}", &plan.days.to_string()),
            1 => l.pricing_month_one.to_owned(),
            n => l.pricing_months.replace("{}", &n.to_string()),
        };
        grid = grid.child(
            article()
                .class("card price-card")
                .child(h3().text(length.clone()))
                .child(
                    p().class("amount")
                        .text(price(plan.cents))
                        .child(span().text(format!(" · {}", l.pricing_once))),
                )
                .child(p().text(l.pricing_paid_text.replace("{}", &length))),
        );
    }
    section()
        .id("pricing")
        .class("section")
        .child(h2().text(l.pricing_title))
        .child(p().class("section-lead").text(l.pricing_lead))
        .child(grid)
        .child(
            p().class("hint center")
                .text(l.pricing_note)
                .child(text(" "))
                .child(
                    a().href(site.url(&lang.path("/terms/")))
                        .text(legal::docs(lang)[1].title),
                ),
        )
}

/// Une page légale ou d'aide ([`legal::Doc`]).
pub fn legal_page(site: &Site, lang: Lang, doc: &legal::Doc) -> Page {
    let l = legal::labels(lang);
    let path = format!("/{}/", doc.slug);
    let mut article_ = article()
        .class("legal")
        .child(h1().text(doc.title))
        .child(p().class("lead").text(doc.lead))
        .child(
            p().class("updated")
                .text(format!("{} {}", l.updated, legal::updated(lang))),
        );
    for (title, paragraphs) in doc.sections {
        let mut part = section().child(h2().text(*title));
        for text_ in *paragraphs {
            part = part.child(p().text(legal::fill(text_, lang)));
        }
        article_ = article_.child(part);
    }
    article_ = article_.child(
        p().child(
            a().class("button ghost")
                .href(site.url(&lang.path("/")))
                .text(l.back),
        ),
    );
    Page::new(format!("{} — Alley", doc.title))
        .lang(lang.code())
        .description(doc.lead)
        .seo(seo(site, lang, &path))
        .plain()
        .head(head_tags(site, lang, &path))
        .stylesheet(versioned(site, "/assets/site.css"))
        .head(boot(site, "site"))
        .body(site_header(site, lang))
        .body(html::main().class("section narrow").child(article_))
        .body(site_footer(site, lang))
}

fn logo(site: &Site, lang: Lang) -> Element {
    a().class("logo")
        .href(site.url(&lang.path("/")))
        .attr("aria-label", lang.texts().home_label)
        .child(
            img()
                .attr("src", site.url("/assets/favicon.svg"))
                .attr("alt", "")
                .attr("width", "28")
                .attr("height", "28"),
        )
        .child(span().text("Alley"))
}

/// Les trois langues, chacune vers la même page.
fn languages(site: &Site, lang: Lang, path: &str) -> Element {
    let mut list = nav()
        .class("langs")
        .attr("aria-label", lang.texts().language);
    for other in LANGS {
        let mut link = a()
            .href(site.url(&other.path(path)))
            .attr("hreflang", other.code())
            .attr("lang", other.code())
            .attr("data-lang", other.code())
            .text(other.code().to_uppercase());
        if other == lang {
            link = link.attr("aria-current", "true");
        }
        list = list.child(link);
    }
    list
}

/// Une partie de la vitrine, depuis n'importe quelle page.
fn anchor(site: &Site, lang: Lang, id: &str) -> String {
    format!("{}#{id}", site.url(&lang.path("/")))
}

fn site_header(site: &Site, lang: Lang) -> Element {
    let t = lang.texts();
    header()
        .class("top")
        .child(logo(site, lang))
        .child(
            nav()
                .child(
                    a().href(anchor(site, lang, "features"))
                        .text(t.nav_features),
                )
                .child(
                    a().href(anchor(site, lang, "security"))
                        .text(t.nav_security),
                )
                .child(
                    a().href(anchor(site, lang, "pricing"))
                        .text(legal::labels(lang).pricing),
                )
                .child(a().href(anchor(site, lang, "install")).text(t.nav_install))
                .child(
                    a().class("button small")
                        .href(site.url(&lang.path("/app/")))
                        .text(t.open_app),
                ),
        )
        .child(languages(site, lang, "/"))
}

fn hero(site: &Site, lang: Lang) -> Element {
    let t = lang.texts();
    section()
        .class("hero")
        .child(
            div()
                .class("hero-text")
                .child(p().class("eyebrow").text(t.eyebrow))
                .child(h1().text(t.headline))
                .child(p().class("lead").text(t.lead))
                .child(
                    div()
                        .class("cta")
                        .child(
                            a().class("button")
                                .href(site.url(&lang.path("/app/")))
                                .text(t.cta_enter),
                        )
                        .child(a().class("button ghost").href("#security").text(t.cta_how)),
                )
                .child(p().class("hint").text(t.hero_hint)),
        )
        .child(hero_demo(lang))
}

/// La même conversation, vue par vous et vue par le serveur.
fn hero_demo(lang: Lang) -> Element {
    let t = lang.texts();
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
                .child(div().class("pane-title").text(t.demo_you))
                .child(bubble("Inès", "#7c5cff", t.demo_messages[0]))
                .child(bubble("Malo", "#22d3a6", t.demo_messages[1]))
                .child(bubble("Inès", "#7c5cff", t.demo_messages[2])),
        )
        .child(
            div()
                .class("pane server")
                .child(div().class("pane-title").text(t.demo_server))
                .child(cipher("[[BROCOLIS:v1:q9Xw3J0+Lr2c…Pz8=]]"))
                .child(cipher("[[BROCOLIS:v1:7HfK1aQe9mZ0…tY4=]]"))
                .child(cipher("[[BROCOLIS:v1:Wm2Lr8sB1cXn…Qa0=]]")),
        )
}

fn card((icon, title, text): (&'static str, &'static str, &'static str)) -> Element {
    article()
        .class("card")
        .child(span().class("icon").text(icon))
        .child(h3().text(title))
        .child(p().text(text))
}

fn features(lang: Lang) -> Element {
    let t = lang.texts();
    let mut grid = div().class("grid");
    for c in t.features {
        grid = grid.child(card(c));
    }
    section()
        .id("features")
        .class("section")
        .child(h2().text(t.features_title))
        .child(p().class("section-lead").text(t.features_lead))
        .child(grid)
}

fn security(lang: Lang) -> Element {
    let t = lang.texts();
    let mut steps = ol().class("steps");
    for (i, (title, text)) in t.steps.iter().enumerate() {
        steps = steps.child(
            li().class("step")
                .child(span().class("num").text((i + 1).to_string()))
                .child(div().child(h3().text(*title)).child(p().text(*text))),
        );
    }
    let mut tech = div().class("grid tech");
    for c in t.tech {
        tech = tech.child(card(c));
    }
    section()
        .id("security")
        .class("section")
        .child(h2().text(t.security_title))
        .child(steps)
        .child(tech)
}

fn platforms(lang: Lang) -> Element {
    let t = lang.texts();
    const OS: [(&str, &str); 6] = [
        ("ios", "📱"),
        ("android", "🤖"),
        ("windows", "🪟"),
        ("macos", "🍎"),
        ("linux", "🐧"),
        ("chromeos", "💻"),
    ];
    let mut grid = div().class("grid platforms");
    for ((id, icon), (name, how)) in OS.iter().zip(t.platforms) {
        grid = grid.child(
            article()
                .class("platform")
                .attr("data-os", *id)
                .child(span().class("icon").text(*icon))
                .child(h3().text(name))
                .child(p().text(how)),
        );
    }
    section()
        .id("install")
        .class("section")
        .child(h2().text(t.platforms_title))
        .child(p().class("section-lead").text(t.platforms_lead))
        .child(
            div()
                .class("install-now")
                .child(
                    button()
                        .id("install")
                        .class("button")
                        .attr("hidden", "")
                        .text(t.install),
                )
                .child(p().id("install-hint").class("hint").text("")),
        )
        .child(grid)
}

fn faq(lang: Lang) -> Element {
    let t = lang.texts();
    let mut section_ = section()
        .id("faq")
        .class("section narrow")
        .child(h2().text(t.faq_title));
    for (q, a_) in t.faq {
        section_ = section_.child(details().child(summary().text(q)).child(p().text(a_)));
    }
    section_
}

/// Le pied de page : la marque, puis le produit, l'aide et le légal, et les
/// langues.
fn site_footer(site: &Site, lang: Lang) -> Element {
    let t = lang.texts();
    let l = legal::labels(lang);
    let docs = legal::docs(lang);
    let page = |slug: &str| site.url(&lang.path(&format!("/{slug}/")));
    let column = |title: &'static str, links: Vec<Element>| {
        let mut list = ul();
        for link in links {
            list = list.child(li().child(link));
        }
        nav()
            .attr("aria-label", title)
            .child(h2().text(title))
            .child(list)
    };
    let product = column(
        l.product,
        vec![
            a().href(site.url(&lang.path("/app/"))).text(t.open_app),
            a().href(anchor(site, lang, "features"))
                .text(t.nav_features),
            a().href(anchor(site, lang, "security"))
                .text(t.nav_security),
            a().href(anchor(site, lang, "pricing")).text(l.pricing),
            a().href(anchor(site, lang, "install")).text(t.nav_install),
        ],
    );
    let help = column(
        l.help,
        vec![
            a().href(anchor(site, lang, "faq")).text(l.faq),
            a().href(page("rules")).text(docs[3].title),
            a().href(PUBLISHER.contact_href()).text(l.contact),
            a().href(format!("{}/api/v1/stats", site.api))
                .text(l.status),
            a().href(legal::SOURCE)
                .attr("rel", "noopener")
                .text(l.source),
        ],
    );
    let legal_ = column(
        l.legal,
        vec![
            a().href(page("legal")).text(docs[0].title),
            a().href(page("terms")).text(docs[1].title),
            a().href(page("privacy")).text(docs[2].title),
        ],
    );
    footer()
        .class("bottom")
        .child(
            div()
                .class("footer-grid")
                .child(
                    div()
                        .class("footer-brand")
                        .child(logo(site, lang))
                        .child(p().text(l.tagline))
                        .child(
                            a().class("button small")
                                .href(site.url(&lang.path("/app/")))
                                .text(t.cta_enter),
                        ),
                )
                .child(product)
                .child(help)
                .child(legal_),
        )
        .child(
            div()
                .class("footer-bar")
                .child(p().text(format!("© 2026 Alley. {}", l.rights)))
                .child(p().text(t.footer_made))
                .child(languages(site, lang, "/")),
        )
}

/// L'application : seulement la coquille ; le WebAssembly déchiffre et affiche tout
/// dans le navigateur.
pub fn app_shell(site: &Site, lang: Lang) -> Page {
    let t = lang.texts();
    Page::new("Alley")
        .lang(lang.code())
        .description(t.description)
        .seo(seo(site, lang, "/app/"))
        .plain()
        .head(head_tags(site, lang, "/app/"))
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
                .child(p().text(t.app_loading)),
        )
        .body(noscript().text(t.noscript))
}

/// La page 404, dans les trois langues à la fois (GitHub Pages n'en sert
/// qu'une).
pub fn not_found(site: &Site) -> Page {
    let mut hero_ = section().class("hero notfound");
    for lang in LANGS {
        let t = lang.texts();
        hero_ = hero_.child(
            div()
                .attr("lang", lang.code())
                .child(h2().text(t.notfound_title))
                .child(p().class("lead").text(t.notfound_lead))
                .child(
                    a().class("button")
                        .href(site.url(&lang.path("/")))
                        .text(t.notfound_back),
                ),
        );
    }
    Page::new("404 — Alley")
        .lang("fr")
        .plain()
        .head(head_tags(site, Lang::Fr, "/"))
        .stylesheet(versioned(site, "/assets/site.css"))
        .seo(Seo::new().noindex(true))
        .status(404)
        .body(site_header(site, Lang::Fr))
        .body(hero_)
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

/// Les pages, servies pour `site`, dans chaque langue.
pub fn routes(app: &mut App, site: &Site) {
    for lang in LANGS {
        let s = site.clone();
        let home_path = match lang {
            Lang::Fr => "/".to_owned(),
            other => format!("{}/", other.prefix()),
        };
        app.get(&home_path, move |req: Request| {
            let s = s.clone();
            async move { respond(home(&s, lang), &s, &req) }
        });
        for (i, doc) in legal::docs(lang).iter().enumerate() {
            let s = site.clone();
            app.get(
                &lang.path(&format!("/{}/", doc.slug)),
                move |req: Request| {
                    let s = s.clone();
                    async move { respond(legal_page(&s, lang, &legal::docs(lang)[i]), &s, &req) }
                },
            );
        }
        // `/app` y mène aussi (Vitesse ignore la barre finale).
        let s = site.clone();
        app.get(&lang.path("/app/"), move |req: Request| {
            let s = s.clone();
            async move { respond(app_shell(&s, lang), &s, &req) }
        });
    }
    let s = site.clone();
    app.fallback(move |req: Request| {
        let s = s.clone();
        async move {
            if req.path().starts_with("/api/") {
                return Response::new()
                    .status(404)
                    .json(json!({ "error": "not_found" }));
            }
            respond(not_found(&s), &s, &req)
        }
    });
}
