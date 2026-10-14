//! Ce que l'appareil garde (`localStorage`) est **signé**, et vérifié à
//! chaque lecture. N'importe qui peut modifier le `localStorage` d'un
//! navigateur (les outils de développement, une extension, un programme sur
//! l'ordinateur) : une donnée dont une signature ne correspond pas est
//! écartée, et l'on prévient.
//!
//! Chaque donnée est signée par plusieurs points indépendants, et aucune de
//! leurs clés n'est dans le `localStorage` :
//!
//! - **l'appareil** : une clé HMAC-SHA256 tirée par WebCrypto, **non
//!   exportable**, rangée dans IndexedDB. La page s'en sert pour signer et
//!   vérifier ; personne ne peut la lire ni la copier, pas même les outils de
//!   développement. Toute donnée doit porter sa signature ;
//! - **le compte**, si l'on est connecté : une clé tirée du mot de passe
//!   ([`crate::crypto::account_vault_key`]), qui n'existe qu'en mémoire.
//!   Une donnée écrite connecté porte aussi sa signature, vérifiée dès que ce
//!   compte se reconnecte : sans le mot de passe, impossible de la refaire ;
//! - **le serveur**, pour les salons retenus : il signe ce qu'il dit d'un
//!   salon (identifiant, nom, privé ou non, fin), et l'appareil le vérifie
//!   avec sa clé publique ([`crate::recent`]).
//!
//! Sans IndexedDB (certaines navigations privées), rien n'est gardé : mieux
//! vaut ne rien retenir que retenir sans pouvoir vérifier.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{CryptoKey, IdbDatabase, IdbOpenDbRequest, IdbRequest, IdbTransactionMode};

use crate::crypto::{self, Key};
use crate::dom::window;

/// Une donnée gardée, avec ses signatures.
#[derive(Serialize, Deserialize)]
struct Sealed {
    v: u8,
    /// Son nom (la clé du `localStorage`) : une donnée ne peut pas prendre la
    /// place d'une autre.
    name: String,
    data: String,
    /// Le compte qui l'a écrite (empreinte de sa marque), sinon vide.
    #[serde(default)]
    who: String,
    /// La signature de l'appareil (base64).
    dev: String,
    /// La signature du compte (base64), sinon vide.
    #[serde(default)]
    acc: String,
}

/// Le compte connecté, pour signer et vérifier : l'empreinte de sa marque et
/// sa clé (tirée du mot de passe).
#[derive(Clone)]
pub struct Signer {
    pub who: String,
    pub key: Key,
}

impl Signer {
    pub fn new(tag: &str, seed: &[u8]) -> Self {
        Signer {
            who: crypto::sha256_hex(&format!("alley-vault-who:{tag}"))[..16].to_owned(),
            key: crypto::account_vault_key(seed),
        }
    }
}

thread_local! {
    static DEVICE: RefCell<Option<CryptoKey>> = const { RefCell::new(None) };
    static ACCOUNT: RefCell<Option<Signer>> = const { RefCell::new(None) };
    /// Les écritures en cours, par nom : seule la plus récente s'enregistre.
    static WRITES: RefCell<HashMap<String, u64>> = RefCell::new(HashMap::new());
    static TAMPERED: Cell<bool> = const { Cell::new(false) };
}

/// Le compte connecté (ou `None` en se déconnectant).
pub fn set_account(account: Option<Signer>) {
    ACCOUNT.with(|a| *a.borrow_mut() = account);
}

fn account() -> Option<Signer> {
    ACCOUNT.with(|a| a.borrow().clone())
}

/// Une donnée modifiée hors d'Alley a-t-elle été écartée depuis le dernier
/// appel ?
pub fn take_tampered() -> bool {
    TAMPERED.with(|t| t.replace(false))
}

fn storage() -> Option<web_sys::Storage> {
    window().local_storage().ok().flatten()
}

/// Attend la fin d'une requête IndexedDB et renvoie son résultat.
async fn done(req: &IdbRequest) -> Result<JsValue, JsValue> {
    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        req.set_onsuccess(Some(&resolve));
        req.set_onerror(Some(&reject));
    });
    JsFuture::from(promise).await?;
    req.result()
}

async fn open_db() -> Result<IdbDatabase, JsValue> {
    let factory = window()
        .indexed_db()?
        .ok_or_else(|| JsValue::from_str("IndexedDB indisponible"))?;
    let req: IdbOpenDbRequest = factory.open_with_u32("alley", 1)?;
    let upgrade = Closure::once(move |e: web_sys::Event| {
        if let Some(req) = e.target().and_then(|t| t.dyn_into::<IdbRequest>().ok())
            && let Ok(db) = req.result()
        {
            let _ = db
                .unchecked_into::<IdbDatabase>()
                .create_object_store("keys");
        }
    });
    req.set_onupgradeneeded(Some(upgrade.as_ref().unchecked_ref()));
    let db = done(&req).await;
    req.set_onupgradeneeded(None);
    Ok(db?.unchecked_into())
}

async fn stored_key(db: &IdbDatabase) -> Result<Option<CryptoKey>, JsValue> {
    let store = db
        .transaction_with_str_and_mode("keys", IdbTransactionMode::Readonly)?
        .object_store("keys")?;
    let value = done(&store.get(&"device".into())?).await?;
    Ok(value.dyn_into::<CryptoKey>().ok())
}

