//! Les périodes payantes, avec **Stripe** : on paie **une fois** une période
//! de 3 ou de 7 jours — ni abonnement, ni renouvellement, rien à résilier.
//!
//! | Période | Prix, en une fois |
//! |---|---|
//! | `3d` : 3 jours | 4,99 € |
//! | `7d` : 7 jours | 9,99 € |
//!
//! Pendant sa période, les salons que crée l'acheteur restent ouverts jusqu'à
//! la fin de celle-ci (le minuit, chez lui, qui clôt le 3ᵉ ou le 7ᵉ jour) au
//! lieu de disparaître à minuit. **Aucun message n'est gardé** pour autant.
//! Le paiement est un paiement unique (Stripe Checkout, `mode=payment`) :
//! rien n'est jamais reprélevé.
//!
//! **Rien n'est gardé sur le serveur** :
//!
//! - **Stripe fait foi.** De retour du paiement, le navigateur retrouve sa
//!   période par la session payée, et garde l'identifiant du paiement dans son
//!   `localStorage` (signé). Il le présente ; le serveur le relit chez Stripe
//!   (quelques minutes en cache, en mémoire), vérifie qu'il appartient à ce
//!   compte, qu'il est payé et pas remboursé. Sur un autre appareil, la
//!   recherche de Stripe le retrouve.
//! - Les **salons de plusieurs jours** survivent aux redémarrages grâce aux
//!   navigateurs : chacun garde la description du salon signée par le
//!   serveur, et la lui présente s'il l'a oubliée ([`Rooms::revive`]). Il
//!   suffit que la clé de signature ne change pas : `ALLEY_SIGNING_KEY`.
//! - **SQLite** (un fichier local, `alley.db`) garde en plus la clé de
//!   signature et les salons de plusieurs jours tant que le disque dure. Rien
//!   n'en dépend : sur Heroku, dont le disque s'efface à chaque redémarrage,
//!   le `localStorage` et Stripe suffisent.
//!
//! Qui paie ? Le compte (pseudonyme + mot de passe) : le navigateur en tire
//! un **secret de paiement** (HKDF de la graine du mot de passe), envoyé dans
//! le corps des requêtes, jamais dans une adresse. Stripe n'en reçoit que
//! l'empreinte ([`billing_id`]) ; le serveur ne connaît ni le pseudonyme ni
//! le mot de passe. L'e-mail et la carte restent chez Stripe.
//!
//! Réglage : `STRIPE_SECRET_KEY`, et une clé de signature durable
//! (`ALLEY_SIGNING_KEY`). Sans eux, l'offre n'est pas proposée.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use paquets::prelude::*;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::rooms::{Kept, Rooms, next_midnight, now_ms};

/// Une période qu'on peut payer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    pub id: &'static str,
    /// Combien de jours elle dure.
    pub days: u32,
    /// Son prix, payé une fois, en centimes d'euro.
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

pub fn plan(id: &str) -> Option<Plan> {
    PLANS.into_iter().find(|p| p.id == id)
}

const DAY_MS: i64 = 24 * 3600 * 1000;

/// La fin d'une période commencée à `start` (millisecondes) chez quelqu'un
/// décalé de `offset_min` minutes : le minuit qui clôt son dernier jour.
pub fn period_end(start: i64, offset_min: i64, days: u32) -> i64 {
    next_midnight(start, offset_min) + i64::from(days.max(1) - 1) * DAY_MS
}

/// Un salon de plusieurs jours, gardé dans SQLite.
#[model(table = "alley_rooms")]
pub struct KeptRoom {
    #[memoire(primary_key)]
    pub id: String,
    pub name: String,
    pub private: bool,
    pub expires_at: i64,
}

/// Un réglage gardé dans SQLite (la clé qui signe les salons).
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

/// La clé de signature des salons donnée par la configuration
/// (`ALLEY_SIGNING_KEY`, 64 caractères hexadécimaux).
pub fn env_signing_key() -> Option<[u8; 32]> {
    let text = std::env::var("ALLEY_SIGNING_KEY").ok()?;
    unhex(text.trim()).and_then(|b| b.try_into().ok())
}

/// L'identifiant de paiement d'un compte : l'empreinte de son secret
/// (64 caractères hexadécimaux), ou `None` si ce n'en est pas un.
pub fn billing_id(secret: &str) -> Option<String> {
    let ok = secret.len() == 64 && secret.bytes().all(|b| b.is_ascii_hexdigit());
    ok.then(|| hex(&Sha256::digest(format!("alley-billing-v1:{secret}"))))
}

