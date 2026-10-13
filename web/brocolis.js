// Le chiffrement d'Alley : EXACTEMENT celui de la messagerie Brocolis 🥦
// (app iOS, site, `site/src/crypto.rs`), avec la WebCrypto du navigateur :
//
// - AES-256-GCM, représentation « combined » de CryptoKit :
//   nonce (12 octets) ‖ chiffré ‖ tag (16 octets), en base64 standard ;
// - jetons `[[BROCOLIS:v1:<base64>]]` ;
// - photos : le clair est `BRCM1:img:<base64 JPEG>` ;
// - empreinte : 8 premiers octets de SHA-256(clé), en hexa groupé par 4.
//
// Ce module ne touche ni au DOM ni au réseau : il marche aussi sous Node
// (`node --test web/brocolis.test.mjs`).

export const TOKEN_V1 = "[[BROCOLIS:v1:";
export const TOKEN_END = "]]";
export const IMAGE_PREFIX = "BRCM1:img:";

const subtle = globalThis.crypto.subtle;
const encoder = new TextEncoder();
const decoder = new TextDecoder("utf-8", { fatal: true });

export function randomBytes(n) {
  return globalThis.crypto.getRandomValues(new Uint8Array(n));
}

export function toB64(bytes) {
  let bin = "";
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    bin += String.fromCharCode.apply(null, bytes.subarray(i, i + chunk));
  }
  return btoa(bin);
}

