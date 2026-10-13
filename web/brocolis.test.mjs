// Le chiffrement du navigateur, recoupé avec les valeurs de référence de
// Brocolis (`site/src/crypto.rs`, tests `fixed_vectors_match_the_app`).
//
//     node --test web/

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  derive,
  fingerprint,
  fromB64Url,
  generateKey,
  importKey,
  openToken,
  sealToken,
  toB64Url,
  TOKEN_END,
  TOKEN_V1,
} from "./brocolis.js";

test("un jeton produit par Brocolis se déchiffre ici", async () => {
  const key = await importKey(new Uint8Array(32).fill(9));
  const token = "[[BROCOLIS:v1:uAI9wrumtTlX/P8fBdLphS15X//kXm4SZje5yfVDld2L3Ac=]]";
  assert.equal(await openToken(key, token), "bonjour");
});

test("l'empreinte est celle de l'app Brocolis", async () => {
  assert.equal(await fingerprint(new Uint8Array(32).fill(7)), "4BB0-6F8E-4E3A-7715");
});

test("aller-retour, et rien pour une autre clé", async () => {
  const key = await importKey(generateKey());
  const token = await sealToken(key, "salut 🥦");
  assert.ok(token.startsWith(TOKEN_V1) && token.endsWith(TOKEN_END));
  assert.equal(await openToken(key, token), "salut 🥦");
  assert.equal(await openToken(await importKey(generateKey()), token), null);
  assert.equal(await openToken(key, "du clair"), null);
  assert.equal(await openToken(key, "[[BROCOLIS:v1:%%%]]"), null);
});

test("identifiants dérivés de la clé : hexadécimaux, stables", async () => {
  const raw = new Uint8Array(32).fill(7);
  const a = await derive(raw);
  const b = await derive(raw);
  assert.deepEqual(a, b);
  assert.match(a.gid, /^[0-9a-f]{32}$/);
  assert.match(a.access, /^[0-9a-f]{64}$/);
  // L'identifiant reprend le début de l'identifiant de conversation Brocolis
  // (`0cPmmicIaMca…` en base64url pour cette clé).
  assert.equal(toB64Url(Buffer.from(a.gid.slice(0, 18), "hex")), "0cPmmicIaMca");
  assert.notEqual(a.gid, a.access.slice(0, 32));
});

test("la clé survit au lien d'invitation", () => {
  const raw = generateKey();
  assert.deepEqual(fromB64Url(toB64Url(raw)), raw);
});
