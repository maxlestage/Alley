//! Le chiffrement d'Alley : celui de la messagerie **Brocolis 🥦**
//! (`site/src/crypto.rs`, app iOS), en Rust, dans le navigateur
//! (WebAssembly) comme dans les tests du serveur :
//!
//! - AES-256-GCM, représentation « combined » de CryptoKit :
//!   nonce (12 octets) ‖ chiffré ‖ tag (16 octets), en base64 standard ;
//! - jetons `[[BROCOLIS:v1:<base64>]]` ;
//! - identité X25519 ; clé d'emballage = HKDF-SHA256(secret partagé, sel
//!   `brocolis-x25519-wrap`, info = les deux clés publiques triées) : c'est
//!   ainsi que la clé d'un salon est remise à chaque nouvel arrivant ;
//! - empreinte : 8 premiers octets de SHA-256, en hexa groupé par 4.
//!
//! S'y ajoutent, pour Alley, les identifiants dérivés d'une clé.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use hkdf::Hkdf;
use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey, StaticSecret};

pub const TOKEN_V1: &str = "[[BROCOLIS:v1:";
pub const TOKEN_END: &str = "]]";
const WRAP_SALT: &[u8] = b"brocolis-x25519-wrap";

/// Une clé de serveur (AES-256).
pub type Key = [u8; 32];

/// Des octets aléatoires (`crypto.getRandomValues` dans le navigateur).
pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut out = [0u8; N];
    OsRng.fill_bytes(&mut out);
    out
}

pub fn b64(data: &[u8]) -> String {
    STANDARD.encode(data)
}

pub fn unb64(text: &str) -> Option<Vec<u8>> {
    STANDARD.decode(text.trim()).ok()
}

pub fn b64url(data: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(data)
}

