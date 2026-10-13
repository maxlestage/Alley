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

    for (from, to) in [
        ("/", "index.html"),
        ("/app/", "app/index.html"),
        ("/404", "404.html"),
    ] {
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
    for (path, _, bytes) in assets::FILES {
        write(path, bytes)?;
    }
    write(
        assets::MANIFEST_URL,
        assets::manifest(site).to_json().as_bytes(),
    )?;
    write(assets::WORKER_URL, assets::worker(site).as_bytes())?;
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
