//! Se connecter : un pseudonyme et un mot de passe, que le **navigateur
//! retient** (son gestionnaire de mots de passe, synchronisé entre appareils).
//!
//! Le mot de passe ne quitte jamais l'appareil : WebCrypto en tire une graine
//! (PBKDF2-SHA256, 310 000 tours, salé par le pseudonyme), dont on tire une
//! marque ([`crate::crypto::account_tag`]). Le serveur ne voit que cette
//! marque : chaque salon où l'on entre connecté la retient tant qu'il vit, et
//! la même marque retrouve ces salons depuis n'importe quel appareil.

use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use crate::crypto;
use crate::dom::*;

/// La graine d'un compte (32 octets).
pub async fn seed(pseudonym: &str, password: &str) -> Result<Vec<u8>, JsValue> {
    let subtle = window().crypto()?.subtle();
    let key = JsFuture::from(subtle.import_key_with_str(
        "raw",
        &js_sys::Uint8Array::from(password.as_bytes()),
        "PBKDF2",
        false,
        &js_sys::Array::of1(&"deriveBits".into()),
    )?)
    .await?;
    let salt = js_sys::Uint8Array::from(crypto::account_salt(pseudonym).as_slice());
    let params =
        web_sys::Pbkdf2Params::new_with_str("PBKDF2", "SHA-256", crypto::ACCOUNT_ITERATIONS, &salt);
    let bits =
        JsFuture::from(subtle.derive_bits_with_object(&params, key.unchecked_ref(), 256)?).await?;
    Ok(js_sys::Uint8Array::new(&bits).to_vec())
}

/// Se connecter : la marque du compte, et sa signature pour ce que garde
/// l'appareil ([`crate::vault`]), toutes deux tirées du mot de passe.
pub async fn sign_in(
    pseudonym: &str,
    password: &str,
) -> Result<(String, crate::vault::Signer), JsValue> {
    let seed = seed(pseudonym, password).await?;
    let tag = crypto::account_tag(&seed);
    let signer = crate::vault::Signer::new(&tag, &seed);
    Ok((tag, signer))
}

/// Propose au navigateur d'enregistrer le pseudonyme et le mot de passe
/// (Chrome, Edge, Android ; Safari et Firefox le proposent d'eux-mêmes à
/// l'envoi du formulaire).
pub fn offer_to_save(pseudonym: &str, password: &str) {
    if !has(&window(), "PasswordCredential") {
        return;
    }
    let data = js_sys::Object::new();
    for (k, v) in [
        ("id", pseudonym),
        ("password", password),
        ("name", pseudonym),
    ] {
        let _ = js_sys::Reflect::set(&data, &k.into(), &v.into());
    }
    let class = get(&window(), "PasswordCredential");
    let Ok(class) = class.dyn_into::<js_sys::Function>() else {
        return;
    };
    let Ok(credential) = js_sys::Reflect::construct(&class, &js_sys::Array::of1(&data)) else {
        return;
    };
    let container = get(&window().navigator(), "credentials");
    if let Ok(p) = call(&container, "store", &[credential]) {
        spawn(async move {
            let _ = JsFuture::from(js_sys::Promise::from(p)).await;
        });
    }
}
