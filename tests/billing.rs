//! Les abonnements, de bout en bout, contre un faux Stripe : le paiement se
//! demande au mois seulement, un webhook mal signé est refusé, l'abonné crée
//! des salons de 3 ou 7 jours qui survivent à un redémarrage, et un
//! abonnement à l'année n'ouvre aucun droit.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use hmac::Mac;
use paquets::prelude::*;
use paquets::serde_json::{Value, json};
use sha2::Sha256;

/// Ce que le faux Stripe a reçu, et l'abonnement qu'il décrit.
static STRIPE: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(Default::default);

fn form(body: &str) -> HashMap<String, String> {
    body.split('&')
        .filter_map(|kv| kv.split_once('='))
        .map(|(k, v)| (decode(k), decode(v)))
        .collect()
}

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

async fn fake_checkout(req: Request) -> Json<Value> {
    let body = req.text().await.unwrap();
    let auth = req.header("authorization").unwrap_or_default().to_owned();
    let mut seen = STRIPE.lock().unwrap();
    seen.insert("auth".into(), auth);
    seen.extend(form(&body));
    Json(json!({ "id": "cs_test", "url": "https://checkout.stripe.test/cs_test" }))
}

async fn fake_subscription(req: Request) -> (u16, Json<Value>) {
    let id = req.param("id").unwrap_or_default().to_owned();
    let seen = STRIPE.lock().unwrap();
    let get = |k: &str| seen.get(k).cloned().unwrap_or_default();
    let (interval, mut status) = match id.as_str() {
        "sub_month" => ("month", "active".to_owned()),
        "sub_year" => ("year", "active".to_owned()),
        "sub_canceled" => ("month", "canceled".to_owned()),
        _ => {
            let error = json!({ "error": { "message": "No such subscription" } });
            return (404, Json(error));
        }
    };
    if let Some(forced) = seen.get(&format!("status:{id}")) {
        status = forced.clone();
    }
    (
        200,
        Json(json!({
            "id": id,
            "customer": "cus_test",
            "status": status,
            "metadata": {
                "alley_billing": get("subscription_data[metadata][alley_billing]"),
                "alley_plan": get("subscription_data[metadata][alley_plan]"),
            },
            "items": { "data": [{
                "current_period_end": 4_102_444_800i64,
                "price": { "recurring": { "interval": interval, "interval_count": 1 } }
            }] }
        })),
    )
}

