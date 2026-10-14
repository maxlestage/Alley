//! Alley de bout en bout, avec la même application que la production
//! (`alley::app()`) : l'API sur le client de test, et de vrais salons en
//! WebSocket sur un vrai port, chiffrés par le code même du navigateur
//! (`alley_web::crypto`, compilé ici sans WebAssembly).

use std::time::Duration;

use alley_web::crypto::{self, Identity};
use futures_util::{SinkExt, StreamExt};
use paquets::serde_json::{Value, json};
use paquets::test::TestClient;
use tokio_tungstenite::tungstenite::Message;

async fn client() -> TestClient {
    alley::app().test().await
}

#[tokio::test]
async fn rooms_are_found_by_name_and_private_ones_are_hidden() {
    let client = client().await;
    let res = client
        .post("/api/v1/rooms")
        .json(&json!({ "name": "  Soirée   jeux ", "tz": 120 }))
        .await;
    assert_eq!(res.status(), 201, "{}", res.text());
    let room: Value = res.json();
    assert_eq!(room["name"], "Soirée jeux");
    let expires = room["expires_at"].as_i64().unwrap();
    assert!(
        expires > alley::rooms::now_ms() && expires <= alley::rooms::now_ms() + 24 * 3600 * 1000
    );

    // Le même nom, accents et majuscules mis à part, mène au même salon.
    let again: Value = client
        .post("/api/v1/rooms")
        .json(&json!({ "name": "soiree JEUX" }))
        .await
        .json();
    assert_eq!(again["id"], room["id"]);

    let found: Value = client.get("/api/v1/rooms?q=SOIR").await.json();
    assert_eq!(found["rooms"].as_array().unwrap().len(), 1);
    let none: Value = client.get("/api/v1/rooms?q=th%C3%A9").await.json();
    assert!(none["rooms"].as_array().unwrap().is_empty());

    // Un salon privé : pas dans la liste, mais joignable par son lien.
    let private: Value = client
        .post("/api/v1/rooms")
        .json(&json!({ "name": "Secret", "private": true }))
        .await
        .json();
    let id = private["id"].as_str().unwrap();
    assert_eq!(id.len(), 32);
    let all: Value = client.get("/api/v1/rooms").await.json();
    assert!(
        all["rooms"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["name"] != "Secret")
    );
    assert_eq!(
        client.get(&format!("/api/v1/rooms/{id}")).await.status(),
        200
    );
    assert_eq!(client.get("/api/v1/rooms/inconnu").await.status(), 404);

    for bad in [json!({ "name": "" }), json!({ "name": "x".repeat(41) })] {
        let res = client.post("/api/v1/rooms").json(&bad).await;
        assert_eq!(res.status(), 400);
        assert_eq!(res.json::<Value>()["error"], "bad_name");
    }
    // Un nom de salon est public : ni sexe, ni drogue.
    for (name, reason) in [("Porno gratuit", "sexual"), ("vente de cocaïne", "drugs")] {
        let res = client
            .post("/api/v1/rooms")
            .json(&json!({ "name": name }))
            .await;
        assert_eq!(res.status(), 422, "{name}");
        let body: Value = res.json();
        assert_eq!(
            (body["error"].as_str(), body["reason"].as_str()),
            (Some("forbidden_name"), Some(reason))
        );
    }
    let stats: Value = client.get("/api/v1/stats").await.json();
    assert_eq!(stats["messages_stored"], 0);
    assert_eq!(stats["pseudonyms_known"], 0);
}

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Une vraie application Alley sur un port libre.
async fn serve() -> u16 {
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let mut app = alley::app();
    app.port(port);
    app.quiet();
    tokio::spawn(app.serve());
    for _ in 0..50 {
        if tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_ok()
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    port
}

async fn connect(port: u16, room: &str, identity: &Identity) -> Ws {
    connect_with(port, room, identity, "").await
}

async fn connect_with(port: u16, room: &str, identity: &Identity, extra: &str) -> Ws {
    let x = crypto::b64url(&identity.public_raw());
    let url = format!("ws://127.0.0.1:{port}/api/v1/rooms/{room}/live?x={x}{extra}");
    tokio_tungstenite::connect_async(url)
        .await
        .expect("connexion")
        .0
}

async fn next(ws: &mut Ws) -> Value {
    next_within(ws, 5).await
}

/// Comme [`next`], en attendant plus longtemps : vérifier une image peut
/// d'abord attendre le chargement des modèles (machine chargée, CI).
async fn next_within(ws: &mut Ws, seconds: u64) -> Value {
    loop {
        let msg = tokio::time::timeout(Duration::from_secs(seconds), ws.next())
            .await
            .expect("un événement à temps")
            .expect("connexion ouverte")
            .expect("message");
        if let Message::Text(text) = msg {
            return paquets::serde_json::from_str(text.as_str()).unwrap();
        }
    }
}

async fn send(ws: &mut Ws, value: Value) {
    ws.send(Message::text(value.to_string())).await.unwrap();
}

#[tokio::test]
async fn a_room_hands_its_key_over_relays_and_forgets() {
    let port = serve().await;
    let http = reqwest_like(port);
    let room: Value = http("POST", "/api/v1/rooms", Some(json!({ "name": "Café" }))).await;
    let id = room["id"].as_str().unwrap().to_owned();

    // Ada arrive la première : seule, elle tire la clé du salon.
    let ada_id = Identity::generate();
    let mut ada = connect(port, &id, &ada_id).await;
    let welcome = next(&mut ada).await;
    assert_eq!(welcome["type"], "welcome");
    assert!(welcome["peers"].as_array().unwrap().is_empty());
    let key = crypto::generate_key();

    // Un message avant l'arrivée de Bob : il ne le verra jamais.
    let before = crypto::seal_token(&key, r#"{"k":"msg","x":"avant"}"#);
    send(&mut ada, json!({ "type": "relay", "data": before })).await;

    // Bob arrive : Ada apprend sa clé publique et lui remet la clé du salon.
    let bob_id = Identity::generate();
    let mut bob = connect(port, &id, &bob_id).await;
    let welcome = next(&mut bob).await;
    let ada_conn = welcome["peers"][0]["id"].as_str().unwrap().to_owned();
    let joined = next(&mut ada).await;
    assert_eq!(joined["type"], "joined");
    let bob_conn = joined["id"].as_str().unwrap().to_owned();
    let blob = ada_id
        .wrap_group_key(&key, joined["x"].as_str().unwrap())
        .unwrap();
    send(
        &mut ada,
        json!({ "type": "key", "to": bob_conn, "blob": blob }),
    )
    .await;
    let handed = next(&mut bob).await;
    assert_eq!(
        (handed["type"].as_str(), handed["from"].as_str()),
        (Some("key"), Some(ada_conn.as_str()))
    );
    let x_ada = welcome["peers"][0]["x"].as_str().unwrap();
    assert_eq!(
        bob_id.unwrap_group_key(handed["blob"].as_str().unwrap(), x_ada),
        Some(key)
    );

    // Le clair est refusé ; un jeton passe, avec son expéditeur.
    send(
        &mut ada,
        json!({ "type": "relay", "data": "salut en clair" }),
    )
    .await;
    let token = crypto::seal_token(&key, r#"{"k":"msg","x":"salut"}"#);
    send(&mut ada, json!({ "type": "relay", "data": token })).await;
    let got = next(&mut bob).await;
    assert_eq!(got["type"], "relay");
    assert_eq!(got["from"], ada_conn);
    let clear = crypto::open_token(&key, got["data"].as_str().unwrap()).unwrap();
    assert!(
        clear.contains("salut") && !clear.contains("avant"),
        "{clear}"
    );

    // Une signalisation pour Bob seul.
    let signal = crypto::seal_token(&key, r#"{"k":"signal"}"#);
    send(
        &mut bob,
        json!({ "type": "relay", "data": signal, "to": ada_conn }),
    )
    .await;
    assert_eq!(next(&mut ada).await["from"], bob_conn);

    // Bob part ; Ada aussi : le salon reste, vide, jusqu'à minuit, et
    // l'appareil qui s'en souvient le retrouve. Aucun message n'est gardé.
    bob.close(None).await.unwrap();
    assert_eq!(next(&mut ada).await["type"], "left");
    ada.close(None).await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let still: Value = http("GET", &format!("/api/v1/rooms/{id}"), None).await;
    assert_eq!(
        (still["id"].as_str(), still["members"].as_u64()),
        (Some(id.as_str()), Some(0))
    );
    let known: Value = http(
        "POST",
        "/api/v1/known",
        Some(json!({ "ids": [id, "0123456789ab"] })),
    )
    .await;
    assert_eq!(known["rooms"].as_array().unwrap().len(), 1);
    assert_eq!(known["rooms"][0]["id"], id.as_str());
    let stats: Value = http("GET", "/api/v1/stats", None).await;
    assert_eq!(
        (
            stats["rooms"].as_u64(),
            stats["connections"].as_u64(),
            stats["messages_stored"].as_u64()
        ),
        (Some(1), Some(0), Some(0))
    );
}

/// Une requête HTTP minimale (sans dépendance de plus) : le corps JSON, ou
/// `{"status": …}` pour une erreur.
fn reqwest_like(
    port: u16,
) -> impl Fn(&str, &str, Option<Value>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Value>>>
{
    move |method, path, body| {
        let (method, path) = (method.to_owned(), path.to_owned());
        Box::pin(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .unwrap();
            let body = body.map(|b| b.to_string()).unwrap_or_default();
            let request = format!(
                "{method} {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(request.as_bytes()).await.unwrap();
            let mut raw = String::new();
            stream.read_to_string(&mut raw).await.unwrap();
            let status: u16 = raw
                .split(' ')
                .nth(1)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let body = raw.split("\r\n\r\n").nth(1).unwrap_or_default();
            if status >= 400 {
                return json!({ "status": status });
            }
            paquets::serde_json::from_str(body).unwrap_or(Value::Null)
        })
    }
}

#[tokio::test]
async fn a_password_finds_its_rooms_again_from_any_device() {
    let port = serve().await;
    let http = reqwest_like(port);
    let room: Value = http(
        "POST",
        "/api/v1/rooms",
        Some(json!({ "name": "Bureau", "private": true })),
    )
    .await;
    let id = room["id"].as_str().unwrap().to_owned();
    // La marque, tirée du pseudonyme et du mot de passe (ici sans PBKDF2 :
    // c'est le navigateur qui le calcule).
    let tag = crypto::account_tag(b"graine d'Ada");
    let mut ada = connect_with(port, &id, &Identity::generate(), &format!("&t={tag}")).await;
    assert_eq!(next(&mut ada).await["type"], "welcome");
    // Depuis un autre appareil, la même marque retrouve le salon privé.
    let mine: Value = http("POST", "/api/v1/mine", Some(json!({ "tag": tag }))).await;
    assert_eq!(mine["rooms"][0]["id"], id.as_str());
    let other: Value = http(
        "POST",
        "/api/v1/mine",
        Some(json!({ "tag": crypto::account_tag(b"autre") })),
    )
    .await;
    assert!(other["rooms"].as_array().unwrap().is_empty());
    let bad: Value = http("POST", "/api/v1/mine", Some(json!({ "tag": "court" }))).await;
    assert_eq!(bad["status"], 400);
}

/// Les modèles de modération sont-ils là (`cargo xtask models`) ?
fn models_ready() -> bool {
    let dir = alley::moderation::Moderation::models_dir();
    let ok = dir.join("nsfw.onnx").exists() && dir.join("clip-visual.onnx").exists();
    if !ok {
        eprintln!("modèles absents : lancez `cargo xtask models` (test de modération sauté)");
    }
    ok
}

#[tokio::test]
async fn an_image_is_checked_and_a_forbidden_one_blocks_the_address() {
    if !models_ready() {
        return;
    }
    let port = serve().await;
    let http = reqwest_like(port);
    let room: Value = http("POST", "/api/v1/rooms", Some(json!({ "name": "Photos" }))).await;
    let id = room["id"].as_str().unwrap().to_owned();
    let mut ada = connect(port, &id, &Identity::generate()).await;
    next(&mut ada).await;
    let mut bob = connect(port, &id, &Identity::generate()).await;
    next(&mut bob).await;
    next(&mut ada).await;
    let room_key = crypto::generate_key();

    let image = |jpeg: &[u8], mid: &str| {
        let key = crypto::generate_key();
        json!({
            "type": "image",
            "id": mid,
            "key": crypto::b64(&key),
            "c": crypto::seal_bytes_token(&key, jpeg),
            "data": crypto::seal_token(
                &room_key,
                &json!({ "k": "msg", "kind": "image", "mid": mid, "ik": crypto::b64(&key) }).to_string(),
            ),
        })
    };

    // Une image ordinaire passe : Bob la reçoit avec l'empreinte de sa clé.
    let ordinary = std::fs::read("tests/fixtures/ordinary.jpg").unwrap();
    send(&mut ada, image(&ordinary, "m1")).await;
    let got = next_within(&mut bob, 60).await;
    assert_eq!(got["type"], "relay");
    let payload = crypto::open_token(&room_key, got["data"].as_str().unwrap()).unwrap();
    let payload: Value = paquets::serde_json::from_str(&payload).unwrap();
    let key: crypto::Key = crypto::unb64(payload["ik"].as_str().unwrap())
        .unwrap()
        .try_into()
        .unwrap();
    assert_eq!(got["kc"].as_str().unwrap(), crypto::key_commitment(&key));
    assert_eq!(
        crypto::open_bytes_token(&key, got["c"].as_str().unwrap()).unwrap(),
        ordinary
    );
    assert_eq!(next(&mut ada).await["type"], "sent");

    // Ce qui n'est pas une image JPEG est refusé, sans plus.
    tokio::time::sleep(Duration::from_millis(2100)).await;
    send(&mut ada, image(b"GIF89a pas une photo", "m2")).await;
    let refused = next_within(&mut ada, 60).await;
    assert_eq!(
        (refused["type"].as_str(), refused["reason"].as_str()),
        (Some("moderated"), Some("invalid"))
    );

    // Une image montrant de la drogue : refusée, et l'adresse est bloquée.
    tokio::time::sleep(Duration::from_millis(2100)).await;
    let drugs = std::fs::read("tests/fixtures/drugs.jpg").unwrap();
    send(&mut ada, image(&drugs, "m3")).await;
    let refused = next_within(&mut ada, 60).await;
    assert_eq!(refused["type"], "moderated");
    assert_eq!(refused["reason"], "drugs");
    assert_eq!(refused["banned"], true);
    // Toutes les connexions de l'adresse sont fermées (ici, Bob aussi : même
    // machine), puis toutes ses requêtes refusées.
    let closed = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match ada.next().await {
                Some(Ok(Message::Close(Some(frame)))) => return u16::from(frame.code),
                Some(Ok(_)) => continue,
                _ => return 0,
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(closed, 4403);
    let blocked: Value = http("GET", "/api/v1/rooms", None).await;
    assert_eq!(blocked["status"], 403);
}

#[tokio::test]
async fn an_unknown_room_refuses_the_connection() {
    let port = serve().await;
    let mut ws = connect(port, "inconnu", &Identity::generate()).await;
    let closed = tokio::time::timeout(Duration::from_secs(5), ws.next())
        .await
        .unwrap();
    match closed {
        Some(Ok(Message::Close(Some(frame)))) => assert_eq!(u16::from(frame.code), 4404),
        other => panic!("fermeture attendue, reçu {other:?}"),
    }
}

#[tokio::test]
async fn github_pages_may_call_the_api_and_http_goes_to_https() {
    let client = client().await;
    // Le pré-vol CORS de la version GitHub Pages.
    let res = client
        .request(paquets::vitesse::Method::OPTIONS, "/api/v1/rooms")
        .header("origin", "https://maxlestage.github.io")
        .header("access-control-request-method", "POST")
        .header("access-control-request-headers", "content-type")
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
    assert!(allowed.contains("content-type"), "{allowed}");
    // Et l'écriture elle-même, venue de ce site.
    let res = client
        .post("/api/v1/rooms")
        .header("origin", "https://maxlestage.github.io")
        .header("sec-fetch-site", "cross-site")
        .header("sec-fetch-mode", "cors")
        .json(&json!({ "name": "depuis Pages" }))
        .await;
    assert_eq!(res.status(), 201, "{}", res.text());
    // Un autre site n'écrit pas.
    let res = client
        .post("/api/v1/rooms")
        .header("origin", "https://evil.example")
        .header("sec-fetch-site", "cross-site")
        .header("sec-fetch-mode", "cors")
        .json(&json!({ "name": "pirate" }))
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
    assert!(html.contains(r#"<html lang="fr""#), "{html}");
    assert!(html.contains(r#"<link rel="manifest" href="/manifest.webmanifest"/>"#));
    assert!(html.contains(r#"hreflang="en" href="/en/""#));
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
        shell.contains(r#"<script type="module" src="/assets/alley_web.js?v="#),
        "{shell}"
    );
    assert!(
        !shell.contains("<script type=\"module\">"),
        "aucun script en ligne"
    );
    assert!(shell.contains(r#"<meta name="alley-page" content="app"/>"#));
    assert!(
        !shell.contains("app.js"),
        "plus de JavaScript écrit à la main"
    );
    assert_eq!(client.get("/app").await.status(), 200);

    // En anglais et en espagnol aussi, du premier au dernier mot.
    for (path, lang, words) in [
        ("/en/", "en", "Talk. At midnight"),
        ("/es/", "es", "A medianoche, todo se borra."),
        ("/en/app/", "en", "Opening Alley…"),
        ("/es/app/", "es", "Abriendo Alley…"),
    ] {
        let page = client.get(path).await;
        assert_eq!(page.status(), 200, "{path}");
        let html = page.text();
        assert!(html.contains(&format!(r#"<html lang="{lang}""#)), "{path}");
        assert!(html.contains(words), "{path}");
        assert!(
            !html.contains("Ouvrir Alley"),
            "{path} : du français oublié"
        );
    }
    let manifest: Value = client.get("/es/manifest.webmanifest").await.json();
    assert_eq!(
        (manifest["lang"].as_str(), manifest["start_url"].as_str()),
        (Some("es"), Some("/es/app/"))
    );
    let manifest: Value = client.get("/manifest.webmanifest").await.json();
    assert_eq!(manifest["start_url"], "/app/");
    assert_eq!(manifest["display"], "standalone");
    assert!(manifest["icons"].as_array().unwrap().len() >= 3);

    assert_eq!(
        client.get("/sw.js").await.status(),
        404,
        "plus de service worker"
    );
    let css = client.get("/assets/app.css").await.text();
    assert!(
        css.starts_with(":root{--bg:"),
        "les styles, rendus depuis Rust"
    );

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
    assert_eq!(stats["messages_stored"], 0);
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
    assert!(app.contains(r#"src="/Alley/assets/alley_web.js?v="#));
    assert!(app.contains(r#"<meta name="alley-api" content="https://alley.example"/>"#));
    assert!(read("index.html").contains(r#"href="/Alley/app/""#));
    assert!(read("404.html").contains("nulle part"));
    assert!(read("404.html").contains("leads nowhere"));
    assert!(read("en/index.html").contains(r#"href="/Alley/en/app/""#));
    assert!(read("es/app/index.html").contains(r#"<html lang="es""#));
    assert!(dir.join("en/manifest.webmanifest").exists());
    let manifest: Value = paquets::serde_json::from_str(&read("manifest.webmanifest")).unwrap();
    assert_eq!(manifest["start_url"], "/Alley/app/");
    assert!(read("assets/site.css").starts_with(":root{"));
    assert!(!dir.join("sw.js").exists());
    assert!(dir.join("assets/icon-512.png").exists());
    assert!(dir.join(".nojekyll").exists());
    std::fs::remove_dir_all(&dir).unwrap();
}
