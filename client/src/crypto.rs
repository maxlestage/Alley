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
//! S'y ajoutent, pour Alley, les identifiants dérivés d'une clé, et les
//! **signatures** : chaque membre signe tout ce qu'il envoie avec une clé
//! Ed25519 tirée pour le salon ([`Signer`], [`seal_signed`],
//! [`open_signed`]). Le chiffrement empêche le serveur de lire ou de
//! fabriquer un message ; la signature garantit en plus *qui* l'a écrit, et
//! qu'il n'a été ni modifié, ni rejoué, ni détourné vers un autre salon ou
//! un autre destinataire.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use ed25519_dalek::{Signature, Signer as _, SigningKey, VerifyingKey};
use hkdf::Hkdf;
use rand_core::{OsRng, RngCore};
use serde_json::{Value, json};
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

/// Un jeton pour des octets (une photo).
pub fn seal_bytes_token(key: &Key, bytes: &[u8]) -> String {
    format!("{TOKEN_V1}{}{TOKEN_END}", b64(&seal(key, bytes)))
}

/// Les octets d'un jeton.
pub fn open_bytes_token(key: &Key, token: &str) -> Option<Vec<u8>> {
    let inner = token.strip_prefix(TOKEN_V1)?.strip_suffix(TOKEN_END)?;
    open(key, &unb64(inner)?)
}

/// L'empreinte d'une clé de photo, comme la calcule le serveur : la photo
/// reçue est bien celle qu'il a vérifiée.
pub fn key_commitment(key: &Key) -> String {
    hex(&sha256(&[key]))
}

/// Le sel du compte : le pseudonyme, en minuscules, préfixé.
pub fn account_salt(pseudonym: &str) -> Vec<u8> {
    format!("alley:v1:{}", pseudonym.trim().to_lowercase()).into_bytes()
}

/// Le nombre d'itérations PBKDF2-SHA256 qui tirent la graine d'un compte de
/// son mot de passe (le navigateur les calcule avec WebCrypto).
pub const ACCOUNT_ITERATIONS: u32 = 310_000;

