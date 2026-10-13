// Alley : l'application. Tout est chiffré, signé et déchiffré ici, dans le
// navigateur ; le serveur ne reçoit que des jetons Brocolis.
//
// Ce que garde l'appareil (localStorage « alley.v2 ») : le pseudo, et pour
// chaque serveur son trousseau de clés (la plus récente d'abord), le secret
// de membre, l'identité X25519 (pour recevoir une clé renouvelée), la clé de
// signature ECDSA et, pour son créateur, le secret d'admin.

import * as B from "./brocolis.js";
import { Call } from "./call.js";

// ---------------------------------------------------------------------------
// Petits outils

const $ = (sel, root = document) => root.querySelector(sel);
const meta = (name) => document.querySelector(`meta[name="${name}"]`)?.content || "";

/** `h("div.card#x", { onclick }, enfants…)` : un élément du DOM, sans HTML. */
function h(spec, attrs, ...children) {
  const [, tag = "div", rest = ""] = spec.match(/^([a-z0-9-]*)(.*)$/i);
  const el = document.createElement(tag || "div");
  for (const part of rest.match(/[.#][^.#]+/g) || []) {
    if (part[0] === ".") el.classList.add(part.slice(1));
    else el.id = part.slice(1);
  }
  if (attrs && (typeof attrs !== "object" || attrs instanceof Node || Array.isArray(attrs))) {
    children.unshift(attrs);
    attrs = null;
  }
  for (const [k, v] of Object.entries(attrs || {})) {
    if (v == null || v === false) continue;
    if (k.startsWith("on")) el.addEventListener(k.slice(2), v);
    else if (k === "style" && typeof v === "object") {
      for (const [prop, value] of Object.entries(v)) el.style.setProperty(prop, value);
    } else if (k === "dataset") Object.assign(el.dataset, v);
    else if (k in el && typeof v !== "string") el[k] = v;
    else el.setAttribute(k, v === true ? "" : v);
  }
  append(el, children);
  return el;
}

function append(el, children) {
  for (const c of children.flat(Infinity)) {
    if (c == null || c === false) continue;
    el.append(c instanceof Node ? c : document.createTextNode(String(c)));
  }
}

const isTouch = matchMedia("(pointer: coarse)").matches;
const narrow = () => matchMedia("(max-width: 860px)").matches;

function hue(id) {
  let n = 0;
  for (const ch of String(id)) n = (n * 31 + ch.charCodeAt(0)) >>> 0;
  return n % 360;
}

function initials(name) {
  const words = (name || "?").trim().split(/\s+/).filter(Boolean);
  const first = [...(words[0] || "?")][0] || "?";
  const second = words[1] ? [...words[1]][0] : "";
  return (first + second).toUpperCase();
}

const timeFmt = new Intl.DateTimeFormat("fr-FR", { hour: "2-digit", minute: "2-digit" });
const dayFmt = new Intl.DateTimeFormat("fr-FR", { weekday: "long", day: "numeric", month: "long", year: "numeric" });

function dayLabel(ts) {
  const d = new Date(ts);
  if (d.toDateString() === new Date().toDateString()) return "Aujourd'hui";
  if (d.toDateString() === new Date(Date.now() - 86400000).toDateString()) return "Hier";
  return dayFmt.format(d);
}

function toast(text, kind = "") {
  const el = h("div.toast" + (kind ? "." + kind : ""), { role: "status" }, text);
  $("#toasts")?.append(el);
  setTimeout(() => el.classList.add("out"), 3600);
  setTimeout(() => el.remove(), 4000);
}

async function copy(text, done = "Copié !") {
  try {
    await navigator.clipboard.writeText(text);
    toast(done);
  } catch {
    prompt("Copiez :", text);
  }
}

const uniq = (list) => [...new Set(list)];
const randomId = () => B.toHex(B.randomBytes(8));

// ---------------------------------------------------------------------------
// Ce que garde l'appareil

const STORE = "alley.v2";

function load() {
  try {
    const d = JSON.parse(localStorage.getItem(STORE) || "{}");
    return {
      name: d.name || "",
      guilds: Array.isArray(d.guilds) ? d.guilds : [],
      notify: !!d.notify,
      last: d.last || null,
      server: d.server || "",
    };
  } catch {
    return { name: "", guilds: [], notify: false, last: null, server: "" };
  }
}

const saved = load();

function save() {
  try {
    localStorage.setItem(STORE, JSON.stringify({ ...saved, last: state.current }));
  } catch {
    toast("Impossible d'enregistrer sur cet appareil (navigation privée ?)", "error");
  }
}

// ---------------------------------------------------------------------------
// Où sont le site et l'API

const BASE = meta("alley-base");
/** L'API : celle choisie dans les réglages, sinon celle de la page. */
const apiBase = () => (saved.server || meta("alley-api")).replace(/\/+$/, "");
/** Sur GitHub Pages sans API configurée, il faut une adresse de serveur. */
const needsServer = () => !apiBase() && /\.github\.io$/.test(location.hostname);
const wsBase = () => (apiBase() || location.origin).replace(/^http/, "ws");

// ---------------------------------------------------------------------------
// L'état de l'application

const state = {
  current: saved.last, // gid du serveur ouvert
  guilds: new Map(), // gid → serveur chargé
  replyTo: null,
  editing: null,
  showMembers: !narrow(),
  drawer: false,
  call: null, // { g, channel, call: Call }
  ice: null,
  recording: null,
};

/** Un serveur : ce qui est gardé, plus ce qui est chargé et déchiffré. */
async function hydrate(entry) {
  const g = {
    entry,
    gid: entry.gid,
    me: (await B.sha256Hex(entry.member)).slice(0, 16),
    name: entry.name || "Serveur",
    admin: false,
    state: { name: entry.name || "Serveur", channels: [], keys: [] },
    revision: 0,
    epoch: 0,
    members: new Map(), // id → { id, n, x, s }
    presence: new Map(), // id → { seen, voice, muted, video }
    msgs: new Map(), // id → message déchiffré
    oldest: null,
    more: true,
    pageLoaded: false,
    loaded: false,
    unread: new Map(),
    typing: new Map(),
    socket: null,
    retry: 0,
    heartbeat: null,
  };
  await setKeys(g, entry.keys);
  return g;
}

/** Le trousseau : la clé actuelle (chiffre), et toutes (déchiffrent). */
async function setKeys(g, keys) {
  g.entry.keys = uniq(keys);
  const raw = B.fromB64Url(g.entry.keys[0]);
  g.raw = raw;
  g.key = await B.importKey(raw);
  const d = await B.derive(raw);
  g.access = d.access;
  g.fingerprint = d.fingerprint;
  g.ring = await Promise.all(g.entry.keys.map((k) => B.importKey(B.fromB64Url(k))));
}

/** Ajoute les anciennes clés que porte l'état du serveur (pour l'historique). */
async function learnKeys(g, keys) {
  const known = new Set(g.entry.keys);
  const fresh = (keys || []).filter((k) => typeof k === "string" && !known.has(k));
  if (!fresh.length) return;
  await setKeys(g, [...g.entry.keys, ...fresh]);
  save();
}

const enc = (g, text) => B.sealToken(g.key, text);

async function dec(g, token) {
  if (!token) return null;
  for (const key of g.ring) {
    const clear = await B.openToken(key, token);
    if (clear != null) return clear;
  }
  return null;
}

async function decJson(g, token) {
  const clear = await dec(g, token);
  if (clear == null) return null;
  try {
    return JSON.parse(clear);
  } catch {
    return null;
  }
}

// ---------------------------------------------------------------------------
// L'API

class ApiError extends Error {
  constructor(status, message) {
    super(message);
    this.status = status;
  }
}

async function request(method, path, { headers = {}, body } = {}) {
  if (body !== undefined) headers["Content-Type"] = "application/json";
  let res;
  try {
    res = await fetch(apiBase() + path, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
      cache: "no-store",
    });
  } catch {
    throw new ApiError(0, "Pas de connexion au serveur Alley.");
  }
  if (res.status === 204) return null;
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw new ApiError(res.status, data.message || data.detail || data.title || `Erreur ${res.status}`);
  return data;
}

function api(g, method, path, body, extra = {}) {
  const headers = { "X-Alley-Access": g.access, ...extra };
  if (g.entry.admin) headers["X-Alley-Admin"] = g.entry.admin;
  return request(method, path, { headers, body });
}

const memberHeaders = (g) => ({ "X-Alley-Member": g.entry.member });

// ---------------------------------------------------------------------------
// Identités d'un membre sur un serveur

async function newEntry(raw, gid, admin = null) {
  return {
    gid,
    keys: [B.toB64Url(raw)],
    member: B.secret(),
    admin,
    idx: (await B.hasX25519()) ? await B.newIdentity() : null,
    sig: await B.newSigner(),
    channel: null,
    name: "",
  };
}

/** Le profil chiffré : pseudo et clés publiques, lié à l'identifiant du membre. */
async function announce(g) {
  const profile = { id: g.me, n: saved.name || "Anonyme", x: g.entry.idx?.pub || null, s: g.entry.sig.pub };
  await api(g, "PUT", `/api/v1/guilds/${g.gid}/members/me`, { profile: await enc(g, JSON.stringify(profile)) }, memberHeaders(g));
  g.members.set(g.me, profile);
  g.announced = true;
}

async function readProfile(g, row) {
  const p = await decJson(g, row.profile);
  // Un profil doit parler de lui-même : sinon, c'est un collage du serveur.
  if (!p || p.id !== row.id || typeof p.n !== "string") return null;
  return { id: row.id, n: p.n.slice(0, 40), x: p.x || null, s: p.s || null };
}

// ---------------------------------------------------------------------------
// Serveurs : créer, rejoindre, charger, quitter

async function createGuild(name) {
  const raw = B.generateKey();
  const key = await B.importKey(raw);
  const d = await B.derive(raw);
  const admin = B.secret();
  const entry = await newEntry(raw, d.gid, admin);
  entry.name = name;
  const st = {
    name,
    channels: [
      { id: randomId(), name: "général", topic: "", kind: "text" },
      { id: randomId(), name: "Salon vocal", topic: "", kind: "voice" },
    ],
    keys: [],
  };
  await request("POST", "/api/v1/guilds", {
    body: { gid: d.gid, access: d.access, admin, state: await B.sealToken(key, JSON.stringify(st)) },
  });
  saved.guilds.push(entry);
  save();
  const g = await hydrate(entry);
  state.guilds.set(g.gid, g);
  return g;
}

/** Rejoint un serveur à partir d'un lien d'invitation. */
async function joinFromLink(text) {
  const link = text.trim();
  const params = new URLSearchParams(link.includes("#") ? link.slice(link.indexOf("#") + 1) : "");
  const keyText = params.get("rejoindre");
  if (!keyText) throw new Error("Ce lien ne contient pas d'invitation Alley.");
  let raw;
  try {
    raw = B.fromB64Url(keyText);
  } catch {
    throw new Error("La clé de ce lien est abîmée.");
  }
  if (raw.length !== 32) throw new Error("La clé de ce lien est abîmée.");
  const gid = /^[0-9a-f]{32}$/.test(params.get("g") || "") ? params.get("g") : (await B.derive(raw)).gid;
  const admin = /^[0-9a-f]{64}$/.test(params.get("admin") || "") ? params.get("admin") : null;
  const existing = state.guilds.get(gid);
  if (existing) {
    if (!existing.entry.keys.includes(B.toB64Url(raw))) await setKeys(existing, [B.toB64Url(raw), ...existing.entry.keys]);
    if (admin && !existing.entry.admin) {
      existing.entry.admin = admin;
      toast("Droits d'administration ajoutés.");
    }
    save();
    return existing;
  }
  const entry = await newEntry(raw, gid, admin);
  const g = await hydrate(entry);
  g.joining = true;
  await refreshGuild(g); // 404 si le lien ne mène nulle part
  g.joining = false;
  saved.guilds.push(entry);
  save();
  state.guilds.set(g.gid, g);
  return g;
}

function inviteLink(g, withAdmin = false) {
  const base = `${location.origin}${BASE}/app/#rejoindre=${g.entry.keys[0]}&g=${g.gid}`;
  return withAdmin && g.entry.admin ? `${base}&admin=${g.entry.admin}` : base;
}

async function applyState(g, data) {
  const st = await decJson(g, data.state);
  if (st) {
    g.state = {
      name: String(st.name || "Serveur"),
      channels: Array.isArray(st.channels) ? st.channels.filter((c) => c && typeof c.id === "string") : [],
      keys: Array.isArray(st.keys) ? st.keys : [],
    };
    await learnKeys(g, g.state.keys);
  } else {
    g.state = { name: "Serveur illisible", channels: [], keys: [] };
  }
  g.revision = data.revision;
  g.epoch = data.epoch;
  g.name = g.state.name;
  g.entry.name = g.name;
  if (!g.entry.channel || !g.state.channels.some((c) => c.id === g.entry.channel)) {
    g.entry.channel = (g.state.channels.find((c) => c.kind !== "voice") || g.state.channels[0])?.id ?? null;
  }
}

async function refreshGuild(g) {
  let data;
  try {
    data = await api(g, "GET", `/api/v1/guilds/${g.gid}`);
  } catch (err) {
    // La clé a peut-être été renouvelée pendant notre absence.
    if (err.status === 404 && !g.joining && !g.adopting && (await adoptShare(g))) return;
    throw err;
  }
  g.admin = data.admin;
  await applyState(g, data);
  const profiles = await Promise.all(data.members.map((m) => readProfile(g, m)));
  g.members = new Map(profiles.filter(Boolean).map((p) => [p.id, p]));
  g.loaded = true;
  save();
  await verifyPending(g);
}

/**
 * La clé a été renouvelée : récupère la nouvelle, emballée pour nous. On ne
 * l'adopte que si l'état qu'elle ouvre contient notre clé actuelle : seul
 * quelqu'un qui la connaissait (pas le serveur) a pu le fabriquer.
 */
async function adoptShare(g) {
  if (!g.entry.idx || g.adopting) return false;
  g.adopting = true;
  try {
    let share;
    try {
      share = await request("GET", `/api/v1/guilds/${g.gid}/keys`, { headers: memberHeaders(g) });
    } catch (err) {
      if (err.status === 403) g.excluded = true;
      return false;
    }
    const raw = await B.unwrapGroupKey(g.entry.idx, share.blob, share.sender);
    if (!raw) return false;
    const d = await B.derive(raw);
    let data;
    try {
      data = await request("GET", `/api/v1/guilds/${g.gid}`, { headers: { "X-Alley-Access": d.access } });
    } catch {
      return false;
    }
    const clear = await B.openToken(await B.importKey(raw), data.state);
    let st = null;
    try {
      st = clear ? JSON.parse(clear) : null;
    } catch {
      st = null;
    }
    if (!st || !Array.isArray(st.keys) || !st.keys.includes(g.entry.keys[0])) {
      toast("Une clé suspecte a été refusée : elle ne vient pas d'un membre du serveur.", "error");
      return false;
    }
    await setKeys(g, [B.toB64Url(raw), ...g.entry.keys, ...st.keys]);
    save();
    await refreshGuild(g);
    toast(`La clé de « ${g.name} » a été renouvelée. Nouvelle empreinte : ${g.fingerprint}`);
    return true;
  } finally {
    g.adopting = false;
  }
}

/** Admin : nouvelle clé, remise à chaque membre restant ; `remove` est exclu. */
async function rotateKey(g, remove = []) {
  await refreshGuild(g);
  const raw = B.generateKey();
  const key = await B.importKey(raw);
  const d = await B.derive(raw);
  const st = { ...g.state, keys: uniq([...(g.state.keys || []), ...g.entry.keys]) };
  const shares = [];
  const missing = [];
  for (const [id, p] of g.members) {
    if (remove.includes(id)) continue;
    if (!p.x || !g.entry.idx) {
      missing.push(p.n);
      continue;
    }
    shares.push({ member: id, blob: await B.wrapGroupKey(g.entry.idx, raw, p.x), sender: g.entry.idx.pub });
  }
  if (state.call?.g === g) leaveCall();
  g.rotating = true;
  try {
    await api(g, "POST", `/api/v1/guilds/${g.gid}/rotate`, {
      epoch: g.epoch + 1,
      access: d.access,
      state: await B.sealToken(key, JSON.stringify(st)),
      remove,
      shares,
    });
    disconnect(g);
    await setKeys(g, [B.toB64Url(raw), ...g.entry.keys]);
    save();
  } finally {
    g.rotating = false;
  }
  for (const id of remove) {
    g.members.delete(id);
    g.presence.delete(id);
  }
  if (missing.length) toast(`Sans X25519 (navigateur trop ancien), à réinviter par lien : ${missing.join(", ")}`, "error");
  await refreshGuild(g);
  connect(g);
  render();
}

/** Admin : modifie l'état chiffré (nom, salons), en reprenant si besoin. */
async function putState(g, change) {
  for (let attempt = 0; attempt < 2; attempt++) {
    const next = change(structuredClone(g.state));
    try {
      const data = await api(g, "PUT", `/api/v1/guilds/${g.gid}/state`, {
        state: await enc(g, JSON.stringify(next)),
        revision: g.revision,
      });
      await applyState(g, data);
      render();
      return;
    } catch (err) {
      if (err.status !== 409 || attempt) throw err;
      await refreshGuild(g);
    }
  }
}

function disconnect(g) {
  clearInterval(g.heartbeat);
  const s = g.socket;
  g.socket = null;
  s?.close();
}

function forget(g, message) {
  if (state.call?.g === g) leaveCall();
  disconnect(g);
  state.guilds.delete(g.gid);
  saved.guilds = saved.guilds.filter((e) => e !== g.entry);
  if (state.current === g.gid) state.current = state.guilds.keys().next().value ?? null;
  save();
  if (message) toast(message);
  render();
}

function lost(g) {
  forget(g, g.excluded ? `Vous avez été exclu de « ${g.name} ».` : `« ${g.name} » n'est plus accessible.`);
}

// ---------------------------------------------------------------------------
// Messages : enveloppes chiffrées et signées.
//
// Clair d'un message = {"b": enveloppe, "g": signature ECDSA, "p": remplissage}
// enveloppe = {"gid","a" (auteur),"c" (salon),"t" (heure),"k" (text, image,
// audio),"x" (contenu),"m" (type du média),"r" (réponse à),"s" (sel),"e"
// (modifié le)}. La taille est arrondie au demi-kilo-octet.

async function seal(g, env) {
  const b = JSON.stringify({ gid: g.gid, a: g.me, ...env });
  const sig = await B.sign(g.entry.sig, b);
  const bare = JSON.stringify({ b, g: sig, p: "" }).length;
  return enc(g, JSON.stringify({ b, g: sig, p: " ".repeat((512 - (bare % 512)) % 512) }));
}

async function openEnvelope(g, token) {
  const outer = await decJson(g, token);
  if (!outer || typeof outer.b !== "string") return null;
  let env;
  try {
    env = JSON.parse(outer.b);
  } catch {
    return null;
  }
  if (env.gid !== g.gid) return null;
  return { env, b: outer.b, sig: outer.g };
}

async function checkSignature(g, m) {
  const p = g.members.get(m.author);
  if (!p?.s) return null; // auteur inconnu (parti, ou pas encore chargé)
  return B.verify(p.s, m.sig, m.b);
}

async function decryptMessage(g, raw) {
  const opened = await openEnvelope(g, raw.body);
  if (!opened) return { id: raw.id, locked: true, channel: null };
  const { env } = opened;
  const kind = ["text", "image", "audio"].includes(env.k) ? env.k : "text";
  const m = {
    id: raw.id,
    channel: env.c,
    author: String(env.a),
    t: Number(env.t) || 0,
    kind,
    text: kind === "text" ? String(env.x ?? "") : null,
    media:
      kind === "image"
        ? `data:image/jpeg;base64,${env.x}`
        : kind === "audio"
          ? `data:${/^audio\/[a-z0-9.+-]+$/.test(env.m) ? env.m : "audio/mp4"};base64,${env.x}`
          : null,
    reply: Number(env.r) || 0,
    salt: env.s,
    edited: raw.edited || !!env.e,
    b: opened.b,
    sig: opened.sig,
    verified: null,
  };
  m.verified = await checkSignature(g, m);
  return m;
}

async function verifyPending(g) {
  for (const m of g.msgs.values()) if (m.verified === null && !m.locked) m.verified = await checkSignature(g, m);
}

const proofOf = (g, salt) => B.sha256Hex(`${g.entry.member}:${salt}`);

async function send(g, channel, kind, x, mime = "") {
  const salt = B.secret();
  const body = await seal(g, { c: channel, t: Date.now(), k: kind, x, m: mime, r: state.replyTo?.id || 0, s: salt, e: 0 });
  const owner = await B.sha256Hex(await proofOf(g, salt));
  const raw = await api(g, "POST", `/api/v1/messages/${g.gid}`, { body, owner });
  // Affiché tout de suite, sans attendre l'écho du temps réel.
  if (!g.msgs.has(raw.id)) {
    g.msgs.set(raw.id, await decryptMessage(g, raw));
    if (state.current === g.gid && g.entry.channel === channel) renderMessages(true);
  }
}

async function editMessage(g, m, text) {
  const body = await seal(g, { c: m.channel, t: m.t, k: "text", x: text, m: "", r: m.reply, s: m.salt, e: Date.now() });
  await api(g, "PATCH", `/api/v1/messages/${g.gid}/${m.id}`, { body, proof: await proofOf(g, m.salt) });
}

async function deleteMessage(g, m) {
  if (!confirm("Supprimer ce message pour tout le monde ?")) return;
  try {
    const extra = m.author === g.me ? { "X-Alley-Proof": await proofOf(g, m.salt) } : {};
    await api(g, "DELETE", `/api/v1/messages/${g.gid}/${m.id}`, undefined, extra);
  } catch (err) {
    toast(err.message, "error");
  }
}

/** Charge une page d'historique du serveur (tous salons confondus). */
async function loadPage(g, older = false) {
  if (g.loadingPage || (older && !g.more)) return;
  g.loadingPage = true;
  try {
    const before = older && g.oldest ? `&before=${g.oldest}` : "";
    const data = await api(g, "GET", `/api/v1/messages/${g.gid}?limit=200${before}`);
    for (const raw of data.messages) g.msgs.set(raw.id, await decryptMessage(g, raw));
    if (data.messages.length && (g.oldest == null || data.messages[0].id < g.oldest)) g.oldest = data.messages[0].id;
    if (older || !g.pageLoaded) g.more = data.more;
    g.pageLoaded = true;
  } finally {
    g.loadingPage = false;
  }
}

function channelMessages(g, cid) {
  return [...g.msgs.values()].filter((m) => m.channel === cid).sort((a, b) => a.id - b.id);
}

function readDataUrl(blob) {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(reader.result);
    reader.onerror = () => reject(new Error("Fichier illisible."));
    reader.readAsDataURL(blob);
  });
}