async fn fake_portal(req: Request) -> Json<Value> {
    let body = form(&req.text().await.unwrap());
    STRIPE
        .lock()
        .unwrap()
        .insert("portal_customer".into(), body["customer"].clone());
    Json(json!({ "url": "https://billing.stripe.test/session" }))
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

const WEBHOOK_SECRET: &str = "whsec_test";

fn signed(payload: &str) -> String {
    let t = alley::rooms::now_ms() / 1000;
    let mut mac = <hmac::Hmac<Sha256> as Mac>::new_from_slice(WEBHOOK_SECRET.as_bytes()).unwrap();
    mac.update(format!("{t}.{payload}").as_bytes());
    let sig: String = mac
        .finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("t={t},v1={sig}")
}

async fn webhook(client: &paquets::test::TestClient, event: Value) -> u16 {
    let payload = event.to_string();
    client
        .post("/api/v1/billing/webhook")
        .header("content-type", "application/json")
        .header("stripe-signature", &signed(&payload))
        .body(payload)
        .await
        .status()
        .as_u16()
}

async fn alley_with(db: &str) -> paquets::test::TestClient {
    let mut app = alley::app();
    app.database(db.to_owned());
    app.test().await
}

#[tokio::test]
async fn monthly_subscriptions_make_rooms_last_three_or_seven_days() {
    let stripe = fake_stripe().await;
    // SAFETY : ce fichier de tests ne contient que ce test, rien ne lit
    // l'environnement en même temps.
    unsafe {
        std::env::set_var("STRIPE_SECRET_KEY", "sk_test_123");
        std::env::set_var("STRIPE_WEBHOOK_SECRET", WEBHOOK_SECRET);
        std::env::set_var("STRIPE_API_BASE", format!("http://127.0.0.1:{stripe}"));
    }
    let dir = std::env::temp_dir().join(format!("alley-billing-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let db = format!("sqlite://{}", dir.join("alley.db").display());
    // Sans base durable, l'offre n'est pas proposée (un abonnement serait
    // perdu au redémarrage).
    let memory = alley_with("memory").await;
    let plans: Value = memory.get("/api/v1/plans").await.json();
    assert_eq!(plans["enabled"], false);
    let res = memory
        .post("/api/v1/billing/checkout")
        .json(&json!({ "secret": "5e".repeat(32), "plan": "3d", "return_to": "https://maxlestage.github.io/" }))
        .await;
    assert_eq!(res.status(), 503);
    let client = alley_with(&db).await;

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
    // Une adresse de retour étrangère est refusée.
    let res = client
        .post("/api/v1/billing/checkout")
        .json(&json!({ "secret": secret, "plan": "3d", "return_to": "https://evil.example/" }))
        .await;
    assert_eq!(res.status(), 400);
    // Une formule à l'année n'existe pas.
    let res = client
        .post("/api/v1/billing/checkout")
        .json(&json!({ "secret": secret, "plan": "1y", "return_to": back }))
        .await;
    assert_eq!(res.status(), 400);

    let res = client
        .post("/api/v1/billing/checkout")
        .json(&json!({ "secret": secret, "plan": "3d", "lang": "fr", "return_to": back }))
        .await;
    assert_eq!(res.status(), 200, "{}", res.text());
    assert_eq!(
        res.json::<Value>()["url"],
        "https://checkout.stripe.test/cs_test"
    );
    {
        let seen = STRIPE.lock().unwrap();
        assert_eq!(seen["auth"], "Bearer sk_test_123");
        assert_eq!(seen["mode"], "subscription");
        assert_eq!(
            seen["line_items[0][price_data][recurring][interval]"],
            "month"
        );
        assert_eq!(
            seen["line_items[0][price_data][recurring][interval_count]"],
            "1"
        );
        assert_eq!(seen["line_items[0][price_data][unit_amount]"], "499");
        assert_eq!(seen["line_items[0][price_data][currency]"], "eur");
        assert_eq!(seen["success_url"], format!("{back}#abonnement=merci"));
        assert_eq!(seen["locale"], "fr");
        // Stripe ne reçoit que l'empreinte du secret.
        assert_ne!(seen["client_reference_id"], secret);
        assert!(!seen.values().any(|v| v.contains(&secret)));
    }

    // Avant le paiement : un salon ordinaire, jusqu'à minuit.
    let create =
        |name: &str| json!({ "name": name, "private": true, "tz": 120, "billing": secret });
    let room: Value = client
        .post("/api/v1/rooms")
        .json(&create("Avant"))
        .await
        .json();
    let day = 24 * 3600 * 1000;
    assert!(room["expires_at"].as_i64().unwrap() <= alley::rooms::now_ms() + day);

    // Un webhook mal signé est refusé ; un vrai fait relire l'abonnement.
    let event = json!({ "type": "checkout.session.completed",
        "data": { "object": { "subscription": "sub_month" } } });
    let forged = client
        .post("/api/v1/billing/webhook")
        .header("stripe-signature", "t=1,v1=00")
        .body(event.to_string())
        .await;
    assert_eq!(forged.status(), 400);
    let status: Value = client
        .post("/api/v1/billing/status")
        .json(&json!({ "secret": secret }))
        .await
        .json();
    assert_eq!(status["active"], false);
    assert_eq!(webhook(&client, event).await, 200);
    let status: Value = client
        .post("/api/v1/billing/status")
        .json(&json!({ "secret": secret }))
        .await
        .json();
    assert_eq!(
        (status["active"].as_bool(), status["days"].as_u64()),
        (Some(true), Some(3)),
        "{status}"
    );

    // Abonné : ses salons durent 3 jours (jusqu'au minuit du troisième).
    let room: Value = client
        .post("/api/v1/rooms")
        .json(&create("Trois jours"))
        .await
        .json();
    let left = room["expires_at"].as_i64().unwrap() - alley::rooms::now_ms();
    assert!(left > 2 * day && left <= 3 * day, "{left}");
    let id = room["id"].as_str().unwrap().to_owned();
    // Un compte déjà abonné ne repaie pas.
    let again = client
        .post("/api/v1/billing/checkout")
        .json(&json!({ "secret": secret, "plan": "7d", "return_to": back }))
        .await;
    assert_eq!(again.status(), 409);
    // Le portail de Stripe, pour résilier.
    let portal = client
        .post("/api/v1/billing/portal")
        .json(&json!({ "secret": secret, "return_to": back }))
        .await;
    assert_eq!(
        portal.json::<Value>()["url"],
        "https://billing.stripe.test/session"
    );
    assert_eq!(STRIPE.lock().unwrap()["portal_customer"], "cus_test");

    // Le serveur redémarre : le salon de trois jours est toujours là, avec la
    // même clé de signature.
    let key: Value = client.get("/api/v1/key").await.json();
    // (Une seconde instance sur la même base, comme après un redémarrage.)
    let client = alley_with(&db).await;
    let back_again = client.get(&format!("/api/v1/rooms/{id}")).await;
    assert_eq!(back_again.status(), 200, "le salon survit au redémarrage");
    assert_eq!(back_again.json::<Value>()["name"], "Trois jours");
    let key_again: Value = client.get("/api/v1/key").await.json();
    assert_eq!(key, key_again);

    // Un abonnement à l'année n'ouvre aucun droit.
    let year = "7a".repeat(32);
    client
        .post("/api/v1/billing/checkout")
        .json(&json!({ "secret": year, "plan": "7d", "return_to": back }))
        .await;
    let event = json!({ "type": "customer.subscription.created",
        "data": { "object": { "id": "sub_year" } } });
    assert_eq!(webhook(&client, event).await, 200);
    let status: Value = client
        .post("/api/v1/billing/status")
        .json(&json!({ "secret": year }))
        .await
        .json();
    assert_eq!(
        (status["active"].as_bool(), status["status"].as_str()),
        (Some(false), Some("not_monthly"))
    );
    let room: Value = client
        .post("/api/v1/rooms")
        .json(&json!({ "name": "Annuel", "billing": year }))
        .await
        .json();
    assert!(room["expires_at"].as_i64().unwrap() <= alley::rooms::now_ms() + day);

    // Résilié : retour aux salons d'un jour.
    let event = json!({ "type": "customer.subscription.deleted",
        "data": { "object": { "id": "sub_canceled" } } });
    STRIPE.lock().unwrap().insert(
        "subscription_data[metadata][alley_billing]".into(),
        billing_of(&secret),
    );
    assert_eq!(webhook(&client, event).await, 200);
    let status: Value = client
        .post("/api/v1/billing/status")
        .json(&json!({ "secret": secret }))
        .await
        .json();
    assert_eq!(
        status["active"], true,
        "un ancien abonnement résilié n'efface pas l'actif"
    );

    // L'abonnement lui-même est résilié : les salons redeviennent d'un jour.
    STRIPE
        .lock()
        .unwrap()
        .insert("status:sub_month".into(), "canceled".into());
    let event = json!({ "type": "customer.subscription.deleted",
        "data": { "object": { "id": "sub_month" } } });
    assert_eq!(webhook(&client, event).await, 200);
    let status: Value = client
        .post("/api/v1/billing/status")
        .json(&json!({ "secret": secret }))
        .await
        .json();
    assert_eq!(
        (status["active"].as_bool(), status["days"].as_u64()),
        (Some(false), Some(1))
    );
    let room: Value = client
        .post("/api/v1/rooms")
        .json(&create("Après"))
        .await
        .json();
    assert!(room["expires_at"].as_i64().unwrap() <= alley::rooms::now_ms() + day);

    let _ = std::fs::remove_dir_all(&dir);
}

fn billing_of(secret: &str) -> String {
    alley::billing::billing_id(secret).unwrap()
}