/// La marque d'un compte, tirée de sa graine (PBKDF2 du mot de passe) : ce
/// que voit le serveur pour retrouver ses salons, et rien d'autre.
pub fn account_tag(seed: &[u8]) -> String {
    let hk = Hkdf::<Sha256>::new(Some(b"alley-account"), seed);
    let mut out = [0u8; 32];
    hk.expand(b"rooms-tag", &mut out).expect("32 octets");
    hex(&out)
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

/// Ce que le serveur signe d'un salon (même texte que `room_statement` du
/// serveur, `src/protocol.rs`) : identifiant, nom, privé ou non, fin.
pub fn room_statement(id: &str, name: &str, private: bool, expires_at: i64) -> Vec<u8> {
    let mut out = b"alley-room-v1\0".to_vec();
    for part in [id, name, if private { "1" } else { "0" }] {
        out.extend_from_slice(part.as_bytes());
        out.push(0);
    }
    out.extend_from_slice(expires_at.to_string().as_bytes());
    out
}

/// La signature du serveur sur un salon (`sig`, base64) est-elle valable
/// pour sa clé publique (base64) ?
pub fn room_signed_by(
    server_key: &str,
    id: &str,
    name: &str,
    private: bool,
    expires_at: i64,
    sig: &str,
) -> bool {
    let Some(public) = unb64(server_key)
        .and_then(|raw| <[u8; 32]>::try_from(raw).ok())
        .and_then(|raw| VerifyingKey::from_bytes(&raw).ok())
    else {
        return false;
    };
    let Some(sig) = unb64(sig).and_then(|raw| <[u8; 64]>::try_from(raw).ok()) else {
        return false;
    };
    public
        .verify_strict(
            &room_statement(id, name, private, expires_at),
            &Signature::from_bytes(&sig),
        )
        .is_ok()
}

/// Ce qui est signé d'une donnée gardée par l'appareil : son nom, le compte
/// qui l'a écrite (vide : sans compte) et la donnée.
pub fn vault_statement(name: &str, who: &str, data: &str) -> Vec<u8> {
    let mut out = b"alley-vault-v1\0".to_vec();
    for part in [name, who] {
        out.extend_from_slice(part.as_bytes());
        out.push(0);
    }
    out.extend_from_slice(data.as_bytes());
    out
}

/// La clé du compte pour signer ce que garde l'appareil, tirée de sa graine
/// (le mot de passe) : jamais enregistrée, elle n'existe que le temps de la
/// visite.
pub fn account_vault_key(seed: &[u8]) -> Key {
    let hk = Hkdf::<Sha256>::new(Some(b"alley-account"), seed);
    let mut out = [0u8; 32];
    hk.expand(b"vault", &mut out).expect("32 octets");
    out
}

/// Le secret d'abonnement d'un compte, tiré de sa graine (le mot de passe) :
/// il relie un abonnement payant au compte. Le serveur n'en garde que
/// l'empreinte ; il n'apprend ni le pseudonyme ni le mot de passe.
pub fn billing_secret(seed: &[u8]) -> String {
    let hk = Hkdf::<Sha256>::new(Some(b"alley-account"), seed);
    let mut out = [0u8; 32];
    hk.expand(b"billing", &mut out).expect("32 octets");
    hex(&out)
}

/// HMAC-SHA256.
pub fn mac(key: &Key, data: &[u8]) -> [u8; 32] {
    use hmac::Mac;
    let mut m = <hmac::Hmac<Sha256> as Mac>::new_from_slice(key).expect("toute longueur de clé");
    m.update(data);
    m.finalize().into_bytes().into()
}

/// Vérifie un HMAC-SHA256 (en temps constant).
pub fn mac_ok(key: &Key, data: &[u8], tag: &[u8]) -> bool {
    use hmac::Mac;
    let mut m = <hmac::Hmac<Sha256> as Mac>::new_from_slice(key).expect("toute longueur de clé");
    m.update(data);
    m.verify_slice(tag).is_ok()
}

/// Le domaine des signatures d'Alley : une signature faite ici ne vaut rien
/// ailleurs.
const SIG_DOMAIN: &[u8] = b"alley-sig-v1";

/// Une clé de signature Ed25519, tirée pour le salon (comme l'identité
/// X25519) : sa moitié publique est annoncée en entrant, et chacun vérifie
/// avec elle tout ce que l'on envoie.
pub struct Signer(SigningKey);

impl Signer {
    pub fn generate() -> Self {
        Signer(SigningKey::from_bytes(&random_bytes::<32>()))
    }

    /// Une clé donnée (vecteurs de test de l'application iOS).
    pub fn from_raw(raw: [u8; 32]) -> Self {
        Signer(SigningKey::from_bytes(&raw))
    }

    pub fn public_raw(&self) -> [u8; 32] {
        self.0.verifying_key().to_bytes()
    }

    pub fn public_b64(&self) -> String {
        b64(&self.public_raw())
    }
}

/// Ce qui est signé : le domaine, le salon, le destinataire (vide : tout le
/// salon), le numéro d'ordre de l'envoi, puis le contenu, séparés par des
/// zéros (aucun ne peut en contenir : identifiants hexadécimaux, JSON).
fn signed_bytes(room: &str, to: &str, n: u64, body: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(SIG_DOMAIN.len() + room.len() + to.len() + body.len() + 11);
    for part in [SIG_DOMAIN, room.as_bytes(), to.as_bytes()] {
        out.extend_from_slice(part);
        out.push(0);
    }
    out.extend_from_slice(&n.to_be_bytes());
    out.extend_from_slice(body.as_bytes());
    out
}

/// Chiffre un contenu signé : le jeton porte `{"b": contenu, "to", "n",
/// "sig"}`. `n` augmente à chaque envoi (une fois envoyé, un numéro ne
/// resert jamais).
pub fn seal_signed(
    key: &Key,
    signer: &Signer,
    room: &str,
    to: Option<&str>,
    n: u64,
    body: &str,
) -> String {
    let to = to.unwrap_or_default();
    let sig = signer.0.sign(&signed_bytes(room, to, n, body));
    let envelope = json!({ "b": body, "to": to, "n": n, "sig": b64(&sig.to_bytes()) });
    seal_token(key, &envelope.to_string())
}

/// Les numéros d'envoi déjà vus d'un membre : chacun ne passe qu'une fois.
/// Les envois peuvent arriver dans le désordre (une photo attend sa
/// vérification par le serveur pendant que la suite passe) : on retient les
/// numéros vus dans une fenêtre sous le plus grand, et tout ce qui est plus
/// ancien que la fenêtre est refusé.
#[derive(Clone, Debug, Default)]
pub struct Replay {
    max: u64,
    seen: std::collections::BTreeSet<u64>,
}

impl Replay {
    /// La largeur de la fenêtre, en envois.
    pub const WINDOW: u64 = 4096;

    /// `n` n'a jamais été vu (et n'est pas trop ancien pour le savoir).
    pub fn fresh(&self, n: u64) -> bool {
        n > 0 && n + Self::WINDOW > self.max && !self.seen.contains(&n)
    }

    /// Retient `n`.
    pub fn mark(&mut self, n: u64) {
        self.seen.insert(n);
        self.max = self.max.max(n);
        let floor = self.max.saturating_sub(Self::WINDOW);
        self.seen = self.seen.split_off(&floor);
    }
}

/// Pourquoi un jeton reçu est écarté.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejected {
    /// Pas pour nous : autre clé, ou jeton abîmé.
    Unreadable,
    /// L'expéditeur n'a annoncé aucune clé de signature.
    Unsigned,
    /// La signature ne correspond pas : contenu modifié, ou pas de lui.
    Forged,
    /// Un numéro déjà vu : un envoi rejoué.
    Replayed,
    /// Signé pour quelqu'un d'autre.
    Misdirected,
}