/** Une photo : réduite (1600 px), recompressée en JPEG, en base64. */
async function readImage(file) {
  if (!file.type.startsWith("image/")) throw new Error("Seules les images sont acceptées.");
  const source = await readDataUrl(file);
  const img = await new Promise((resolve, reject) => {
    const i = new Image();
    i.onload = () => resolve(i);
    i.onerror = () => reject(new Error("Format d'image non pris en charge."));
    i.src = source;
  });
  const scale = Math.min(1, 1600 / Math.max(img.naturalWidth, img.naturalHeight));
  const canvas = document.createElement("canvas");
  canvas.width = Math.max(1, Math.round(img.naturalWidth * scale));
  canvas.height = Math.max(1, Math.round(img.naturalHeight * scale));
  const ctx = canvas.getContext("2d");
  ctx.fillStyle = "#fff";
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
  return canvas.toDataURL("image/jpeg", 0.82).split(",")[1];
}

// ---------------------------------------------------------------------------
// Temps réel : événements du serveur, et relais chiffrés et signés entre
// membres (présence, « j'écris », signalisation des appels). Le serveur ne
// sait pas qui est connecté.

function connect(g) {
  if (g.socket || !state.guilds.has(g.gid)) return;
  const socket = new WebSocket(`${wsBase()}/api/v1/guilds/${g.gid}/live?access=${g.access}`);
  g.socket = socket;
  socket.onopen = () => {
    g.retry = 0;
    // Ce qui a pu arriver pendant la coupure.
    if (g.pageLoaded) resync(g);
    here(g, true);
    clearInterval(g.heartbeat);
    g.heartbeat = setInterval(() => here(g), 25000);
    renderStatus();
  };
  socket.onmessage = (e) => {
    let event;
    try {
      event = JSON.parse(e.data);
    } catch {
      return;
    }
    onEvent(g, event).catch(console.error);
  };
  socket.onclose = (e) => {
    if (g.socket !== socket) return;
    g.socket = null;
    clearInterval(g.heartbeat);
    if (e.code === 4404) {
      // La clé a changé pendant que nous étions déconnectés ?
      adoptShare(g).then((ok) => (ok ? connect(g) : lost(g)));
      return;
    }
    g.retry = Math.min(g.retry + 1, 6);
    renderStatus();
    setTimeout(() => connect(g), 500 * 2 ** g.retry);
  };
}

