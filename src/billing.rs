//! Les abonnements payants, avec **Stripe**, **au mois seulement** — et
//! **sans base de données** :
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
//! n'est gardé** pour autant.
//!
//! **Rien n'est gardé sur le serveur** :
//!
//! - **Stripe fait foi** pour les abonnements. Le navigateur garde
//!   l'identifiant de son abonnement (dans son `localStorage`, signé) et le
//!   présente ; le serveur le relit chez Stripe (quelques minutes en cache,
//!   en mémoire) et vérifie qu'il appartient bien à ce compte. Sur un autre
//!   appareil, le serveur le retrouve par la recherche de Stripe. Aucun
//!   webhook à régler.
//! - Les **salons de plusieurs jours** survivent aux redémarrages grâce aux
//!   navigateurs : chacun garde la description du salon signée par le
//!   serveur, et la lui présente s'il l'a oublié ([`Rooms::revive`]). Il
//!   suffit que la clé de signature du serveur ne change pas :
//!   `ALLEY_SIGNING_KEY` (64 caractères hexadécimaux).
//! - **SQLite**, facultatif (`DATABASE_URL=sqlite://alley.db`, pour un
//!   hébergeur dont le disque dure) : il garde en plus la clé de signature et
//!   les salons de plusieurs jours, qui rouvrent alors d'eux-mêmes. Rien n'en
//!   dépend : sur Heroku, dont le disque s'efface à chaque redémarrage, le
//!   `localStorage` et Stripe suffisent.
//!
//! Qui paie ? Le compte (pseudonyme + mot de passe) : le navigateur en tire
//! un **secret d'abonnement** (HKDF de la graine du mot de passe), envoyé
//! dans le corps des requêtes, jamais dans une adresse. Stripe n'en reçoit
//! que l'empreinte ([`billing_id`]) ; le serveur ne connaît ni le pseudonyme
//! ni le mot de passe. L'e-mail et la carte restent chez Stripe (Checkout,
//! puis son portail client pour résilier).
//!
//! Réglage : `STRIPE_SECRET_KEY`, et une clé de signature durable
//! (`ALLEY_SIGNING_KEY`, ou SQLite). Sans eux, l'offre n'est pas proposée.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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

/// Un salon de plusieurs jours, gardé dans SQLite si l'on en a un.
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

/// L'identifiant d'abonnement d'un compte : l'empreinte de son secret
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

/// Garde un salon de plusieurs jours dans SQLite, si l'on en a un.
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

/// Ce que Stripe dit d'un abonnement d'Alley.
#[derive(Clone, Debug, PartialEq)]
pub struct Entitlement {
    pub subscription: String,
    pub customer: String,
    pub plan: String,
    pub status: String,
    /// La fin de la période payée, en millisecondes.
    pub period_end: i64,
}

impl Entitlement {
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

    pub fn to_json(&self, now: i64) -> Value {
        json!({
            "active": self.active(now),
            "plan": self.plan,
            "days": self.days(now),
            "status": self.status,
            "until": self.period_end,
            "subscription": self.subscription,
        })
    }
}

/// Lit un abonnement de Stripe : `None` s'il n'est pas à ce compte
/// (`billing`) ou pas d'Alley. Un rythme autre que mensuel n'ouvre aucun
/// droit (`not_monthly`).
pub fn entitlement(sub: &Value, billing: &str) -> Option<Entitlement> {
    let meta = &sub["metadata"];
    if meta["alley_billing"].as_str() != Some(billing) {
        return None;
    }
    let plan = plan(meta["alley_plan"].as_str()?)?;
    let item = &sub["items"]["data"][0];
    let recurring = &item["price"]["recurring"];
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
    Some(Entitlement {
        subscription: sub["id"].as_str()?.to_owned(),
        customer: sub["customer"].as_str().unwrap_or_default().to_owned(),
        plan: plan.id.to_owned(),
        status: status.to_owned(),
        period_end,
    })
}

/// Un identifiant de Stripe (`sub_…`, `cs_…`) : rien d'autre ne part dans
/// une adresse de son API.
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
type Cache = HashMap<String, (Instant, Option<Entitlement>)>;

/// Le réglage des abonnements, dans l'état de l'application.
#[derive(Clone)]
pub struct Billing {
    pub stripe: Option<Stripe>,
    /// La clé de signature des salons est-elle durable (configuration, ou
    /// SQLite) ? Sans elle, un salon de plusieurs jours ne pourrait pas
    /// rouvrir après un redémarrage : l'offre n'est alors pas proposée.
    durable: Arc<AtomicBool>,
    /// Ce que Stripe a dit d'un compte, pour quelques minutes.
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

