//! Les abonnements payants, avec **Stripe**, **au mois seulement** :
//!
//! | Formule | Salons qui durent | Prix |
//! |---|---|---|
//! | `3d` | 3 jours | 4,99 € par mois |
//! | `7d` | 7 jours | 9,99 € par mois |
//!
//! Il n'existe **aucune formule à l'année** : le prix est envoyé à Stripe
//! avec son rythme (`recurring[interval]=month`, tous les mois) à chaque
//! paiement, sans prix configuré à l'avance qu'on pourrait changer, et un
//! abonnement dont le rythme ne serait pas mensuel n'ouvre aucun droit.
//!
//! Un abonnement apporte une chose : les salons que l'abonné crée restent
//! ouverts 3 ou 7 jours au lieu de disparaître à minuit. **Aucun message
//! n'est gardé** pour autant : on revient dans le salon, pas dans son
//! historique.
//!
//! Qui paie ? Le compte (pseudonyme + mot de passe) : le navigateur en tire
//! un **secret d'abonnement** (HKDF de la graine du mot de passe), envoyé
//! dans le corps des requêtes, jamais dans une adresse. Le serveur n'en garde
//! que l'empreinte, qui relie l'abonnement Stripe au compte ; il ne connaît
//! ni le pseudonyme ni le mot de passe. Stripe, lui, connaît l'e-mail et le
//! moyen de paiement : Alley ne les voit jamais (Stripe Checkout et le
//! portail client de Stripe s'en chargent).
//!
//! Ce qui est gardé en base (`DATABASE_URL`, Heroku Postgres) : les
//! abonnements, les salons de plusieurs jours (identifiant, nom, type, fin)
//! et la clé qui signe ce que le serveur dit des salons. Rien d'autre.
//!
//! Réglages : `STRIPE_SECRET_KEY`, `STRIPE_WEBHOOK_SECRET` (le webhook :
//! `POST /api/v1/billing/webhook`, événements `checkout.session.completed` et
//! `customer.subscription.*`). Sans eux, l'offre n'est pas proposée.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use hmac::Mac;
use paquets::prelude::*;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::rooms::{Kept, Rooms, now_ms};

/// Une formule d'abonnement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    pub id: &'static str,
    /// Combien de jours durent les salons de l'abonné.
    pub days: u32,
    /// Le prix mensuel, en centimes d'euro.
    pub cents: u32,
}

pub const PLANS: [Plan; 2] = [
    Plan {
        id: "3d",
        days: 3,
        cents: 499,
    },
    Plan {
        id: "7d",
        days: 7,
        cents: 999,
    },
];

pub const CURRENCY: &str = "eur";
/// Le seul rythme de paiement : chaque mois.
pub const INTERVAL: &str = "month";

pub fn plan(id: &str) -> Option<Plan> {
    PLANS.into_iter().find(|p| p.id == id)
}

/// Un abonnement, tel que Stripe le décrit (relu à chaque événement).
#[model(table = "alley_subscriptions")]
pub struct Subscription {
    /// L'empreinte du secret d'abonnement du compte ([`billing_id`]).
    #[memoire(primary_key)]
    pub id: String,
    pub customer: String,
    pub subscription: String,
    pub plan: String,
    pub status: String,
    /// La fin de la période payée, en millisecondes.
    pub period_end: i64,
}

impl Subscription {
    /// Ouvre-t-il des droits maintenant ? Actif ; ou un paiement en retard,
    /// jusqu'à la fin du mois déjà payé.
    pub fn active(&self, now: i64) -> bool {
        match self.status.as_str() {
            "active" | "trialing" => true,
            "past_due" => now < self.period_end,
            _ => false,
        }
    }

    /// Combien de jours durent les salons qu'il crée.
    pub fn days(&self, now: i64) -> u32 {
        match plan(&self.plan) {
            Some(p) if self.active(now) => p.days,
            _ => 1,
        }
    }
}

