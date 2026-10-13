//! Ce que le serveur sait (et surtout ne sait pas) du chiffrement.
//!
//! Alley reprend le protocole de la messagerie **Brocolis 🥦** : chaque texte
//! (nom de serveur, nom de salon, pseudo, message, image) est chiffré dans le
//! navigateur en AES-256-GCM, représentation « combined » de CryptoKit
//! (`nonce(12) ‖ chiffré ‖ tag(16)`), puis écrit comme un jeton
//! `[[BROCOLIS:v1:<base64>]]`. Un jeton d'Alley se déchiffre donc dans l'app
//! Brocolis avec la même clé, et inversement.
//!
//! Le serveur ne reçoit jamais la clé : elle voyage dans le fragment (`#…`) du
//! lien d'invitation, que les navigateurs n'envoient pas. Il ne peut que
//! vérifier la **forme** des jetons, et refuse tout ce qui ressemble à du
//! clair : rien n'est stocké sans être chiffré.
//!
//! Identifiants et secrets (tous en hexadécimal, que le pare-feu laisse passer
//! sans surprise) :
//!
//! - l'identifiant d'un serveur est dérivé de sa clé par le navigateur
//!   (`SHA-256("brocolis-conv-v1" ‖ clé)`, comme une conversation Brocolis) ;
//! - le **jeton d'accès** (`SHA-256("alley-access-v1" ‖ clé)`) prouve qu'on
//!   connaît la clé ; le serveur n'en garde que l'empreinte ;
//! - le **secret d'admin** est tiré au hasard par le créateur ;
//! - chaque membre tire un **secret de membre** : son identifiant public est
//!   l'empreinte de ce secret, et lui seul peut modifier ses messages.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use sha2::{Digest, Sha256};

pub const TOKEN_V1: &str = "[[BROCOLIS:v1:";
pub const TOKEN_END: &str = "]]";

/// Nonce (12) + tag (16) : un jeton plus court ne contient rien.
const MIN_COMBINED: usize = 12 + 16;

/// Taille maximale d'un jeton de nom (serveur, salon, pseudo).
pub const MAX_NAME_TOKEN: usize = 1024;
/// Taille maximale d'un profil chiffré (pseudo et clés publiques).
pub const MAX_PROFILE_TOKEN: usize = 4096;
/// Taille maximale de l'état chiffré d'un serveur (nom, salons, anciennes clés).
pub const MAX_STATE_TOKEN: usize = 256 * 1024;
/// Taille maximale d'un message relayé en direct (présence, signalisation des appels).
pub const MAX_RELAY_TOKEN: usize = 64 * 1024;
/// Taille maximale d'un message : du texte, ou une image chiffrée.
pub const MAX_MESSAGE_TOKEN: usize = 6 * 1024 * 1024;

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
    // Le décodage valide l'alphabet et le remplissage en une passe.
    match STANDARD.decode(inner) {
        Ok(bytes) => bytes.len() >= MIN_COMBINED,
        Err(_) => false,
    }
}

/// Du base64 standard qui décode en exactement `len` octets (une clé
/// publique X25519, une clé emballée).
pub fn is_b64_len(text: &str, len: usize) -> bool {
    text.len() <= len * 2 && STANDARD.decode(text).is_ok_and(|b| b.len() == len)
}

/// Une clé de serveur emballée pour un membre (format Brocolis
/// `wrapGroupKey`) : nonce (12) + clé (32) + tag (16).
pub const WRAPPED_KEY_LEN: usize = 12 + 32 + 16;
/// Une clé publique X25519 brute.
pub const PUBLIC_KEY_LEN: usize = 32;

/// Un identifiant ou un secret : `len` caractères hexadécimaux minuscules.
pub fn is_hex(text: &str, len: usize) -> bool {
    text.len() == len && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Longueur d'un identifiant de serveur (128 bits).
pub const GUILD_ID_LEN: usize = 32;
/// Longueur d'un secret (256 bits) : accès, admin, membre.
pub const SECRET_LEN: usize = 64;

/// L'empreinte qu'on garde d'un secret, en hexadécimal.
pub fn digest(secret: &str) -> String {
    hex(&Sha256::digest(secret.as_bytes()))
}

/// L'identifiant public d'un membre : les 16 premiers caractères de
/// l'empreinte de son secret (assez pour ne jamais se confondre).
pub fn member_id(secret: &str) -> String {
    digest(secret)[..16].to_owned()
}

/// Compare deux empreintes sans s'arrêter au premier écart.
pub fn same(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_well_formed_tokens_pass() {
        // Le jeton de référence de Brocolis (« bonjour », clé 0x09 × 32).
        let token = "[[BROCOLIS:v1:uAI9wrumtTlX/P8fBdLphS15X//kXm4SZje5yfVDld2L3Ac=]]";
        assert!(is_token(token, MAX_NAME_TOKEN));
        assert!(!is_token(token, 10), "trop long");
        assert!(!is_token("bonjour", MAX_NAME_TOKEN), "du clair");
        assert!(
            !is_token("[[BROCOLIS:v1:bonjour]]", MAX_NAME_TOKEN),
            "pas du base64"
        );
        assert!(
            !is_token("[[BROCOLIS:v1:AAAA]]", MAX_NAME_TOKEN),
            "trop court"
        );
        assert!(
            !is_token(&format!("salut {token}"), MAX_NAME_TOKEN),
            "clair autour"
        );
        assert!(
            !is_token(&format!("{token}{token}"), MAX_NAME_TOKEN),
            "deux jetons"
        );
    }

    #[test]
    fn hex_ids_and_digests() {
        assert!(is_hex(&"a1".repeat(16), GUILD_ID_LEN));
        assert!(!is_hex(&"A1".repeat(16), GUILD_ID_LEN));
        assert!(!is_hex("--", 2));
        let d = digest("secret");
        assert_eq!(d.len(), 64);
        assert_eq!(member_id("secret"), d[..16]);
        assert!(same(&d, &digest("secret")));
        assert!(is_b64_len(
            "pOCSkrZRwni5dyxWn1+puxPZBrRqtoyd+dwrRAn4ogk=",
            PUBLIC_KEY_LEN
        ));
        assert!(!is_b64_len(
            "pOCSkrZRwni5dyxWn1+puxPZBrRqtoyd+dwrRAn4ogk=",
            WRAPPED_KEY_LEN
        ));
        assert!(!is_b64_len("pas du base64", PUBLIC_KEY_LEN));
        assert!(!same(&d, &digest("autre")));
    }
}