    /// Au démarrage : avec SQLite, la clé de signature y est gardée.
    pub fn set_database(&self, kind: &str) {
        if kind != "memory" {
            self.durable.store(true, Ordering::Relaxed);
        }
        if self.stripe.is_some() && !self.durable.load(Ordering::Relaxed) {
            tracing::warn!(
                "abonnements désactivés : réglez ALLEY_SIGNING_KEY (64 caractères hexadécimaux), \
                 sans quoi les salons de plusieurs jours ne rouvriraient pas après un redémarrage"
            );
        }
    }

    /// Peut-on vendre un abonnement ? Stripe, et une clé de signature durable.
    pub fn ready(&self) -> Option<&Stripe> {
        self.stripe
            .as_ref()
            .filter(|_| self.durable.load(Ordering::Relaxed))
    }

    fn cached(&self, key: &str) -> Option<Option<Entitlement>> {
        let cache = self.cache.lock().unwrap();
        cache
            .get(key)
            .filter(|(at, _)| at.elapsed() < CACHE)
            .map(|(_, e)| e.clone())
    }

    fn remember(&self, key: &str, value: Option<Entitlement>) {
        let mut cache = self.cache.lock().unwrap();
        cache.retain(|_, (at, _)| at.elapsed() < CACHE);
        cache.insert(key.to_owned(), (Instant::now(), value));
    }

    /// L'abonnement d'un compte, d'après Stripe : celui que le navigateur
    /// présente (`subscription`), sinon celui que la recherche de Stripe
    /// retrouve (un autre appareil) — un actif d'abord s'il y en a plusieurs.
    pub async fn lookup(
        &self,
        secret: &str,
        subscription: Option<&str>,
    ) -> Result<Option<Entitlement>, String> {
        let Some(stripe) = self.stripe.as_ref() else {
            return Ok(None);
        };
        let Some(billing) = billing_id(secret) else {
            return Ok(None);
        };
        let now = now_ms();
        if let Some(id) = subscription.filter(|id| stripe_id(id, "sub_")) {
            let key = format!("{billing}:{id}");
            let found = match self.cached(&key) {
                Some(found) => found,
                None => {
                    let path = format!("/v1/subscriptions/{id}");
                    let (status, sub) = stripe.call("GET", &path, Vec::new()).await?;
                    if status != 200 && status != 404 {
                        return Err(format!("Stripe {status}"));
                    }
                    let found = if status == 200 {
                        entitlement(&sub, &billing)
                    } else {
                        None
                    };
                    self.remember(&key, found.clone());
                    found
                }
            };
            if found.as_ref().is_some_and(|e| e.active(now)) {
                return Ok(found);
            }
        }
        if let Some(found) = self.cached(&billing) {
            return Ok(found);
        }
        let query = encode(&format!("metadata['alley_billing']:'{billing}'"));
        let path = format!("/v1/subscriptions/search?query={query}&limit=20");
        let list = stripe.ok("GET", &path, Vec::new()).await?;
        let mut found: Vec<Entitlement> = list["data"]
            .as_array()
            .map(|subs| {
                subs.iter()
                    .filter_map(|s| entitlement(s, &billing))
                    .collect()
            })
            .unwrap_or_default();
        found.sort_by_key(|e| (!e.active(now), -e.period_end));
        let best = found.into_iter().next();
        self.remember(&billing, best.clone());
        Ok(best)
    }

    /// Oublie ce qu'on sait d'un compte (après un paiement, avant le portail).
    fn forget(&self, secret: &str) {
        if let Some(billing) = billing_id(secret) {
            self.cache
                .lock()
                .unwrap()
                .retain(|k, _| !k.starts_with(&billing));
        }
    }