/// Un salon de plusieurs jours (voir [`Kept`]).
#[model(table = "alley_rooms")]
pub struct KeptRoom {
    #[memoire(primary_key)]
    pub id: String,
    pub name: String,
    pub private: bool,
    pub expires_at: i64,
}

/// Un réglage gardé en base (la clé qui signe les salons).
#[model(table = "alley_settings")]
pub struct Setting {
    #[memoire(primary_key)]
    pub name: String,
    pub value: String,
}

const SIGNING_KEY: &str = "signing_key";

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}

/// L'identifiant d'abonnement d'un compte : l'empreinte de son secret
/// (64 caractères hexadécimaux), ou `None` si ce n'en est pas un.
pub fn billing_id(secret: &str) -> Option<String> {
    let ok = secret.len() == 64 && secret.bytes().all(|b| b.is_ascii_hexdigit());
    ok.then(|| hex(&Sha256::digest(format!("alley-billing-v1:{secret}"))))
}

/// Au démarrage : la clé de signature des salons (tirée une fois, puis
/// relue), et les salons de plusieurs jours encore ouverts.
pub async fn restore(db: &Db, rooms: &Rooms) -> paquets::Result<()> {
    match Setting::find(db, SIGNING_KEY.to_owned()).await? {
        Some(s) => {
            if let Some(seed) = unhex(&s.value).and_then(|b| <[u8; 32]>::try_from(b).ok()) {
                rooms.set_signing_key(seed);
            }
        }
        None => {
            Setting {
                name: SIGNING_KEY.to_owned(),
                value: hex(&rooms.signing_seed()),
            }
            .upsert(db)
            .await?;
        }
    }
    KeptRoom::delete_many()
        .filter(KeptRoom::EXPIRES_AT.le(now_ms()))
        .execute(db)
        .await?;
    let kept = KeptRoom::all(db).await?;
    rooms.restore(
        kept.into_iter()
            .map(|k| Kept {
                id: k.id,
                name: k.name,
                private: k.private,
                expires_at: k.expires_at,
            })
            .collect(),
    );
    Ok(())
}

/// Garde en base un salon de plusieurs jours.
pub async fn keep(db: &Db, kept: Kept) -> paquets::Result<()> {
    KeptRoom::delete_many()
        .filter(KeptRoom::EXPIRES_AT.le(now_ms()))
        .execute(db)
        .await?;
    KeptRoom {
        id: kept.id,
        name: kept.name,
        private: kept.private,
        expires_at: kept.expires_at,
    }
    .upsert(db)
    .await?;
    Ok(())
}

/// Combien de jours durent les salons que crée ce compte (1 sans
/// abonnement).
pub async fn days_for(db: &Db, secret: Option<&str>) -> u32 {
    let Some(id) = secret.and_then(billing_id) else {
        return 1;
    };
    match Subscription::find(db, id).await {
        Ok(Some(sub)) => sub.days(now_ms()),
        _ => 1,
    }
}

/// La signature d'un webhook de Stripe (`Stripe-Signature: t=…,v1=…`) :
/// HMAC-SHA256 de `t.corps` avec le secret du webhook, au plus 5 minutes
/// d'écart (un ancien événement rejoué est refusé).
pub fn verify_signature(secret: &str, payload: &str, header: &str, now_secs: i64) -> bool {
    let mut t: Option<i64> = None;
    let mut sigs = Vec::new();
    for part in header.split(',') {
        match part.trim().split_once('=') {
            Some(("t", v)) => t = v.parse().ok(),
            Some(("v1", v)) => sigs.push(v),
            _ => {}
        }
    }
    let Some(t) = t else { return false };
    if (now_secs - t).abs() > 300 {
        return false;
    }
    sigs.iter().filter_map(|s| unhex(s)).any(|sig| {
        let mut mac = <hmac::Hmac<Sha256> as Mac>::new_from_slice(secret.as_bytes())
            .expect("toute longueur de clé");
        mac.update(format!("{t}.{payload}").as_bytes());
        mac.verify_slice(&sig).is_ok()
    })
}