/// Ouvre un jeton signé reçu de quelqu'un dont la clé publique (base64) est
/// `signer`, dans le salon `room`, par moi (`me`), quand `seen` tient les
/// numéros déjà vus de lui. Renvoie le contenu et son numéro (à retenir dans
/// `seen` une fois le contenu accepté).
pub fn open_signed(
    key: &Key,
    token: &str,
    signer: Option<&str>,
    room: &str,
    me: &str,
    seen: &Replay,
) -> Result<(String, u64), Rejected> {
    let clear = open_token(key, token).ok_or(Rejected::Unreadable)?;
    let envelope: Value = serde_json::from_str(&clear).map_err(|_| Rejected::Unreadable)?;
    let public: [u8; 32] = signer
        .and_then(unb64)
        .and_then(|raw| raw.try_into().ok())
        .ok_or(Rejected::Unsigned)?;
    let public = VerifyingKey::from_bytes(&public).map_err(|_| Rejected::Unsigned)?;
    let (Some(body), Some(to), Some(n), Some(sig)) = (
        envelope["b"].as_str(),
        envelope["to"].as_str(),
        envelope["n"].as_u64(),
        envelope["sig"].as_str(),
    ) else {
        return Err(Rejected::Forged);
    };
    let sig: [u8; 64] = unb64(sig)
        .and_then(|raw| raw.try_into().ok())
        .ok_or(Rejected::Forged)?;
    public
        .verify_strict(
            &signed_bytes(room, to, n, body),
            &Signature::from_bytes(&sig),
        )
        .map_err(|_| Rejected::Forged)?;
    if !to.is_empty() && to != me {
        return Err(Rejected::Misdirected);
    }
    if !seen.fresh(n) {
        return Err(Rejected::Replayed);
    }
    Ok((body.to_owned(), n))
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

    #[test]
    fn signed_tokens_reject_any_change() {
        let key = generate_key();
        let (ada, eve) = (Signer::generate(), Signer::generate());
        let ada_pub = ada.public_b64();
        let fresh = Replay::default();
        let mut seen_one = Replay::default();
        seen_one.mark(1);
        let open = |token: &str, signer: &str, room: &str, me: &str, seen: &Replay| {
            open_signed(&key, token, Some(signer), room, me, seen)
        };
        let token = seal_signed(&key, &ada, "salon", None, 1, r#"{"k":"msg","x":"salut"}"#);
        assert_eq!(
            open(&token, &ada_pub, "salon", "bob", &fresh),
            Ok((r#"{"k":"msg","x":"salut"}"#.to_owned(), 1))
        );
        // Une autre clé que celle annoncée par l'expéditeur.
        assert_eq!(
            open(&token, &eve.public_b64(), "salon", "bob", &fresh),
            Err(Rejected::Forged)
        );
        // Sans clé annoncée, rien ne passe.
        assert_eq!(
            open_signed(&key, &token, None, "salon", "bob", &fresh),
            Err(Rejected::Unsigned)
        );
        // Rejoué dans un autre salon, ou une seconde fois.
        assert_eq!(
            open(&token, &ada_pub, "autre", "bob", &fresh),
            Err(Rejected::Forged)
        );
        assert_eq!(
            open(&token, &ada_pub, "salon", "bob", &seen_one),
            Err(Rejected::Replayed)
        );
        // Un membre (qui a la clé du salon) modifie le contenu, le numéro ou
        // le destinataire : la signature ne correspond plus.
        let clear = open_token(&key, &token).unwrap();
        for (field, value) in [
            ("b", json!(r#"{"k":"msg","x":"modifié"}"#)),
            ("n", json!(7)),
            ("to", json!("bob")),
        ] {
            let mut envelope: Value = serde_json::from_str(&clear).unwrap();
            envelope[field] = value;
            let altered = seal_token(&key, &envelope.to_string());
            assert_eq!(
                open(&altered, &ada_pub, "salon", "bob", &fresh),
                Err(Rejected::Forged),
                "{field}"
            );
        }
        // Un envoi pour Carl n'est pas pour Bob.
        let private = seal_signed(&key, &ada, "salon", Some("carl"), 2, "{}");
        assert_eq!(
            open(&private, &ada_pub, "salon", "bob", &fresh),
            Err(Rejected::Misdirected)
        );
        assert!(open(&private, &ada_pub, "salon", "carl", &seen_one).is_ok());
        // Un jeton non signé (l'ancien format) est écarté.
        let bare = seal_token(&key, r#"{"k":"msg","x":"salut"}"#);
        assert_eq!(
            open(&bare, &ada_pub, "salon", "bob", &fresh),
            Err(Rejected::Forged)
        );
        assert_eq!(
            open_signed(
                &generate_key(),
                &token,
                Some(&ada_pub),
                "salon",
                "bob",
                &fresh
            ),
            Err(Rejected::Unreadable)
        );
    }

    #[test]
    fn each_number_passes_once_even_out_of_order() {
        let mut seen = Replay::default();
        assert!(!seen.fresh(0), "0 n'est jamais émis");
        for n in [1, 2, 5, 3] {
            assert!(seen.fresh(n), "{n}");
            seen.mark(n);
        }
        // 4 arrive en retard (une photo vérifiée) : il passe, une fois.
        assert!(seen.fresh(4));
        seen.mark(4);
        assert!(!seen.fresh(4) && !seen.fresh(2));
        // Trop ancien pour savoir s'il a été vu : refusé.
        seen.mark(10 + Replay::WINDOW);
        assert!(!seen.fresh(9));
        assert!(seen.fresh(11) && seen.fresh(11 + Replay::WINDOW));
        assert!(seen.seen.len() <= 2, "la mémoire reste bornée");
    }

    #[test]
    fn room_statements_match_the_server_and_resist_edits() {
        let key = SigningKey::from_bytes(&[3u8; 32]);
        let public = b64(&key.verifying_key().to_bytes());
        let statement = room_statement("ab12", "Café", true, 1_791_936_000_000);
        assert_eq!(
            statement,
            b"alley-room-v1\0ab12\0Caf\xc3\xa9\x001\x001791936000000".to_vec()
        );
        let sig = b64(&key.sign(&statement).to_bytes());
        assert!(room_signed_by(
            &public,
            "ab12",
            "Café",
            true,
            1_791_936_000_000,
            &sig
        ));
        assert!(!room_signed_by(
            &public,
            "ab12",
            "Phishing",
            true,
            1_791_936_000_000,
            &sig
        ));
        assert!(!room_signed_by(
            &public,
            "ab12",
            "Café",
            false,
            1_791_936_000_000,
            &sig
        ));
        assert!(!room_signed_by(
            &public,
            "ab12",
            "Café",
            true,
            1_891_936_000_000,
            &sig
        ));
        assert!(!room_signed_by(
            &public,
            "ab12",
            "Café",
            true,
            1_791_936_000_000,
            "abîmé"
        ));
    }

    #[test]
    fn the_account_signature_needs_the_password() {
        let key = account_vault_key(b"graine du mot de passe");
        assert_ne!(key, account_vault_key(b"une autre graine"));
        let data = vault_statement("alley.rooms.v1", "compte", "[]");
        let tag = mac(&key, &data);
        assert!(mac_ok(&key, &data, &tag));
        assert!(!mac_ok(
            &key,
            &vault_statement("alley.rooms.v1", "compte", "[{}]"),
            &tag
        ));
        assert!(!mac_ok(
            &key,
            &vault_statement("alley.server", "compte", "[]"),
            &tag
        ));
        assert!(!mac_ok(
            &key,
            &vault_statement("alley.rooms.v1", "", "[]"),
            &tag
        ));
        assert!(!mac_ok(&account_vault_key(b"autre"), &data, &tag));
    }
}