pub fn unb64url(text: &str) -> Option<Vec<u8>> {
    URL_SAFE_NO_PAD
        .decode(text.trim().trim_end_matches('='))
        .ok()
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn sha256(parts: &[&[u8]]) -> [u8; 32] {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

/// SHA-256 d'un texte, en hexadécimal (identifiants, preuves d'auteur).
pub fn sha256_hex(text: &str) -> String {
    hex(&sha256(&[text.as_bytes()]))
}

/// Un secret de 256 bits, en hexadécimal (admin, membre, sel).
pub fn secret() -> String {
    hex(&random_bytes::<32>())
}

/// L'identifiant public d'un membre : le début de l'empreinte de son secret
/// (le serveur calcule le même).
pub fn member_id(secret: &str) -> String {
    sha256_hex(secret)[..16].to_owned()
}

/// Une clé de serveur : 32 octets aléatoires.
pub fn generate_key() -> Key {
    random_bytes()
}

/// Une clé lue dans un lien (base64url) ou une sauvegarde.
pub fn key_from_b64url(text: &str) -> Option<Key> {
    unb64url(text)?.try_into().ok()
}

/// AES-256-GCM, représentation « combined » de CryptoKit.
pub fn seal(key: &Key, plain: &[u8]) -> Vec<u8> {
    let cipher = Aes256Gcm::new_from_slice(key).expect("clé de 32 octets");
    let nonce = random_bytes::<12>();
    let mut out = nonce.to_vec();
    out.extend(
        cipher
            .encrypt(&Nonce::from(nonce), plain)
            .expect("chiffrement AES-GCM"),
    );
    out
}

pub fn open(key: &Key, combined: &[u8]) -> Option<Vec<u8>> {
    if combined.len() < 12 + 16 {
        return None;
    }
    let cipher = Aes256Gcm::new_from_slice(key).ok()?;
    let (nonce, body) = combined.split_at(12);
    let nonce: [u8; 12] = nonce.try_into().ok()?;
    cipher.decrypt(&Nonce::from(nonce), body).ok()
}

/// Un jeton `[[BROCOLIS:v1:…]]` pour `text`.
pub fn seal_token(key: &Key, text: &str) -> String {
    format!("{TOKEN_V1}{}{TOKEN_END}", b64(&seal(key, text.as_bytes())))
}

/// Le clair d'un jeton, ou `None` (autre clé, jeton abîmé).
pub fn open_token(key: &Key, token: &str) -> Option<String> {
    let inner = token.strip_prefix(TOKEN_V1)?.strip_suffix(TOKEN_END)?;
    String::from_utf8(open(key, &unb64(inner)?)?).ok()
}

/// Le clair d'un jeton, avec la première clé du trousseau qui l'ouvre.
pub fn open_with_any(ring: &[Key], token: &str) -> Option<String> {
    ring.iter().find_map(|k| open_token(k, token))
}

/// Ce que l'on dérive d'une clé de serveur.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Derived {
    /// L'identifiant du serveur (128 bits), comme une conversation Brocolis.
    pub gid: String,
    /// La preuve qu'on connaît la clé ; le serveur n'en garde que l'empreinte.
    pub access: String,
    pub fingerprint: String,
}

pub fn derive(key: &Key) -> Derived {
    Derived {
        gid: hex(&sha256(&[b"brocolis-conv-v1", key]))[..32].to_owned(),
        access: hex(&sha256(&[b"alley-access-v1", key])),
        fingerprint: fingerprint(key),
    }
}

/// L'empreinte à comparer de vive voix (`4BB0-6F8E-4E3A-7715`).
pub fn fingerprint(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    let hex: String = digest[..8].iter().map(|b| format!("{b:02X}")).collect();
    hex.as_bytes()
        .chunks(4)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect::<Vec<_>>()
        .join("-")
}

/// Une identité X25519 (comme `Identity` de Brocolis) : pour recevoir une clé
/// de serveur renouvelée, chiffrée pour soi seul.
pub struct Identity(StaticSecret);

impl Identity {
    pub fn generate() -> Self {
        Identity(StaticSecret::from(random_bytes::<32>()))
    }

    pub fn from_raw(raw: [u8; 32]) -> Self {
        Identity(StaticSecret::from(raw))
    }

    pub fn from_b64(text: &str) -> Option<Self> {
        Some(Identity::from_raw(unb64(text)?.try_into().ok()?))
    }

    pub fn secret_b64(&self) -> String {
        b64(&self.0.to_bytes())
    }

    pub fn public_raw(&self) -> [u8; 32] {
        PublicKey::from(&self.0).to_bytes()
    }

    pub fn public_b64(&self) -> String {
        b64(&self.public_raw())
    }

    /// La clé d'emballage partagée avec `peer` (même valeur des deux côtés).
    pub fn wrap_key(&self, peer_b64: &str) -> Option<Key> {
        let peer: [u8; 32] = unb64(peer_b64)?.try_into().ok()?;
        let shared = self.0.diffie_hellman(&PublicKey::from(peer));
        let mine = self.public_raw();
        let info = if mine[..] <= peer[..] {
            [mine, peer].concat()
        } else {
            [peer, mine].concat()
        };
        let mut out = [0u8; 32];
        Hkdf::<Sha256>::new(Some(WRAP_SALT), shared.as_bytes())
            .expand(&info, &mut out)
            .ok()?;
        Some(out)
    }

    /// Emballe une clé de serveur pour `peer` (comme `wrapGroupKey`).
    pub fn wrap_group_key(&self, group_key: &Key, peer_b64: &str) -> Option<String> {
        Some(b64(&seal(&self.wrap_key(peer_b64)?, group_key)))
    }

    /// Déballe une clé reçue de `from` (comme `unwrapGroupKey`).
    pub fn unwrap_group_key(&self, blob_b64: &str, from_b64: &str) -> Option<Key> {
        open(&self.wrap_key(from_b64)?, &unb64(blob_b64)?)?
            .try_into()
            .ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Les valeurs de référence de Brocolis (`fixed_vectors_match_the_app`).
    #[test]
    fn brocolis_vectors() {
        let alice = Identity::from_raw([1u8; 32]);
        let bob = Identity::from_raw([2u8; 32]);
        assert_eq!(
            alice.public_b64(),
            "pOCSkrZRwni5dyxWn1+puxPZBrRqtoyd+dwrRAn4ogk="
        );
        let k = alice.wrap_key(&bob.public_b64()).unwrap();
        assert_eq!(k, bob.wrap_key(&alice.public_b64()).unwrap());
        assert_eq!(b64(&k), "rv+EmjbG8ZYNWUlwkDV57/YVEL9zkAScnM6iKz0T+UM=");
        assert_eq!(fingerprint(&[7u8; 32]), "4BB0-6F8E-4E3A-7715");
        let blob =
            "81p2c6nvKV6OSrdquUov4sR9A5t47pJNiL7Jp8fiFhB7ar7PZf8em8dWQKxQqu/NP/BohUyZB0nEsJ5M";
        assert_eq!(
            bob.unwrap_group_key(blob, &alice.public_b64()),
            Some([9u8; 32])
        );
        assert_eq!(
            Identity::generate().unwrap_group_key(blob, &alice.public_b64()),
            None
        );
        let token = "[[BROCOLIS:v1:uAI9wrumtTlX/P8fBdLphS15X//kXm4SZje5yfVDld2L3Ac=]]";
        assert_eq!(open_token(&[9u8; 32], token).as_deref(), Some("bonjour"));
    }

    #[test]
    fn tokens_round_trip() {
        let key = generate_key();
        let token = seal_token(&key, "salut 🥦");
        assert!(token.starts_with(TOKEN_V1) && token.ends_with(TOKEN_END));
        assert_eq!(open_token(&key, &token).as_deref(), Some("salut 🥦"));
        assert_eq!(open_token(&generate_key(), &token), None);
        assert_eq!(open_token(&key, "du clair"), None);
        assert_eq!(
            open_with_any(&[generate_key(), key], &token).as_deref(),
            Some("salut 🥦")
        );
    }

    #[test]
    fn derived_ids_are_hex_and_match_brocolis() {
        let d = derive(&[7u8; 32]);
        assert_eq!(d, derive(&[7u8; 32]));
        assert_eq!(d.gid.len(), 32);
        assert_eq!(d.access.len(), 64);
        // Le début de l'identifiant de conversation Brocolis de cette clé
        // (`0cPmmicIaMca…` en base64url).
        let bytes: Vec<u8> = (0..9)
            .map(|i| u8::from_str_radix(&d.gid[2 * i..2 * i + 2], 16).unwrap())
            .collect();
        assert_eq!(b64url(&bytes), "0cPmmicIaMca");
        assert_eq!(d.fingerprint, "4BB0-6F8E-4E3A-7715");
        let key = generate_key();
        assert_eq!(key_from_b64url(&b64url(&key)), Some(key));
        assert_eq!(member_id("secret"), sha256_hex("secret")[..16]);
    }

    #[test]
    fn group_keys_travel_wrapped() {
        let (admin, member) = (Identity::generate(), Identity::generate());
        let key = generate_key();
        let blob = admin.wrap_group_key(&key, &member.public_b64()).unwrap();
        assert_eq!(
            member.unwrap_group_key(&blob, &admin.public_b64()),
            Some(key)
        );
        let back = Identity::from_b64(&member.secret_b64()).unwrap();
        assert_eq!(back.public_b64(), member.public_b64());
    }
}