/// Stripe, s'il est configuré.
#[derive(Clone)]
pub struct Stripe {
    secret: String,
    webhook_secret: String,
    /// `https://api.stripe.com` (`STRIPE_API_BASE` pour les tests).
    base: String,
}

impl Stripe {
    pub fn from_env() -> Option<Stripe> {
        let var = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
        Some(Stripe {
            secret: var("STRIPE_SECRET_KEY")?,
            webhook_secret: var("STRIPE_WEBHOOK_SECRET")?,
            base: var("STRIPE_API_BASE").unwrap_or_else(|| "https://api.stripe.com".into()),
        })
    }

    /// Un appel à l'API de Stripe (formulaire encodé), hors du fil des
    /// requêtes.
    async fn call(
        &self,
        method: &'static str,
        path: &str,
        form: Vec<(String, String)>,
    ) -> Result<Value, String> {
        let url = format!("{}{path}", self.base.trim_end_matches('/'));
        let auth = format!("Bearer {}", self.secret);
        tokio::task::spawn_blocking(move || {
            let agent: ureq::Agent = ureq::Agent::config_builder()
                .http_status_as_error(false)
                .timeout_global(Some(Duration::from_secs(20)))
                .build()
                .into();
            let res = if method == "GET" {
                agent.get(&url).header("Authorization", &auth).call()
            } else {
                agent
                    .post(&url)
                    .header("Authorization", &auth)
                    .send_form(form.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            }
            .map_err(|e| format!("Stripe : {e}"))?;
            let status = res.status().as_u16();
            let text = res.into_body().read_to_string().unwrap_or_default();
            let body: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
            if status >= 300 {
                return Err(format!(
                    "Stripe {status} : {}",
                    body["error"]["message"].as_str().unwrap_or(&text)
                ));
            }
            Ok(body)
        })
        .await
        .map_err(|e| e.to_string())?
    }
}

/// Le réglage des abonnements, dans l'état de l'application.
#[derive(Clone)]
pub struct Billing {
    pub stripe: Option<Stripe>,
    /// Une vraie base est-elle branchée ? Sans elle (en mémoire), un
    /// abonnement payé serait perdu au premier redémarrage : l'offre n'est
    /// alors pas proposée.
    stored: Arc<AtomicBool>,
}

impl Billing {
    pub fn from_env() -> Billing {
        Billing {
            stripe: Stripe::from_env(),
            stored: Arc::default(),
        }
    }

    /// Au démarrage : la base est-elle durable ?
    pub fn set_database(&self, kind: &str) {
        let stored = kind != "memory";
        self.stored.store(stored, Ordering::Relaxed);
        if self.stripe.is_some() && !stored {
            tracing::warn!(
                "abonnements désactivés : Stripe est configuré, mais sans base durable (DATABASE_URL), \
                 un abonnement payé serait perdu au premier redémarrage"
            );
        }
    }