async function relay(g, payload) {
  if (g.socket?.readyState !== WebSocket.OPEN) return;
  const b = JSON.stringify({ a: g.me, t: Date.now(), ...payload });
  const data = await enc(g, JSON.stringify({ b, g: await B.sign(g.entry.sig, b) }));
  if (g.socket?.readyState === WebSocket.OPEN) g.socket.send(JSON.stringify({ type: "relay", data }));
}

function here(g, ask = false) {
  const call = state.call?.g === g ? state.call : null;
  return relay(g, {
    type: "here",
    q: ask ? 1 : 0,
    voice: call ? call.channel : null,
    muted: call ? call.call.muted : false,
    video: call ? !!(call.call.camera || call.call.screen) : false,
  });
}

async function onRelay(g, token, retried = false) {
  const outer = await decJson(g, token);
  if (!outer || typeof outer.b !== "string") return;
  let p;
  try {
    p = JSON.parse(outer.b);
  } catch {
    return;
  }
  if (p.a === g.me || Math.abs(Date.now() - p.t) > 120000) return;
  const profile = g.members.get(p.a);
  if (!profile) {
    // Peut-être un nouveau membre dont le profil n'est pas encore arrivé :
    // on recharge les membres, puis on relit ce message une fois.
    if (retried) return;
    g.refreshing ||= refreshGuild(g)
      .catch(() => {})
      .finally(() => (g.refreshing = null));
    await g.refreshing;
    if (state.current === g.gid) renderMembers();
    return onRelay(g, token, true);
  }
  // Signé par ce membre, ou ignoré : personne ne parle au nom d'un autre.
  if (!profile.s || !(await B.verify(profile.s, outer.g, outer.b))) return;
  const current = state.current === g.gid;
  switch (p.type) {
    case "here": {
      const before = g.presence.get(p.a);
      g.presence.set(p.a, { seen: Date.now(), voice: p.voice || null, muted: !!p.muted, video: !!p.video });
      if (p.q) here(g);
      if (!before || before.voice !== p.voice || before.muted !== !!p.muted || before.video !== !!p.video) {
        syncCall();
        if (current) {
          renderChannels();
          renderMembers();
          renderVoice();
        }
      }
      break;
    }
    case "bye":
      g.presence.delete(p.a);
      syncCall();
      if (current) {
        renderChannels();
        renderMembers();
        renderVoice();
      }
      break;
    case "typing":
      g.typing.set(p.a, { channel: p.c, until: Date.now() + 6000 });
      if (current) renderTyping();
      setTimeout(() => state.current === g.gid && renderTyping(), 6100);
      break;
    case "signal":
      if (p.to === g.me && state.call?.g === g && p.c === state.call.channel) {
        await state.call.call.receive(p.a, p.d);
        syncCall();
      }
      break;
  }
}

async function resync(g) {
  try {
    await refreshGuild(g);
    await loadPage(g);
    render();
  } catch (err) {
    if (err.status === 404) lost(g);
  }
}

async function onEvent(g, event) {
  const current = state.current === g.gid;
  switch (event.type) {
    case "relay":
      await onRelay(g, event.data);
      break;
    case "message": {
      const m = await decryptMessage(g, event);
      g.msgs.set(m.id, m);
      if (m.locked) break;
      g.typing.delete(m.author);
      const visible = current && g.entry.channel === m.channel && document.visibilityState === "visible";
      if (m.author !== g.me && !visible) {
        g.unread.set(m.channel, (g.unread.get(m.channel) || 0) + 1);
        notify(g, m);
      }
      if (current && g.entry.channel === m.channel) {
        renderMessages(true);
        renderTyping();
      }
      renderRail();
      if (current) renderChannels();
      updateBadge();
      break;
    }
    case "message.updated": {
      const m = await decryptMessage(g, event);
      g.msgs.set(m.id, m);
      if (current && g.entry.channel === m.channel) renderMessages(false);
      break;
    }
    case "message.deleted":
      g.msgs.delete(event.id);
      if (current) renderMessages(false);
      break;
    case "state":
      await applyState(g, event);
      save();
      render();
      break;
    case "member": {
      const p = await readProfile(g, event.member);
      if (p) {
        g.members.set(p.id, p);
        await verifyPending(g);
      }
      if (current) render();
      break;
    }
    case "member.removed":
      g.members.delete(event.id);
      g.presence.delete(event.id);
      if (current) render();
      break;
    case "rotated":
      // L'admin a changé de clé : on va chercher la nôtre (sauf si c'est lui).
      if (g.rotating || g.adopting || event.epoch <= g.epoch) break;
      disconnect(g);
      if (state.call?.g === g) leaveCall();
      if (await adoptShare(g)) {
        connect(g);
        render();
      } else {
        lost(g);
      }
      break;
    case "guild.deleted":
      forget(g, `Le serveur « ${g.name} » a été supprimé.`);
      break;
    case "resync":
      resync(g);
      break;
  }
}

function online(g, id) {
  if (id === g.me) return true;
  const p = g.presence.get(id);
  return !!p && Date.now() - p.seen < 70000;
}

let lastTyping = 0;
function sendTyping(g) {
  if (Date.now() - lastTyping < 3000) return;
  lastTyping = Date.now();
  relay(g, { type: "typing", c: g.entry.channel });
}

addEventListener("pagehide", () => {
  for (const g of state.guilds.values()) relay(g, { type: "bye" });
});

// ---------------------------------------------------------------------------
// Appels audio et vidéo

function voiceMembers(g, channel) {
  const ids = [...g.presence.entries()].filter(([id, p]) => p.voice === channel && online(g, id)).map(([id]) => id);
  if (state.call?.g === g && state.call.channel === channel) ids.unshift(g.me);
  return uniq(ids);
}

async function iceServers() {
  if (!state.ice) {
    state.ice = await request("GET", "/api/v1/ice")
      .then((d) => d.iceServers)
      .catch(() => [{ urls: ["stun:stun.cloudflare.com:3478", "stun:stun.l.google.com:19302"] }]);
  }
  return state.ice;
}

