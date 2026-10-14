//! Ce que font les workflows GitHub Actions, en Rust : les fichiers YAML ne
//! font qu'appeler `cargo xtask …`.
//!
//! - `merge` : fusionne la branche poussée dans `master` (CI verte), puis
//!   lance le déploiement (un push fait avec `GITHUB_TOKEN` ne déclenche
//!   aucun workflow) ;
//! - `heroku` : construit et publie le serveur sur Heroku par l'API Platform
//!   (`HEROKU_API_KEY`, `HEROKU_APP_NAME`, `HEROKU_REGION`) ;
//! - `pages-enabled` : GitHub Pages est-il activé (source : Actions) ?
//! - `api-url` : l'adresse de l'API pour la version GitHub Pages
//!   (`ALLEY_API_URL`, sinon l'app Heroku).

use std::process::Command;
use std::time::Duration;

use serde_json::{Value, json};

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

fn git(args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .args(args)
        .output()
        .map_err(|e| format!("git : {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git {} : {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// Une sortie d'étape (`$GITHUB_OUTPUT`) ou un résumé (`$GITHUB_STEP_SUMMARY`).
fn append(var: &str, text: &str) {
    if let Some(path) = env(var) {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
        {
            let _ = writeln!(f, "{text}");
        }
    }
}

fn warning(title: &str, text: &str) {
    println!("::warning title={title}::{text}");
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(120)))
        .build()
        .into()
}

/// Une requête à l'API GitHub, avec le jeton du workflow.
fn github(method: &str, path: &str, body: Option<Value>) -> Result<(u16, Value), String> {
    let token = env("GH_TOKEN")
        .or_else(|| env("GITHUB_TOKEN"))
        .ok_or("GH_TOKEN manque")?;
    let url = format!("https://api.github.com{path}");
    let agent = agent();
    let auth = format!("Bearer {token}");
    let res = match (method, body) {
        ("GET", _) => agent
            .get(&url)
            .header("Authorization", &auth)
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "alley-xtask")
            .call(),
        (_, body) => agent
            .post(&url)
            .header("Authorization", &auth)
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "alley-xtask")
            .send_json(body.unwrap_or(json!({}))),
    }
    .map_err(|e| e.to_string())?;
    let status = res.status().as_u16();
    let text = res.into_body().read_to_string().unwrap_or_default();
    Ok((status, serde_json::from_str(&text).unwrap_or(Value::Null)))
}

fn repository() -> Result<String, String> {
    env("GITHUB_REPOSITORY").ok_or_else(|| "GITHUB_REPOSITORY manque".into())
}

/// Lance un workflow sur `master`, en réessayant : juste après la fusion,
/// GitHub peut ne pas encore connaître un workflow ajouté par la branche.
pub fn dispatch(workflow: &str) -> Result<(), String> {
    let repo = repository()?;
    for attempt in 1..=6 {
        let (status, body) = github(
            "POST",
            &format!("/repos/{repo}/actions/workflows/{workflow}/dispatches"),
            Some(json!({ "ref": "master" })),
        )?;
        if status == 204 {
            println!("{workflow} lancé.");
            return Ok(());
        }
        println!("{workflow} : {status} {body} (essai {attempt})");
        std::thread::sleep(Duration::from_secs(10));
    }
    warning(
        "Déploiement",
        &format!("impossible de lancer {workflow} : Actions → {workflow} → Run workflow."),
    );
    Ok(())
}

pub fn merge() -> Result<(), String> {
    let branch = env("BRANCH").ok_or("BRANCH manque")?;
    let sha = env("SHA").ok_or("SHA manque")?;
    git(&["config", "user.name", "github-actions[bot]"])?;
    git(&[
        "config",
        "user.email",
        "41898282+github-actions[bot]@users.noreply.github.com",
    ])?;
    if git(&["merge-base", "--is-ancestor", &sha, "HEAD"]).is_ok() {
        println!("{branch} est déjà dans master.");
    } else {
        let message = format!("Merge branch '{branch}' into master (fusion automatique)");
        git(&["merge", "--no-ff", "--no-edit", &sha, "-m", &message])?;
        git(&["push", "origin", "HEAD:master"])?;
        println!("::notice title=Fusion automatique::{branch} fusionnée dans master");
    }
    dispatch("heroku.yml")?;
    dispatch("pages.yml")
}