    /// Peut-on vendre un abonnement ? Stripe, et une base durable.
    pub fn ready(&self) -> Option<&Stripe> {
        self.stripe
            .as_ref()
            .filter(|_| self.stored.load(Ordering::Relaxed))
    }
}

fn fail(status: u16, code: &str) -> (u16, Json<Value>) {
    (status, Json(json!({ "error": code })))
}

/// Une adresse de retour acceptable : sur ce site, ou sur une origine
/// autorisée (la version GitHub Pages).
fn return_url(req: &Request, wanted: &str) -> Option<String> {
    let host = req.header("host").unwrap_or_default();
    let mut origins = crate::allowed_origins();
    origins.push(format!("https://{host}"));
    if host.starts_with("localhost") || host.starts_with("127.0.0.1") {
        origins.push(format!("http://{host}"));
    }
    let ok = origins.iter().any(|o| {
        wanted == o.as_str()
            || wanted
                .strip_prefix(o.as_str())
                .is_some_and(|rest| rest.starts_with('/'))
    });
    (ok && !wanted.contains(['#', '"', '<', '>', ' '])).then(|| wanted.to_owned())
}

#[data]
pub struct Checkout {
    pub secret: String,
    pub plan: String,
    #[serde(default)]
    pub lang: String,
    #[serde(default)]
    pub return_to: String,
}

#[data]
pub struct Secret {
    pub secret: String,
    #[serde(default)]
    pub return_to: String,
}

/// `GET /api/v1/plans` : les formules, au mois seulement.
async fn plans(req: Request) -> Json<Value> {
    let enabled = req.state::<Billing>().ready().is_some();
    let plans: Vec<Value> = PLANS
        .iter()
        .map(|p| json!({ "id": p.id, "days": p.days, "amount": p.cents }))
        .collect();
    Json(json!({
        "enabled": enabled,
        "currency": CURRENCY,
        "interval": INTERVAL,
        "plans": plans,
    }))
}

/// `POST /api/v1/billing/checkout` : ouvre le paiement Stripe d'une
/// formule ; renvoie l'adresse où aller payer.
async fn checkout(req: Request) -> (u16, Json<Value>) {
    let Some(stripe) = req.state::<Billing>().ready().cloned() else {
        return fail(503, "billing_disabled");
    };
    let Ok(body) = req.json::<Checkout>().await else {
        return fail(400, "bad_request");
    };
    let (Some(id), Some(plan)) = (billing_id(&body.secret), plan(&body.plan)) else {
        return fail(400, "bad_request");
    };
    let Some(back) = return_url(&req, &body.return_to) else {
        return fail(400, "bad_return_url");
    };
    let existing = Subscription::find(req.db(), id.clone())
        .await
        .ok()
        .flatten();
    if existing.as_ref().is_some_and(|s| s.active(now_ms())) {
        return fail(409, "already_subscribed");
    }
    let locale = match body.lang.as_str() {
        "fr" | "en" | "es" => body.lang.clone(),
        _ => "auto".to_owned(),
    };
    let product = format!("Alley — salons de {} jours", plan.days);
    let mut form: Vec<(String, String)> = [
        ("mode", "subscription".to_owned()),
        ("line_items[0][quantity]", "1".to_owned()),
        ("line_items[0][price_data][currency]", CURRENCY.to_owned()),
        (
            "line_items[0][price_data][unit_amount]",
            plan.cents.to_string(),
        ),
        (
            "line_items[0][price_data][recurring][interval]",
            INTERVAL.to_owned(),
        ),
        (
            "line_items[0][price_data][recurring][interval_count]",
            "1".to_owned(),
        ),
        ("line_items[0][price_data][product_data][name]", product),
        ("client_reference_id", id.clone()),
        ("metadata[alley_billing]", id.clone()),
        ("metadata[alley_plan]", plan.id.to_owned()),
        ("subscription_data[metadata][alley_billing]", id.clone()),
        (
            "subscription_data[metadata][alley_plan]",
            plan.id.to_owned(),
        ),
        ("success_url", format!("{back}#abonnement=merci")),
        ("cancel_url", back),
        ("locale", locale),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v))
    .collect();
    if let Some(customer) = existing.map(|s| s.customer).filter(|c| !c.is_empty()) {
        form.push(("customer".into(), customer));
    }
    match stripe.call("POST", "/v1/checkout/sessions", form).await {
        Ok(session) => match session["url"].as_str() {
            Some(url) => (200, Json(json!({ "url": url }))),
            None => fail(502, "stripe"),
        },
        Err(e) => {
            tracing::warn!("{e}");
            fail(502, "stripe")
        }
    }
}

/// `POST /api/v1/billing/status` : l'abonnement d'un compte.
async fn status(req: Request) -> (u16, Json<Value>) {
    let Some(id) = req
        .json::<Secret>()
        .await
        .ok()
        .and_then(|b| billing_id(&b.secret))
    else {
        return fail(400, "bad_request");
    };
    let now = now_ms();
    let sub = Subscription::find(req.db(), id).await.ok().flatten();
    let body = match sub {
        Some(s) => json!({
            "active": s.active(now),
            "plan": s.plan,
            "days": s.days(now),
            "status": s.status,
            "until": s.period_end,
        }),
        None => json!({ "active": false, "days": 1 }),
    };
    (200, Json(body))
}

