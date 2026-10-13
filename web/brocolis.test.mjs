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

test("X25519 : mêmes clés publiques et même clé d'emballage que Brocolis", async () => {
  const { identityFromRaw, wrapKeyBytes, unwrapGroupKey, wrapGroupKey, newIdentity, toB64 } = await import("./brocolis.js");
  const alice = await identityFromRaw(new Uint8Array(32).fill(1));
  const bob = await identityFromRaw(new Uint8Array(32).fill(2));
  assert.equal(alice.pub, "pOCSkrZRwni5dyxWn1+puxPZBrRqtoyd+dwrRAn4ogk=");
  const k = await wrapKeyBytes(alice, bob.pub);
  assert.equal(toB64(k), "rv+EmjbG8ZYNWUlwkDV57/YVEL9zkAScnM6iKz0T+UM=");
  assert.equal(toB64(await wrapKeyBytes(bob, alice.pub)), toB64(k));
  // Une clé emballée par Brocolis se déballe ici.
  const blob = "81p2c6nvKV6OSrdquUov4sR9A5t47pJNiL7Jp8fiFhB7ar7PZf8em8dWQKxQqu/NP/BohUyZB0nEsJ5M";
  assert.deepEqual(await unwrapGroupKey(bob, blob, alice.pub), new Uint8Array(32).fill(9));
  // Et un tiers n'y arrive pas.
  const eve = await newIdentity();
  assert.equal(await unwrapGroupKey(eve, blob, alice.pub), null);
  const again = await wrapGroupKey(alice, new Uint8Array(32).fill(5), bob.pub);
  assert.deepEqual(await unwrapGroupKey(bob, again, alice.pub), new Uint8Array(32).fill(5));
});

test("signatures : valides pour leur auteur seulement", async () => {
  const { newSigner, sign, verify } = await import("./brocolis.js");
  const ada = await newSigner();
  const bob = await newSigner();
  const sig = await sign(ada, "bonjour");
  assert.ok(await verify(ada.pub, sig, "bonjour"));
  assert.equal(await verify(ada.pub, sig, "bonjouR"), false);
  assert.equal(await verify(bob.pub, sig, "bonjour"), false);
});
