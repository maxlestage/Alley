//! Les périodes payantes, de bout en bout, contre un faux Stripe : un
//! paiement unique (rien de mensuel, rien d'annuel), relu chez Stripe ; le
//! navigateur garde son paiement et les descriptions signées des salons, qui
//! rouvrent après un redémarrage sans base.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use paquets::prelude::*;
use paquets::serde_json::{Value, json};

/// Un paiement du faux Stripe.
#[derive(Clone)]
struct Payment {
    billing: String,
    plan: String,
    tz: String,
    amount: u64,
    status: String,
    refunded: bool,
}

#[derive(Default)]
struct Fake {
    /// Le dernier formulaire de Checkout.
    checkout: HashMap<String, String>,
    auth: String,
    payments: HashMap<String, Payment>,
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

fn payment_json(id: &str, p: &Payment) -> Value {
    json!({
        "id": id, "status": p.status, "currency": "eur",
        "created": alley::rooms::now_ms() / 1000,
        "amount_received": if p.status == "succeeded" { p.amount } else { 0 },
        "metadata": { "alley_billing": p.billing, "alley_plan": p.plan, "alley_tz": p.tz },
        "latest_charge": { "refunded": p.refunded, "amount_refunded": if p.refunded { p.amount } else { 0 } }
    })
}

async fn fake_checkout(req: Request) -> Json<Value> {
    let f = form(&req.text().await.unwrap());
    let mut fake = STRIPE.lock().unwrap();
    fake.auth = req.header("authorization").unwrap_or_default().to_owned();
    // Un paiement en attente.
    let id = format!("pi_{}", fake.payments.len() + 1);
    let payment = Payment {
        billing: f["payment_intent_data[metadata][alley_billing]"].clone(),
        plan: f["payment_intent_data[metadata][alley_plan]"].clone(),
        tz: f["payment_intent_data[metadata][alley_tz]"].clone(),
        amount: f["line_items[0][price_data][unit_amount]"].parse().unwrap(),
        status: "requires_payment_method".into(),
        refunded: false,
    };
    fake.payments.insert(id.clone(), payment);
    fake.checkout = f;
    Json(
        json!({ "id": format!("cs_{id}"), "url": format!("https://checkout.stripe.test/cs_{id}") }),
    )
}

async fn fake_session(req: Request) -> (u16, Json<Value>) {
    let id = req.param("id").unwrap_or_default().to_owned();
    let fake = STRIPE.lock().unwrap();
    let pi = id.trim_start_matches("cs_");
    match fake.payments.get(pi) {
        Some(p) => (
            200,
            Json(json!({
                "id": id, "payment_intent": pi,
                "payment_status": if p.status == "succeeded" { "paid" } else { "unpaid" },
                "metadata": { "alley_billing": p.billing }
            })),
        ),
        None => (
            404,
            Json(json!({ "error": { "message": "No such session" } })),
        ),
    }
}

async fn fake_payment(req: Request) -> (u16, Json<Value>) {
    let id = req.param("id").unwrap_or_default().to_owned();
    let fake = STRIPE.lock().unwrap();
    if id == "search" {
        let query = req
            .query("query")
            .map(|q| q.into_owned())
            .unwrap_or_default();
        let data: Vec<Value> = fake
            .payments
            .iter()
            .filter(|(_, p)| query.contains(&p.billing) && p.status == "succeeded")
            .map(|(id, p)| payment_json(id, p))
            .collect();
        return (200, Json(json!({ "data": data })));
    }
    match fake.payments.get(&id) {
        Some(p) => (200, Json(payment_json(&id, p))),
        None => (
            404,
            Json(json!({ "error": { "message": "No such payment_intent" } })),
        ),
    }
}

fn pay(id: &str) {
    STRIPE.lock().unwrap().payments.get_mut(id).unwrap().status = "succeeded".into();
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
    app.get("/v1/payment_intents/:id", fake_payment);
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
async fn a_period_is_paid_once_and_rooms_last_until_it_ends() {
    let stripe = fake_stripe().await;
    // SAFETY : ce fichier de tests ne contient que ce test, rien ne lit
    // l'environnement en même temps.
    unsafe {
        std::env::set_var("STRIPE_SECRET_KEY", "sk_test_123");
        std::env::set_var("STRIPE_API_BASE", format!("http://127.0.0.1:{stripe}"));
    }
    // Sans clé de signature durable, l'offre n'est pas proposée.
    let no_key = alley_with("memory").await;
    let plans: Value = no_key.get("/api/v1/plans").await.json();
    assert_eq!(plans["enabled"], false);

    let key = "4b".repeat(32);
    unsafe { std::env::set_var("ALLEY_SIGNING_KEY", &key) };
    let client = alley_with("memory").await;

    // Les périodes : payées une fois.
    let plans: Value = client.get("/api/v1/plans").await.json();
    assert_eq!(plans["enabled"], true);
    assert_eq!(
        (plans["currency"].as_str(), plans["mode"].as_str()),
        (Some("eur"), Some("payment"))
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
    let checkout = |plan: &str, back: &str| json!({ "secret": secret, "plan": plan, "lang": "fr", "tz": 120, "return_to": back, "consent": true });
    // Sans accord exprès (conditions, renonciation à la rétractation) : refusé.
    let mut no_consent = checkout("3d", back);
    no_consent["consent"] = json!(false);
    let (status, body) = post(&client, "/api/v1/billing/checkout", no_consent).await;
    assert_eq!(
        (status, body["error"].as_str()),
        (400, Some("consent_required"))
    );
    let evil = checkout("3d", "https://evil.example/");
    assert_eq!(post(&client, "/api/v1/billing/checkout", evil).await.0, 400);
    let monthly = checkout("1m", back);
    assert_eq!(
        post(&client, "/api/v1/billing/checkout", monthly).await.0,
        400
    );
    let (status, body) = post(&client, "/api/v1/billing/checkout", checkout("3d", back)).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["url"], "https://checkout.stripe.test/cs_pi_1");
    {
        let fake = STRIPE.lock().unwrap();
        let f = &fake.checkout;
        assert_eq!(fake.auth, "Bearer sk_test_123");
        // Un paiement unique : ni abonnement, ni rythme.
        assert_eq!(f["mode"], "payment");
        assert!(
            !f.keys()
                .any(|k| k.contains("recurring") || k.contains("subscription"))
        );
        assert_eq!(f["line_items[0][price_data][unit_amount]"], "499");
        assert_eq!(f["line_items[0][price_data][currency]"], "eur");
        assert_eq!(f["payment_intent_data[metadata][alley_tz]"], "120");
        assert!(
            f["payment_intent_data[metadata][alley_consent]"]
                .parse::<i64>()
                .is_ok()
        );
        assert_eq!(
            f["success_url"],
            format!("{back}#periode={{CHECKOUT_SESSION_ID}}")
        );
        // Stripe ne reçoit que l'empreinte du secret.
        assert!(!f.values().any(|v| v.contains(&secret)));
    }

    // Pas encore payé : un salon ordinaire, et la session ne donne rien.
    let create = |name: &str, payment: Option<&str>| json!({ "name": name, "private": true, "tz": 120, "billing": secret, "payment": payment });
    let (_, room) = post(&client, "/api/v1/rooms", create("Avant", None)).await;
    assert!(left(&room) <= DAY);
    let (_, status) = post(
        &client,
        "/api/v1/billing/status",
        json!({ "secret": secret, "session": "cs_pi_1" }),
    )
    .await;
    assert_eq!(status["active"], false);

    // Payé ; de retour de Checkout, la session mène au paiement.
    pay("pi_1");
    let (_, status) = post(
        &client,
        "/api/v1/billing/status",
        json!({ "secret": secret, "session": "cs_pi_1" }),
    )
    .await;
    assert_eq!(
        (
            status["active"].as_bool(),
            status["days"].as_u64(),
            status["payment"].as_str()
        ),
        (Some(true), Some(3), Some("pi_1")),
        "{status}"
    );
    let until = status["until"].as_i64().unwrap();
    let period = until - alley::rooms::now_ms();
    assert!(period > 2 * DAY && period <= 3 * DAY, "{period}");
    // La session payée par un autre compte ne donne rien.
    let (_, other) = post(
        &client,
        "/api/v1/billing/status",
        json!({ "secret": "6f".repeat(32), "session": "cs_pi_1" }),
    )
    .await;
    assert_eq!(other["active"], false);

    // Pendant la période, ses salons restent ouverts jusqu'à sa fin.
    let (_, room) = post(
        &client,
        "/api/v1/rooms",
        create("Trois jours", Some("pi_1")),
    )
    .await;
    assert_eq!(room["expires_at"].as_i64(), Some(until));
    // Un autre appareil, qui ne connaît pas le paiement : Stripe le retrouve.
    let (_, room2) = post(&client, "/api/v1/rooms", create("Autre appareil", None)).await;
    assert_eq!(room2["expires_at"].as_i64(), Some(until));
    // Le paiement d'un autre ne compte pas.
    let stolen = json!({ "name": "Volé", "billing": "6f".repeat(32), "payment": "pi_1" });
    let (_, stolen) = post(&client, "/api/v1/rooms", stolen).await;
    assert!(left(&stolen) <= DAY);
    // Une période en cours : pas de second paiement.
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
    // Une autre clé de signature : rien ne rouvre.
    unsafe { std::env::set_var("ALLEY_SIGNING_KEY", "77".repeat(32)) };
    let other_key = alley_with("memory").await;
    let (_, known) = post(&other_key, "/api/v1/known", signed("Trois jours")).await;
    assert!(known["rooms"].as_array().unwrap().is_empty());
    unsafe { std::env::set_var("ALLEY_SIGNING_KEY", &key) };

    // Remboursé : la période n'ouvre plus de droits.
    STRIPE
        .lock()
        .unwrap()
        .payments
        .get_mut("pi_1")
        .unwrap()
        .refunded = true;
    let fresh = alley_with("memory").await;
    let (_, status) = post(
        &fresh,
        "/api/v1/billing/status",
        json!({ "secret": secret, "payment": "pi_1" }),
    )
    .await;
    assert_eq!(
        (status["active"].as_bool(), status["status"].as_str()),
        (Some(false), Some("refunded"))
    );
    let (_, after) = post(&fresh, "/api/v1/rooms", create("Après", Some("pi_1"))).await;
    assert!(left(&after) <= DAY);

    // Avec SQLite, les salons de plusieurs jours rouvrent d'eux-mêmes.
    let dir = std::env::temp_dir().join(format!("alley-billing-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let db = format!("sqlite://{}", dir.join("alley.db").display());
    let sqlite = alley_with(&db).await;
    let secret7 = "8c".repeat(32);
    let (_, body) = post(
        &sqlite,
        "/api/v1/billing/checkout",
        json!({ "secret": secret7, "plan": "7d", "tz": 0, "return_to": back, "consent": true }),
    )
    .await;
    let pi = body["url"]
        .as_str()
        .unwrap()
        .rsplit("cs_")
        .next()
        .unwrap()
        .to_owned();
    pay(&pi);
    let week = json!({ "name": "Semaine", "billing": secret7, "payment": pi });
    let (_, week) = post(&sqlite, "/api/v1/rooms", week).await;
    assert!(left(&week) > 6 * DAY && left(&week) <= 7 * DAY);
    let restarted = alley_with(&db).await;
    let wid = week["id"].as_str().unwrap();
    let found = restarted.get(&format!("/api/v1/rooms/{wid}")).await;
    assert_eq!(found.status(), 200);
    let _ = std::fs::remove_dir_all(&dir);
}