/// `POST /api/v1/billing/portal` : le portail de Stripe, pour gérer son
/// abonnement (résilier, changer de carte, factures).
async fn portal(req: Request) -> (u16, Json<Value>) {
    let Some(stripe) = req.state::<Billing>().stripe.clone() else {
        return fail(503, "billing_disabled");
    };
    let Ok(body) = req.json::<Secret>().await else {
        return fail(400, "bad_request");
    };
    let Some(id) = billing_id(&body.secret) else {
        return fail(400, "bad_request");
    };
    let Some(back) = return_url(&req, &body.return_to) else {
        return fail(400, "bad_return_url");
    };
    let Some(sub) = Subscription::find(req.db(), id).await.ok().flatten() else {
        return fail(404, "no_subscription");
    };
    let form = vec![
        ("customer".to_owned(), sub.customer),
        ("return_url".to_owned(), back),
    ];
    match stripe
        .call("POST", "/v1/billing_portal/sessions", form)
        .await
    {
        Ok(session) => match session["url"].as_str() {
            Some(url) => (200, Json(json!({ "url": url }))),
            None => fail(502, "stripe"),
        },
        Err(e) => {
            tracing::warn!("{e}");
            fail(502, "stripe")
        }
    }
}

/// Relit un abonnement chez Stripe et le garde : c'est Stripe qui fait foi,
/// pas le contenu de l'événement.
async fn sync(db: &Db, stripe: &Stripe, subscription: &str) -> Result<(), String> {
    if subscription.is_empty()
        || !subscription
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Ok(());
    }
    let sub = stripe
        .call(
            "GET",
            &format!("/v1/subscriptions/{subscription}"),
            Vec::new(),
        )
        .await?;
    let meta = &sub["metadata"];
    let (Some(id), Some(plan)) = (
        meta["alley_billing"].as_str().filter(|i| i.len() == 64),
        meta["alley_plan"].as_str().and_then(plan),
    ) else {
        // Pas un abonnement d'Alley.
        return Ok(());
    };
    let item = &sub["items"]["data"][0];
    let recurring = &item["price"]["recurring"];
    // Au mois, et seulement au mois : tout autre rythme n'ouvre aucun droit.
    let monthly =
        recurring["interval"] == INTERVAL && recurring["interval_count"].as_u64().unwrap_or(1) == 1;
    let status = if monthly {
        sub["status"].as_str().unwrap_or("incomplete")
    } else {
        "not_monthly"
    };
    let period_end = sub["current_period_end"]
        .as_i64()
        .or_else(|| item["current_period_end"].as_i64())
        .unwrap_or_default()
        * 1000;
    let record = Subscription {
        id: id.to_owned(),
        customer: sub["customer"].as_str().unwrap_or_default().to_owned(),
        subscription: sub["id"].as_str().unwrap_or(subscription).to_owned(),
        plan: plan.id.to_owned(),
        status: status.to_owned(),
        period_end,
    };
    let now = now_ms();
    // Un ancien abonnement qui se termine n'efface pas celui qui le remplace.
    if let Ok(Some(current)) = Subscription::find(db, record.id.clone()).await
        && current.subscription != record.subscription
        && current.active(now)
        && !record.active(now)
    {
        return Ok(());
    }
    record.upsert(db).await.map_err(|e| e.to_string())
}

