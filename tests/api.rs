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
        assert_eq!(client.post("/api/v1/rooms").json(&bad).await.status(), 400);
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
    let x = crypto::b64url(&identity.public_raw());
    let url = format!("ws://127.0.0.1:{port}/api/v1/rooms/{room}/live?x={x}");
    tokio_tungstenite::connect_async(url)
        .await
        .expect("connexion")
        .0
}

async fn next(ws: &mut Ws) -> Value {
    loop {
        let msg = tokio::time::timeout(Duration::from_secs(5), ws.next())
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

    // Bob part ; Ada aussi : le salon disparaît avec elle.
    bob.close(None).await.unwrap();
    assert_eq!(next(&mut ada).await["type"], "left");
    ada.close(None).await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let gone: Value = http("GET", &format!("/api/v1/rooms/{id}"), None).await;
    assert_eq!(gone["status"], 404);
    let stats: Value = http("GET", "/api/v1/stats", None).await;
    assert_eq!(
        (stats["rooms"].as_u64(), stats["messages_stored"].as_u64()),
        (Some(0), Some(0))
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
