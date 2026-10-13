//! L'API de bout en bout, avec la même application que la production
//! (`alley::app().test()`), et des jetons chiffrés comme le navigateur.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use paquets::serde_json::{Value, json};
use paquets::test::TestClient;
use sha2::{Digest, Sha256};

/// Un serveur vu par un navigateur : sa clé et ce qui en dérive.
struct Guild {
    key: [u8; 32],
    gid: String,
    access: String,
    admin: String,
}

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

impl Guild {
    fn new(seed: u8) -> Guild {
        let key = [seed; 32];
        Guild {
            key,
            gid: hex(&sha(&[b"brocolis-conv-v1", &key]))[..32].to_owned(),
            access: hex(&sha(&[b"alley-access-v1", &key])),
            admin: hex(&sha(&[b"admin", &key])),
        }
    }

    /// Un jeton `[[BROCOLIS:v1:…]]`, comme `sealToken` dans le navigateur.
    fn seal(&self, text: &str) -> String {
        let nonce = [seed_nonce(text); 12];
        let cipher = Aes256Gcm::new_from_slice(&self.key).unwrap();
        let mut combined = nonce.to_vec();
        combined.extend(
            cipher
                .encrypt(Nonce::from_slice(&nonce), text.as_bytes())
                .unwrap(),
        );
        format!("[[BROCOLIS:v1:{}]]", STANDARD.encode(combined))
    }

    fn open(&self, token: &str) -> String {
        let inner = token
            .strip_prefix("[[BROCOLIS:v1:")
            .and_then(|t| t.strip_suffix("]]"))
            .unwrap();
        let combined = STANDARD.decode(inner).unwrap();
        let cipher = Aes256Gcm::new_from_slice(&self.key).unwrap();
        let plain = cipher
            .decrypt(Nonce::from_slice(&combined[..12]), &combined[12..])
            .unwrap();
        String::from_utf8(plain).unwrap()
    }
}

/// Un nonce différent par texte (les tests restent déterministes).
fn seed_nonce(text: &str) -> u8 {
    text.bytes()
        .fold(7u8, |a, b| a.wrapping_mul(31).wrapping_add(b))
}

fn member(n: u8) -> String {
    hex(&[n; 32])
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
            "name": g.seal("Les copains"),
            "channel": g.seal("général"),
        }))
        .await;
    assert_eq!(res.status(), 201, "{}", res.text());
    res.json()
}