async function joinCall(g, channel) {
  if (state.call) leaveCall();
  if (!window.RTCPeerConnection || !navigator.mediaDevices?.getUserMedia) {
    toast("Ce navigateur ne permet pas les appels.", "error");
    return;
  }
  const call = new Call({
    me: g.me,
    channel,
    iceServers: await iceServers(),
    signal: (to, d) => relay(g, { type: "signal", to, c: channel, d }),
    onChange: () => {
      if (state.call?.call === call) {
        renderVoice();
        renderCallPanel();
      }
    },
  });
  try {
    await call.start();
  } catch {
    toast("Micro inaccessible : autorisez-le dans les réglages du navigateur.", "error");
    return;
  }
  state.call = { g, channel, call };
  await here(g);
  syncCall();
  render();
}

function leaveCall() {
  const c = state.call;
  if (!c) return;
  state.call = null;
  c.call.stop();
  videos.clear();
  $("#call-media")?.replaceChildren();
  here(c.g);
  render();
}

function syncCall() {
  const c = state.call;
  if (!c) return;
  c.call.sync(voiceMembers(c.g, c.channel));
  // Le son des autres : un <audio> par participant, hors du rendu.
  const box = $("#call-media");
  for (const [id, peer] of c.call.peers) {
    let el = box.querySelector(`audio[data-id="${id}"]`);
    if (!el) {
      el = h("audio", { autoplay: true, dataset: { id } });
      el.setAttribute("playsinline", "");
      box.append(el);
    }
    if (el.srcObject !== peer.stream) el.srcObject = peer.stream;
    el.play?.().catch(() => {});
  }
  for (const el of [...box.children]) if (!c.call.peers.has(el.dataset.id)) el.remove();
}

setInterval(() => {
  // Qui parle, et qui n'est plus là.
  const c = state.call;
  if (c) for (const el of document.querySelectorAll("[data-speaker]")) el.classList.toggle("speaking", c.call.speaking(el.dataset.speaker));
  for (const g of state.guilds.values()) {
    let changed = false;
    for (const [id, p] of g.presence) {
      if (Date.now() - p.seen > 70000) {
        g.presence.delete(id);
        changed = true;
      }
    }
    if (!changed) continue;
    if (c?.g === g) syncCall();
    if (state.current === g.gid) {
      renderMembers();
      renderChannels();
      renderVoice();
    }
  }
}, 300);

// ---------------------------------------------------------------------------
// Notifications et badge

function notify(g, m) {
  if (!saved.notify || document.visibilityState === "visible" || !("Notification" in window)) return;
  if (Notification.permission !== "granted") return;
  const who = g.members.get(m.author)?.n || "Quelqu'un";
  const channel = g.state.channels.find((c) => c.id === m.channel)?.name || "";
  const body = m.kind === "image" ? "📷 Photo" : m.kind === "audio" ? "🎤 Message vocal" : m.text.slice(0, 140);
  const title = `${who} · #${channel} — ${g.name}`;
  const options = { body, icon: `${BASE}/assets/icon-192.png`, badge: `${BASE}/assets/icon-192.png`, tag: `${g.gid}:${m.channel}` };
  try {
    const n = new Notification(title, options);
    n.onclick = () => {
      window.focus();
      selectGuild(g.gid, m.channel);
    };
  } catch {
    navigator.serviceWorker?.ready.then((reg) => reg.showNotification(title, options)).catch(() => {});
  }
}

function updateBadge() {
  let n = 0;
  for (const g of state.guilds.values()) for (const v of g.unread.values()) n += v;
  document.title = n ? `(${n}) Alley` : "Alley";
  if ("setAppBadge" in navigator) (n ? navigator.setAppBadge(n) : navigator.clearAppBadge()).catch(() => {});
}

document.addEventListener("visibilitychange", () => {
  if (document.visibilityState !== "visible") return;
  const g = currentGuild();
  if (g && g.unread.get(g.entry.channel)) {
    g.unread.delete(g.entry.channel);
    renderRail();
    renderChannels();
    updateBadge();
  }
});

// ---------------------------------------------------------------------------
// Mise en forme : **gras**, *italique*, ~~barré~~, `code`, ```bloc```,
// > citation, liens. Des nœuds DOM : aucun HTML injecté.

