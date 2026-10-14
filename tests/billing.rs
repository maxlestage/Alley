//! Les abonnements, de bout en bout, contre un faux Stripe — sans base de
//! données : Stripe fait foi, le navigateur garde son abonnement et les
//! descriptions signées des salons, qui rouvrent après un redémarrage.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use paquets::prelude::*;
use paquets::serde_json::{Value, json};

/// Un abonnement du faux Stripe : compte, formule, rythme, statut.
type Sub = (String, String, String, String);

#[derive(Default)]
struct Fake {
    /// Le dernier formulaire de Checkout.
    checkout: HashMap<String, String>,
    auth: String,
    subs: HashMap<String, Sub>,
    portal_customer: String,
}

static STRIPE: LazyLock<Mutex<Fake>> = LazyLock::new(Default::default);

fn decode(s: &str) -> String {
    let s = s.replace('+', " ");
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap()
}

fn form(body: &str) -> HashMap<String, String> {
    body.split('&')
        .filter_map(|kv| kv.split_once('='))
        .map(|(k, v)| (decode(k), decode(v)))
        .collect()
}

fn subscription_json(id: &str, s: &Sub) -> Value {
    json!({
        "id": id, "customer": "cus_test", "status": s.3,
        "metadata": { "alley_billing": s.0, "alley_plan": s.1 },
        "items": { "data": [{ "current_period_end": 4_102_444_800i64,
            "price": { "recurring": { "interval": s.2, "interval_count": 1 } } }] }
    })
}

async fn fake_checkout(req: Request) -> Json<Value> {
    let f = form(&req.text().await.unwrap());
    let mut fake = STRIPE.lock().unwrap();
    fake.auth = req.header("authorization").unwrap_or_default().to_owned();
    // Un abonnement en attente de paiement.
    let id = format!("sub_{}", fake.subs.len() + 1);
    let sub = (
        f["subscription_data[metadata][alley_billing]"].clone(),
        f["subscription_data[metadata][alley_plan]"].clone(),
        f["line_items[0][price_data][recurring][interval]"].clone(),
        "incomplete".to_owned(),
    );
    fake.subs.insert(id.clone(), sub);
    fake.checkout = f;
    Json(
        json!({ "id": format!("cs_{id}"), "url": format!("https://checkout.stripe.test/cs_{id}") }),
    )
}

async fn fake_session(req: Request) -> (u16, Json<Value>) {
    let id = req.param("id").unwrap_or_default().to_owned();
    let fake = STRIPE.lock().unwrap();
    let sub = id.trim_start_matches("cs_");
    match fake.subs.get(sub) {
        Some(s) => (
            200,
            Json(json!({ "id": id, "subscription": sub, "metadata": { "alley_billing": s.0 } })),
        ),
        None => (
            404,
            Json(json!({ "error": { "message": "No such session" } })),
        ),
    }
}

async fn fake_subscription(req: Request) -> (u16, Json<Value>) {
    let id = req.param("id").unwrap_or_default().to_owned();
    let fake = STRIPE.lock().unwrap();
    if id == "search" {
        let query = req
            .query("query")
            .map(|q| q.into_owned())
            .unwrap_or_default();
        let data: Vec<Value> = fake
            .subs
            .iter()
            .filter(|(_, s)| query.contains(&s.0))
            .map(|(id, s)| subscription_json(id, s))
            .collect();
        return (200, Json(json!({ "data": data })));
    }
    match fake.subs.get(&id) {
        Some(s) => (200, Json(subscription_json(&id, s))),
        None => (
            404,
            Json(json!({ "error": { "message": "No such subscription" } })),
        ),
    }
}

async fn fake_portal(req: Request) -> Json<Value> {
    let f = form(&req.text().await.unwrap());
    STRIPE.lock().unwrap().portal_customer = f["customer"].clone();
    Json(json!({ "url": "https://billing.stripe.test/session" }))
}

fn set_status(sub: &str, status: &str) {
    STRIPE.lock().unwrap().subs.get_mut(sub).unwrap().3 = status.into();
}

fn add_sub(id: &str, secret: &str, plan: &str, interval: &str) {
    let billing = alley::billing::billing_id(secret).unwrap();
    let sub = (billing, plan.into(), interval.into(), "active".into());
    STRIPE.lock().unwrap().subs.insert(id.into(), sub);
}