pub fn pages_enabled() -> Result<(), String> {
    let repo = repository()?;
    let (status, body) = github("GET", &format!("/repos/{repo}/pages"), None)?;
    let enabled = status == 200 && body["build_type"] == "workflow";
    append("GITHUB_OUTPUT", &format!("enabled={enabled}"));
    if !enabled {
        warning(
            "GitHub Pages est désactivé",
            "Settings → Pages → Source : GitHub Actions, puis relancez ce workflow.",
        );
        append(
            "GITHUB_STEP_SUMMARY",
            "### GitHub Pages n'est pas encore activé\nSettings → Pages → Build and deployment → Source : **GitHub Actions**, puis Actions → Pages → Run workflow.",
        );
    }
    Ok(())
}

/// Une requête à l'API Platform de Heroku.
fn heroku(method: &str, path: &str, body: Option<Value>) -> Result<(u16, Value), String> {
    let key = env("HEROKU_API_KEY").ok_or("HEROKU_API_KEY manque")?;
    let url = format!("https://api.heroku.com{path}");
    let agent = agent();
    let auth = format!("Bearer {key}");
    let accept = "application/vnd.heroku+json; version=3";
    let res = match method {
        "GET" => agent
            .get(&url)
            .header("Authorization", &auth)
            .header("Accept", accept)
            .call(),
        "PATCH" => agent
            .patch(&url)
            .header("Authorization", &auth)
            .header("Accept", accept)
            .send_json(body.unwrap_or(json!({}))),
        _ => agent
            .post(&url)
            .header("Authorization", &auth)
            .header("Accept", accept)
            .send_json(body.unwrap_or(json!({}))),
    }
    .map_err(|e| e.to_string())?;
    let status = res.status().as_u16();
    let text = res.into_body().read_to_string().unwrap_or_default();
    Ok((status, serde_json::from_str(&text).unwrap_or(Value::Null)))
}

pub fn api_url() -> Result<(), String> {
    let mut url = env("ALLEY_API_URL").unwrap_or_default();
    if url.is_empty()
        && env("HEROKU_API_KEY").is_some()
        && let Some(app) = env("HEROKU_APP_NAME")
        && let Ok((200, info)) = heroku("GET", &format!("/apps/{app}"), None)
    {
        url = info["web_url"].as_str().unwrap_or_default().to_owned();
    }
    let url = url.trim_end_matches('/');
    if url.is_empty() {
        warning(
            "Pas d'API configurée",
            "L'app demandera l'adresse du serveur Alley. Déployez l'API (workflow Heroku) ou définissez la variable ALLEY_API_URL.",
        );
    }
    append("GITHUB_OUTPUT", &format!("url={url}"));
    println!("API : {}", if url.is_empty() { "(aucune)" } else { url });
    Ok(())
}

