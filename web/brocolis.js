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
    const plain = await subtle.decrypt(
      { name: "AES-GCM", iv: combined.subarray(0, 12) },
      key,
      combined.subarray(12),
    );
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