export function fromB64(text) {
  const bin = atob(text.trim());
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

export function toB64Url(bytes) {
  return toB64(bytes).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

export function fromB64Url(text) {
  const b64 = text.trim().replace(/-/g, "+").replace(/_/g, "/");
  return fromB64(b64 + "=".repeat((4 - (b64.length % 4)) % 4));
}

export function toHex(bytes) {
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

export function concat(...parts) {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let at = 0;
  for (const p of parts) {
    out.set(p, at);
    at += p.length;
  }
  return out;
}

export async function sha256(bytes) {
  return new Uint8Array(await subtle.digest("SHA-256", bytes));
}

/** Une clé de serveur : 32 octets aléatoires. */
export function generateKey() {
  return randomBytes(32);
}

/** La clé brute, prête pour AES-GCM (non exportable). */
export function importKey(raw) {
  if (raw.length !== 32) throw new Error("une clé fait 32 octets");
  return subtle.importKey("raw", raw, "AES-GCM", false, ["encrypt", "decrypt"]);
}

export async function seal(key, plain) {
  const nonce = randomBytes(12);
  const body = new Uint8Array(await subtle.encrypt({ name: "AES-GCM", iv: nonce }, key, plain));
  return concat(nonce, body);
}

export async function open(key, combined) {
  if (combined.length < 12 + 16) return null;
  try {
    const plain = await subtle.decrypt({ name: "AES-GCM", iv: combined.subarray(0, 12) }, key, combined.subarray(12));
    return new Uint8Array(plain);
  } catch {
    return null;
  }
}

/** Un jeton `[[BROCOLIS:v1:…]]` pour `text`. */
export async function sealToken(key, text) {
  return TOKEN_V1 + toB64(await seal(key, encoder.encode(text))) + TOKEN_END;
}

/** Le clair d'un jeton, ou `null` (autre clé, jeton abîmé). */
export async function openToken(key, token) {
  if (typeof token !== "string" || !token.startsWith(TOKEN_V1) || !token.endsWith(TOKEN_END)) {
    return null;
  }
  let combined;
  try {
    combined = fromB64(token.slice(TOKEN_V1.length, -TOKEN_END.length));
  } catch {
    return null;
  }
  const plain = await open(key, combined);
  if (!plain) return null;
  try {
    return decoder.decode(plain);
  } catch {
    return null;
  }
}

/** Tout ce que l'on dérive d'une clé de serveur. */
export async function derive(raw) {
  const conv = await sha256(concat(encoder.encode("brocolis-conv-v1"), raw));
  const access = await sha256(concat(encoder.encode("alley-access-v1"), raw));
  return {
    // L'identifiant du serveur (128 bits), comme une conversation Brocolis.
    gid: toHex(conv).slice(0, 32),
    // La preuve qu'on connaît la clé ; le serveur n'en garde que l'empreinte.
    access: toHex(access),
    fingerprint: await fingerprint(raw),
  };
}

/** L'empreinte à comparer de vive voix (`4BB0-6F8E-4E3A-7715`). */
export async function fingerprint(raw) {
  const hex = toHex((await sha256(raw)).subarray(0, 8)).toUpperCase();
  return hex.match(/.{4}/g).join("-");
}

/** Un secret de 256 bits, en hexadécimal (admin, membre). */
export function secret() {
  return toHex(randomBytes(32));
}

// ---------------------------------------------------------------------------
// Appairage X25519 (comme `Identity` de Brocolis) : redistribuer une clé de
// serveur renouvelée à chaque membre, chiffrée pour lui seul.
//
// clé d'emballage = HKDF-SHA256(secret partagé, sel « brocolis-x25519-wrap »,
// info = les deux clés publiques triées), puis AES-GCM « combined ».

const WRAP_SALT = encoder.encode("brocolis-x25519-wrap");
// En-tête PKCS#8 d'une clé privée X25519 brute (RFC 8410).
const X25519_PKCS8 = Uint8Array.from([0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x6e, 0x04, 0x22, 0x04, 0x20]);

/** Le navigateur sait-il faire du X25519 ? (Safari 17, Chrome 133, Firefox 130.) */
export async function hasX25519() {
  try {
    await subtle.generateKey({ name: "X25519" }, true, ["deriveBits"]);
    return true;
  } catch {
    return false;
  }
}

/** Une identité X25519 : `{ priv: base64 PKCS#8, pub: base64 brut }`. */
export async function newIdentity() {
  const pair = await subtle.generateKey({ name: "X25519" }, true, ["deriveBits"]);
  return {
    priv: toB64(new Uint8Array(await subtle.exportKey("pkcs8", pair.privateKey))),
    pub: toB64(new Uint8Array(await subtle.exportKey("raw", pair.publicKey))),
  };
}

/** L'identité d'une clé privée brute de 32 octets (vecteurs de test). */
export async function identityFromRaw(raw) {
  const pkcs8 = concat(X25519_PKCS8, raw);
  const key = await subtle.importKey("pkcs8", pkcs8, { name: "X25519" }, true, ["deriveBits"]);
  const jwk = await subtle.exportKey("jwk", key);
  return { priv: toB64(pkcs8), pub: toB64(fromB64Url(jwk.x)) };
}

function sortedConcat(a, b) {
  for (let i = 0; i < Math.min(a.length, b.length); i++) {
    if (a[i] !== b[i]) return a[i] < b[i] ? concat(a, b) : concat(b, a);
  }
  return a.length <= b.length ? concat(a, b) : concat(b, a);
}

/** La clé d'emballage partagée avec `peerPub` (même valeur des deux côtés). */
export async function wrapKeyBytes(identity, peerPub) {
  const priv = await subtle.importKey("pkcs8", fromB64(identity.priv), { name: "X25519" }, false, ["deriveBits"]);
  const peerRaw = fromB64(peerPub);
  const peer = await subtle.importKey("raw", peerRaw, { name: "X25519" }, false, []);
  const shared = await subtle.deriveBits({ name: "X25519", public: peer }, priv, 256);
  const hkdf = await subtle.importKey("raw", shared, "HKDF", false, ["deriveBits"]);
  const info = sortedConcat(fromB64(identity.pub), peerRaw);
  return new Uint8Array(await subtle.deriveBits({ name: "HKDF", hash: "SHA-256", salt: WRAP_SALT, info }, hkdf, 256));
}

/** Emballe une clé de serveur pour `peerPub` (comme `wrapGroupKey`). */
export async function wrapGroupKey(identity, groupKey, peerPub) {
  const key = await importKey(await wrapKeyBytes(identity, peerPub));
  return toB64(await seal(key, groupKey));
}

/** Déballe une clé reçue de `fromPub` (comme `unwrapGroupKey`), ou `null`. */
export async function unwrapGroupKey(identity, blob, fromPub) {
  try {
    const key = await importKey(await wrapKeyBytes(identity, fromPub));
    const raw = await open(key, fromB64(blob));
    return raw && raw.length === 32 ? raw : null;
  } catch {
    return null;
  }
}

// ---------------------------------------------------------------------------
// Signatures ECDSA P-256 : chaque membre signe ses messages et ses signaux,
// pour qu'aucun autre membre (qui a pourtant la clé du serveur) ne puisse
// parler en son nom.

const ECDSA = { name: "ECDSA", namedCurve: "P-256" };
const SIGN = { name: "ECDSA", hash: "SHA-256" };

/** `{ priv: base64 PKCS#8, pub: base64 brut (65 octets) }`. */
export async function newSigner() {
  const pair = await subtle.generateKey(ECDSA, true, ["sign", "verify"]);
  return {
    priv: toB64(new Uint8Array(await subtle.exportKey("pkcs8", pair.privateKey))),
    pub: toB64(new Uint8Array(await subtle.exportKey("raw", pair.publicKey))),
  };
}

const signers = new Map();
const verifiers = new Map();

export async function sign(signer, text) {
  let key = signers.get(signer.priv);
  if (!key) {
    key = await subtle.importKey("pkcs8", fromB64(signer.priv), ECDSA, false, ["sign"]);
    signers.set(signer.priv, key);
  }
  return toB64(new Uint8Array(await subtle.sign(SIGN, key, encoder.encode(text))));
}

export async function verify(pub, signature, text) {
  try {
    let key = verifiers.get(pub);
    if (!key) {
      key = await subtle.importKey("raw", fromB64(pub), ECDSA, false, ["verify"]);
      verifiers.set(pub, key);
    }
    return await subtle.verify(SIGN, key, fromB64(signature), encoder.encode(text));
  } catch {
    return false;
  }
}

/** SHA-256 d'un texte, en hexadécimal (identifiants, preuves d'auteur). */
export async function sha256Hex(text) {
  return toHex(await sha256(encoder.encode(text)));
}
