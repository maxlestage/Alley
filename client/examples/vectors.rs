//! Des vecteurs de test pour l'application iOS (AlleyKit) : produits par le
//! code Rust du navigateur, ils doivent s'ouvrir côté Swift.
//! `cargo run -p alley-web --example vectors`
use alley_web::crypto::*;

fn main() {
    let key: Key = [7u8; 32];
    let seed = [1u8; 32];
    let a = Identity::from_raw([1u8; 32]);
    let b = Identity::from_raw([2u8; 32]);
    let signer = Signer::from_raw([3u8; 32]);
    let v = serde_json::json!({
        "key": b64(&key),
        "token": seal_token(&key, "Bonjour 🌍 de Rust"),
        "fingerprint": fingerprint(&key),
        "commitment": key_commitment(&key),
        "tag": account_tag(&seed),
        "billing": billing_secret(&seed),
        "vault": b64(&account_vault_key(&seed)),
        "a_public": a.public_b64(),
        "b_public": b.public_b64(),
        "wrapped_for_b": a.wrap_group_key(&key, &b.public_b64()).unwrap(),
        "signer_public": signer.public_b64(),
        "envelope": seal_signed(&key, &signer, "room1", Some("me42"), 5, r#"{"k":"msg","x":"salut"}"#),
        "statement": b64(&room_statement("ab12cd", "Café", true, 1791936000000)),
    });
    println!("{}", serde_json::to_string_pretty(&v).unwrap());
}