function formatInline(text) {
  const out = [];
  const re = /(`[^`\n]+`)|(\*\*[^*\n]+\*\*)|(~~[^~\n]+~~)|(\*[^*\n]+\*|_[^_\n]+_)|(https?:\/\/[^\s<>"]+[^\s<>".,;:!?)\]'])/g;
  let last = 0;
  for (const m of text.matchAll(re)) {
    if (m.index > last) out.push(text.slice(last, m.index));
    const t = m[0];
    if (m[1]) out.push(h("code", t.slice(1, -1)));
    else if (m[2]) out.push(h("strong", formatInline(t.slice(2, -2))));
    else if (m[3]) out.push(h("s", formatInline(t.slice(2, -2))));
    else if (m[4]) out.push(h("em", formatInline(t.slice(1, -1))));
    else out.push(h("a", { href: t, target: "_blank", rel: "noopener noreferrer nofollow" }, t));
    last = m.index + t.length;
  }
  if (last < text.length) out.push(text.slice(last));
  return out;
}

function formatText(text) {
  const parts = text.split(/```(?:[a-z0-9]*\n)?([\s\S]*?)```/g);
  const out = [];
  parts.forEach((part, i) => {
    if (i % 2 === 1) {
      out.push(h("pre", h("code", part.replace(/\n$/, ""))));
      return;
    }
    const lines = part.split("\n");
    lines.forEach((line, j) => {
      if (line.startsWith("> ")) out.push(h("blockquote", formatInline(line.slice(2))));
      else {
        out.push(...formatInline(line));
        if (j < lines.length - 1) out.push(h("br"));
      }
    });
  });
  return out;
}

const onlyEmoji = (t) => /^(\p{Extended_Pictographic}|\p{Emoji_Component}|\s){1,12}$/u.test(t) && !/^[\d\s#*]+$/.test(t);

// ---------------------------------------------------------------------------
// Rendu

const root = () => $("#root");
const currentGuild = () => (state.current ? state.guilds.get(state.current) : null);
const currentChannel = (g = currentGuild()) => g?.state.channels.find((c) => c.id === g.entry.channel) ?? null;
const nameOf = (g, id) => g.members.get(id)?.n || `Membre ${String(id).slice(0, 4)}`;

function avatar(id, name, size = "") {
  return h("span.avatar" + (size ? "." + size : ""), { style: { "--h": hue(id) }, "aria-hidden": "true" }, initials(name));
}

function render() {
  const r = root();
  r.className = "";
  r.replaceChildren();
  if (!saved.name || state.guilds.size === 0 || needsServer()) {
    r.append(welcome());
    return;
  }
  if (!currentGuild()) state.current = state.guilds.keys().next().value;
  r.append(
    h(
      "div.layout" + (state.drawer ? ".drawer-open" : "") + (state.showMembers ? ".members-open" : ""),
      h("nav.rail#rail", { "aria-label": "Serveurs" }),
      h("aside.sidebar#sidebar"),
      h("main.chat#chat"),
      h("aside.members#members", { "aria-label": "Membres" }),
      h("div.scrim", { onclick: () => (toggleDrawer(false), toggleMembers(false)) }),
    ),
  );
  renderRail();
  renderChannels();
  renderChat();
  renderMembers();
}

function toggleDrawer(open) {
  state.drawer = open;
  $(".layout")?.classList.toggle("drawer-open", open);
  if (narrow() && open) {
    state.showMembers = false;
    $(".layout")?.classList.remove("members-open");
  }
}

function toggleMembers(open = !state.showMembers) {
  state.showMembers = open;
  $(".layout")?.classList.toggle("members-open", open);
  if (narrow() && open) toggleDrawer(false);
}

function renderRail() {
  const rail = $("#rail");
  if (!rail) return;
  const items = [...state.guilds.values()].map((g) => {
    const unread = [...g.unread.values()].reduce((a, b) => a + b, 0);
    return h(
      "button.guild" + (g.gid === state.current ? ".active" : ""),
      {
        title: g.name,
        "aria-label": g.name + (unread ? `, ${unread} non lus` : ""),
        onclick: () => selectGuild(g.gid),
        style: { "--h": hue(g.gid) },
      },
      initials(g.name),
      unread ? h("span.badge", unread > 99 ? "99+" : unread) : null,
      state.call?.g === g ? h("span.in-call", "🔊") : null,
    );
  });
  rail.replaceChildren(
    h(
      "a.home",
      { href: `${BASE}/`, title: "Accueil d'Alley", "aria-label": "Accueil d'Alley" },
      h("img", { src: `${BASE}/assets/favicon.svg`, alt: "", width: 28, height: 28 }),
    ),
    h("hr"),
    ...items,
    h(
      "button.guild.add",
      { title: "Créer ou rejoindre un serveur", "aria-label": "Créer ou rejoindre un serveur", onclick: () => addGuildDialog() },
      "+",
    ),
  );
}

function channelButton(g, c) {
  const unread = g.unread.get(c.id) || 0;
  const voice = c.kind === "voice";
  const inside = voice ? voiceMembers(g, c.id) : [];
  return h(
    "li",
    h(
      "button.channel" + (c.id === g.entry.channel ? ".active" : "") + (unread ? ".unread" : ""),
      { onclick: () => selectChannel(g, c.id) },
      h("span.hash", voice ? "🔊" : "#"),
      h("span.name", c.name),
      unread ? h("span.badge", unread > 99 ? "99+" : unread) : null,
      g.admin
        ? h(
            "span.gear",
            { title: "Modifier le salon", role: "button", tabindex: 0, onclick: (e) => (e.stopPropagation(), channelDialog(g, c)) },
            "⚙",
          )
        : null,
    ),
    inside.length
      ? h(
          "ul.voice-members",
          inside.map((id) =>
            h(
              "li",
              { dataset: { speaker: id } },
              avatar(id, nameOf(g, id), "tiny"),
              h("span", nameOf(g, id)),
              (id === g.me ? state.call?.call.muted : g.presence.get(id)?.muted) ? h("span.muted-icon", "🔇") : null,
            ),
          ),
        )
      : null,
  );
}

function renderChannels() {
  const side = $("#sidebar");
  const g = currentGuild();
  if (!side || !g) return;
  const texts = g.state.channels.filter((c) => c.kind !== "voice");
  const voices = g.state.channels.filter((c) => c.kind === "voice");
  side.replaceChildren(
    h(
      "header.guild-head",
      h("button.guild-name", { onclick: () => guildMenu(g), "aria-haspopup": "dialog" }, h("span", g.name), h("span.chev", "▾")),
    ),
    h(
      "div.channels",
      h(
        "div.section-title",
        h("span", "Salons textuels"),
        g.admin
          ? h(
              "button.icon-btn",
              { title: "Créer un salon textuel", "aria-label": "Créer un salon textuel", onclick: () => channelDialog(g, null, "text") },
              "+",
            )
          : null,
      ),
      h(
        "ul",
        texts.map((c) => channelButton(g, c)),
      ),
      h(
        "div.section-title",
        h("span", "Salons vocaux"),
        g.admin
          ? h(
              "button.icon-btn",
              { title: "Créer un salon vocal", "aria-label": "Créer un salon vocal", onclick: () => channelDialog(g, null, "voice") },
              "+",
            )
          : null,
      ),
      h(
        "ul",
        voices.map((c) => channelButton(g, c)),
      ),
    ),
    h("div#call-panel"),
    h(
      "footer.me",
      avatar(g.me, saved.name),
      h("div.me-text", h("strong", saved.name), h("small#conn", connLabel(g))),
      h("button.icon-btn", { title: "Réglages", "aria-label": "Réglages", onclick: () => settingsDialog() }, "⚙"),
    ),
  );
  renderCallPanel();
}

function renderCallPanel() {
  const el = $("#call-panel");
  if (!el) return;
  const c = state.call;
  if (!c) return el.replaceChildren();
  const channel = c.g.state.channels.find((x) => x.id === c.channel);
  el.replaceChildren(
    h(
      "div.call-panel",
      h(
        "button.call-text",
        { onclick: () => selectGuild(c.g.gid, c.channel) },
        h("strong", "🔊 En appel"),
        h("small", `${channel?.name || ""} · ${c.g.name}`),
      ),
      h(
        "button.icon-btn" + (c.call.muted ? ".off" : ""),
        {
          title: c.call.muted ? "Réactiver le micro" : "Couper le micro",
          "aria-label": "Micro",
          onclick: () => (c.call.toggleMute(), here(c.g)),
        },
        c.call.muted ? "🔇" : "🎙️",
      ),
      h("button.icon-btn.hangup", { title: "Raccrocher", "aria-label": "Raccrocher", onclick: leaveCall }, "📞"),
    ),
  );
}

function connLabel(g) {
  if (!g) return "";
  if (g.socket?.readyState === WebSocket.OPEN) return "🔒 Connecté, chiffré";
  return g.retry ? "Reconnexion…" : "Connexion…";
}

function renderStatus() {
  const el = $("#conn");
  if (el) el.textContent = connLabel(currentGuild());
}

function renderChat() {
  const chat = $("#chat");
  const g = currentGuild();
  if (!chat || !g) return;
  const c = currentChannel(g);
  const voice = c?.kind === "voice";
  const head = h(
    "header.chat-head",
    h("button.icon-btn.menu", { "aria-label": "Serveurs et salons", onclick: () => toggleDrawer(!state.drawer) }, "☰"),
    h("span.hash", voice ? "🔊" : "#"),
    h("strong.title", c ? c.name : "aucun salon"),
    c?.topic ? h("span.topic", c.topic) : null,
    h("span.spacer"),
    h("span.lock", { title: `Chiffré de bout en bout · empreinte ${g.fingerprint}` }, "🔒"),
    h("button.icon-btn", { title: "Inviter", "aria-label": "Inviter", onclick: () => inviteDialog(g) }, "🔗"),
    h("button.icon-btn", { title: "Membres", "aria-label": "Afficher les membres", onclick: () => toggleMembers() }, "👥"),
  );
  if (voice) {
    chat.replaceChildren(head, h("div.voice#voice"));
    renderVoice();
    return;
  }
  chat.replaceChildren(
    head,
    h("div.messages#messages", { role: "log", "aria-live": "polite" }),
    h("div.typing#typing", { "aria-live": "polite" }),
    composer(g, c),
  );
  const box = $("#messages");
  box.addEventListener("scroll", () => {
    if (box.scrollTop < 120 && c) loadOlder(g, c.id);
  });
  renderMessages(true);
  renderTyping();
}

const videos = new Map(); // id → <video> réutilisé entre deux rendus

function videoFor(id, stream, mirror) {
  let v = videos.get(id);
  if (!v) {
    v = h("video", { autoplay: true });
    v.muted = true; // le son passe par les <audio> de #call-media
    v.setAttribute("playsinline", "");
    videos.set(id, v);
  }
  if (v.srcObject !== stream) v.srcObject = stream;
  v.classList.toggle("mirror", !!mirror);
  v.play?.().catch(() => {});
  return v;
}

function renderVoice() {
  const box = $("#voice");
  const g = currentGuild();
  const c = currentChannel(g);
  if (!box || !g || c?.kind !== "voice") return;
  const call = state.call?.g === g && state.call.channel === c.id ? state.call.call : null;
  const ids = voiceMembers(g, c.id);
  const tiles = ids.map((id) => {
    let stream = null;
    let mirror = false;
    if (call && id === g.me) {
      const track = call.videoTrack();
      if (track) {
        stream = call.selfView ||= new MediaStream();
        for (const t of stream.getTracks()) if (t !== track) stream.removeTrack(t);
        if (!stream.getTracks().includes(track)) stream.addTrack(track);
        mirror = !!call.camera && !call.screen;
      }
    } else if (call) {
      const peer = call.peers.get(id);
      const live = peer?.stream.getVideoTracks().some((t) => t.readyState === "live" && !t.muted);
      if (live && g.presence.get(id)?.video) stream = peer.stream;
    }
    const link = id === g.me ? null : call?.peers.get(id)?.pc.connectionState;
    const muted = id === g.me ? call?.muted : g.presence.get(id)?.muted;
    return h(
      "div.tile" + (stream ? ".has-video" : ""),
      { dataset: { speaker: id } },
      stream ? videoFor(id === g.me ? "me" : id, stream, mirror) : avatar(id, nameOf(g, id), "big"),
      h(
        "span.tile-name",
        nameOf(g, id),
        muted ? " 🔇" : "",
        link && link !== "connected" ? ` · ${link === "failed" ? "échec de connexion" : "connexion…"}` : "",
      ),
    );
  });
  const controls = call
    ? h(
        "div.call-controls",
        h(
          "button" + (call.muted ? ".off" : ""),
          { onclick: () => (call.toggleMute(), here(g)), title: "Micro" },
          call.muted ? "🔇" : "🎙️",
          h("span", call.muted ? "Micro coupé" : "Micro"),
        ),
        h(
          "button" + (call.camera ? ".on" : ""),
          {
            onclick: () =>
              call
                .toggleCamera()
                .then(() => here(g))
                .catch(() => toast("Caméra inaccessible.", "error")),
            title: "Caméra",
          },
          "📷",
          h("span", call.camera ? "Couper la caméra" : "Caméra"),
        ),
        Call.canShareScreen()
          ? h(
              "button" + (call.screen ? ".on" : ""),
              {
                onclick: () =>
                  call
                    .toggleScreen()
                    .then(() => here(g))
                    .catch(() => {}),
                title: "Partager l'écran",
              },
              "🖥️",
              h("span", call.screen ? "Arrêter le partage" : "Écran"),
            )
          : null,
        h("button.hangup", { onclick: leaveCall, title: "Raccrocher" }, "📞", h("span", "Raccrocher")),
      )
    : h(
        "div.call-join",
        h("p", ids.length ? `${ids.length} personne(s) dans ce salon.` : "Personne pour l'instant."),
        h("button.primary", { onclick: () => joinCall(g, c.id) }, "Rejoindre l'appel"),
        h(
          "p.muted.small",
          "Audio et vidéo de pair à pair, chiffrés de bout en bout (DTLS-SRTP). Le serveur ne relaie que la mise en relation, chiffrée et signée elle aussi.",
        ),
      );
  box.replaceChildren(h("div.tiles" + (ids.length > 4 ? ".many" : ""), tiles), controls);
}

async function loadOlder(g, cid) {
  if (!g.more || g.loadingPage) return;
  const box = $("#messages");
  const before = box.scrollHeight - box.scrollTop;
  const count = channelMessages(g, cid).length;
  // Le salon est dans le chiffré : on remonte l'historique du serveur
  // jusqu'à trouver de quoi afficher.
  for (let i = 0; i < 5 && g.more && channelMessages(g, cid).length === count; i++) await loadPage(g, true);
  renderMessages(false);
  if (box.isConnected) box.scrollTop = box.scrollHeight - before;
}

function renderMessages(stick) {
  const box = $("#messages");
  const g = currentGuild();
  const c = currentChannel(g);
  if (!box || !g || !c || c.kind === "voice") return;
  const atBottom = box.scrollHeight - box.scrollTop - box.clientHeight < 80;
  const list = channelMessages(g, c.id);
  const nodes = [];
  if (g.pageLoaded && !g.more) {
    nodes.push(
      h(
        "div.channel-start",
        h("div.big-hash", "#"),
        h("h2", `Bienvenue dans #${c.name} !`),
        h(
          "p",
          "C'est le début du salon. Tout ce qui s'écrit ici est chiffré de bout en bout, jusqu'à l'auteur et l'heure : le serveur ne voit que des jetons illisibles.",
        ),
      ),
    );
  } else if (!g.pageLoaded) {
    nodes.push(h("div.loading", "Déchiffrement…"));
  } else {
    nodes.push(h("button.load-more", { onclick: () => loadOlder(g, c.id) }, "Messages plus anciens"));
  }
  let prev = null;
  for (const m of list) {
    if (!prev || dayLabel(prev.t) !== dayLabel(m.t)) nodes.push(h("div.day", h("span", dayLabel(m.t))));
    const grouped =
      prev &&
      prev.author === m.author &&
      m.t - prev.t < 5 * 60000 &&
      !m.reply &&
      dayLabel(prev.t) === dayLabel(m.t) &&
      m.verified !== false;
    nodes.push(messageNode(g, m, grouped));
    prev = m;
  }
  box.replaceChildren(...nodes);
  if (stick || atBottom) box.scrollTop = box.scrollHeight;
}

