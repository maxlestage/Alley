//! Les périodes achetées sur iPhone (achats intégrés, StoreKit 2).
//!
//! L'App Store remet à l'application une transaction **signée** (JWS) :
//! l'en-tête porte la chaîne de certificats (feuille, intermédiaire, racine),
//! la racine doit être l'*Apple Root CA - G3* (embarquée ici), chaque
//! certificat doit être signé par le suivant et porter la marque d'Apple
//! (OID), et la transaction doit être signée par la feuille. Rien à
//! demander à Apple, rien à garder : comme pour Stripe, l'appareil présente
//! sa preuve, le serveur la vérifie.
//!
//! La transaction est liée au compte : l'application achète avec un
//! `appAccountToken` tiré de l'identifiant de paiement du compte
//! ([`account_token`]) ; une transaction d'un autre compte n'ouvre rien.

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use serde_json::Value;
use sha2::{Digest, Sha256, Sha384};
use x509_cert::Certificate;
use x509_cert::der::{Decode, Encode, oid::ObjectIdentifier};

use crate::billing::{Pass, period_end, plan};

/// L'autorité racine d'Apple (valable jusqu'en 2039).
pub const APPLE_ROOT_G3: &[u8] = include_bytes!("AppleRootCA-G3.cer");

/// Les produits de l'App Store : `app.alley.ios.period.3d`, `….7d`, `….1m`,
/// `….3m` (abonnements non renouvelables : une période, payée une fois).
pub const PRODUCT_PREFIX: &str = "app.alley.ios.period.";

const ECDSA_SHA256: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.4.3.2");
const ECDSA_SHA384: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.4.3.3");
const P256: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.3.1.7");
const P384: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.3.132.0.34");
/// La marque d'Apple sur le certificat qui signe les transactions…
const LEAF_MARKER: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113635.100.6.11.1");
/// … et sur son intermédiaire (Apple Worldwide Developer Relations).
const INTERMEDIATE_MARKER: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113635.100.6.2.1");

/// L'`appAccountToken` d'un compte : un UUID tiré de son identifiant de
/// paiement (le même calcul dans l'application).
pub fn account_token(billing_id: &str) -> String {
    let d = Sha256::digest(format!("alley-apple-v1:{billing_id}"));
    let mut b: [u8; 16] = d[..16].try_into().expect("16 octets");
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    )
}

fn public_key(cert: &Certificate) -> Option<(ObjectIdentifier, Vec<u8>)> {
    let spki = &cert.tbs_certificate.subject_public_key_info;
    let curve: ObjectIdentifier = spki.algorithm.parameters.as_ref()?.decode_as().ok()?;
    Some((curve, spki.subject_public_key.raw_bytes().to_vec()))
}

/// `data` est-il signé (`sig`, ECDSA en DER ou brut) par cette clé ?
fn verify(
    curve: ObjectIdentifier,
    key: &[u8],
    data: &[u8],
    sha384: bool,
    sig: &[u8],
    der: bool,
) -> bool {
    use p256::ecdsa::signature::hazmat::PrehashVerifier;
    let digest: Vec<u8> = if sha384 {
        Sha384::digest(data).to_vec()
    } else {
        Sha256::digest(data).to_vec()
    };
    if curve == P256 {
        let Ok(vk) = p256::ecdsa::VerifyingKey::from_sec1_bytes(key) else {
            return false;
        };
        let sig = if der {
            p256::ecdsa::Signature::from_der(sig)
        } else {
            p256::ecdsa::Signature::from_slice(sig)
        };
        sig.is_ok_and(|s| vk.verify_prehash(&digest, &s).is_ok())
    } else if curve == P384 {
        let Ok(vk) = p384::ecdsa::VerifyingKey::from_sec1_bytes(key) else {
            return false;
        };
        let sig = if der {
            p384::ecdsa::Signature::from_der(sig)
        } else {
            p384::ecdsa::Signature::from_slice(sig)
        };
        sig.is_ok_and(|s| vk.verify_prehash(&digest, &s).is_ok())
    } else {
        false
    }
}