/// Au démarrage, avec SQLite : la clé de signature des salons (si la
/// configuration n'en donne pas, elle y est tirée une fois), et les salons
/// de plusieurs jours qu'il a gardés.
pub async fn restore(db: &Db, rooms: &Rooms) -> paquets::Result<()> {
    if db.kind() == "memory" {
        return Ok(());
    }
    if env_signing_key().is_none() {
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

/// Garde un salon de plusieurs jours dans SQLite.
pub async fn keep(db: &Db, kept: Kept) -> paquets::Result<()> {
    if db.kind() == "memory" {
        return Ok(());
    }
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

/// Une période payée, telle que Stripe la décrit.
#[derive(Clone, Debug, PartialEq)]
pub struct Pass {
    /// Le paiement (`pi_…`).
    pub payment: String,
    pub plan: String,
    /// `paid`, `refunded` ou `pending`.
    pub status: String,
    /// Son début et sa fin, en millisecondes.
    pub start: i64,
    pub end: i64,
}

impl Pass {
    /// Ouvre-t-elle des droits maintenant ?
    pub fn active(&self, now: i64) -> bool {
        self.status == "paid" && now < self.end
    }

    pub fn to_json(&self, now: i64) -> Value {
        json!({
            "active": self.active(now),
            "plan": self.plan,
            "days": plan(&self.plan).map_or(1, |p| p.days),
            "status": self.status,
            "until": self.end,
            "payment": self.payment,
        })
    }
}

/// Lit un paiement de Stripe (un `PaymentIntent`, avec `latest_charge`) :
/// `None` s'il n'est pas à ce compte (`billing`) ou pas d'Alley.
pub fn pass(pi: &Value, billing: &str) -> Option<Pass> {
    let meta = &pi["metadata"];
    if meta["alley_billing"].as_str() != Some(billing) {
        return None;
    }
    let plan = plan(meta["alley_plan"].as_str()?)?;
    let offset: i64 = meta["alley_tz"]
        .as_str()
        .and_then(|t| t.parse().ok())
        .unwrap_or(0);
    let start = pi["created"].as_i64()? * 1000;
    let charge = &pi["latest_charge"];
    // Le montant entier, en euros : un paiement d'un autre montant n'ouvre
    // aucun droit.
    let full = pi["amount_received"].as_u64() == Some(u64::from(plan.cents))
        && pi["currency"].as_str() == Some(CURRENCY);
    let refunded = charge["refunded"].as_bool() == Some(true)
        || charge["amount_refunded"].as_u64().unwrap_or(0) > 0;
    let status = match pi["status"].as_str() {
        _ if refunded => "refunded",
        Some("succeeded") if full => "paid",
        _ => "pending",
    };
    Some(Pass {
        payment: pi["id"].as_str()?.to_owned(),
        plan: plan.id.to_owned(),
        status: status.to_owned(),
        start,
        end: period_end(start, offset, plan.days),
    })
}

/// Un identifiant de Stripe (`pi_…`, `cs_…`) : rien d'autre ne part dans une
/// adresse de son API.
fn stripe_id(id: &str, prefix: &str) -> bool {
    id.starts_with(prefix)
        && id.len() < 255
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Stripe, s'il est configuré.
#[derive(Clone)]
pub struct Stripe {
    secret: String,
    /// `https://api.stripe.com` (`STRIPE_API_BASE` pour les tests).
    base: String,
}

impl Stripe {
    pub fn from_env() -> Option<Stripe> {
        let var = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
        Some(Stripe {
            secret: var("STRIPE_SECRET_KEY")?,
            base: var("STRIPE_API_BASE").unwrap_or_else(|| "https://api.stripe.com".into()),
        })
    }

    /// Un appel à l'API de Stripe (formulaire encodé), hors du fil des
    /// requêtes : le statut et le corps.
    async fn call(
        &self,
        method: &'static str,
        path: &str,
        form: Vec<(String, String)>,
    ) -> Result<(u16, Value), String> {
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
            Ok((status, serde_json::from_str(&text).unwrap_or(Value::Null)))
        })
        .await
        .map_err(|e| e.to_string())?
    }

    /// Comme [`Stripe::call`], une réponse d'erreur devenant une erreur.
    async fn ok(
        &self,
        method: &'static str,
        path: &str,
        form: Vec<(String, String)>,
    ) -> Result<Value, String> {
        let (status, body) = self.call(method, path, form).await?;
        if status >= 300 {
            return Err(format!(
                "Stripe {status} : {}",
                body["error"]["message"].as_str().unwrap_or_default()
            ));
        }
        Ok(body)
    }
}

/// Le temps qu'une réponse de Stripe reste en mémoire.
const CACHE: Duration = Duration::from_secs(120);

/// Les réponses de Stripe gardées en mémoire : quand, et ce qu'il a dit.
type Cache = HashMap<String, (Instant, Option<Pass>)>;

/// Le réglage des paiements, dans l'état de l'application.
#[derive(Clone)]
pub struct Billing {
    pub stripe: Option<Stripe>,
    /// La clé de signature des salons est-elle durable ? Sans elle, un salon
    /// de plusieurs jours ne pourrait pas rouvrir après un redémarrage :
    /// l'offre n'est alors pas proposée.
    durable: Arc<AtomicBool>,
    cache: Arc<Mutex<Cache>>,
}

impl Billing {
    pub fn from_env() -> Billing {
        Billing {
            stripe: Stripe::from_env(),
            durable: Arc::new(AtomicBool::new(env_signing_key().is_some())),
            cache: Arc::default(),
        }
    }

    /// Au démarrage : SQLite garde la clé de signature, sauf sur un
    /// hébergeur dont le disque s'efface (Heroku) — il faut alors
    /// `ALLEY_SIGNING_KEY`.
    pub fn set_database(&self, kind: &str) {
        let ephemeral_disk = std::env::var_os("DYNO").is_some();
        if kind != "memory" && !ephemeral_disk {
            self.durable.store(true, Ordering::Relaxed);
        }
        if self.stripe.is_some() && !self.durable.load(Ordering::Relaxed) {
            tracing::warn!(
                "paiements désactivés : réglez ALLEY_SIGNING_KEY (64 caractères hexadécimaux), \
                 sans quoi les salons de plusieurs jours ne rouvriraient pas après un redémarrage"
            );
        }
    }

    /// Peut-on vendre une période ? Stripe, et une clé de signature durable.
    pub fn ready(&self) -> Option<&Stripe> {
        self.stripe
            .as_ref()
            .filter(|_| self.durable.load(Ordering::Relaxed))
    }

    fn cached(&self, key: &str) -> Option<Option<Pass>> {
        let cache = self.cache.lock().unwrap();
        cache
            .get(key)
            .filter(|(at, _)| at.elapsed() < CACHE)
            .map(|(_, e)| e.clone())
    }

    fn remember(&self, key: &str, value: Option<Pass>) {
        let mut cache = self.cache.lock().unwrap();
        cache.retain(|_, (at, _)| at.elapsed() < CACHE);
        cache.insert(key.to_owned(), (Instant::now(), value));
    }

    /// La période d'un compte, d'après Stripe : celle que le navigateur
    /// présente (`payment`), sinon celle que la recherche de Stripe retrouve
    /// (un autre appareil) — celle qui finit le plus tard parmi les actives.
    pub async fn lookup(
        &self,
        secret: &str,
        payment: Option<&str>,
    ) -> Result<Option<Pass>, String> {
        let Some(stripe) = self.stripe.as_ref() else {
            return Ok(None);
        };
        let Some(billing) = billing_id(secret) else {
            return Ok(None);
        };
        let now = now_ms();
        if let Some(id) = payment.filter(|id| stripe_id(id, "pi_")) {
            let key = format!("{billing}:{id}");
            let found = match self.cached(&key) {
                Some(found) => found,
                None => {
                    let path = format!("/v1/payment_intents/{id}?expand[]=latest_charge");
                    let (status, pi) = stripe.call("GET", &path, Vec::new()).await?;
                    if status != 200 && status != 404 {
                        return Err(format!("Stripe {status}"));
                    }
                    let found = if status == 200 {
                        pass(&pi, &billing)
                    } else {
                        None
                    };
                    self.remember(&key, found.clone());
                    found
                }
            };
            if found.as_ref().is_some_and(|p| p.active(now)) {
                return Ok(found);
            }
        }
        if let Some(found) = self.cached(&billing) {
            return Ok(found);
        }
        let query = encode(&format!(
            "metadata['alley_billing']:'{billing}' AND status:'succeeded'"
        ));
        let path = format!(
            "/v1/payment_intents/search?query={query}&limit=20&expand[]=data.latest_charge"
        );
        let list = stripe.ok("GET", &path, Vec::new()).await?;
        let mut found: Vec<Pass> = list["data"]
            .as_array()
            .map(|all| all.iter().filter_map(|pi| pass(pi, &billing)).collect())
            .unwrap_or_default();
        found.sort_by_key(|p| (!p.active(now), -p.end));
        let best = found.into_iter().next();
        self.remember(&billing, best.clone());
        Ok(best)
    }

    /// Oublie ce qu'on sait d'un compte (après un paiement).
    fn forget(&self, secret: &str) {
        if let Some(billing) = billing_id(secret) {
            self.cache
                .lock()
                .unwrap()
                .retain(|k, _| !k.starts_with(&billing));
        }
    }

    /// Jusqu'à quand restent ouverts les salons que crée ce compte : la fin
    /// de sa période payée, ou `None` (minuit).
    pub async fn until_for(&self, secret: Option<&str>, payment: Option<&str>) -> Option<i64> {
        self.ready()?;
        let now = now_ms();
        match self.lookup(secret?, payment).await {
            Ok(Some(p)) if p.active(now) => Some(p.end),
            Ok(_) => None,
            Err(e) => {
                tracing::warn!("{e}");
                None
            }
        }
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
    (ok && !wanted.contains(['#', '"', '<', '>', ' ', '{', '}'])).then(|| wanted.to_owned())
}

#[data]
pub struct Checkout {
    pub secret: String,
    pub plan: String,
    #[serde(default)]
    pub lang: String,
    #[serde(default)]
    pub return_to: String,
    /// Le décalage horaire de l'acheteur, en minutes : sa période finit à
    /// minuit chez lui.
    #[serde(default)]
    pub tz: i64,
    /// La période que le navigateur connaît déjà.
    #[serde(default)]
    pub payment: Option<String>,
}

#[data]
pub struct Status {
    pub secret: String,
    #[serde(default)]
    pub payment: Option<String>,
    /// De retour de Stripe Checkout : la session payée.
    #[serde(default)]
    pub session: Option<String>,
}

/// `GET /api/v1/plans` : les périodes, payées une fois.
async fn plans(req: Request) -> Json<Value> {
    let enabled = req.state::<Billing>().ready().is_some();
    let plans: Vec<Value> = PLANS
        .iter()
        .map(|p| json!({ "id": p.id, "days": p.days, "amount": p.cents }))
        .collect();
    Json(json!({
        "enabled": enabled,
        "currency": CURRENCY,
        "mode": "payment",
        "plans": plans,
    }))
}

/// `POST /api/v1/billing/checkout` : ouvre le paiement Stripe d'une
/// période ; renvoie l'adresse où aller payer.
async fn checkout(req: Request) -> (u16, Json<Value>) {
    let billing = req.state::<Billing>().clone();
    let Some(stripe) = billing.ready().cloned() else {
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
    match billing.lookup(&body.secret, body.payment.as_deref()).await {
        Ok(Some(p)) if p.active(now_ms()) => return fail(409, "already_active"),
        Ok(_) => {}
        Err(e) => {
            tracing::warn!("{e}");
            return fail(502, "stripe");
        }
    }
    let locale = match body.lang.as_str() {
        "fr" | "en" | "es" => body.lang.clone(),
        _ => "auto".to_owned(),
    };
    let tz = body.tz.clamp(-14 * 60, 14 * 60).to_string();
    let product = format!("Alley — salons ouverts {} jours", plan.days);
    let mut form: Vec<(String, String)> = [
        // Un paiement unique : rien n'est jamais reprélevé.
        ("mode", "payment".to_owned()),
        ("line_items[0][quantity]", "1".to_owned()),
        ("line_items[0][price_data][currency]", CURRENCY.to_owned()),
        (
            "line_items[0][price_data][unit_amount]",
            plan.cents.to_string(),
        ),
        ("line_items[0][price_data][product_data][name]", product),
        ("client_reference_id", id.clone()),
        ("metadata[alley_billing]", id.clone()),
        ("metadata[alley_plan]", plan.id.to_owned()),
        ("payment_intent_data[metadata][alley_billing]", id.clone()),
        (
            "payment_intent_data[metadata][alley_plan]",
            plan.id.to_owned(),
        ),
        ("payment_intent_data[metadata][alley_tz]", tz),
        // Stripe remplace `{CHECKOUT_SESSION_ID}` : le navigateur, de retour,
        // retrouve ainsi sa période et la garde.
        (
            "success_url",
            format!("{back}#periode={{CHECKOUT_SESSION_ID}}"),
        ),
        ("cancel_url", back),
        ("locale", locale),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v))
    .collect();
    form.retain(|(_, v)| !v.is_empty());
    match stripe.ok("POST", "/v1/checkout/sessions", form).await {
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

/// `POST /api/v1/billing/status` : la période d'un compte, d'après Stripe.
/// De retour de Checkout (`session`), elle est retrouvée par la session
/// payée. La réponse porte l'identifiant du paiement, que le navigateur
/// garde.
async fn status(req: Request) -> (u16, Json<Value>) {
    let billing = req.state::<Billing>().clone();
    let Ok(body) = req.json::<Status>().await else {
        return fail(400, "bad_request");
    };
    let Some(id) = billing_id(&body.secret) else {
        return fail(400, "bad_request");
    };
    let Some(stripe) = billing.stripe.clone() else {
        return (200, Json(json!({ "active": false })));
    };
    let mut payment = body.payment.clone();
    if let Some(session) = body.session.as_deref().filter(|s| stripe_id(s, "cs_")) {
        let path = format!("/v1/checkout/sessions/{session}");
        match stripe.ok("GET", &path, Vec::new()).await {
            // Seulement une session payée par ce compte.
            Ok(s)
                if s["metadata"]["alley_billing"].as_str() == Some(id.as_str())
                    && s["payment_status"] == "paid" =>
            {
                payment = s["payment_intent"].as_str().map(str::to_owned);
                billing.forget(&body.secret);
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("{e}"),
        }
    }
    let now = now_ms();
    match billing.lookup(&body.secret, payment.as_deref()).await {
        Ok(Some(p)) => (200, Json(p.to_json(now))),
        Ok(None) => (200, Json(json!({ "active": false }))),
        Err(e) => {
            tracing::warn!("{e}");
            fail(502, "stripe")
        }
    }
}

pub fn routes(app: &mut App) {
    app.get("/api/v1/plans", plans);
    app.post("/api/v1/billing/checkout", checkout);
    app.post("/api/v1/billing/status", status);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn periods_are_paid_once() {
        assert_eq!(plan("3d").map(|p| (p.days, p.cents)), Some((3, 499)));
        assert_eq!(plan("7d").map(|p| (p.days, p.cents)), Some((7, 999)));
        assert_eq!(plan("1m"), None);
    }

    #[test]
    fn a_period_ends_at_the_midnight_closing_its_last_day() {
        // 2026-10-14 21:30 à Paris (UTC+2).
        let start = 1_792_006_200_000;
        let end = period_end(start, 120, 3);
        // Mercredi, jeudi, vendredi : samedi 00:00 à Paris.
        assert_eq!(end, 1_792_188_000_000);
        assert_eq!(period_end(start, 120, 7) - end, 4 * DAY_MS);
    }

    fn pi(billing: &str, status: &str, amount: u64, refunded: bool) -> Value {
        json!({
            "id": "pi_1", "status": status, "created": 1_792_006_200i64,
            "amount_received": amount, "currency": "eur",
            "metadata": { "alley_billing": billing, "alley_plan": "3d", "alley_tz": "120" },
            "latest_charge": { "refunded": refunded, "amount_refunded": if refunded { amount } else { 0 } }
        })
    }

    #[test]
    fn stripe_says_what_a_payment_is_worth() {
        let b = "b".repeat(64);
        let start = 1_792_006_200_000;
        let p = pass(&pi(&b, "succeeded", 499, false), &b).unwrap();
        assert_eq!((p.status.as_str(), p.start), ("paid", start));
        assert!(p.active(start + DAY_MS));
        assert!(!p.active(p.end), "la période finie");
        // Un autre compte : rien.
        assert!(pass(&pi(&b, "succeeded", 499, false), &"c".repeat(64)).is_none());
        // Remboursé, pas encore payé, ou pas le bon montant : aucun droit.
        assert_eq!(
            pass(&pi(&b, "succeeded", 499, true), &b).unwrap().status,
            "refunded"
        );
        assert_eq!(
            pass(&pi(&b, "processing", 0, false), &b).unwrap().status,
            "pending"
        );
        assert_eq!(
            pass(&pi(&b, "succeeded", 1, false), &b).unwrap().status,
            "pending"
        );
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
    fn only_stripe_ids_reach_its_api() {
        assert!(stripe_id("pi_1Abc", "pi_"));
        assert!(stripe_id("cs_test_a1", "cs_"));
        assert!(!stripe_id("pi_../../v1/customers", "pi_"));
        assert!(!stripe_id("sub_1", "pi_"));
        assert_eq!(
            encode("metadata['alley_billing']:'ab'"),
            "metadata%5B%27alley_billing%27%5D%3A%27ab%27"
        );
    }
}