async fn fake_stripe() -> u16 {
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let mut app = App::new();
    app.security(None);
    app.quiet();
    app.port(port);
    app.post("/v1/checkout/sessions", fake_checkout);
    app.get("/v1/checkout/sessions/:id", fake_session);
    app.get("/v1/subscriptions/:id", fake_subscription);
    app.post("/v1/billing_portal/sessions", fake_portal);
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

async fn alley_with(db: &str) -> paquets::test::TestClient {
    let mut app = alley::app();
    app.database(db.to_owned());
    app.test().await
}

async fn post(client: &paquets::test::TestClient, path: &str, body: Value) -> (u16, Value) {
    let res = client.post(path).json(&body).await;
    (res.status().as_u16(), res.json())
}

const DAY: i64 = 24 * 3600 * 1000;

fn left(room: &Value) -> i64 {
    room["expires_at"].as_i64().unwrap() - alley::rooms::now_ms()
}

#[tokio::test]
async fn monthly_subscriptions_without_a_database() {
    let stripe = fake_stripe().await;
    // SAFETY : ce fichier de tests ne contient que ce test, rien ne lit
    // l'environnement en même temps.
    unsafe {
        std::env::set_var("STRIPE_SECRET_KEY", "sk_test_123");
        std::env::set_var("STRIPE_API_BASE", format!("http://127.0.0.1:{stripe}"));
    }
    // Sans clé de signature durable, l'offre n'est pas proposée : un salon
    // de plusieurs jours ne rouvrirait pas après un redémarrage.
    let no_key = alley_with("memory").await;
    let plans: Value = no_key.get("/api/v1/plans").await.json();
    assert_eq!(plans["enabled"], false);

    let key = "4b".repeat(32);
    unsafe { std::env::set_var("ALLEY_SIGNING_KEY", &key) };
    let client = alley_with("memory").await;

    // Les formules : au mois, et seulement au mois.
    let plans: Value = client.get("/api/v1/plans").await.json();
    assert_eq!(plans["enabled"], true);
    assert_eq!(
        (plans["currency"].as_str(), plans["interval"].as_str()),
        (Some("eur"), Some("month"))
    );
    assert_eq!(
        plans["plans"][0],
        json!({ "id": "3d", "days": 3, "amount": 499 })
    );
    assert_eq!(
        plans["plans"][1],
        json!({ "id": "7d", "days": 7, "amount": 999 })
    );

    let secret = "5e".repeat(32);
    let back = "https://maxlestage.github.io/Alley/app/";
    let checkout = |plan: &str, back: &str| json!({ "secret": secret, "plan": plan, "lang": "fr", "return_to": back });
    let evil = checkout("3d", "https://evil.example/");
    assert_eq!(post(&client, "/api/v1/billing/checkout", evil).await.0, 400);
    let yearly = checkout("1y", back);
    assert_eq!(
        post(&client, "/api/v1/billing/checkout", yearly).await.0,
        400
    );
    let (status, body) = post(&client, "/api/v1/billing/checkout", checkout("3d", back)).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["url"], "https://checkout.stripe.test/cs_sub_1");
    {
        let fake = STRIPE.lock().unwrap();
        let f = &fake.checkout;
        assert_eq!(fake.auth, "Bearer sk_test_123");
        assert_eq!(f["mode"], "subscription");
        assert_eq!(f["line_items[0][price_data][recurring][interval]"], "month");
        assert_eq!(
            f["line_items[0][price_data][recurring][interval_count]"],
            "1"
        );
        assert_eq!(f["line_items[0][price_data][unit_amount]"], "499");
        assert_eq!(f["line_items[0][price_data][currency]"], "eur");
        assert_eq!(
            f["success_url"],
            format!("{back}#abonnement={{CHECKOUT_SESSION_ID}}")
        );
        // Stripe ne reçoit que l'empreinte du secret.
        assert!(!f.values().any(|v| v.contains(&secret)));
    }

    // Pas encore payé : un salon ordinaire.
    let create = |name: &str, sub: Option<&str>| json!({ "name": name, "private": true, "tz": 120, "billing": secret, "subscription": sub });
    let (_, room) = post(&client, "/api/v1/rooms", create("Avant", None)).await;
    assert!(left(&room) <= DAY);

    // Payé ; de retour de Checkout, la session mène à l'abonnement.
    set_status("sub_1", "active");
    let (_, status) = post(
        &client,
        "/api/v1/billing/status",
        json!({ "secret": secret, "session": "cs_sub_1" }),
    )
    .await;
    assert_eq!(
        (
            status["active"].as_bool(),
            status["days"].as_u64(),
            status["subscription"].as_str()
        ),
        (Some(true), Some(3), Some("sub_1")),
        "{status}"
    );
    // La session payée par un autre compte ne donne rien.
    let (_, other) = post(
        &client,
        "/api/v1/billing/status",
        json!({ "secret": "6f".repeat(32), "session": "cs_sub_1" }),
    )
    .await;
    assert_eq!(other["active"], false);

    // Abonné : ses salons durent 3 jours.
    let (_, room) = post(
        &client,
        "/api/v1/rooms",
        create("Trois jours", Some("sub_1")),
    )
    .await;
    assert!(left(&room) > 2 * DAY && left(&room) <= 3 * DAY);
    // Un autre appareil, qui ne connaît pas l'abonnement : Stripe le retrouve.
    let (_, room2) = post(&client, "/api/v1/rooms", create("Autre appareil", None)).await;
    assert!(left(&room2) > 2 * DAY);
    // L'abonnement d'un autre ne compte pas.
    let stolen = json!({ "name": "Volé", "billing": "6f".repeat(32), "subscription": "sub_1" });
    let (_, stolen) = post(&client, "/api/v1/rooms", stolen).await;
    assert!(left(&stolen) <= DAY);
    // Déjà abonné : pas de second paiement.
    let again = checkout("7d", back);
    assert_eq!(
        post(&client, "/api/v1/billing/checkout", again).await.0,
        409
    );

    // Le serveur redémarre, sans base : le salon est oublié…
    let id = room["id"].as_str().unwrap().to_owned();
    let client = alley_with("memory").await;
    let gone = client.get(&format!("/api/v1/rooms/{id}")).await;
    assert_eq!(gone.status(), 404);
    // … une description trafiquée ne le rouvre pas…
    let signed = |name: &str| {
        json!({ "ids": [id], "rooms": [{ "id": id, "name": name, "private": true,
            "expires_at": room["expires_at"], "sig": room["sig"] }] })
    };
    let (_, known) = post(&client, "/api/v1/known", signed("Trafiqué")).await;
    assert!(known["rooms"].as_array().unwrap().is_empty());
    // … la vraie, gardée par le navigateur, si.
    let (_, known) = post(&client, "/api/v1/known", signed("Trois jours")).await;
    assert_eq!(known["rooms"][0]["name"], "Trois jours");
    assert_eq!(known["rooms"][0]["sig"], room["sig"]);
    let back_again = client.get(&format!("/api/v1/rooms/{id}")).await;
    assert_eq!(back_again.status(), 200);

    // Une autre clé de signature : rien ne rouvre.
    unsafe { std::env::set_var("ALLEY_SIGNING_KEY", "77".repeat(32)) };
    let other_key = alley_with("memory").await;
    let (_, known) = post(&other_key, "/api/v1/known", signed("Trois jours")).await;
    assert!(known["rooms"].as_array().unwrap().is_empty());
    unsafe { std::env::set_var("ALLEY_SIGNING_KEY", &key) };

    // Le portail de Stripe ; une résiliation se voit au retour.
    let (_, portal) = post(
        &client,
        "/api/v1/billing/portal",
        json!({ "secret": secret, "subscription": "sub_1", "return_to": back }),
    )
    .await;
    assert_eq!(portal["url"], "https://billing.stripe.test/session");
    assert_eq!(STRIPE.lock().unwrap().portal_customer, "cus_test");
    set_status("sub_1", "canceled");
    let (_, status) = post(
        &client,
        "/api/v1/billing/status",
        json!({ "secret": secret, "subscription": "sub_1" }),
    )
    .await;
    assert_eq!(
        (status["active"].as_bool(), status["days"].as_u64()),
        (Some(false), Some(1))
    );

    // Un abonnement à l'année n'ouvre aucun droit.
    let year = "7a".repeat(32);
    add_sub("sub_year", &year, "7d", "year");
    let (_, status) = post(
        &client,
        "/api/v1/billing/status",
        json!({ "secret": year, "subscription": "sub_year" }),
    )
    .await;
    assert_eq!(
        (status["active"].as_bool(), status["status"].as_str()),
        (Some(false), Some("not_monthly"))
    );

    // Avec SQLite (un hébergeur au disque durable), les salons de plusieurs
    // jours rouvrent d'eux-mêmes au démarrage.
    let dir = std::env::temp_dir().join(format!("alley-billing-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let db = format!("sqlite://{}", dir.join("alley.db").display());
    let sqlite = alley_with(&db).await;
    let secret7 = "8c".repeat(32);
    add_sub("sub_7", &secret7, "7d", "month");
    let week = json!({ "name": "Semaine", "billing": secret7, "subscription": "sub_7" });
    let (_, week) = post(&sqlite, "/api/v1/rooms", week).await;
    assert!(left(&week) > 6 * DAY && left(&week) <= 7 * DAY);
    let restarted = alley_with(&db).await;
    let wid = week["id"].as_str().unwrap();
    let found = restarted.get(&format!("/api/v1/rooms/{wid}")).await;
    assert_eq!(found.status(), 200);
    let _ = std::fs::remove_dir_all(&dir);
}