function messageNode(g, m, grouped) {
  const name = nameOf(g, m.author);
  const mine = m.author === g.me;
  let content;
  if (m.kind === "image")
    content = h(
      "button.image",
      { onclick: () => lightbox(m.media), "aria-label": "Agrandir la photo" },
      h("img", { src: m.media, alt: "Photo", loading: "lazy" }),
    );
  else if (m.kind === "audio") content = h("audio.voice-note", { controls: true, preload: "metadata", src: m.media });
  else content = h("div.text" + (onlyEmoji(m.text) ? ".jumbo" : ""), formatText(m.text));
  let reply = null;
  if (m.reply) {
    const target = g.msgs.get(m.reply);
    const what =
      target && !target.locked
        ? target.kind === "image"
          ? "📷 Photo"
          : target.kind === "audio"
            ? "🎤 Vocal"
            : target.text.slice(0, 90)
        : "message plus ancien";
    reply = h(
      "div.reply-ref",
      { onclick: () => scrollToMessage(m.reply) },
      h("span.curve"),
      target && !target.locked ? h("strong", "@" + nameOf(g, target.author)) : null,
      h("span", what),
    );
  }
  const forged = m.verified === false;
  const actions = h(
    "div.actions",
    h("button", { title: "Répondre", "aria-label": "Répondre", onclick: () => startReply(m) }, "↩"),
    mine && m.kind === "text" ? h("button", { title: "Modifier", "aria-label": "Modifier", onclick: () => startEdit(m) }, "✎") : null,
    m.kind === "text" ? h("button", { title: "Copier", "aria-label": "Copier le texte", onclick: () => copy(m.text) }, "⧉") : null,
    mine || g.admin
      ? h("button.danger", { title: "Supprimer", "aria-label": "Supprimer", onclick: () => deleteMessage(g, m) }, "🗑")
      : null,
  );
  const node = h(
    "article.message" + (grouped ? ".grouped" : "") + (mine ? ".mine" : "") + (forged ? ".forged" : ""),
    { id: `m${m.id}` },
    reply,
    grouped ? h("time.hover-time", timeFmt.format(m.t)) : avatar(m.author, name),
    h(
      "div.body",
      grouped
        ? null
        : h(
            "div.meta",
            h("strong.author", { style: { "--h": hue(m.author) } }, name),
            forged
              ? h(
                  "span.warn",
                  { title: "La signature ne correspond pas : ce message n'a pas été écrit par ce membre." },
                  "⚠ signature invalide",
                )
              : null,
            m.verified === null ? h("span.unverified", { title: "Auteur inconnu : signature non vérifiable." }, "?") : null,
            h(
              "time",
              { title: new Date(m.t).toLocaleString("fr-FR") },
              `${dayLabel(m.t) === "Aujourd'hui" ? "" : dayLabel(m.t) + " "}${timeFmt.format(m.t)}`,
            ),
          ),
      content,
      m.edited ? h("small.edited", "(modifié)") : null,
    ),
    actions,
  );
  if (isTouch) {
    let timer;
    node.addEventListener("touchstart", () => (timer = setTimeout(() => node.classList.add("show-actions"), 450)), { passive: true });
    node.addEventListener("touchend", () => clearTimeout(timer));
    node.addEventListener("touchmove", () => clearTimeout(timer), { passive: true });
  }
  return node;
}

function scrollToMessage(id) {
  const el = document.getElementById(`m${id}`);
  if (!el) return;
  el.scrollIntoView({ behavior: "smooth", block: "center" });
  el.classList.add("flash");
  setTimeout(() => el.classList.remove("flash"), 1400);
}

function renderTyping() {
  const el = $("#typing");
  const g = currentGuild();
  if (!el || !g) return;
  const now = Date.now();
  const names = [...g.typing.entries()].filter(([, t]) => t.until > now && t.channel === g.entry.channel).map(([id]) => nameOf(g, id));
  el.replaceChildren(
    names.length ? h("span.dots", h("i"), h("i"), h("i")) : "",
    names.length === 0
      ? ""
      : names.length === 1
        ? ` ${names[0]} écrit…`
        : names.length < 4
          ? ` ${names.join(", ")} écrivent…`
          : " Plusieurs personnes écrivent…",
  );
}

function renderMembers() {
  const el = $("#members");
  const g = currentGuild();
  if (!el || !g) return;
  const all = [...g.members.values()].sort((a, b) => a.n.localeCompare(b.n, "fr"));
  const on = all.filter((p) => online(g, p.id));
  const off = all.filter((p) => !online(g, p.id));
  const row = (p, isOn) =>
    h(
      "li.member" + (isOn ? ".on" : ""),
      h("span.avatar-wrap", avatar(p.id, p.n), h("span.dot")),
      h("span.name", p.n, p.id === g.me ? h("small", " (vous)") : null, g.presence.get(p.id)?.voice ? h("small", " 🔊") : null),
      g.admin && p.id !== g.me
        ? h("button.icon-btn.kick", { title: `Exclure ${p.n}`, "aria-label": `Exclure ${p.n}`, onclick: () => excludeDialog(g, p) }, "⨯")
        : null,
    );
  el.replaceChildren();
  append(el, [
    h(
      "header.members-head",
      h("strong", "Membres"),
      h("button.icon-btn", { "aria-label": "Fermer", onclick: () => toggleMembers(false) }, "✕"),
    ),
    h("div.section-title", `En ligne — ${on.length}`),
    h(
      "ul",
      on.map((p) => row(p, true)),
    ),
    off.length ? h("div.section-title", `Hors ligne — ${off.length}`) : null,
    h(
      "ul",
      off.map((p) => row(p, false)),
    ),
    h(
      "div.fp",
      h("small", "Empreinte du serveur"),
      h("code", g.fingerprint),
      h("small.muted", "Identique chez tous les membres = personne ne s'est intercalé."),
    ),
  ]);
}

function composer(g, c) {
  const input = h("textarea#input", {
    rows: 1,
    placeholder: c ? `Message chiffré dans #${c.name}` : "",
    enterkeyhint: isTouch ? "enter" : "send",
    autocomplete: "off",
    "aria-label": "Votre message",
    maxlength: 20000,
  });
  const file = h("input", { type: "file", accept: "image/*", hidden: true });
  const autosize = () => {
    input.style.height = "auto";
    input.style.height = Math.min(input.scrollHeight, 200) + "px";
  };
  const submit = async () => {
    const text = input.value.trim();
    if (!text || !c) return;
    input.value = "";
    autosize();
    try {
      if (state.editing) {
        const m = state.editing;
        cancelBars();
        await editMessage(g, m, text);
      } else {
        await send(g, c.id, "text", text);
        cancelBars();
      }
    } catch (err) {
      input.value = text;
      autosize();
      toast(err.message, "error");
    }
  };
  input.addEventListener("input", () => {
    autosize();
    if (input.value) sendTyping(g);
  });
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && !e.shiftKey && !isTouch && !e.isComposing) {
      e.preventDefault();
      submit();
    } else if (e.key === "Escape") {
      cancelBars();
    } else if (e.key === "ArrowUp" && !input.value) {
      const mine = channelMessages(g, c.id)
        .filter((m) => m.author === g.me && m.kind === "text")
        .at(-1);
      if (mine) {
        e.preventDefault();
        startEdit(mine);
      }
    }
  });
  file.addEventListener("change", async () => {
    const f = file.files?.[0];
    file.value = "";
    if (!f || !c) return;
    toast("Chiffrement de la photo…");
    try {
      await send(g, c.id, "image", await readImage(f));
      cancelBars();
    } catch (err) {
      toast(err.message, "error");
    }
  });
  const canRecord = !!(window.MediaRecorder && navigator.mediaDevices?.getUserMedia);
  return h(
    "div.composer",
    h("div#bar"),
    h(
      "div.input-row",
      h(
        "button.icon-btn.attach",
        { title: "Envoyer une photo (chiffrée)", "aria-label": "Envoyer une photo", onclick: () => file.click() },
        "＋",
      ),
      file,
      input,
      canRecord
        ? h(
            "button.icon-btn.mic#mic",
            { title: "Message vocal (chiffré)", "aria-label": "Enregistrer un message vocal", onclick: () => toggleRecording(g, c) },
            "🎤",
          )
        : null,
      h("button.send", { title: "Envoyer", "aria-label": "Envoyer", onclick: submit }, "➤"),
    ),
  );
}

