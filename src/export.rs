//! L'export statique pour **GitHub Pages** : les mêmes pages et fichiers que
//! le serveur, sous le chemin du dépôt (`/Alley`), et une app qui parle à
//! l'API d'un serveur Alley déployé ailleurs (Heroku…), en HTTPS.
//!
//! ```sh
//! BASE_PATH=/Alley ALLEY_API_URL=https://alley.herokuapp.com alley export dist
//! ```
//!
//! GitHub Pages n'envoie pas d'en-têtes sur mesure : la politique de sécurité
//! de chaque page est donc écrite dans la page (`<meta http-equiv>`).

use std::path::Path;

use paquets::App;

use crate::i18n::LANGS;
use crate::{Site, assets, pages};

/// Le site pour GitHub Pages, d'après `BASE_PATH` et `ALLEY_API_URL`.
pub fn site_from_env() -> Site {
    let clean = |name: &str| {
        std::env::var(name)
            .unwrap_or_default()
            .trim()
            .trim_end_matches('/')
            .to_owned()
    };
    let base = clean("BASE_PATH");
    let base = if base.is_empty() || base.starts_with('/') {
        base
    } else {
        format!("/{base}")
    };
    Site {
        base,
        api: clean("ALLEY_API_URL"),
    }
}

/// Écrit le site dans `dir`.
pub async fn run(dir: &Path, site: &Site) -> std::io::Result<()> {
    let mut app = App::new();
    app.security(None);
    app.quiet();
    assets::routes(&mut app, site);
    pages::routes(&mut app, site);
    let client = app.test().await;

    let write = |path: &str, bytes: &[u8]| -> std::io::Result<()> {
        let file = dir.join(path.trim_start_matches('/'));
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(file, bytes)
    };

    let mut pages = vec![("/404".to_owned(), "404.html".to_owned())];
    for lang in LANGS {
        let home = lang.path("/");
        let app = lang.path("/app/");
        pages.push((
            home.clone(),
            format!("{}index.html", home.trim_start_matches('/')),
        ));
        pages.push((
            app.clone(),
            format!("{}index.html", app.trim_start_matches('/')),
        ));
        for doc in crate::legal::docs(lang) {
            let path = lang.path(&format!("/{}/", doc.slug));
            pages.push((
                path.clone(),
                format!("{}index.html", path.trim_start_matches('/')),
            ));
        }
    }
    for (from, to) in &pages {
        let res = client.get(from).await;
        let csp = res
            .header("content-security-policy")
            .unwrap_or_default()
            // Sans effet dans une balise <meta> (et signalé en console).
            .replace("; frame-ancestors 'none'", "");
        let html = res.text().replacen(
            "<head>",
            &format!(
                r#"<head><meta http-equiv="Content-Security-Policy" content="{}"/>"#,
                csp.replace('"', "&quot;")
            ),
            1,
        );
        write(to, html.as_bytes())?;
    }
    for (path, _, bytes) in assets::files() {
        write(path, bytes)?;
    }
    for lang in LANGS {
        write(
            &assets::manifest_url(lang),
            assets::manifest(site, lang).to_json().as_bytes(),
        )?;
    }
    // Les fichiers commençant par « _ » ou « . » passent tels quels.
    write(".nojekyll", b"")?;
    println!(
        "Site exporté dans {} (chemin « {} », API « {} »).",
        dir.display(),
        if site.base.is_empty() {
            "/"
        } else {
            &site.base
        },
        if site.api.is_empty() {
            "même site"
        } else {
            &site.api
        }
    );
    Ok(())
}
