//! Ce que le serveur sait (et surtout ne sait pas) du chiffrement.
//!
//! Alley reprend le protocole de la messagerie **Brocolis 🥦** : tout ce que
//! se disent les membres d'un salon est chiffré dans le navigateur en
//! AES-256-GCM (représentation « combined » de CryptoKit : `nonce(12) ‖
//! chiffré ‖ tag(16)`), sous forme de jetons `[[BROCOLIS:v1:<base64>]]`. La
//! clé du salon passe d'un membre à l'autre emballée par X25519 + HKDF-SHA256,
//! exactement comme l'appairage Brocolis.
//!
//! Le serveur ne voit jamais la clé : il ne peut que vérifier la **forme** de
//! ce qu'il relaie, et refuse tout ce qui ressemble à du clair.

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};

pub const TOKEN_V1: &str = "[[BROCOLIS:v1:";
pub const TOKEN_END: &str = "]]";

/// Nonce (12) + tag (16) : un jeton plus court ne contient rien.
const MIN_COMBINED: usize = 12 + 16;

/// Taille maximale d'un jeton relayé : un message, une photo ou un vocal.
pub const MAX_TOKEN: usize = 8 * 1024 * 1024;

/// Une clé de salon emballée pour un membre (format Brocolis
/// `wrapGroupKey`) : nonce (12) + clé (32) + tag (16).
pub const WRAPPED_KEY_LEN: usize = 12 + 32 + 16;
/// Une clé publique X25519 brute.
pub const PUBLIC_KEY_LEN: usize = 32;

/// `true` si `text` est exactement un jeton Brocolis v1 bien formé, d'au plus
/// `max` octets : un base64 standard qui contient au moins un nonce et un tag.
pub fn is_token(text: &str, max: usize) -> bool {
    if text.len() > max {
        return false;
    }
    let Some(inner) = text
        .strip_prefix(TOKEN_V1)
        .and_then(|rest| rest.strip_suffix(TOKEN_END))
    else {
        return false;
    };
    match STANDARD.decode(inner) {
        Ok(bytes) => bytes.len() >= MIN_COMBINED,
        Err(_) => false,
    }
}

/// Du base64 standard qui décode en exactement `len` octets.
pub fn is_b64_len(text: &str, len: usize) -> bool {
    text.len() <= len * 2 && STANDARD.decode(text).is_ok_and(|b| b.len() == len)
}

/// Une clé publique X25519 reçue dans une adresse (base64url), remise en
/// base64 standard ; `None` si ce n'en est pas une.
pub fn public_key_from_url(text: &str) -> Option<String> {
    let bytes = URL_SAFE_NO_PAD
        .decode(text.trim().trim_end_matches('='))
        .ok()?;
    (bytes.len() == PUBLIC_KEY_LEN).then(|| STANDARD.encode(bytes))
}

/// Un identifiant aléatoire et imprévisible (salons, connexions), en
/// hexadécimal : `bytes` octets tirés du générateur du système.
pub fn random_id(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    getrandom::getrandom(&mut buf).expect("générateur aléatoire du système");
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_well_formed_tokens_pass() {
        // Le jeton de référence de Brocolis (« bonjour », clé 0x09 × 32).
        let token = "[[BROCOLIS:v1:uAI9wrumtTlX/P8fBdLphS15X//kXm4SZje5yfVDld2L3Ac=]]";
        assert!(is_token(token, MAX_TOKEN));
        assert!(!is_token(token, 10), "trop long");
        assert!(!is_token("bonjour", MAX_TOKEN), "du clair");
        assert!(
            !is_token("[[BROCOLIS:v1:bonjour]]", MAX_TOKEN),
            "pas du base64"
        );
        assert!(!is_token("[[BROCOLIS:v1:AAAA]]", MAX_TOKEN), "trop court");
        assert!(
            !is_token(&format!("salut {token}"), MAX_TOKEN),
            "clair autour"
        );
    }

    #[test]
    fn keys_and_ids() {
        let public = "pOCSkrZRwni5dyxWn1-puxPZBrRqtoyd-dwrRAn4ogk";
        assert_eq!(
            public_key_from_url(public).as_deref(),
            Some("pOCSkrZRwni5dyxWn1+puxPZBrRqtoyd+dwrRAn4ogk=")
        );
        assert!(public_key_from_url("trop court").is_none());
        assert!(is_b64_len(
            "pOCSkrZRwni5dyxWn1+puxPZBrRqtoyd+dwrRAn4ogk=",
            PUBLIC_KEY_LEN
        ));
        assert!(!is_b64_len("pas du base64", PUBLIC_KEY_LEN));
        let (a, b) = (random_id(8), random_id(8));
        assert_eq!(a.len(), 16);
        assert_ne!(a, b);
    }
}