/// `child` est-il signé par `parent` ?
fn signed_by(child: &Certificate, parent: &Certificate) -> bool {
    let alg = child.signature_algorithm.oid;
    if alg != ECDSA_SHA256 && alg != ECDSA_SHA384 {
        return false;
    }
    let (Some((curve, key)), Ok(tbs)) = (public_key(parent), child.tbs_certificate.to_der()) else {
        return false;
    };
    verify(
        curve,
        &key,
        &tbs,
        alg == ECDSA_SHA384,
        child.signature.raw_bytes(),
        true,
    )
}

fn has_extension(cert: &Certificate, oid: ObjectIdentifier) -> bool {
    cert.tbs_certificate
        .extensions
        .as_ref()
        .is_some_and(|exts| exts.iter().any(|e| e.extn_id == oid))
}

/// Le certificat était-il valable à cet instant (en millisecondes) ?
fn valid_at(cert: &Certificate, ms: i64) -> bool {
    let v = &cert.tbs_certificate.validity;
    let secs = ms / 1000;
    let from = v.not_before.to_unix_duration().as_secs() as i64;
    let to = v.not_after.to_unix_duration().as_secs() as i64;
    (from..=to).contains(&secs)
}

/// Vérifie une transaction signée de l'App Store, avec `root` pour racine
/// (l'Apple Root CA - G3 en production) ; renvoie son contenu.
pub fn verify_jws(jws: &str, root: &[u8]) -> Result<Value, &'static str> {
    let mut parts = jws.split('.');
    let (Some(h), Some(p), Some(s), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err("jws");
    };
    let header: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(h).map_err(|_| "jws")?)
        .map_err(|_| "jws")?;
    if header["alg"] != "ES256" {
        return Err("alg");
    }
    let chain: Vec<Vec<u8>> = header["x5c"]
        .as_array()
        .ok_or("x5c")?
        .iter()
        .map(|c| {
            c.as_str()
                .and_then(|c| STANDARD.decode(c).ok())
                .ok_or("x5c")
        })
        .collect::<Result<_, _>>()?;
    let [leaf, intermediate, top] = chain.as_slice() else {
        return Err("x5c");
    };
    if top.as_slice() != root {
        return Err("root");
    }
    let leaf = Certificate::from_der(leaf).map_err(|_| "x5c")?;
    let intermediate = Certificate::from_der(intermediate).map_err(|_| "x5c")?;
    let root = Certificate::from_der(root).map_err(|_| "root")?;
    if !has_extension(&leaf, LEAF_MARKER) || !has_extension(&intermediate, INTERMEDIATE_MARKER) {
        return Err("marker");
    }
    if !signed_by(&intermediate, &root) || !signed_by(&leaf, &intermediate) {
        return Err("chain");
    }
    let (curve, key) = public_key(&leaf).ok_or("x5c")?;
    let sig = URL_SAFE_NO_PAD.decode(s).map_err(|_| "jws")?;
    if curve != P256
        || !verify(
            curve,
            &key,
            format!("{h}.{p}").as_bytes(),
            false,
            &sig,
            false,
        )
    {
        return Err("signature");
    }
    let payload: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(p).map_err(|_| "jws")?)
        .map_err(|_| "jws")?;
    // Les certificats valaient au moment où Apple a signé.
    let signed = payload["signedDate"].as_i64().ok_or("payload")?;
    if ![&leaf, &intermediate, &root]
        .iter()
        .all(|c| valid_at(c, signed))
    {
        return Err("expired");
    }
    Ok(payload)
}

/// L'identifiant de l'application iOS (`ALLEY_IOS_BUNDLE_ID`).
pub fn bundle_id() -> String {
    std::env::var("ALLEY_IOS_BUNDLE_ID").unwrap_or_else(|_| "app.alley.ios".into())
}

/// Les achats de test (TestFlight, bac à sable d'Apple) ouvrent-ils des
/// droits ? Oui, sauf `ALLEY_APPLE_SANDBOX=0` (une fois l'app publiée).
fn sandbox_allowed() -> bool {
    std::env::var("ALLEY_APPLE_SANDBOX").map_or(true, |v| v != "0")
}