/** Un message vocal : enregistré, puis chiffré comme une photo (2 min max). */
async function toggleRecording(g, c) {
  if (state.recording) {
    state.recording.stop();
    return;
  }
  let stream;
  try {
    stream = await navigator.mediaDevices.getUserMedia({ audio: true });
  } catch {
    toast("Micro inaccessible.", "error");
    return;
  }
  const mime =
    ["audio/mp4", "audio/webm;codecs=opus", "audio/webm", "audio/ogg;codecs=opus"].find((t) => MediaRecorder.isTypeSupported?.(t)) || "";
  const rec = new MediaRecorder(stream, mime ? { mimeType: mime, audioBitsPerSecond: 32000 } : undefined);
  const chunks = [];
  const started = Date.now();
  const mic = () => $("#mic");
  rec.ondataavailable = (e) => e.data.size && chunks.push(e.data);
  const tick = setInterval(() => {
    const s = Math.floor((Date.now() - started) / 1000);
    if (mic()) mic().textContent = `⏺ ${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
    if (s >= 120) rec.stop();
  }, 250);
  rec.onstop = async () => {
    clearInterval(tick);
    for (const t of stream.getTracks()) t.stop();
    state.recording = null;
    if (mic()) {
      mic().textContent = "🎤";
      mic().classList.remove("recording");
    }
    if (Date.now() - started < 600) return;
    const type = (rec.mimeType || mime || "audio/webm").split(";")[0];
    try {
      const data = (await readDataUrl(new Blob(chunks, { type }))).split(",")[1];
      await send(g, c.id, "audio", data, type);
    } catch (err) {
      toast(err.message, "error");
    }
  };
  state.recording = rec;
  mic()?.classList.add("recording");
  rec.start(1000);
  toast("Enregistrement… touchez ⏺ pour envoyer.");
}

function renderBar() {
  const bar = $("#bar");
  if (!bar) return;
  const g = currentGuild();
  if (state.editing) {
    bar.replaceChildren(
      h("div.bar", h("span", "✎ Modification du message"), h("button.icon-btn", { "aria-label": "Annuler", onclick: cancelBars }, "✕")),
    );
  } else if (state.replyTo) {
    bar.replaceChildren(
      h(
        "div.bar",
        h("span", "Réponse à ", h("strong", "@" + nameOf(g, state.replyTo.author))),
        h("button.icon-btn", { "aria-label": "Annuler", onclick: cancelBars }, "✕"),
      ),
    );
  } else bar.replaceChildren();
}

function startReply(m) {
  state.editing = null;
  state.replyTo = m;
  renderBar();
  $("#input")?.focus();
}

function startEdit(m) {
  state.replyTo = null;
  state.editing = m;
  const input = $("#input");
  input.value = m.text;
  input.dispatchEvent(new Event("input"));
  renderBar();
  input.focus();
}

function cancelBars() {
  const wasEditing = !!state.editing;
  state.replyTo = null;
  state.editing = null;
  if (wasEditing && $("#input")) $("#input").value = "";
  renderBar();
}

// ---------------------------------------------------------------------------
// Navigation

async function selectGuild(gid, channel) {
  const g = state.guilds.get(gid);
  if (!g) return;
  state.current = gid;
  state.replyTo = state.editing = null;
  if (channel != null) g.entry.channel = channel;
  save();
  if (!g.loaded || !g.announced) {
    try {
      if (!g.loaded) await refreshGuild(g);
      if (!g.announced) await announce(g);
    } catch (err) {
      if (err.status === 404 || err.status === 403) return lost(g);
      toast(err.message, "error");
    }
  }
  connect(g);
  render();
  if (!g.pageLoaded) {
    try {
      await loadPage(g);
    } catch (err) {
      toast(err.message, "error");
    }
  }
  openChannel(g, g.entry.channel);
}

function selectChannel(g, cid) {
  g.entry.channel = cid;
  state.replyTo = state.editing = null;
  save();
  if (narrow()) toggleDrawer(false);
  renderChannels();
  renderChat();
  openChannel(g, cid);
}

function openChannel(g, cid) {
  g.unread.delete(cid);
  updateBadge();
  renderRail();
  renderChannels();
  if (g.entry.channel === cid && state.current === g.gid) renderMessages(true);
  if (!isTouch) $("#input")?.focus();
}

// ---------------------------------------------------------------------------
// Fenêtres

function dialog(title, body, actions = []) {
  const d = h("dialog.modal", h("form.modal-body", { method: "dialog" }, h("h2", title), body, h("div.modal-actions", actions)));
  d.addEventListener("close", () => d.remove());
  d.addEventListener("click", (e) => e.target === d && d.close());
  document.body.append(d);
  d.showModal();
  return d;
}

function field(label, input, hint) {
  return h("label.field", h("span", label), input, hint ? h("small.muted", hint) : null);
}

function welcome() {
  const name = h("input", { value: saved.name, placeholder: "Votre pseudo", maxlength: 32, autocomplete: "nickname", required: true });
  const server = h("input", {
    value: saved.server,
    placeholder: "https://mon-alley.herokuapp.com",
    type: "url",
    inputmode: "url",
    autocomplete: "url",
  });
  const hasInvite = !!pendingInvite;
  const go = async (e) => {
    e.preventDefault();
    const n = name.value.trim();
    if (!n) return name.focus();
    if (needsServer()) {
      const s = server.value.trim().replace(/\/+$/, "");
      if (!/^https:\/\/[^\s/]+$/.test(s)) return toast("Indiquez l'adresse https:// du serveur Alley.", "error");
      saved.server = s;
    }
    saved.name = n;
    save();
    if (pendingInvite) return handleInvite();
    if (state.guilds.size) return selectGuild(state.current || state.guilds.keys().next().value);
    render();
    addGuildDialog(true);
  };
  return h(
    "div.welcome",
    h(
      "form.welcome-card",
      { onsubmit: go },
      h("img", { src: `${BASE}/assets/icon-192.png`, alt: "", width: 72, height: 72 }),
      h("h1", hasInvite ? "Vous êtes invité sur Alley" : "Bienvenue sur Alley"),
      h(
        "p.muted",
        "Des serveurs, des salons et des appels comme sur Discord, chiffrés de bout en bout. Pas de compte : vos clés restent sur cet appareil.",
      ),
      field("Comment voulez-vous apparaître ?", name, "Votre pseudo est chiffré, comme vos messages."),
      needsServer()
        ? field(
            "Adresse du serveur Alley",
            server,
            "Cette page est hébergée sur GitHub Pages : indiquez le serveur qui relaie les messages chiffrés.",
          )
        : null,
      h("button.primary", { type: "submit" }, hasInvite ? "Rejoindre le serveur" : "Continuer"),
      h("p.small.muted", "🔒 AES-256-GCM · X25519 · ECDSA · protocole Brocolis · serveur Paquets"),
    ),
  );
}

function addGuildDialog(first = false) {
  const name = h("input", {
    placeholder: "Ex. : Les copains du lycée",
    maxlength: 60,
    required: true,
    value: first ? `Le serveur de ${saved.name}` : "",
  });
  const link = h("input", { placeholder: "https://…/app/#rejoindre=…", autocomplete: "off" });
  const d = dialog(
    "Créer ou rejoindre un serveur",
    h(
      "div.stack",
      h(
        "section.choice",
        h("h3", "🏰 Créer un serveur"),
        field("Nom du serveur", name),
        h(
          "button.primary",
          {
            type: "button",
            onclick: async () => {
              const n = name.value.trim();
              if (!n) return name.focus();
              try {
                const g = await createGuild(n);
                d.close();
                await selectGuild(g.gid);
                inviteDialog(g, true);
              } catch (err) {
                toast(err.message, "error");
              }
            },
          },
          "Créer",
        ),
      ),
      h(
        "section.choice",
        h("h3", "🔗 Rejoindre avec un lien"),
        field("Lien d'invitation", link, "La clé est dans le lien : demandez-le à un membre du serveur."),
        h(
          "button",
          {
            type: "button",
            onclick: async () => {
              try {
                const g = await joinFromLink(link.value);
                d.close();
                await selectGuild(g.gid);
                toast(`Bienvenue sur « ${g.name} » !`);
              } catch (err) {
                toast(err.status === 404 ? "Serveur introuvable : le lien est-il complet et à jour ?" : err.message, "error");
              }
            },
          },
          "Rejoindre",
        ),
      ),
    ),
    [h("button.ghost", { value: "cancel" }, "Fermer")],
  );
  setTimeout(() => name.focus(), 50);
}

function inviteDialog(g, justCreated = false) {
  const link = inviteLink(g);
  const actions = [h("button.ghost", { value: "cancel" }, "Fermer")];
  if (navigator.share) {
    actions.unshift(
      h(
        "button",
        {
          type: "button",
          onclick: () =>
            navigator
              .share({
                title: `Rejoins « ${g.name} » sur Alley`,
                text: `Rejoins « ${g.name} » sur Alley (chiffré de bout en bout) :`,
                url: link,
              })
              .catch(() => {}),
        },
        "Partager…",
      ),
    );
  }
  actions.unshift(h("button.primary", { type: "button", onclick: () => copy(link, "Lien copié !") }, "Copier le lien"));
  dialog(
    justCreated ? "Votre serveur est prêt 🎉" : `Inviter sur « ${g.name} »`,
    h(
      "div.stack",
      h(
        "p",
        "Toute personne qui a ce lien peut rejoindre le serveur. Envoyez-le par un canal de confiance — et si un lien fuit, l'admin renouvelle la clé : l'ancien lien ne mène plus nulle part.",
      ),
      h("input.link", { value: link, readonly: true, onfocus: (e) => e.target.select() }),
      h("p.muted.small", "La clé est après le « # » : cette partie du lien n'est jamais envoyée au serveur."),
      h("div.fp-inline", h("span", "Empreinte : "), h("code", g.fingerprint)),
      g.entry.admin
        ? h(
            "details",
            h("summary", "Lien d'administration (à garder pour vous)"),
            h("p.muted.small", "Il donne aussi les droits d'administrateur : ouvrez-le sur vos autres appareils, ne le partagez pas."),
            h(
              "button",
              { type: "button", onclick: () => copy(inviteLink(g, true), "Lien d'administration copié") },
              "Copier le lien d'administration",
            ),
          )
        : null,
    ),
    actions,
  );
}

function guildMenu(g) {
  const d = dialog(
    g.name,
    h(
      "div.menu",
      h("button", { type: "button", onclick: () => (d.close(), inviteDialog(g)) }, "🔗 Inviter des gens"),
      g.admin ? h("button", { type: "button", onclick: () => (d.close(), channelDialog(g, null, "text")) }, "➕ Créer un salon") : null,
      g.admin ? h("button", { type: "button", onclick: () => (d.close(), guildSettings(g)) }, "⚙ Paramètres du serveur") : null,
      h("button", { type: "button", onclick: () => (d.close(), securityDialog(g)) }, "🔒 Chiffrement et empreinte"),
      h(
        "button.danger",
        {
          type: "button",
          onclick: async () => {
            if (!confirm(`Quitter « ${g.name} » ? La clé sera effacée de cet appareil : il vous faudra un nouveau lien pour revenir.`))
              return;
            d.close();
            await api(g, "DELETE", `/api/v1/guilds/${g.gid}/members/me`, undefined, memberHeaders(g)).catch(() => {});
            forget(g, `Vous avez quitté « ${g.name} ».`);
          },
        },
        "🚪 Quitter le serveur",
      ),
    ),
    [h("button.ghost", { value: "cancel" }, "Fermer")],
  );
}

function securityDialog(g) {
  const d = dialog(
    "Chiffrement de bout en bout",
    h(
      "div.stack",
      h(
        "p",
        "Messages, photos, vocaux, noms et pseudos — et l'auteur, le salon et l'heure de chaque message — sont chiffrés sur votre appareil (AES-256-GCM) et signés (ECDSA) avant de partir.",
      ),
      h("div.fp-big", h("small", "Empreinte du serveur"), h("code", g.fingerprint)),
      h("p.muted.small", "Comparez-la de vive voix avec les autres membres : identique pour tout le monde, personne n'a remplacé la clé."),
      g.admin
        ? h(
            "section.choice",
            h("h3", "♻️ Renouveler la clé"),
            h(
              "p.muted.small",
              "Si un lien a fui : une nouvelle clé est remise à chaque membre actuel, chiffrée pour lui seul (X25519). Les anciens liens ne mènent plus nulle part.",
            ),
            h(
              "button",
              {
                type: "button",
                onclick: async () => {
                  try {
                    await rotateKey(g);
                    d.close();
                    toast(`Clé renouvelée. Nouvelle empreinte : ${g.fingerprint}`);
                  } catch (err) {
                    toast(err.message, "error");
                  }
                },
              },
              "Renouveler maintenant",
            ),
          )
        : null,
      h(
        "details",
        h("summary", "Clé au format Brocolis 🥦"),
        h(
          "p.muted.small",
          "Les jetons d'Alley sont ceux de la messagerie Brocolis (AES-256-GCM, [[BROCOLIS:v1:…]]). Cette clé (base64) donne accès à tout le serveur : gardez-la secrète.",
        ),
        h("button", { type: "button", onclick: () => copy(B.toB64(g.raw), "Clé copiée") }, "Copier la clé"),
      ),
    ),
    [h("button.ghost", { value: "cancel" }, "Fermer")],
  );
}

function excludeDialog(g, p) {
  const d = dialog(
    `Exclure ${p.n} ?`,
    h(
      "div.stack",
      h(
        "p",
        `${p.n} ne pourra plus rien lire ni écrire : la clé du serveur est renouvelée et remise, chiffrée, à chacun des autres membres. Ses anciens liens ne marchent plus et son identifiant est banni.`,
      ),
      h("p.muted.small", "Ce qui est déjà sur son appareil y reste : aucune messagerie ne peut l'effacer à distance."),
    ),
    [
      h("button.ghost", { value: "cancel" }, "Annuler"),
      h(
        "button.primary.danger-fill",
        {
          type: "button",
          onclick: async () => {
            try {
              await rotateKey(g, [p.id]);
              d.close();
              toast(`${p.n} a été exclu. Nouvelle empreinte : ${g.fingerprint}`);
            } catch (err) {
              toast(err.message, "error");
            }
          },
        },
        "Exclure",
      ),
    ],
  );
}

function guildSettings(g) {
  const name = h("input", { value: g.name, maxlength: 60, required: true });
  const d = dialog(
    "Paramètres du serveur",
    h(
      "div.stack",
      field("Nom du serveur", name, "Chiffré : seuls les membres le voient."),
      h(
        "details.danger-zone",
        h("summary", "Zone dangereuse"),
        h("p.muted.small", "Supprimer le serveur efface définitivement tous ses salons et messages, pour tout le monde."),
        h(
          "button.danger",
          {
            type: "button",
            onclick: async () => {
              if (prompt(`Pour confirmer, tapez le nom du serveur : ${g.name}`) !== g.name) return;
              try {
                await api(g, "DELETE", `/api/v1/guilds/${g.gid}`);
                d.close();
                forget(g, "Serveur supprimé.");
              } catch (err) {
                toast(err.message, "error");
              }
            },
          },
          "Supprimer le serveur",
        ),
      ),
    ),
    [
      h("button.ghost", { value: "cancel" }, "Annuler"),
      h(
        "button.primary",
        {
          type: "button",
          onclick: async () => {
            const n = name.value.trim();
            if (!n) return;
            try {
              await putState(g, (s) => ({ ...s, name: n }));
              d.close();
            } catch (err) {
              toast(err.message, "error");
            }
          },
        },
        "Enregistrer",
      ),
    ],
  );
}

function channelDialog(g, c = null, kind = "text") {
  kind = c?.kind || kind;
  const name = h("input", {
    value: c?.name || "",
    placeholder: kind === "voice" ? "Salon vocal" : "nouveau-salon",
    maxlength: 60,
    required: true,
  });
  const topic = h("input", { value: c?.topic || "", placeholder: "De quoi parle-t-on ici ?", maxlength: 200 });
  const clean = (v) => (kind === "voice" ? v.trim() : v.trim().toLowerCase().replace(/\s+/g, "-"));
  const d = dialog(
    c ? `Modifier ${kind === "voice" ? "🔊 " : "#"}${c.name}` : kind === "voice" ? "Créer un salon vocal" : "Créer un salon textuel",
    h(
      "div.stack",
      field("Nom du salon", name),
      kind === "text" ? field("Sujet", topic, "Facultatif. Chiffré, comme le nom.") : null,
      c && g.state.channels.length > 1
        ? h(
            "button.danger",
            {
              type: "button",
              onclick: async () => {
                if (!confirm(`Supprimer « ${c.name} » ?`)) return;
                try {
                  await putState(g, (s) => ({ ...s, channels: s.channels.filter((x) => x.id !== c.id) }));
                  d.close();
                } catch (err) {
                  toast(err.message, "error");
                }
              },
            },
            "Supprimer le salon",
          )
        : null,
    ),
    [
      h("button.ghost", { value: "cancel" }, "Annuler"),
      h(
        "button.primary",
        {
          type: "button",
          onclick: async () => {
            const n = clean(name.value);
            if (!n) return name.focus();
            const t = topic.value.trim();
            try {
              if (c) {
                await putState(g, (s) => ({ ...s, channels: s.channels.map((x) => (x.id === c.id ? { ...x, name: n, topic: t } : x)) }));
                d.close();
              } else {
                if (g.state.channels.length >= 100) throw new Error("Ce serveur a déjà 100 salons.");
                const id = randomId();
                await putState(g, (s) => ({ ...s, channels: [...s.channels, { id, name: n, topic: t, kind }] }));
                d.close();
                selectChannel(g, id);
              }
            } catch (err) {
              toast(err.message, "error");
            }
          },
        },
        c ? "Enregistrer" : "Créer",
      ),
    ],
  );
  setTimeout(() => name.focus(), 50);
}

function settingsDialog() {
  const name = h("input", { value: saved.name, maxlength: 32, required: true });
  const server = h("input", { value: saved.server, placeholder: meta("alley-api") || location.origin, type: "url" });
  const canNotify = "Notification" in window;
  const notif = h("input", {
    type: "checkbox",
    checked: saved.notify && canNotify && Notification.permission === "granted",
    disabled: !canNotify,
  });
  const d = dialog(
    "Réglages",
    h(
      "div.stack",
      field("Pseudo", name, "Mis à jour (chiffré) sur tous vos serveurs."),
      h("label.check", notif, h("span", "Notifications des nouveaux messages")),
      canNotify
        ? null
        : h(
            "p.muted.small",
            "Sur iPhone et iPad, installez d'abord Alley sur l'écran d'accueil (Partager → Sur l'écran d'accueil) pour recevoir des notifications.",
          ),
      h("details", h("summary", "Avancé"), field("Serveur Alley", server, "Laissez vide pour utiliser celui de cette page.")),
      h("p.muted.small", `Alley garde ${state.guilds.size} serveur(s) et leurs clés sur cet appareil uniquement.`),
    ),
    [
      h("button.ghost", { value: "cancel" }, "Annuler"),
      h(
        "button.primary",
        {
          type: "button",
          onclick: async () => {
            const n = name.value.trim();
            if (!n) return name.focus();
            const s = server.value.trim().replace(/\/+$/, "");
            if (s && !/^https:\/\/[^\s/]+$/.test(s) && !/^http:\/\/localhost(:\d+)?$/.test(s))
              return toast("Le serveur doit être en https://", "error");
            if (notif.checked && canNotify && Notification.permission !== "granted") {
              const p = await Notification.requestPermission();
              if (p !== "granted") toast("Notifications refusées par le navigateur.", "error");
              saved.notify = p === "granted";
            } else saved.notify = notif.checked;
            const changed = n !== saved.name;
            const moved = s !== saved.server;
            saved.name = n;
            saved.server = s;
            save();
            d.close();
            if (moved) return location.reload();
            if (changed) {
              for (const g of state.guilds.values()) if (g.loaded) announce(g).catch(() => {});
              render();
            }
          },
        },
        "Enregistrer",
      ),
    ],
  );
}

function lightbox(src) {
  const d = h(
    "dialog.lightbox",
    h("img", { src, alt: "Photo" }),
    h("button.icon-btn.close", { "aria-label": "Fermer", onclick: () => d.close() }, "✕"),
  );
  d.addEventListener("click", () => d.close());
  d.addEventListener("close", () => d.remove());
  document.body.append(d);
  d.showModal();
}

// ---------------------------------------------------------------------------
// Démarrage

let pendingInvite = null;

async function handleInvite() {
  const link = pendingInvite;
  pendingInvite = null;
  try {
    const g = await joinFromLink(link);
    await selectGuild(g.gid);
    toast(`Bienvenue sur « ${g.name} » !`);
  } catch (err) {
    toast(err.status === 404 ? "Serveur introuvable : le lien a peut-être été renouvelé." : err.message, "error");
    render();
  }
}

/** iOS : garde la zone de saisie au-dessus du clavier. */
function trackViewport() {
  const vv = window.visualViewport;
  const set = () => document.documentElement.style.setProperty("--app-height", `${vv ? vv.height : innerHeight}px`);
  set();
  vv?.addEventListener("resize", set);
  addEventListener("resize", set);
}

/** Le chiffrement du navigateur n'existe qu'en HTTPS : on y renvoie. */
function insecure() {
  root().replaceChildren(
    h(
      "div.welcome",
      h(
        "div.welcome-card",
        h("h1", "Connexion non sécurisée"),
        h("p", "Alley chiffre tout dans votre navigateur, ce qui n'est possible qu'en HTTPS."),
        h("a.button-link", { href: location.href.replace(/^http:/, "https:") }, "Ouvrir la version sécurisée"),
      ),
    ),
  );
}

async function start() {
  document.body.append(h("div#toasts", { "aria-live": "polite" }), h("div#call-media", { hidden: true }));
  trackViewport();
  if (!window.isSecureContext || !globalThis.crypto?.subtle) return insecure();
  if ("serviceWorker" in navigator) navigator.serviceWorker.register(`${BASE}/sw.js`, { scope: `${BASE}/` }).catch(() => {});

  // Le lien d'invitation : la clé est dans le fragment, qu'on efface aussitôt
  // de la barre d'adresse et de l'historique.
  if (location.hash.includes("rejoindre=")) {
    pendingInvite = location.href;
    history.replaceState(null, "", `${BASE}/app/`);
  }
  const wantsNew = location.hash === "#nouveau";
  if (wantsNew) history.replaceState(null, "", `${BASE}/app/`);

  for (const entry of saved.guilds) {
    try {
      const g = await hydrate(entry);
      state.guilds.set(g.gid, g);
    } catch {
      // Une entrée abîmée : on l'ignore.
    }
  }
  if (!saved.name || needsServer()) return render();
  if (pendingInvite) return handleInvite();
  if (state.guilds.size === 0) {
    render();
    if (wantsNew) addGuildDialog(true);
    return;
  }
  await selectGuild(state.guilds.has(state.current) ? state.current : state.guilds.keys().next().value);
  if (wantsNew) addGuildDialog();
  // Les autres serveurs : en ligne aussi, pour les badges de non-lus.
  for (const g of state.guilds.values()) {
    if (g.gid === state.current) continue;
    refreshGuild(g)
      .then(() => announce(g))
      .then(() => loadPage(g))
      .then(() => connect(g))
      .then(renderRail)
      .catch((err) => (err.status === 404 || err.status === 403) && lost(g));
  }
}

addEventListener("online", () => {
  for (const g of state.guilds.values()) if (!g.socket && g.loaded) connect(g);
});

matchMedia("(max-width: 860px)").addEventListener("change", () => render());

start().catch((err) => {
  console.error(err);
  root().replaceChildren(h("div.welcome", h("div.welcome-card", h("h1", "Oups"), h("p", "Alley n'a pas pu démarrer : " + err.message))));
});