    /// Combien de jours durent les salons que crée ce compte (1 sans
    /// abonnement, ou si Stripe ne répond pas).
    pub async fn days_for(&self, secret: Option<&str>, subscription: Option<&str>) -> u32 {
        let (Some(_), Some(secret)) = (self.ready(), secret) else {
            return 1;
        };
        match self.lookup(secret, subscription).await {
            Ok(Some(e)) => e.days(now_ms()),
            Ok(None) => 1,
            Err(e) => {
                tracing::warn!("{e}");
                1
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
    /// L'abonnement que le navigateur connaît déjà.
    #[serde(default)]
    pub subscription: Option<String>,
}

#[data]
pub struct Status {
    pub secret: String,
    #[serde(default)]
    pub subscription: Option<String>,
    /// De retour de Stripe Checkout : la session payée.
    #[serde(default)]
    pub session: Option<String>,
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
    let existing = match billing
        .lookup(&body.secret, body.subscription.as_deref())
        .await
    {
        Ok(existing) => existing,
        Err(e) => {
            tracing::warn!("{e}");
            return fail(502, "stripe");
        }
    };
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
        // Stripe remplace `{CHECKOUT_SESSION_ID}` : le navigateur, de retour,
        // retrouve ainsi son abonnement et le garde.
        (
            "success_url",
            format!("{back}#abonnement={{CHECKOUT_SESSION_ID}}"),
        ),
        ("cancel_url", back),
        ("locale", locale),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v))
    .collect();
    if let Some(customer) = existing.map(|s| s.customer).filter(|c| !c.is_empty()) {
        form.push(("customer".into(), customer));
    }
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

/// `POST /api/v1/billing/status` : l'abonnement d'un compte, d'après
/// Stripe. De retour de Checkout (`session`), il est retrouvé par la session
/// payée. La réponse porte l'identifiant de l'abonnement, que le navigateur
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
        return (200, Json(json!({ "active": false, "days": 1 })));
    };
    let mut subscription = body.subscription.clone();
    if let Some(session) = body.session.as_deref().filter(|s| stripe_id(s, "cs_")) {
        let path = format!("/v1/checkout/sessions/{session}");
        match stripe.ok("GET", &path, Vec::new()).await {
            // Seulement une session payée par ce compte.
            Ok(s) if s["metadata"]["alley_billing"].as_str() == Some(id.as_str()) => {
                subscription = s["subscription"].as_str().map(str::to_owned);
                billing.forget(&body.secret);
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("{e}"),
        }
    }
    let now = now_ms();
    match billing.lookup(&body.secret, subscription.as_deref()).await {
        Ok(Some(e)) => (200, Json(e.to_json(now))),
        Ok(None) => (200, Json(json!({ "active": false, "days": 1 }))),
        Err(e) => {
            tracing::warn!("{e}");
            fail(502, "stripe")
        }
    }
}

/// `POST /api/v1/billing/portal` : le portail de Stripe, pour gérer son
/// abonnement (résilier, changer de carte, factures).
async fn portal(req: Request) -> (u16, Json<Value>) {
    let billing = req.state::<Billing>().clone();
    let Some(stripe) = billing.stripe.clone() else {
        return fail(503, "billing_disabled");
    };
    let Ok(body) = req.json::<Status>().await else {
        return fail(400, "bad_request");
    };
    let Some(back) = return_url(&req, &body.return_to) else {
        return fail(400, "bad_return_url");
    };
    let customer = match billing
        .lookup(&body.secret, body.subscription.as_deref())
        .await
    {
        Ok(Some(e)) if !e.customer.is_empty() => e.customer,
        Ok(_) => return fail(404, "no_subscription"),
        Err(e) => {
            tracing::warn!("{e}");
            return fail(502, "stripe");
        }
    };
    // Au retour du portail, ce qui a changé (résiliation) se voit aussitôt.
    billing.forget(&body.secret);
    let form = vec![
        ("customer".to_owned(), customer),
        ("return_url".to_owned(), back),
    ];
    match stripe.ok("POST", "/v1/billing_portal/sessions", form).await {
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

pub fn routes(app: &mut App) {
    app.get("/api/v1/plans", plans);
    app.post("/api/v1/billing/checkout", checkout);
    app.post("/api/v1/billing/status", status);
    app.post("/api/v1/billing/portal", portal);
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

    fn sub(billing: &str, interval: &str, status: &str) -> Value {
        json!({
            "id": "sub_1", "customer": "cus_1", "status": status,
            "metadata": { "alley_billing": billing, "alley_plan": "7d" },
            "items": { "data": [{ "current_period_end": 2_000_000_000i64,
                "price": { "recurring": { "interval": interval, "interval_count": 1 } } }] }
        })
    }

    #[test]
    fn stripe_says_what_a_subscription_is_worth() {
        let b = "b".repeat(64);
        let e = entitlement(&sub(&b, "month", "active"), &b).unwrap();
        assert_eq!((e.days(0), e.period_end), (7, 2_000_000_000_000));
        // Un autre compte : rien.
        assert!(entitlement(&sub(&b, "month", "active"), &"c".repeat(64)).is_none());
        // À l'année : aucun droit.
        let yearly = entitlement(&sub(&b, "year", "active"), &b).unwrap();
        assert_eq!((yearly.status.as_str(), yearly.days(0)), ("not_monthly", 1));
        // En retard : jusqu'à la fin du mois payé.
        let late = entitlement(&sub(&b, "month", "past_due"), &b).unwrap();
        assert_eq!(late.days(0), 7);
        assert_eq!(late.days(3_000_000_000_000), 1);
        for status in ["canceled", "unpaid", "incomplete"] {
            assert_eq!(
                entitlement(&sub(&b, "month", status), &b).unwrap().days(0),
                1
            );
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
    fn only_stripe_ids_reach_its_api() {
        assert!(stripe_id("sub_1Abc", "sub_"));
        assert!(stripe_id("cs_test_a1", "cs_"));
        assert!(!stripe_id("sub_../../v1/customers", "sub_"));
        assert!(!stripe_id("cus_1", "sub_"));
        assert_eq!(
            encode("metadata['alley_billing']:'ab'"),
            "metadata%5B%27alley_billing%27%5D%3A%27ab%27"
        );
    }
}