/// La clé de l'appareil : lue dans IndexedDB, ou tirée une fois pour toutes.
async fn device_key() -> Option<CryptoKey> {
    if let Some(key) = DEVICE.with(|d| d.borrow().clone()) {
        return Some(key);
    }
    let key = load_or_create().await.ok()?;
    DEVICE.with(|d| *d.borrow_mut() = Some(key.clone()));
    Some(key)
}

async fn load_or_create() -> Result<CryptoKey, JsValue> {
    let db = open_db().await?;
    if let Some(key) = stored_key(&db).await? {
        return Ok(key);
    }
    let subtle = window().crypto()?.subtle();
    let params = web_sys::HmacKeyGenParams::new_with_str("HMAC", "SHA-256");
    let usages = js_sys::Array::of2(&"sign".into(), &"verify".into());
    // Non exportable : on peut s'en servir, jamais la lire.
    let key: CryptoKey = JsFuture::from(subtle.generate_key_with_object(&params, false, &usages)?)
        .await?
        .unchecked_into();
    let store = db
        .transaction_with_str_and_mode("keys", IdbTransactionMode::Readwrite)?
        .object_store("keys")?;
    // `add` et pas `put` : si un autre onglet l'a tirée entre-temps, c'est
    // la sienne qui compte.
    match done(&store.add_with_key(&key, &"device".into())?).await {
        Ok(_) => Ok(key),
        Err(_) => stored_key(&db)
            .await?
            .ok_or_else(|| JsValue::from_str("clé de l'appareil introuvable")),
    }
}

async fn device_sign(key: &CryptoKey, data: &[u8]) -> Option<Vec<u8>> {
    let subtle = window().crypto().ok()?.subtle();
    let sig = JsFuture::from(subtle.sign_with_str_and_u8_array("HMAC", key, data).ok()?)
        .await
        .ok()?;
    Some(js_sys::Uint8Array::new(&sig).to_vec())
}

async fn device_verify(key: &CryptoKey, sig: &[u8], data: &[u8]) -> bool {
    let Ok(crypto) = window().crypto() else {
        return false;
    };
    let Ok(promise) = crypto
        .subtle()
        .verify_with_str_and_u8_array_and_u8_array("HMAC", key, sig, data)
    else {
        return false;
    };
    JsFuture::from(promise)
        .await
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Lit une donnée : `None` si elle n'existe pas, ou si une signature ne
/// correspond pas (elle est alors effacée, et [`take_tampered`] le dira).
pub async fn read(name: &str) -> Option<String> {
    let raw = storage()?.get_item(name).ok().flatten()?;
    // Sans la clé de l'appareil, on ne peut rien vérifier : rien n'est lu,
    // mais rien n'est effacé non plus.
    let key = device_key().await?;
    match verify(&key, name, &raw).await {
        Some(data) => Some(data),
        None => {
            if let Some(s) = storage() {
                let _ = s.remove_item(name);
            }
            TAMPERED.with(|t| t.set(true));
            None
        }
    }
}

async fn verify(key: &CryptoKey, name: &str, raw: &str) -> Option<String> {
    let sealed: Sealed = serde_json::from_str(raw).ok()?;
    if sealed.v != 1 || sealed.name != name {
        return None;
    }
    let statement = crypto::vault_statement(name, &sealed.who, &sealed.data);
    if !device_verify(key, &crypto::unb64(&sealed.dev)?, &statement).await {
        return None;
    }
    // Écrite par le compte connecté : sa signature doit y être, et juste.
    if let Some(account) = account()
        && !sealed.who.is_empty()
        && sealed.who == account.who
        && !crypto::mac_ok(&account.key, &statement, &crypto::unb64(&sealed.acc)?)
    {
        return None;
    }
    Some(sealed.data)
}

/// Écrit une donnée, signée par l'appareil (et par le compte connecté).
/// Plusieurs écritures rapprochées : seule la dernière s'enregistre.
pub async fn write(name: &str, data: &str) {
    let generation = WRITES.with(|w| {
        let mut w = w.borrow_mut();
        let n = w.entry(name.to_owned()).or_default();
        *n += 1;
        *n
    });
    let Some(key) = device_key().await else {
        return;
    };
    let account = account();
    let who = account.as_ref().map(|a| a.who.clone()).unwrap_or_default();
    let statement = crypto::vault_statement(name, &who, data);
    let Some(dev) = device_sign(&key, &statement).await else {
        return;
    };
    let acc = account
        .map(|a| crypto::b64(&crypto::mac(&a.key, &statement)))
        .unwrap_or_default();
    let latest = WRITES.with(|w| w.borrow().get(name) == Some(&generation));
    if !latest {
        return;
    }
    let sealed = Sealed {
        v: 1,
        name: name.to_owned(),
        data: data.to_owned(),
        who,
        dev: crypto::b64(&dev),
        acc,
    };
    if let (Some(s), Ok(raw)) = (storage(), serde_json::to_string(&sealed)) {
        let _ = s.set_item(name, &raw);
    }
}

/// Signale une donnée modifiée hors d'Alley, repérée ailleurs (la signature
/// du serveur sur un salon retenu).
pub fn mark_tampered() {
    TAMPERED.with(|t| t.set(true));
}

/// Efface une donnée.
pub fn remove(name: &str) {
    WRITES.with(|w| *w.borrow_mut().entry(name.to_owned()).or_default() += 1);
    if let Some(s) = storage() {
        let _ = s.remove_item(name);
    }
}