#[tokio::test]
async fn a_guild_lives_its_life_without_the_server_reading_anything() {
    let client = client().await;
    let g = Guild::new(1);
    let created = create(&client, &g).await;
    let cid = created["channels"][0]["id"].as_i64().unwrap();

    // Un membre (Ada) donne son pseudo, chiffré.
    let ada = member(0xa1);
    let res = client
        .put(&format!("/api/v1/guilds/{}/members/me", g.gid))
        .header("x-alley-access", &g.access)
        .header("x-alley-member", &ada)
        .json(&json!({ "name": g.seal("Ada") }))
        .await;
    assert_eq!(res.status(), 200, "{}", res.text());
    let ada_id = res.json::<Value>()["id"].as_str().unwrap().to_owned();
    assert_eq!(ada_id.len(), 16);

    // Elle écrit.
    let res = client
        .post(&format!("/api/v1/messages/{}/{cid}", g.gid))
        .header("x-alley-access", &g.access)
        .header("x-alley-member", &ada)
        .json(&json!({ "body": g.seal("salut tout le monde 👋") }))
        .await;
    assert_eq!(res.status(), 201, "{}", res.text());
    let mid = res.json::<Value>()["id"].as_i64().unwrap();

    // N'importe quel membre relit l'historique, et le déchiffre.
    let res = client
        .get(&format!("/api/v1/messages/{}/{cid}", g.gid))
        .header("x-alley-access", &g.access)
        .await;
    assert_eq!(res.status(), 200);
    let page: Value = res.json();
    let first = &page["messages"][0];
    assert_eq!(first["author"], ada_id);
    assert_eq!(
        g.open(first["body"].as_str().unwrap()),
        "salut tout le monde 👋"
    );
    assert_eq!(page["more"], false);

    // Le serveur, lui, ne renvoie que des jetons.
    let res = client
        .get(&format!("/api/v1/guilds/{}", g.gid))
        .header("x-alley-access", &g.access)
        .header("x-alley-admin", &g.admin)
        .await;
    let info: Value = res.json();
    assert_eq!(info["admin"], true);
    assert_eq!(g.open(info["name"].as_str().unwrap()), "Les copains");
    assert_eq!(
        g.open(info["channels"][0]["name"].as_str().unwrap()),
        "général"
    );
    assert_eq!(g.open(info["members"][0]["name"].as_str().unwrap()), "Ada");
    let raw = info.to_string();
    assert!(!raw.contains("Les copains") && !raw.contains("Ada"));
    assert!(
        !raw.contains(&g.access),
        "le jeton d'accès n'est jamais renvoyé"
    );

    // Ada corrige son message ; Bob ne peut ni le corriger ni le supprimer.
    let res = client
        .patch(&format!("/api/v1/messages/{}/{cid}/{mid}", g.gid))
        .header("x-alley-access", &g.access)
        .header("x-alley-member", &ada)
        .json(&json!({ "body": g.seal("salut à tous 👋") }))
        .await;
    assert_eq!(res.status(), 200, "{}", res.text());
    assert!(res.json::<Value>()["edited_at"].as_i64().unwrap() > 0);
    let bob = member(0xb0);
    let res = client
        .patch(&format!("/api/v1/messages/{}/{cid}/{mid}", g.gid))
        .header("x-alley-access", &g.access)
        .header("x-alley-member", &bob)
        .json(&json!({ "body": g.seal("piraté") }))
        .await;
    assert_eq!(res.status(), 403);
    let res = client
        .delete(&format!("/api/v1/messages/{}/{cid}/{mid}", g.gid))
        .header("x-alley-access", &g.access)
        .header("x-alley-member", &bob)
        .await;
    assert_eq!(res.status(), 403);

    // L'admin modère.
    let res = client
        .delete(&format!("/api/v1/messages/{}/{cid}/{mid}", g.gid))
        .header("x-alley-access", &g.access)
        .header("x-alley-admin", &g.admin)
        .await;
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn plaintext_is_refused_everywhere() {
    let client = client().await;
    let g = Guild::new(2);

    // Un serveur au nom en clair n'est pas créé.
    let res = client
        .post("/api/v1/guilds")
        .json(&json!({
            "gid": g.gid, "access": g.access, "admin": g.admin,
            "name": "Mon serveur", "channel": g.seal("général"),
        }))
        .await;
    assert_eq!(res.status(), 422);

    let created = create(&client, &g).await;
    let cid = created["channels"][0]["id"].as_i64().unwrap();
    for body in [
        "bonjour".to_owned(),
        format!("bonjour {}", g.seal("x")),
        "[[BROCOLIS:v1:pas du base64 !]]".to_owned(),
        "[[BROCOLIS:v1:AAAA]]".to_owned(),
    ] {
        let res = client
            .post(&format!("/api/v1/messages/{}/{cid}", g.gid))
            .header("x-alley-access", &g.access)
            .header("x-alley-member", &member(1))
            .json(&json!({ "body": body }))
            .await;
        assert_eq!(res.status(), 422, "accepté : {body}");
    }
    let res = client
        .put(&format!("/api/v1/guilds/{}/members/me", g.gid))
        .header("x-alley-access", &g.access)
        .header("x-alley-member", &member(1))
        .json(&json!({ "name": "Ada" }))
        .await;
    assert_eq!(res.status(), 422);
    let res = client
        .post(&format!("/api/v1/guilds/{}/channels", g.gid))
        .header("x-alley-access", &g.access)
        .header("x-alley-admin", &g.admin)
        .json(&json!({ "name": "aléatoire" }))
        .await;
    assert_eq!(res.status(), 422);
}

#[tokio::test]
async fn without_the_key_a_guild_does_not_exist() {
    let client = client().await;
    let g = Guild::new(3);
    let created = create(&client, &g).await;
    let cid = created["channels"][0]["id"].as_i64().unwrap();

    // Sans jeton d'accès, ou avec celui d'un autre serveur : 404, comme si le
    // serveur n'existait pas.
    let other = Guild::new(4);
    for access in [None, Some(other.access.as_str()), Some("zz")] {
        let mut req = client.get(&format!("/api/v1/guilds/{}", g.gid));
        if let Some(a) = access {
            req = req.header("x-alley-access", a);
        }
        assert_eq!(req.await.status(), 404);
        let mut req = client.get(&format!("/api/v1/messages/{}/{cid}", g.gid));
        if let Some(a) = access {
            req = req.header("x-alley-access", a);
        }
        assert_eq!(req.await.status(), 404);
    }

    // Écrire demande un secret de membre.
    let res = client
        .post(&format!("/api/v1/messages/{}/{cid}", g.gid))
        .header("x-alley-access", &g.access)
        .json(&json!({ "body": g.seal("coucou") }))
        .await;
    assert_eq!(res.status(), 401);

    // Le même identifiant de serveur ne peut pas être repris.
    let res = client
        .post("/api/v1/guilds")
        .json(&json!({
            "gid": g.gid, "access": other.access, "admin": other.admin,
            "name": other.seal("Usurpateur"), "channel": other.seal("général"),
        }))
        .await;
    assert_eq!(res.status(), 409);
}

#[tokio::test]
async fn only_the_admin_manages_channels_and_the_guild() {
    let client = client().await;
    let g = Guild::new(5);
    create(&client, &g).await;
    let path = format!("/api/v1/guilds/{}/channels", g.gid);

    // Un membre n'est pas admin.
    let res = client
        .post(&path)
        .header("x-alley-access", &g.access)
        .header("x-alley-member", &member(9))
        .json(&json!({ "name": g.seal("jeux") }))
        .await;
    assert_eq!(res.status(), 403);

    // L'admin crée un salon avec un sujet, le renomme, puis le supprime.
    let res = client
        .post(&path)
        .header("x-alley-access", &g.access)
        .header("x-alley-admin", &g.admin)
        .json(&json!({ "name": g.seal("jeux"), "topic": g.seal("ce soir 21 h") }))
        .await;
    assert_eq!(res.status(), 201, "{}", res.text());
    let channel: Value = res.json();
    assert_eq!(channel["position"], 1);
    let cid = channel["id"].as_i64().unwrap();
    let res = client
        .patch(&format!("{path}/{cid}"))
        .header("x-alley-access", &g.access)
        .header("x-alley-admin", &g.admin)
        .json(&json!({ "name": g.seal("jeux-vidéo") }))
        .await;
    assert_eq!(res.status(), 200);
    assert_eq!(
        g.open(res.json::<Value>()["name"].as_str().unwrap()),
        "jeux-vidéo"
    );

    // Les messages du salon partent avec lui.
    let res = client
        .post(&format!("/api/v1/messages/{}/{cid}", g.gid))
        .header("x-alley-access", &g.access)
        .header("x-alley-member", &member(9))
        .json(&json!({ "body": g.seal("gg") }))
        .await;
    assert_eq!(res.status(), 201);
    let res = client
        .delete(&format!("{path}/{cid}"))
        .header("x-alley-access", &g.access)
        .header("x-alley-admin", &g.admin)
        .await;
    assert_eq!(res.status(), 204);
    let res = client
        .get(&format!("/api/v1/messages/{}/{cid}", g.gid))
        .header("x-alley-access", &g.access)
        .await;
    assert_eq!(res.status(), 404);

    // Le dernier salon reste.
    let info: Value = client
        .get(&format!("/api/v1/guilds/{}", g.gid))
        .header("x-alley-access", &g.access)
        .await
        .json();
    let last = info["channels"][0]["id"].as_i64().unwrap();
    let res = client
        .delete(&format!("{path}/{last}"))
        .header("x-alley-access", &g.access)
        .header("x-alley-admin", &g.admin)
        .await;
    assert_eq!(res.status(), 409);

    // Renommer et supprimer le serveur.
    let res = client
        .patch(&format!("/api/v1/guilds/{}", g.gid))
        .header("x-alley-access", &g.access)
        .header("x-alley-admin", &g.admin)
        .json(&json!({ "name": g.seal("Le QG") }))
        .await;
    assert_eq!(res.status(), 200);
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
async fn history_is_paged_from_the_newest() {
    let client = client().await;
    let g = Guild::new(6);
    let created = create(&client, &g).await;
    let cid = created["channels"][0]["id"].as_i64().unwrap();
    for i in 0..7 {
        let res = client
            .post(&format!("/api/v1/messages/{}/{cid}", g.gid))
            .header("x-alley-access", &g.access)
            .header("x-alley-member", &member(1))
            .json(&json!({ "body": g.seal(&format!("message {i}")) }))
            .await;
        assert_eq!(res.status(), 201);
    }
    let page: Value = client
        .get(&format!("/api/v1/messages/{}/{cid}?limit=3", g.gid))
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
            "/api/v1/messages/{}/{cid}?limit=3&before={oldest}",
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
async fn the_site_and_the_installable_app_are_served() {
    let client = client().await;
    let home = client.get("/").await;
    assert_eq!(home.status(), 200);
    let html = home.text();
    assert!(html.contains("Discord"));
    assert!(html.contains(r#"<link rel="manifest" href="/manifest.webmanifest"/>"#));
    assert!(html.contains("apple-mobile-web-app-capable"));
    assert!(html.contains("viewport-fit=cover"));

    let app = client.get("/app").await;
    assert_eq!(app.status(), 200);
    let csp = app
        .header("content-security-policy")
        .unwrap_or_default()
        .to_owned();
    assert!(csp.contains("script-src 'self'"));
    assert!(app.text().contains("/assets/app.js"));

    let manifest: Value = client.get("/manifest.webmanifest").await.json();
    assert_eq!(manifest["start_url"], "/app");
    assert_eq!(manifest["display"], "standalone");
    assert!(manifest["icons"].as_array().unwrap().len() >= 3);

    let worker = client.get("/sw.js").await.text();
    assert!(worker.contains(r#"const NETWORK_ONLY = ["/api/"];"#));
    assert!(worker.contains("/assets/app.js"));

    for asset in [
        "/assets/app.js",
        "/assets/brocolis.js",
        "/assets/app.css",
        "/assets/icon-192.png",
    ] {
        assert_eq!(client.get(asset).await.status(), 200, "{asset}");
    }
    assert_eq!(client.get("/nulle-part").await.status(), 404);
    assert_eq!(client.get("/api/v1/inconnue").await.status(), 404);

    let stats: Value = client.get("/api/v1/stats").await.json();
    assert_eq!(stats["plaintext_stored"], 0);
}
