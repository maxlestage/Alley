//! L'API de bout en bout, avec la même application que la production
//! (`alley::app().test()`), et des jetons chiffrés par le code même du
//! navigateur (`alley_web::crypto`, compilé ici sans WebAssembly).

use alley_web::crypto;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use paquets::serde_json::{Value, json};
use paquets::test::TestClient;
use sha2::{Digest, Sha256};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha(parts: &[&[u8]]) -> [u8; 32] {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

fn sha_hex(text: &str) -> String {
    hex(&sha(&[text.as_bytes()]))
}

/// Un serveur vu par un navigateur : sa clé actuelle et ce qui en dérive.
struct Guild {
    key: [u8; 32],
    gid: String,
    access: String,
    admin: String,
}

impl Guild {
    fn new(seed: u8) -> Guild {
        let key = [seed; 32];
        Guild {
            key,
            gid: hex(&sha(&[b"brocolis-conv-v1", &key]))[..32].to_owned(),
            access: access_of(&key),
            admin: hex(&sha(&[b"admin", &key])),
        }
    }

    /// Un jeton `[[BROCOLIS:v1:…]]`, comme `sealToken` dans le navigateur.
    fn seal(&self, text: &str) -> String {
        seal(&self.key, text)
    }

    fn open(&self, token: &str) -> String {
        crypto::open_token(&self.key, token).expect("jeton de ce serveur")
    }
}

fn access_of(key: &[u8; 32]) -> String {
    hex(&sha(&[b"alley-access-v1", key]))
}

/// Un jeton `[[BROCOLIS:v1:…]]`, chiffré par le code même du navigateur.
fn seal(key: &[u8; 32], text: &str) -> String {
    crypto::seal_token(key, text)
}

fn member(n: u8) -> String {
    hex(&[n; 32])
}

/// L'identifiant public d'un membre, comme le calcule le navigateur.
fn member_id(secret: &str) -> String {
    sha_hex(secret)[..16].to_owned()
}

async fn client() -> TestClient {
    alley::app().test().await
}

async fn create(client: &TestClient, g: &Guild) -> Value {
    let res = client
        .post("/api/v1/guilds")
        .json(&json!({
            "gid": g.gid,
            "access": g.access,
            "admin": g.admin,
            "state": g.seal(r#"{"name":"Les copains","channels":[],"keys":[]}"#),
        }))
        .await;
    assert_eq!(res.status(), 201, "{}", res.text());
    res.json()
}

async fn join(client: &TestClient, g: &Guild, secret: &str, name: &str) -> u16 {
    client
        .put(&format!("/api/v1/guilds/{}/members/me", g.gid))
        .header("x-alley-access", &g.access)
        .header("x-alley-member", secret)
        .json(&json!({ "profile": g.seal(name) }))
        .await
        .status()
        .as_u16()
}

/// Un message : le jeton, et l'empreinte de la preuve de son auteur.
async fn post(client: &TestClient, g: &Guild, text: &str, proof: &str) -> i64 {
    let res = client
        .post(&format!("/api/v1/messages/{}", g.gid))
        .header("x-alley-access", &g.access)
        .json(&json!({ "body": g.seal(text), "owner": sha_hex(proof) }))
        .await;
    assert_eq!(res.status(), 201, "{}", res.text());
    res.json::<Value>()["id"].as_i64().unwrap()
}

#[tokio::test]
async fn messages_reveal_nothing_and_only_their_author_edits_them() {
    let client = client().await;
    let g = Guild::new(1);
    let created = create(&client, &g).await;
    assert_eq!(created["epoch"], 0);
    assert_eq!(join(&client, &g, &member(0xa1), "Ada").await, 200);

    // L'auteur garde sa preuve pour lui ; le serveur n'en a que l'empreinte.
    let proof = sha_hex(&format!("{}:sel", member(0xa1)));
    let mid = post(&client, &g, "salut 👋", &proof).await;

    let page: Value = client
        .get(&format!("/api/v1/messages/{}", g.gid))
        .header("x-alley-access", &g.access)
        .await
        .json();
    let first = &page["messages"][0];
    assert_eq!(g.open(first["body"].as_str().unwrap()), "salut 👋");
    // Ni auteur, ni salon, ni heure : seulement un numéro et un jeton.
    let keys: Vec<&str> = first
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys.len(), 3, "{keys:?}");
    assert!(keys.contains(&"id") && keys.contains(&"body") && keys.contains(&"edited"));

    // Une mauvaise preuve ne modifie ni ne supprime rien.
    let res = client
        .patch(&format!("/api/v1/messages/{}/{mid}", g.gid))
        .header("x-alley-access", &g.access)
        .json(&json!({ "body": g.seal("piraté"), "proof": sha_hex("autre") }))
        .await;
    assert_eq!(res.status(), 403);
    let res = client
        .delete(&format!("/api/v1/messages/{}/{mid}", g.gid))
        .header("x-alley-access", &g.access)
        .header("x-alley-proof", &sha_hex("autre"))
        .await;
    assert_eq!(res.status(), 403);

    // La bonne, oui.
    let res = client
        .patch(&format!("/api/v1/messages/{}/{mid}", g.gid))
        .header("x-alley-access", &g.access)
        .json(&json!({ "body": g.seal("salut à tous 👋"), "proof": proof }))
        .await;
    assert_eq!(res.status(), 200, "{}", res.text());
    assert_eq!(res.json::<Value>()["edited"], true);
    let res = client
        .delete(&format!("/api/v1/messages/{}/{mid}", g.gid))
        .header("x-alley-access", &g.access)
        .header("x-alley-proof", &proof)
        .await;
    assert_eq!(res.status(), 204);

    // L'admin modère sans preuve.
    let other = post(&client, &g, "spam", &sha_hex("x")).await;
    let res = client
        .delete(&format!("/api/v1/messages/{}/{other}", g.gid))
        .header("x-alley-access", &g.access)
        .header("x-alley-admin", &g.admin)
        .await;
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn plaintext_is_refused_everywhere() {
    let client = client().await;
    let g = Guild::new(2);
    let res = client
        .post("/api/v1/guilds")
        .json(
            &json!({ "gid": g.gid, "access": g.access, "admin": g.admin, "state": "Mon serveur" }),
        )
        .await;
    assert_eq!(res.status(), 422);
    create(&client, &g).await;
    for body in [
        "bonjour".to_owned(),
        format!("bonjour {}", g.seal("x")),
        "[[BROCOLIS:v1:pas du base64 !]]".to_owned(),
        "[[BROCOLIS:v1:AAAA]]".to_owned(),
    ] {
        let res = client
            .post(&format!("/api/v1/messages/{}", g.gid))
            .header("x-alley-access", &g.access)
            .json(&json!({ "body": body, "owner": sha_hex("p") }))
            .await;
        assert_eq!(res.status(), 422, "accepté : {body}");
    }
    let res = client
        .put(&format!("/api/v1/guilds/{}/members/me", g.gid))
        .header("x-alley-access", &g.access)
        .header("x-alley-member", &member(1))
        .json(&json!({ "profile": "Ada" }))
        .await;
    assert_eq!(res.status(), 422);
    let res = client
        .put(&format!("/api/v1/guilds/{}/state", g.gid))
        .header("x-alley-access", &g.access)
        .header("x-alley-admin", &g.admin)
        .json(&json!({ "state": "{\"name\":\"en clair\"}", "revision": 1 }))
        .await;
    assert_eq!(res.status(), 422);
}

#[tokio::test]
async fn without_the_key_a_guild_does_not_exist() {
    let client = client().await;
    let g = Guild::new(3);
    create(&client, &g).await;
    let other = Guild::new(4);
    for access in [None, Some(other.access.as_str()), Some("zz")] {
        let mut req = client.get(&format!("/api/v1/guilds/{}", g.gid));
        if let Some(a) = access {
            req = req.header("x-alley-access", a);
        }
        assert_eq!(req.await.status(), 404);
        let mut req = client.get(&format!("/api/v1/messages/{}", g.gid));
        if let Some(a) = access {
            req = req.header("x-alley-access", a);
        }
        assert_eq!(req.await.status(), 404);
    }
    // Le même identifiant ne peut pas être repris.
    let res = client
        .post("/api/v1/guilds")
        .json(&json!({ "gid": g.gid, "access": other.access, "admin": other.admin, "state": other.seal("{}") }))
        .await;
    assert_eq!(res.status(), 409);
}

#[tokio::test]
async fn the_state_is_admin_only_and_never_overwritten_blindly() {
    let client = client().await;
    let g = Guild::new(5);
    create(&client, &g).await;
    let path = format!("/api/v1/guilds/{}/state", g.gid);
    let res = client
        .put(&path)
        .header("x-alley-access", &g.access)
        .json(&json!({ "state": g.seal("{}"), "revision": 1 }))
        .await;
    assert_eq!(res.status(), 403);
    let res = client
        .put(&path)
        .header("x-alley-access", &g.access)
        .header("x-alley-admin", &g.admin)
        .json(&json!({ "state": g.seal(r#"{"name":"Le QG"}"#), "revision": 1 }))
        .await;
    assert_eq!(res.status(), 200, "{}", res.text());
    assert_eq!(res.json::<Value>()["revision"], 2);
    // Une modification faite sur une version périmée est refusée.
    let res = client
        .put(&path)
        .header("x-alley-access", &g.access)
        .header("x-alley-admin", &g.admin)
        .json(&json!({ "state": g.seal(r#"{"name":"Vieux"}"#), "revision": 1 }))
        .await;
    assert_eq!(res.status(), 409);
    let info: Value = client
        .get(&format!("/api/v1/guilds/{}", g.gid))
        .header("x-alley-access", &g.access)
        .header("x-alley-admin", &g.admin)
        .await
        .json();
    assert_eq!(info["admin"], true);
    assert_eq!(
        g.open(info["state"].as_str().unwrap()),
        r#"{"name":"Le QG"}"#
    );
}

#[tokio::test]
async fn excluding_a_member_rotates_the_key_and_bans_them() {
    let client = client().await;
    let g = Guild::new(6);
    create(&client, &g).await;
    let (ada, eve) = (member(0xa1), member(0xee));
    assert_eq!(join(&client, &g, &ada, "Ada").await, 200);
    assert_eq!(join(&client, &g, &eve, "Ève").await, 200);
    let new_key = [0x66; 32];
    let new_access = access_of(&new_key);
    let rotation = json!({
        "epoch": 1,
        "access": new_access,
        "state": seal(&new_key, "{\"keys\":[\"ancienne\"]}"),
        "remove": [member_id(&eve)],
        "shares": [{
            "member": member_id(&ada),
            "blob": STANDARD.encode([1u8; 60]),
            "sender": STANDARD.encode([2u8; 32]),
        }],
    });
    let path = format!("/api/v1/guilds/{}/rotate", g.gid);

    // Seul l'admin renouvelle la clé.
    let res = client
        .post(&path)
        .header("x-alley-access", &g.access)
        .json(&rotation)
        .await;
    assert_eq!(res.status(), 403);
    // Une clé emballée pour l'exclu est refusée.
    let mut bad = rotation.clone();
    bad["shares"][0]["member"] = member_id(&eve).into();
    let res = client
        .post(&path)
        .header("x-alley-access", &g.access)
        .header("x-alley-admin", &g.admin)
        .json(&bad)
        .await;
    assert_eq!(res.status(), 400);
    let res = client
        .post(&path)
        .header("x-alley-access", &g.access)
        .header("x-alley-admin", &g.admin)
        .json(&rotation)
        .await;
    assert_eq!(res.status(), 200, "{}", res.text());
    assert_eq!(res.json::<Value>()["epoch"], 1);

    // L'ancienne clé ne mène plus nulle part.
    let res = client
        .get(&format!("/api/v1/guilds/{}", g.gid))
        .header("x-alley-access", &g.access)
        .await;
    assert_eq!(res.status(), 404);
    // Ada retrouve la nouvelle, emballée pour elle (sans jeton d'accès).
    let res = client
        .get(&format!("/api/v1/guilds/{}/keys", g.gid))
        .header("x-alley-member", &ada)
        .await;
    assert_eq!(res.status(), 200, "{}", res.text());
    let share: Value = res.json();
    assert_eq!(share["epoch"], 1);
    assert_eq!(share["blob"], STANDARD.encode([1u8; 60]));
    // Ève, exclue, n'a rien, et ne peut pas revenir même avec la nouvelle clé.
    let res = client
        .get(&format!("/api/v1/guilds/{}/keys", g.gid))
        .header("x-alley-member", &eve)
        .await;
    assert_eq!(res.status(), 403);
    let rotated = Guild {
        key: new_key,
        gid: g.gid.clone(),
        access: new_access,
        admin: g.admin.clone(),
    };
    assert_eq!(join(&client, &rotated, &eve, "Ève").await, 403);
    let info: Value = client
        .get(&format!("/api/v1/guilds/{}", g.gid))
        .header("x-alley-access", &rotated.access)
        .await
        .json();
    let ids: Vec<&str> = info["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, [member_id(&ada)]);
    // Rejouer la même rotation ne passe pas.
    let res = client
        .post(&path)
        .header("x-alley-access", &rotated.access)
        .header("x-alley-admin", &g.admin)
        .json(&rotation)
        .await;
    assert_eq!(res.status(), 409);
}

#[tokio::test]
async fn history_is_paged_from_the_newest() {
    let client = client().await;
    let g = Guild::new(7);
    create(&client, &g).await;
    for i in 0..7 {
        post(&client, &g, &format!("message {i}"), "p").await;
    }
    let page: Value = client
        .get(&format!("/api/v1/messages/{}?limit=3", g.gid))
        .header("x-alley-access", &g.access)
        .await
        .json();
    let texts: Vec<String> = page["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| g.open(m["body"].as_str().unwrap()))
        .collect();
    assert_eq!(texts, ["message 4", "message 5", "message 6"]);
    assert_eq!(page["more"], true);
    let oldest = page["messages"][0]["id"].as_i64().unwrap();
    let older: Value = client
        .get(&format!(
            "/api/v1/messages/{}?limit=3&before={oldest}",
            g.gid
        ))
        .header("x-alley-access", &g.access)
        .await
        .json();
    assert_eq!(
        g.open(older["messages"][2]["body"].as_str().unwrap()),
        "message 3"
    );
}

#[tokio::test]
async fn leaving_and_deleting() {
    let client = client().await;
    let g = Guild::new(8);
    create(&client, &g).await;
    assert_eq!(join(&client, &g, &member(1), "Ada").await, 200);
    let res = client
        .delete(&format!("/api/v1/guilds/{}/members/me", g.gid))
        .header("x-alley-access", &g.access)
        .header("x-alley-member", &member(1))
        .await;
    assert_eq!(res.status(), 204);
    let res = client
        .delete(&format!("/api/v1/guilds/{}", g.gid))
        .header("x-alley-access", &g.access)
        .await;
    assert_eq!(res.status(), 403);
    let res = client
        .delete(&format!("/api/v1/guilds/{}", g.gid))
        .header("x-alley-access", &g.access)
        .header("x-alley-admin", &g.admin)
        .await;
    assert_eq!(res.status(), 204);
    let res = client
        .get(&format!("/api/v1/guilds/{}", g.gid))
        .header("x-alley-access", &g.access)
        .await;
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn github_pages_may_call_the_api_and_http_goes_to_https() {
    let client = client().await;
    // Le pré-vol CORS de la version GitHub Pages.
    let res = client
        .request(paquets::vitesse::Method::OPTIONS, "/api/v1/guilds")
        .header("origin", "https://maxlestage.github.io")
        .header("access-control-request-method", "POST")
        .header(
            "access-control-request-headers",
            "content-type,x-alley-access",
        )
        .await;
    assert!(res.status().as_u16() < 300, "{}", res.status());
    assert_eq!(
        res.header("access-control-allow-origin"),
        Some("https://maxlestage.github.io")
    );
    let allowed = res
        .header("access-control-allow-headers")
        .unwrap_or_default()
        .to_lowercase();
    assert!(allowed.contains("x-alley-access"), "{allowed}");
    // Et l'écriture elle-même, venue de ce site.
    let g = Guild::new(9);
    let res = client
        .post("/api/v1/guilds")
        .header("origin", "https://maxlestage.github.io")
        .header("sec-fetch-site", "cross-site")
        .header("sec-fetch-mode", "cors")
        .json(&json!({ "gid": g.gid, "access": g.access, "admin": g.admin, "state": g.seal("{}") }))
        .await;
    assert_eq!(res.status(), 201, "{}", res.text());
    // Un autre site n'écrit pas.
    let g = Guild::new(10);
    let res = client
        .post("/api/v1/guilds")
        .header("origin", "https://evil.example")
        .header("sec-fetch-site", "cross-site")
        .header("sec-fetch-mode", "cors")
        .json(&json!({ "gid": g.gid, "access": g.access, "admin": g.admin, "state": g.seal("{}") }))
        .await;
    assert_eq!(res.status(), 403);
    // Derrière le routeur de Heroku, le HTTP est renvoyé vers HTTPS.
    let res = client
        .get("/app/")
        .header("x-forwarded-proto", "http")
        .header("host", "alley.example")
        .await;
    assert!(res.status().is_redirection(), "statut {}", res.status());
    assert!(
        res.header("location")
            .unwrap_or_default()
            .starts_with("https://")
    );
}

#[tokio::test]
async fn the_site_and_the_installable_app_are_served() {
    let client = client().await;
    let home = client.get("/").await;
    assert_eq!(home.status(), 200);
    let html = home.text();
    assert!(html.contains("Discord"));
    assert!(html.contains(r#"<link rel="manifest" href="/manifest.webmanifest"/>"#));
    assert!(html.contains("apple-mobile-web-app-capable"));
    assert!(html.contains("viewport-fit=cover"));

    let app = client.get("/app/").await;
    assert_eq!(app.status(), 200);
    let csp = app
        .header("content-security-policy")
        .unwrap_or_default()
        .to_owned();
    assert!(csp.contains("script-src 'self'"), "{csp}");
    assert!(csp.contains("media-src 'self' data: blob:"), "{csp}");
    let permissions = app.header("permissions-policy").unwrap_or_default();
    assert!(permissions.contains("microphone=(self)") && permissions.contains("camera=(self)"));
    let shell = app.text();
    assert!(
        shell.contains(r#"import init from "/assets/alley_web.js""#),
        "{shell}"
    );
    assert!(shell.contains(r#"<meta name="alley-page" content="app"/>"#));
    assert!(
        !shell.contains("app.js"),
        "plus de JavaScript écrit à la main"
    );
    assert_eq!(client.get("/app").await.status(), 200);

    let manifest: Value = client.get("/manifest.webmanifest").await.json();
    assert_eq!(manifest["start_url"], "/app/");
    assert_eq!(manifest["display"], "standalone");
    assert!(manifest["icons"].as_array().unwrap().len() >= 3);

    let worker = client.get("/sw.js").await.text();
    assert!(worker.contains(r#"const NETWORK_ONLY = ["/api/"];"#));
    assert!(worker.contains("/assets/alley_web_bg.wasm"));

    for asset in [
        "/assets/alley_web.js",
        "/assets/alley_web_bg.wasm",
        "/assets/app.css",
        "/assets/icon-192.png",
    ] {
        assert_eq!(client.get(asset).await.status(), 200, "{asset}");
    }
    assert_eq!(client.get("/nulle-part").await.status(), 404);
    assert_eq!(client.get("/api/v1/inconnue").await.status(), 404);
    let ice: Value = client.get("/api/v1/ice").await.json();
    assert!(!ice["iceServers"].as_array().unwrap().is_empty());
    let stats: Value = client.get("/api/v1/stats").await.json();
    assert_eq!(stats["plaintext_stored"], 0);
}

#[tokio::test]
async fn the_github_pages_export_is_complete() {
    let dir = std::env::temp_dir().join(format!("alley-export-{}", std::process::id()));
    let site = alley::Site {
        base: "/Alley".into(),
        api: "https://alley.example".into(),
    };
    alley::export::run(&dir, &site).await.unwrap();
    let read = |p: &str| std::fs::read_to_string(dir.join(p)).unwrap();
    let app = read("app/index.html");
    assert!(app.contains(r#"<meta http-equiv="Content-Security-Policy""#));
    assert!(app.contains("connect-src 'self' https://alley.example wss://alley.example"));
    assert!(app.contains(r#"import init from "/Alley/assets/alley_web.js""#));
    assert!(app.contains(r#"<meta name="alley-api" content="https://alley.example"/>"#));
    assert!(read("index.html").contains(r#"href="/Alley/app/""#));
    assert!(read("404.html").contains("nulle part"));
    let manifest: Value = paquets::serde_json::from_str(&read("manifest.webmanifest")).unwrap();
    assert_eq!(manifest["start_url"], "/Alley/app/");
    assert!(read("sw.js").contains("/Alley/assets/alley_web_bg.wasm"));
    assert!(dir.join("assets/icon-512.png").exists());
    assert!(dir.join(".nojekyll").exists());
    std::fs::remove_dir_all(&dir).unwrap();
}