/// `POST /api/v1/billing/webhook` : les événements de Stripe, signés.
async fn webhook(mut req: Request) -> (u16, Json<Value>) {
    req.set_body_limit(WEBHOOK_LIMIT);
    let Some(stripe) = req.state::<Billing>().stripe.clone() else {
        return fail(503, "billing_disabled");
    };
    let signature = req
        .header("stripe-signature")
        .unwrap_or_default()
        .to_owned();
    let Ok(payload) = req.text().await else {
        return fail(400, "bad_request");
    };
    let now = now_ms() / 1000;
    if !verify_signature(&stripe.webhook_secret, &payload, &signature, now) {
        return fail(400, "bad_signature");
    }
    let Ok(event) = serde_json::from_str::<Value>(&payload) else {
        return fail(400, "bad_request");
    };
    let object = &event["data"]["object"];
    let kind = event["type"].as_str().unwrap_or_default();
    let subscription = match kind {
        "checkout.session.completed" => object["subscription"].as_str(),
        k if k.starts_with("customer.subscription.") => object["id"].as_str(),
        _ => None,
    };
    let Some(subscription) = subscription else {
        return (200, Json(json!({ "received": true })));
    };
    match sync(req.db(), &stripe, subscription).await {
        Ok(()) => (200, Json(json!({ "received": true }))),
        Err(e) => {
            // Stripe réessaiera.
            tracing::warn!("webhook {kind} : {e}");
            fail(500, "sync_failed")
        }
    }
}

/// La taille maximale d'un événement de Stripe.
pub const WEBHOOK_LIMIT: usize = 256 * 1024;
pub const WEBHOOK_PATH: &str = "/api/v1/billing/webhook";

pub fn routes(app: &mut App) {
    app.get("/api/v1/plans", plans);
    app.post("/api/v1/billing/checkout", checkout);
    app.post("/api/v1/billing/status", status);
    app.post("/api/v1/billing/portal", portal);
    app.post(WEBHOOK_PATH, webhook);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_are_monthly_only() {
        assert_eq!(INTERVAL, "month");
        assert_eq!(plan("3d").map(|p| (p.days, p.cents)), Some((3, 499)));
        assert_eq!(plan("7d").map(|p| (p.days, p.cents)), Some((7, 999)));
        assert_eq!(plan("1y"), None);
    }

    #[test]
    fn a_subscription_opens_rights_while_paid() {
        let sub = |status: &str, period_end: i64| Subscription {
            id: "x".into(),
            customer: "cus".into(),
            subscription: "sub".into(),
            plan: "7d".into(),
            status: status.into(),
            period_end,
        };
        assert_eq!(sub("active", 0).days(10), 7);
        assert_eq!(sub("past_due", 20).days(10), 7, "le mois déjà payé");
        assert_eq!(sub("past_due", 5).days(10), 1);
        for status in ["canceled", "unpaid", "incomplete", "not_monthly"] {
            assert_eq!(sub(status, 99).days(10), 1, "{status}");
        }
    }

    #[test]
    fn billing_ids_need_a_real_secret() {
        let secret = "ab".repeat(32);
        let id = billing_id(&secret).unwrap();
        assert_eq!(id.len(), 64);
        assert_ne!(id, secret);
        assert!(billing_id("court").is_none());
        assert!(billing_id(&"zz".repeat(32)).is_none());
    }

    #[test]
    fn webhook_signatures_are_checked() {
        let sign = |secret: &str, t: i64, payload: &str| {
            let mut mac = <hmac::Hmac<Sha256> as Mac>::new_from_slice(secret.as_bytes()).unwrap();
            mac.update(format!("{t}.{payload}").as_bytes());
            hex(&mac.finalize().into_bytes())
        };
        let payload = r#"{"type":"customer.subscription.updated"}"#;
        let good = format!("t=1000,v1={}", sign("whsec_x", 1000, payload));
        assert!(verify_signature("whsec_x", payload, &good, 1100));
        assert!(
            !verify_signature("whsec_y", payload, &good, 1100),
            "autre secret"
        );
        assert!(
            !verify_signature("whsec_x", "{}", &good, 1100),
            "corps modifié"
        );
        assert!(
            !verify_signature("whsec_x", payload, &good, 2000),
            "trop ancien"
        );
        assert!(!verify_signature("whsec_x", payload, "t=1000", 1000));
        assert!(!verify_signature("whsec_x", payload, "", 1000));
        // Plusieurs signatures (secret en cours de renouvellement).
        let two = format!("t=1000,v1=00,v1={}", sign("whsec_x", 1000, payload));
        assert!(verify_signature("whsec_x", payload, &two, 1000));
    }
}