pub fn deploy_heroku() -> Result<(), String> {
    let (Some(_), Some(app)) = (env("HEROKU_API_KEY"), env("HEROKU_APP_NAME")) else {
        warning(
            "Heroku non configuré",
            "Ajoutez le secret HEROKU_API_KEY et la variable HEROKU_APP_NAME (Settings → Secrets and variables → Actions) pour mettre l'API Alley en ligne.",
        );
        append(
            "GITHUB_STEP_SUMMARY",
            "### Heroku n'est pas configuré\nAjoutez le secret **HEROKU_API_KEY** et la variable **HEROKU_APP_NAME**, puis relancez ce workflow.",
        );
        return Ok(());
    };
    let region = env("HEROKU_REGION").unwrap_or_else(|| "eu".into());
    let (status, info) = heroku("GET", &format!("/apps/{app}"), None)?;
    let info = match status {
        200 => info,
        404 => {
            println!("Création de l'app {app} ({region})…");
            let (status, info) = heroku(
                "POST",
                "/apps",
                Some(json!({ "name": app, "region": region, "stack": "container" })),
            )?;
            if status >= 300 {
                return Err(format!("création refusée ({status}) : {info}"));
            }
            info
        }
        401 => return Err("Heroku refuse la clé d'API (HEROKU_API_KEY).".into()),
        403 => {
            return Err(format!(
                "ce compte Heroku ne peut pas utiliser l'app « {app} » (nom déjà pris ?)."
            ));
        }
        other => return Err(format!("réponse inattendue de Heroku ({other}) : {info}")),
    };
    if info["build_stack"]["name"] != "container" {
        heroku(
            "PATCH",
            &format!("/apps/{app}"),
            Some(json!({ "build_stack": "container" })),
        )?;
    }
    // La version GitHub Pages a le droit d'appeler l'API.
    if let Some(owner) = env("GITHUB_REPOSITORY_OWNER") {
        heroku(
            "PATCH",
            &format!("/apps/{app}/config-vars"),
            Some(
                json!({ "ALLEY_ALLOWED_ORIGINS": format!("https://{}.github.io", owner.to_lowercase()) }),
            ),
        )?;
    }
    // Les sources, telles que dans le dépôt.
    let archive = std::env::temp_dir().join("alley-source.tgz");
    git(&[
        "archive",
        "--format=tar.gz",
        "-o",
        archive.to_str().unwrap_or("alley-source.tgz"),
        "HEAD",
    ])?;
    let (_, sources) = heroku("POST", "/sources", None)?;
    let put = sources["source_blob"]["put_url"]
        .as_str()
        .ok_or("put_url")?;
    let get = sources["source_blob"]["get_url"]
        .as_str()
        .ok_or("get_url")?;
    let bytes = std::fs::read(&archive).map_err(|e| e.to_string())?;
    let res = agent()
        .put(put)
        .header("Content-Type", "")
        .send(&bytes[..])
        .map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(format!("envoi des sources : {}", res.status()));
    }
    println!("Construction sur Heroku (la première prend plusieurs minutes)…");
    let version = env("GITHUB_SHA").unwrap_or_default();
    let (status, build) = heroku(
        "POST",
        &format!("/apps/{app}/builds"),
        Some(json!({ "source_blob": { "url": get, "version": version } })),
    )?;
    if status >= 300 {
        return Err(format!("construction refusée ({status}) : {build}"));
    }
    let id = build["id"].as_str().ok_or("build id")?.to_owned();
    if let Some(stream) = build["output_stream_url"].as_str()
        && let Ok(res) = agent().get(stream).call()
    {
        let mut body = res.into_body();
        let mut reader = body.as_reader();
        let _ = std::io::copy(&mut reader, &mut std::io::stdout());
    }
    let mut result = "pending".to_owned();
    for _ in 0..240 {
        let (_, b) = heroku("GET", &format!("/apps/{app}/builds/{id}"), None)?;
        result = b["status"].as_str().unwrap_or("pending").to_owned();
        if result != "pending" {
            break;
        }
        std::thread::sleep(Duration::from_secs(5));
    }
    if result != "succeeded" {
        return Err(format!(
            "la construction Heroku s'est terminée avec le statut « {result} »"
        ));
    }
    let (_, info) = heroku("GET", &format!("/apps/{app}"), None)?;
    let url = info["web_url"].as_str().unwrap_or_default();
    append(
        "GITHUB_STEP_SUMMARY",
        &format!("### 🚀 API Alley en ligne : [{url}]({url})"),
    );
    println!("En ligne : {url}");
    // Le site GitHub Pages doit connaître l'adresse de l'API : on le republie.
    dispatch("pages.yml")
}