/// La période qu'ouvre une transaction de l'App Store, pour ce compte
/// (`billing` : son identifiant de paiement) ; `offset` : le décalage
/// horaire, en minutes (la période finit à minuit chez lui).
pub fn pass(jws: &str, billing: &str, offset: i64, root: &[u8]) -> Result<Pass, &'static str> {
    let t = verify_jws(jws, root)?;
    if t["bundleId"].as_str() != Some(bundle_id().as_str()) {
        return Err("bundle");
    }
    match t["environment"].as_str() {
        Some("Production") => {}
        Some("Sandbox" | "Xcode") if sandbox_allowed() => {}
        _ => return Err("environment"),
    }
    let token = t["appAccountToken"].as_str().unwrap_or_default();
    if !token.eq_ignore_ascii_case(&account_token(billing)) {
        return Err("account");
    }
    let product = t["productId"].as_str().unwrap_or_default();
    let plan = product
        .strip_prefix(PRODUCT_PREFIX)
        .and_then(plan)
        .ok_or("product")?;
    let start = t["purchaseDate"].as_i64().ok_or("payload")?;
    let status = if t["revocationDate"].is_null() {
        "paid"
    } else {
        "refunded"
    };
    Ok(Pass {
        payment: format!("apple:{}", t["transactionId"].as_str().unwrap_or_default()),
        plan: plan.id.to_owned(),
        status: status.to_owned(),
        start,
        end: period_end(start, offset, plan.days),
    })
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use rcgen::{BasicConstraints, CertificateParams, CustomExtension, IsCa, KeyPair};

    /// Une fausse chaîne « Apple » : racine P-384, intermédiaire, feuille
    /// P-256 (marques d'Apple comprises), et de quoi signer des transactions.
    pub struct Chain {
        pub root: Vec<u8>,
        intermediate: Vec<u8>,
        leaf: Vec<u8>,
        leaf_key: KeyPair,
    }

    fn marker(oid: &[u64]) -> CustomExtension {
        CustomExtension::from_oid_content(oid, vec![0x05, 0x00])
    }

    impl Chain {
        pub fn new(marked: bool) -> Chain {
            let root_key = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P384_SHA384).unwrap();
            let mut rp = CertificateParams::new(vec![]).unwrap();
            rp.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
            let root = rp.self_signed(&root_key).unwrap();
            let int_key = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P384_SHA384).unwrap();
            let mut ip = CertificateParams::new(vec![]).unwrap();
            ip.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
            if marked {
                ip.custom_extensions
                    .push(marker(&[1, 2, 840, 113635, 100, 6, 2, 1]));
            }
            let intermediate = ip.signed_by(&int_key, &root, &root_key).unwrap();
            let leaf_key = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).unwrap();
            let mut lp = CertificateParams::new(vec![]).unwrap();
            if marked {
                lp.custom_extensions
                    .push(marker(&[1, 2, 840, 113635, 100, 6, 11, 1]));
            }
            let leaf = lp.signed_by(&leaf_key, &intermediate, &int_key).unwrap();
            Chain {
                root: root.der().to_vec(),
                intermediate: intermediate.der().to_vec(),
                leaf: leaf.der().to_vec(),
                leaf_key,
            }
        }

        /// Une transaction signée comme le fait l'App Store.
        pub fn sign(&self, payload: &Value) -> String {
            use p256::ecdsa::signature::Signer;
            use p256::pkcs8::DecodePrivateKey;
            let header = serde_json::json!({
                "alg": "ES256",
                "x5c": [STANDARD.encode(&self.leaf), STANDARD.encode(&self.intermediate), STANDARD.encode(&self.root)],
            });
            let input = format!(
                "{}.{}",
                URL_SAFE_NO_PAD.encode(header.to_string()),
                URL_SAFE_NO_PAD.encode(payload.to_string())
            );
            let key =
                p256::ecdsa::SigningKey::from_pkcs8_der(&self.leaf_key.serialize_der()).unwrap();
            let sig: p256::ecdsa::Signature = key.sign(input.as_bytes());
            format!("{input}.{}", URL_SAFE_NO_PAD.encode(sig.to_bytes()))
        }
    }

    pub fn transaction(billing: &str, product: &str, purchase: i64) -> Value {
        serde_json::json!({
            "transactionId": "2000000123456789",
            "originalTransactionId": "2000000123456789",
            "bundleId": "app.alley.ios",
            "productId": format!("{PRODUCT_PREFIX}{product}"),
            "purchaseDate": purchase,
            "signedDate": purchase + 1000,
            "type": "Non-Renewing Subscription",
            "appAccountToken": account_token(billing),
            "environment": "Production",
        })
    }

    const NOW: i64 = 1_791_000_000_000;

    #[test]
    fn the_apple_root_is_the_real_one() {
        let root = Certificate::from_der(APPLE_ROOT_G3).unwrap();
        assert!(
            root.tbs_certificate
                .subject
                .to_string()
                .contains("Apple Root CA - G3")
        );
        assert_eq!(public_key(&root).unwrap().0, P384);
        assert!(signed_by(&root, &root), "auto-signée");
    }

    #[test]
    fn a_signed_transaction_opens_a_period_for_its_account_only() {
        let chain = Chain::new(true);
        let billing = "a".repeat(64);
        let jws = chain.sign(&transaction(&billing, "1m", NOW));
        let pass = pass(&jws, &billing, 120, &chain.root).unwrap();
        assert_eq!((pass.plan.as_str(), pass.status.as_str()), ("1m", "paid"));
        assert_eq!(pass.payment, "apple:2000000123456789");
        assert_eq!(pass.end, period_end(NOW, 120, 30));
        assert!(pass.active(NOW + 1000));
        // Un autre compte, une autre racine, un autre produit : rien.
        assert_eq!(
            super::pass(&jws, &"b".repeat(64), 0, &chain.root),
            Err("account")
        );
        assert_eq!(super::pass(&jws, &billing, 0, APPLE_ROOT_G3), Err("root"));
        let other = chain.sign(&transaction(&billing, "1y", NOW));
        assert_eq!(
            super::pass(&other, &billing, 0, &chain.root),
            Err("product")
        );
        let mut refunded = transaction(&billing, "3d", NOW);
        refunded["revocationDate"] = serde_json::json!(NOW + 5000);
        let p = super::pass(&chain.sign(&refunded), &billing, 0, &chain.root).unwrap();
        assert!(!p.active(NOW + 6000));
    }

    #[test]
    fn forged_transactions_are_refused() {
        let chain = Chain::new(true);
        let billing = "a".repeat(64);
        let jws = chain.sign(&transaction(&billing, "7d", NOW));
        // Le contenu modifié après la signature.
        let mut parts: Vec<String> = jws.split('.').map(str::to_owned).collect();
        let mut t = transaction(&billing, "3m", NOW);
        t["transactionId"] = "1".into();
        parts[1] = URL_SAFE_NO_PAD.encode(t.to_string());
        assert_eq!(verify_jws(&parts.join("."), &chain.root), Err("signature"));
        // Une chaîne sans les marques d'Apple.
        let unmarked = Chain::new(false);
        assert_eq!(
            verify_jws(
                &unmarked.sign(&transaction(&billing, "7d", NOW)),
                &unmarked.root
            ),
            Err("marker")
        );
        // Une feuille d'une autre chaîne, présentée avec la bonne racine.
        let other = Chain::new(true);
        let mixed = Chain {
            root: chain.root.clone(),
            intermediate: chain.intermediate.clone(),
            leaf: other.leaf.clone(),
            leaf_key: other.leaf_key,
        };
        assert_eq!(
            verify_jws(&mixed.sign(&transaction(&billing, "7d", NOW)), &chain.root),
            Err("chain")
        );
        assert_eq!(verify_jws("pas.un.jws", &chain.root), Err("jws"));
    }

    #[test]
    fn the_account_token_matches_the_iphone() {
        // Le même vecteur que `AccountTests` côté Swift.
        let billing = crate::billing::billing_id(&"5".repeat(64)).unwrap();
        assert_eq!(billing, "d05593d48ed176ac17657955fb34d060100d61a24286e56674dae32b2486bc87");
        assert_eq!(account_token(&billing), "c223ad81-82f3-482f-8871-c3fe9bde14e2");
    }

    #[test]
    fn the_account_token_is_a_uuid() {
        let t = account_token(&"a".repeat(64));
        assert_eq!(t.len(), 36);
        assert_eq!(&t[14..15], "4");
        assert_eq!(t, account_token(&"a".repeat(64)));
        assert_ne!(t, account_token(&"b".repeat(64)));
    }
}
