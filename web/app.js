// Alley : l'application. Tout est chiffré et déchiffré ici, dans le
// navigateur ; le serveur ne reçoit que des jetons Brocolis.
//
// Ce que garde l'appareil (localStorage « alley.v1 ») : le pseudo, et pour
// chaque serveur sa clé, son jeton d'accès, le secret de membre et, pour son
// créateur, le secret d'admin. Rien de tout cela n'est envoyé en clair.

import * as B from "./brocolis.js";

// ---------------------------------------------------------------------------
// Petits outils

const $ = (sel, root = document) => root.querySelector(sel);

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
    }
    else if (k === "dataset") Object.assign(el.dataset, v);
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
  for (const ch of id) n = (n * 31 + ch.charCodeAt(0)) >>> 0;
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
  const today = new Date();
  const yesterday = new Date(Date.now() - 86400000);
  if (d.toDateString() === today.toDateString()) return "Aujourd'hui";
  if (d.toDateString() === yesterday.toDateString()) return "Hier";
  return dayFmt.format(d);
}

function toast(text, kind = "") {
  const el = h("div.toast" + (kind ? "." + kind : ""), { role: "status" }, text);
  $("#toasts").append(el);
  setTimeout(() => el.classList.add("out"), 3200);
  setTimeout(() => el.remove(), 3600);
}

async function copy(text, done = "Copié !") {
  try {
    await navigator.clipboard.writeText(text);
    toast(done);
  } catch {
    prompt("Copiez :", text);
  }
}

// ---------------------------------------------------------------------------
// Ce que garde l'appareil

const STORE = "alley.v1";

function load() {
  try {
    const data = JSON.parse(localStorage.getItem(STORE) || "{}");
    return { name: data.name || "", guilds: Array.isArray(data.guilds) ? data.guilds : [], notify: !!data.notify, last: data.last || null };
  } catch {
    return { name: "", guilds: [], notify: false, last: null };
  }
}

const saved = load();

function save() {
  try {
    localStorage.setItem(
      STORE,
      JSON.stringify({
        name: saved.name,
        notify: saved.notify,
        last: state.current,
        guilds: saved.guilds,
      }),
    );
  } catch {
    toast("Impossible d'enregistrer sur cet appareil (navigation privée ?)", "error");
  }
}

// ---------------------------------------------------------------------------
// L'état de l'application

const state = {
  current: saved.last, // gid du serveur ouvert
  guilds: new Map(), // gid → serveur chargé (voir `openGuild`)
  replyTo: null,
  editing: null,
  showMembers: !narrow(),
  drawer: false,
};

/** Un serveur : ce qui est gardé, plus ce qui est chargé et déchiffré. */
async function hydrate(entry) {
  const raw = B.fromB64Url(entry.key);
  const key = await B.importKey(raw);
  const derived = await B.derive(raw);
  return {
    entry,
    raw,
    key,
    gid: derived.gid,
    access: derived.access,
    fingerprint: derived.fingerprint,
    name: entry.name || "Serveur",
    admin: false,
    channels: [],
    members: new Map(),
    online: new Set(),
    messages: new Map(), // id du salon → { list, more, loaded }
    unread: new Map(),
    typing: new Map(), // membre → { channel, until }
    socket: null,
    retry: 0,
    loaded: false,
    me: null,
  };
}

const enc = (g, text) => B.sealToken(g.key, text);
const dec = async (g, token, fallback = null) => (token ? (await B.openToken(g.key, token)) ?? fallback : fallback);

// ---------------------------------------------------------------------------
// L'API

class ApiError extends Error {
  constructor(status, message) {
    super(message);
    this.status = status;
  }
}

async function api(g, method, path, body) {
  const headers = { "X-Alley-Access": g.access, "X-Alley-Member": g.entry.member };
  if (g.entry.admin) headers["X-Alley-Admin"] = g.entry.admin;
  if (body !== undefined) headers["Content-Type"] = "application/json";
  let res;
  try {
    res = await fetch(path, { method, headers, body: body === undefined ? undefined : JSON.stringify(body), cache: "no-store" });
  } catch {
    throw new ApiError(0, "Pas de connexion au serveur.");
  }
  if (res.status === 204) return null;
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw new ApiError(res.status, data.message || data.detail || data.title || `Erreur ${res.status}`);
  return data;
}

// ---------------------------------------------------------------------------
// Serveurs : créer, rejoindre, charger, quitter

async function createGuild(name, channelName) {
  const raw = B.generateKey();
  const key = await B.importKey(raw);
  const d = await B.derive(raw);
  const admin = B.secret();
  const entry = { key: B.toB64Url(raw), member: B.secret(), admin, name };
  const res = await fetch("/api/v1/guilds", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      gid: d.gid,
      access: d.access,
      admin,
      name: await B.sealToken(key, name),
      channel: await B.sealToken(key, channelName),
    }),
  });
  if (!res.ok) {
    const data = await res.json().catch(() => ({}));
    throw new ApiError(res.status, data.message || "La création a échoué.");
  }
  saved.guilds.push(entry);
  save();
  const g = await hydrate(entry);
  state.guilds.set(g.gid, g);
  return g;
}

/** Rejoint un serveur à partir d'un lien d'invitation (ou de la clé seule). */
async function joinFromLink(text) {
  const link = text.trim();
  const params = new URLSearchParams(link.includes("#") ? link.slice(link.indexOf("#") + 1) : "");
  const keyText = params.get("rejoindre") || (!link.includes("#") && !link.includes("/") ? link : null);
  if (!keyText) throw new Error("Ce lien ne contient pas d'invitation Alley.");
  let raw;
  try {
    raw = keyText.includes("+") || keyText.includes("/") || keyText.endsWith("=") ? B.fromB64(keyText) : B.fromB64Url(keyText);
  } catch {
    throw new Error("La clé de ce lien est abîmée.");
  }
  if (raw.length !== 32) throw new Error("La clé de ce lien est abîmée.");
  const d = await B.derive(raw);
  const admin = params.get("admin");
  const existing = state.guilds.get(d.gid);
  if (existing) {
    if (admin && /^[0-9a-f]{64}$/.test(admin) && !existing.entry.admin) {
      existing.entry.admin = admin;
      save();
      toast("Droits d'administration ajoutés.");
    }
    return existing;
  }
  const entry = { key: B.toB64Url(raw), member: B.secret(), admin: admin && /^[0-9a-f]{64}$/.test(admin) ? admin : null };
  const g = await hydrate(entry);
  // Vérifie que le serveur existe avant de l'ajouter.
  await api(g, "GET", `/api/v1/guilds/${g.gid}`);
  saved.guilds.push(entry);
  save();
  state.guilds.set(g.gid, g);
  return g;
}

function inviteLink(g, withAdmin = false) {
  const base = `${location.origin}/app#rejoindre=${g.entry.key}`;
  return withAdmin && g.entry.admin ? `${base}&admin=${g.entry.admin}` : base;
}

async function refreshGuild(g) {
  const data = await api(g, "GET", `/api/v1/guilds/${g.gid}`);
  g.admin = data.admin;
  g.name = (await dec(g, data.name)) ?? "Serveur illisible";
  g.entry.name = g.name;
  g.channels = await Promise.all(
    data.channels.map(async (c) => ({ id: c.id, position: c.position, name: (await dec(g, c.name)) ?? "illisible", topic: (await dec(g, c.topic)) ?? "" })),
  );
  g.members = new Map(await Promise.all(data.members.map(async (m) => [m.id, (await dec(g, m.name)) ?? "Membre"])));
  g.online = new Set(data.online);
  g.loaded = true;
  if (!g.entry.channel || !g.channels.some((c) => c.id === g.entry.channel)) g.entry.channel = g.channels[0]?.id ?? null;
  save();
}

/** S'assure que le serveur connaît notre pseudo (chiffré). */
async function announce(g) {
  const me = await api(g, "PUT", `/api/v1/guilds/${g.gid}/members/me`, { name: await enc(g, saved.name || "Anonyme") });
  g.me = me.id;
  g.members.set(me.id, saved.name || "Anonyme");
}

async function forget(g, message) {
  g.socket?.close();
  g.socket = null;
  state.guilds.delete(g.gid);
  saved.guilds = saved.guilds.filter((e) => e !== g.entry);
  if (state.current === g.gid) state.current = state.guilds.keys().next().value ?? null;
  save();
  if (message) toast(message);
  render();
}

// ---------------------------------------------------------------------------
// Messages

async function decryptMessage(g, m) {
  const clear = await dec(g, m.body);
  const msg = { ...m, text: null, image: null, locked: clear == null };
  if (clear != null) {
    if (clear.startsWith(B.IMAGE_PREFIX)) msg.image = "data:image/jpeg;base64," + clear.slice(B.IMAGE_PREFIX.length);
    else msg.text = clear;
  }
  return msg;
}

function channelState(g, cid) {
  if (!g.messages.has(cid)) g.messages.set(cid, { list: [], more: true, loaded: false, loading: false });
  return g.messages.get(cid);
}

async function loadMessages(g, cid, older = false) {
  const cs = channelState(g, cid);
  if (cs.loading || (older && !cs.more)) return;
  cs.loading = true;
  try {
    const before = older && cs.list.length ? `&before=${cs.list[0].id}` : "";
    const data = await api(g, "GET", `/api/v1/messages/${g.gid}/${cid}?limit=50${before}`);
    const page = await Promise.all(data.messages.map((m) => decryptMessage(g, m)));
    if (older) {
      cs.list = [...page, ...cs.list];
    } else {
      const known = new Map(cs.list.map((m) => [m.id, m]));
      for (const m of page) known.set(m.id, m);
      cs.list = [...known.values()].sort((a, b) => a.id - b.id);
    }
    cs.more = data.more;
    cs.loaded = true;
  } finally {
    cs.loading = false;
  }
}

async function sendText(g, cid, text) {
  const body = await enc(g, text);
  await api(g, "POST", `/api/v1/messages/${g.gid}/${cid}`, { body, reply_to: state.replyTo?.id || 0 });
}

/** Une photo : réduite (1600 px), recompressée en JPEG, puis chiffrée. Lue
 * en adresse `data:` (la politique de sécurité de la page refuse `blob:`). */
async function sendImage(g, cid, file) {
  if (!file.type.startsWith("image/")) throw new Error("Seules les images sont acceptées.");
  const source = await new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(reader.result);
    reader.onerror = () => reject(new Error("Image illisible."));
    reader.readAsDataURL(file);
  });
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
  // Le JPEG n'a pas de transparence : un fond blanc plutôt que noir.
  ctx.fillStyle = "#fff";
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
  const jpeg = canvas.toDataURL("image/jpeg", 0.82).split(",")[1];
  const body = await enc(g, B.IMAGE_PREFIX + jpeg);
  await api(g, "POST", `/api/v1/messages/${g.gid}/${cid}`, { body, reply_to: state.replyTo?.id || 0 });
}

// ---------------------------------------------------------------------------
// Temps réel

function connect(g) {
  if (g.socket) return;
  const scheme = location.protocol === "https:" ? "wss" : "ws";
  const url = `${scheme}://${location.host}/api/v1/guilds/${g.gid}/live?access=${g.access}&member=${g.entry.member}`;
  const socket = new WebSocket(url);
  g.socket = socket;
  socket.onopen = () => {
    const wasDown = g.retry > 0;
    g.retry = 0;
    if (wasDown) resync(g);
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
    if (g.socket === socket) g.socket = null;
    if (e.code === 4404 || !state.guilds.has(g.gid)) return;
    g.retry = Math.min(g.retry + 1, 6);
    renderStatus();
    setTimeout(() => state.guilds.has(g.gid) && connect(g), 500 * 2 ** g.retry);
  };
}

async function resync(g) {
  try {
    await refreshGuild(g);
    const cid = g.entry.channel;
    if (cid != null) await loadMessages(g, cid);
    render();
  } catch (err) {
    if (err.status === 404) forget(g, "Ce serveur n'existe plus.");
  }
}

async function onEvent(g, event) {
  const current = state.current === g.gid;
  switch (event.type) {
    case "message": {
      const m = await decryptMessage(g, event.message);
      const cs = channelState(g, m.channel);
      if (!cs.list.some((x) => x.id === m.id)) cs.list.push(m);
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
      const m = await decryptMessage(g, event.message);
      const cs = channelState(g, m.channel);
      const i = cs.list.findIndex((x) => x.id === m.id);
      if (i >= 0) cs.list[i] = m;
      if (current && g.entry.channel === m.channel) renderMessages(false);
      break;
    }
    case "message.deleted": {
      const cs = channelState(g, event.channel);
      cs.list = cs.list.filter((x) => x.id !== event.id);
      if (current && g.entry.channel === event.channel) renderMessages(false);
      break;
    }
    case "channel": {
      const c = event.channel;
      const ch = { id: c.id, position: c.position, name: (await dec(g, c.name)) ?? "illisible", topic: (await dec(g, c.topic)) ?? "" };
      const i = g.channels.findIndex((x) => x.id === c.id);
      if (i >= 0) g.channels[i] = ch;
      else g.channels.push(ch);
      g.channels.sort((a, b) => a.position - b.position);
      if (current) render();
      break;
    }
    case "channel.deleted": {
      g.channels = g.channels.filter((c) => c.id !== event.id);
      g.messages.delete(event.id);
      g.unread.delete(event.id);
      if (g.entry.channel === event.id) g.entry.channel = g.channels[0]?.id ?? null;
      if (current) render();
      break;
    }
    case "member":
      g.members.set(event.member.id, (await dec(g, event.member.name)) ?? "Membre");
      if (current) render();
      break;
    case "presence":
      g.online = new Set(event.online);
      if (current) renderMembers();
      break;
    case "typing":
      if (event.member === g.me) return;
      g.typing.set(event.member, { channel: event.channel, until: Date.now() + 6000 });
      if (current) renderTyping();
      setTimeout(() => current && renderTyping(), 6100);
      break;
    case "guild.updated":
      g.name = (await dec(g, event.name)) ?? g.name;
      g.entry.name = g.name;
      save();
      render();
      break;
    case "guild.deleted":
      forget(g, `Le serveur « ${g.name} » a été supprimé.`);
      break;
    case "resync":
      resync(g);
      break;
  }
}

let lastTyping = 0;
function sendTyping(g) {
  if (Date.now() - lastTyping < 3000 || g.socket?.readyState !== WebSocket.OPEN) return;
  lastTyping = Date.now();
  g.socket.send(JSON.stringify({ type: "typing", channel: g.entry.channel }));
}

// ---------------------------------------------------------------------------
// Notifications et badge

function notify(g, m) {
  if (!saved.notify || document.visibilityState === "visible" || !("Notification" in window)) return;
  if (Notification.permission !== "granted") return;
  const who = g.members.get(m.author) || "Quelqu'un";
  const channel = g.channels.find((c) => c.id === m.channel)?.name || "";
  const body = m.image ? "📷 Photo" : m.locked ? "🔒 Message chiffré" : m.text.slice(0, 140);
  const title = `${who} · #${channel} — ${g.name}`;
  const options = { body, icon: "/assets/icon-192.png", badge: "/assets/icon-192.png", tag: `${g.gid}:${m.channel}` };
  try {
    const n = new Notification(title, options);
    n.onclick = () => {
      window.focus();
      selectGuild(g.gid, m.channel);
    };
  } catch {
    // Android et les apps installées n'affichent les notifications que par
    // le service worker.
    navigator.serviceWorker?.ready.then((reg) => reg.showNotification(title, options)).catch(() => {});
  }
}

function totalUnread() {
  let n = 0;
  for (const g of state.guilds.values()) for (const v of g.unread.values()) n += v;
  return n;
}

function updateBadge() {
  const n = totalUnread();
  document.title = n ? `(${n}) Alley` : "Alley";
  if ("setAppBadge" in navigator) (n ? navigator.setAppBadge(n) : navigator.clearAppBadge()).catch(() => {});
}

document.addEventListener("visibilitychange", () => {
  if (document.visibilityState !== "visible") return;
  const g = currentGuild();
  if (g && g.entry.channel != null && g.unread.get(g.entry.channel)) {
    g.unread.delete(g.entry.channel);
    renderRail();
    renderChannels();
    updateBadge();
  }
});

// ---------------------------------------------------------------------------
// Mise en forme des messages : **gras**, *italique*, ~~barré~~, `code`,
// ```bloc```, liens. Construite avec des nœuds DOM : aucun HTML injecté.

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

function currentGuild() {
  return state.current ? state.guilds.get(state.current) : null;
}

function currentChannel(g = currentGuild()) {
  return g?.channels.find((c) => c.id === g.entry.channel) ?? null;
}

function avatar(id, name, size = "") {
  return h("span.avatar" + (size ? "." + size : ""), { style: { "--h": hue(id) }, "aria-hidden": "true" }, initials(name));
}

function render() {
  const r = root();
  r.className = "";
  r.replaceChildren();
  if (!saved.name || state.guilds.size === 0) {
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
      h("div.scrim", { onclick: () => toggleDrawer(false) }),
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
      { title: g.name, "aria-label": g.name + (unread ? `, ${unread} non lus` : ""), onclick: () => selectGuild(g.gid), style: { "--h": hue(g.gid) } },
      initials(g.name),
      unread ? h("span.badge", unread > 99 ? "99+" : unread) : null,
    );
  });
  rail.replaceChildren(
    h("a.home", { href: "/", title: "Accueil d'Alley", "aria-label": "Accueil d'Alley" }, h("img", { src: "/assets/favicon.svg", alt: "", width: 28, height: 28 })),
    h("hr"),
    ...items,
    h("button.guild.add", { title: "Créer ou rejoindre un serveur", "aria-label": "Créer ou rejoindre un serveur", onclick: () => addGuildDialog() }, "+"),
  );
}

function renderChannels() {
  const side = $("#sidebar");
  const g = currentGuild();
  if (!side || !g) return;
  const list = g.channels.map((c) => {
    const unread = g.unread.get(c.id) || 0;
    return h(
      "li",
      h(
        "button.channel" + (c.id === g.entry.channel ? ".active" : "") + (unread ? ".unread" : ""),
        { onclick: () => selectChannel(g, c.id) },
        h("span.hash", "#"),
        h("span.name", c.name),
        unread ? h("span.badge", unread > 99 ? "99+" : unread) : null,
        g.admin ? h("span.gear", { title: "Modifier le salon", role: "button", tabindex: 0, onclick: (e) => (e.stopPropagation(), channelDialog(g, c)) }, "⚙") : null,
      ),
    );
  });
  side.replaceChildren(
    h(
      "header.guild-head",
      h("button.guild-name", { onclick: () => guildMenu(g), "aria-haspopup": "dialog" }, h("span", g.name), h("span.chev", "▾")),
    ),
    h(
      "div.channels",
      h("div.section-title", h("span", "Salons textuels"), g.admin ? h("button.icon-btn", { title: "Créer un salon", "aria-label": "Créer un salon", onclick: () => channelDialog(g) }, "+") : null),
      h("ul", list),
    ),
    h(
      "footer.me",
      avatar(g.me || g.entry.member, saved.name),
      h("div.me-text", h("strong", saved.name), h("small#conn", connLabel(g))),
      h("button.icon-btn", { title: "Réglages", "aria-label": "Réglages", onclick: () => settingsDialog() }, "⚙"),
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
  const head = h(
    "header.chat-head",
    h("button.icon-btn.menu", { "aria-label": "Serveurs et salons", onclick: () => toggleDrawer(!state.drawer) }, "☰"),
    h("span.hash", "#"),
    h("strong.title", c ? c.name : "aucun salon"),
    c?.topic ? h("span.topic", c.topic) : null,
    h("span.spacer"),
    h("span.lock", { title: `Chiffré de bout en bout · empreinte ${g.fingerprint}` }, "🔒"),
    h("button.icon-btn", { title: "Inviter", "aria-label": "Inviter", onclick: () => inviteDialog(g) }, "🔗"),
    h("button.icon-btn", { title: "Membres", "aria-label": "Afficher les membres", onclick: () => toggleMembers() }, "👥"),
  );
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

async function loadOlder(g, cid) {
  const cs = channelState(g, cid);
  if (!cs.more || cs.loading || !cs.loaded) return;
  const box = $("#messages");
  const before = box.scrollHeight - box.scrollTop;
  await loadMessages(g, cid, true);
  renderMessages(false);
  box.scrollTop = box.scrollHeight - before;
}

function renderMessages(stick) {
  const box = $("#messages");
  const g = currentGuild();
  const c = currentChannel(g);
  if (!box || !g || !c) return;
  const atBottom = box.scrollHeight - box.scrollTop - box.clientHeight < 80;
  const cs = channelState(g, c.id);
  const nodes = [];
  if (cs.loaded && !cs.more) {
    nodes.push(
      h(
        "div.channel-start",
        h("div.big-hash", "#"),
        h("h2", `Bienvenue dans #${c.name} !`),
        h("p", "C'est le début du salon. Tout ce qui s'écrit ici est chiffré de bout en bout : le serveur ne voit que des jetons illisibles."),
      ),
    );
  } else if (!cs.loaded) {
    nodes.push(h("div.loading", "Déchiffrement…"));
  }
  let prev = null;
  for (const m of cs.list) {
    if (!prev || dayLabel(prev.created_at) !== dayLabel(m.created_at)) nodes.push(h("div.day", h("span", dayLabel(m.created_at))));
    const grouped = prev && prev.author === m.author && m.created_at - prev.created_at < 5 * 60000 && !m.reply_to && dayLabel(prev.created_at) === dayLabel(m.created_at);
    nodes.push(messageNode(g, c, m, grouped));
    prev = m;
  }
  box.replaceChildren(...nodes);
  if (stick || atBottom) box.scrollTop = box.scrollHeight;
}

function messageNode(g, c, m, grouped) {
  const name = g.members.get(m.author) || `Membre ${m.author.slice(0, 4)}`;
  const mine = m.author === g.me;
  let content;
  if (m.locked) content = h("p.locked", "🔒 Message illisible : chiffré avec une autre clé.");
  else if (m.image) content = h("button.image", { onclick: () => lightbox(m.image), "aria-label": "Agrandir la photo" }, h("img", { src: m.image, alt: "Photo", loading: "lazy" }));
  else content = h("div.text" + (onlyEmoji(m.text) ? ".jumbo" : ""), formatText(m.text));
  let reply = null;
  if (m.reply_to) {
    const target = channelState(g, c.id).list.find((x) => x.id === m.reply_to);
    const who = target ? g.members.get(target.author) || "Membre" : null;
    const what = target ? (target.image ? "📷 Photo" : target.locked ? "🔒" : target.text.slice(0, 90)) : "message plus ancien";
    reply = h("div.reply-ref", { onclick: () => scrollToMessage(m.reply_to) }, h("span.curve"), who ? h("strong", "@" + who) : null, h("span", what));
  }
  const actions = h(
    "div.actions",
    h("button", { title: "Répondre", "aria-label": "Répondre", onclick: () => startReply(g, m) }, "↩"),
    mine && !m.image && !m.locked ? h("button", { title: "Modifier", "aria-label": "Modifier", onclick: () => startEdit(g, m) }, "✎") : null,
    !m.image && !m.locked ? h("button", { title: "Copier", "aria-label": "Copier le texte", onclick: () => copy(m.text) }, "⧉") : null,
    mine || g.admin ? h("button.danger", { title: "Supprimer", "aria-label": "Supprimer", onclick: () => deleteMessage(g, c, m) }, "🗑") : null,
  );
  const node = h(
    "article.message" + (grouped ? ".grouped" : "") + (mine ? ".mine" : ""),
    { id: `m${m.id}`, dataset: { id: m.id } },
    reply,
    grouped ? h("time.hover-time", { datetime: new Date(m.created_at).toISOString() }, timeFmt.format(m.created_at)) : avatar(m.author, name),
    h(
      "div.body",
      grouped ? null : h("div.meta", h("strong.author", { style: { "--h": hue(m.author) } }, name), h("time", { datetime: new Date(m.created_at).toISOString(), title: new Date(m.created_at).toLocaleString("fr-FR") }, `${dayLabel(m.created_at) === "Aujourd'hui" ? "" : dayLabel(m.created_at) + " "}${timeFmt.format(m.created_at)}`)),
      content,
      m.edited_at ? h("small.edited", "(modifié)") : null,
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
  const names = [...g.typing.entries()].filter(([, t]) => t.until > now && t.channel === g.entry.channel).map(([id]) => g.members.get(id) || "Quelqu'un");
  el.replaceChildren(
    names.length ? h("span.dots", h("i"), h("i"), h("i")) : "",
    names.length === 0 ? "" : names.length === 1 ? ` ${names[0]} écrit…` : names.length < 4 ? ` ${names.join(", ")} écrivent…` : " Plusieurs personnes écrivent…",
  );
}

function renderMembers() {
  const el = $("#members");
  const g = currentGuild();
  if (!el || !g) return;
  const all = [...g.members.entries()].sort((a, b) => a[1].localeCompare(b[1], "fr"));
  const online = all.filter(([id]) => g.online.has(id));
  const offline = all.filter(([id]) => !g.online.has(id));
  const row = ([id, name], on) =>
    h("li.member" + (on ? ".on" : ""), h("span.avatar-wrap", avatar(id, name), h("span.dot")), h("span.name", name, id === g.me ? h("small", " (vous)") : null));
  el.replaceChildren();
  append(el, [
    h("header.members-head", h("strong", "Membres"), h("button.icon-btn", { "aria-label": "Fermer", onclick: () => toggleMembers(false) }, "✕")),
    h("div.section-title", `En ligne — ${online.length}`),
    h("ul", online.map((m) => row(m, true))),
    offline.length ? h("div.section-title", `Hors ligne — ${offline.length}`) : null,
    h("ul", offline.map((m) => row(m, false))),
    h("div.fp", h("small", "Empreinte du serveur"), h("code", g.fingerprint), h("small.muted", "Identique chez tous les membres = personne ne s'est intercalé.")),
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
        await api(g, "PATCH", `/api/v1/messages/${g.gid}/${c.id}/${m.id}`, { body: await enc(g, text) });
      } else {
        await sendText(g, c.id, text);
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
      const mine = channelState(g, c.id).list.filter((m) => m.author === g.me && m.text != null).at(-1);
      if (mine) {
        e.preventDefault();
        startEdit(g, mine);
      }
    }
  });
  file.addEventListener("change", async () => {
    const f = file.files?.[0];
    file.value = "";
    if (!f || !c) return;
    toast("Chiffrement de la photo…");
    try {
      await sendImage(g, c.id, f);
      cancelBars();
    } catch (err) {
      toast(err.message, "error");
    }
  });
  return h(
    "div.composer",
    h("div#bar"),
    h(
      "div.input-row",
      h("button.icon-btn.attach", { title: "Envoyer une photo (chiffrée)", "aria-label": "Envoyer une photo", onclick: () => file.click() }, "＋"),
      file,
      input,
      h("button.send", { title: "Envoyer", "aria-label": "Envoyer", onclick: submit }, "➤"),
    ),
  );
}

function renderBar() {
  const bar = $("#bar");
  if (!bar) return;
  const g = currentGuild();
  if (state.editing) {
    bar.replaceChildren(h("div.bar", h("span", "✎ Modification du message"), h("button.icon-btn", { "aria-label": "Annuler", onclick: cancelBars }, "✕")));
  } else if (state.replyTo) {
    const name = g.members.get(state.replyTo.author) || "Membre";
    bar.replaceChildren(h("div.bar", h("span", "Réponse à ", h("strong", "@" + name)), h("button.icon-btn", { "aria-label": "Annuler", onclick: cancelBars }, "✕")));
  } else bar.replaceChildren();
}

function startReply(g, m) {
  state.editing = null;
  state.replyTo = m;
  renderBar();
  $("#input")?.focus();
}

function startEdit(g, m) {
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
  if (wasEditing) {
    const input = $("#input");
    if (input) input.value = "";
  }
  renderBar();
}

async function deleteMessage(g, c, m) {
  if (!confirm("Supprimer ce message pour tout le monde ?")) return;
  try {
    await api(g, "DELETE", `/api/v1/messages/${g.gid}/${c.id}/${m.id}`);
  } catch (err) {
    toast(err.message, "error");
  }
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
  if (!g.loaded) {
    try {
      await refreshGuild(g);
      await announce(g);
    } catch (err) {
      if (err.status === 404) return forget(g, "Ce serveur n'existe plus, ou la clé ne correspond pas.");
      toast(err.message, "error");
    }
  }
  connect(g);
  render();
  if (g.entry.channel != null) await openChannel(g, g.entry.channel);
}

async function selectChannel(g, cid) {
  g.entry.channel = cid;
  state.replyTo = state.editing = null;
  save();
  if (narrow()) toggleDrawer(false);
  renderChannels();
  renderChat();
  await openChannel(g, cid);
}

async function openChannel(g, cid) {
  g.unread.delete(cid);
  updateBadge();
  renderRail();
  renderChannels();
  const cs = channelState(g, cid);
  try {
    await loadMessages(g, cid);
  } catch (err) {
    toast(err.message, "error");
  }
  if (g.entry.channel === cid && state.current === g.gid) renderMessages(true);
  if (!isTouch) $("#input")?.focus();
  return cs;
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
  const hasInvite = !!pendingInvite;
  const go = async (e) => {
    e.preventDefault();
    const n = name.value.trim();
    if (!n) return name.focus();
    saved.name = n;
    save();
    if (pendingInvite) return handleInvite();
    if (state.guilds.size) return selectGuild(state.current || state.guilds.keys().next().value);
    addGuildDialog(true);
  };
  return h(
    "div.welcome",
    h(
      "form.welcome-card",
      { onsubmit: go },
      h("img", { src: "/assets/icon-192.png", alt: "", width: 72, height: 72 }),
      h("h1", hasInvite ? "Vous êtes invité sur Alley" : "Bienvenue sur Alley"),
      h("p.muted", "Des serveurs et des salons comme sur Discord, chiffrés de bout en bout. Pas de compte : vos clés restent sur cet appareil."),
      field("Comment voulez-vous apparaître ?", name, "Votre pseudo est chiffré, comme vos messages."),
      h("button.primary", { type: "submit" }, hasInvite ? "Rejoindre le serveur" : "Continuer"),
      h("p.small.muted", "🔒 AES-256-GCM · protocole Brocolis · serveur Paquets"),
    ),
  );
}

function addGuildDialog(first = false) {
  const name = h("input", { placeholder: "Ex. : Les copains du lycée", maxlength: 60, required: true, value: first ? `Le serveur de ${saved.name}` : "" });
  const link = h("input", { placeholder: "https://…/app#rejoindre=…", autocomplete: "off" });
  const d = dialog(
    "Créer ou rejoindre un serveur",
    h(
      "div.stack",
      h(
        "section.choice",
        h("h3", "🏰 Créer un serveur"),
        field("Nom du serveur", name),
        h("button.primary", {
          type: "button",
          onclick: async () => {
            const n = name.value.trim();
            if (!n) return name.focus();
            try {
              const g = await createGuild(n, "général");
              d.close();
              await selectGuild(g.gid);
              inviteDialog(g, true);
            } catch (err) {
              toast(err.message, "error");
            }
          },
        }, "Créer"),
      ),
      h(
        "section.choice",
        h("h3", "🔗 Rejoindre avec un lien"),
        field("Lien d'invitation", link, "La clé est dans le lien : demandez-le à un membre du serveur."),
        h("button", {
          type: "button",
          onclick: async () => {
            try {
              const g = await joinFromLink(link.value);
              d.close();
              await selectGuild(g.gid);
              toast(`Bienvenue sur « ${g.name} » !`);
            } catch (err) {
              toast(err.status === 404 ? "Serveur introuvable : le lien est-il complet ?" : err.message, "error");
            }
          },
        }, "Rejoindre"),
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
    actions.unshift(h("button", { type: "button", onclick: () => navigator.share({ title: `Rejoins « ${g.name} » sur Alley`, text: `Rejoins « ${g.name} » sur Alley (chiffré de bout en bout) :`, url: link }).catch(() => {}) }, "Partager…"));
  }
  actions.unshift(h("button.primary", { type: "button", onclick: () => copy(link, "Lien copié !") }, "Copier le lien"));
  dialog(
    justCreated ? "Votre serveur est prêt 🎉" : `Inviter sur « ${g.name} »`,
    h(
      "div.stack",
      h("p", "Toute personne qui a ce lien peut rejoindre le serveur et lire ses messages. Envoyez-le par un canal de confiance."),
      h("input.link", { value: link, readonly: true, onfocus: (e) => e.target.select() }),
      h("p.muted.small", "La clé est après le « # » : cette partie du lien n'est jamais envoyée au serveur."),
      h("div.fp-inline", h("span", "Empreinte : "), h("code", g.fingerprint)),
      g.entry.admin
        ? h(
            "details",
            h("summary", "Lien d'administration (à garder pour vous)"),
            h("p.muted.small", "Il donne aussi les droits d'administrateur : ouvrez-le sur vos autres appareils, ne le partagez pas."),
            h("button", { type: "button", onclick: () => copy(inviteLink(g, true), "Lien d'administration copié") }, "Copier le lien d'administration"),
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
      g.admin ? h("button", { type: "button", onclick: () => (d.close(), channelDialog(g)) }, "➕ Créer un salon") : null,
      g.admin ? h("button", { type: "button", onclick: () => (d.close(), guildSettings(g)) }, "⚙ Paramètres du serveur") : null,
      h("button", { type: "button", onclick: () => (d.close(), securityDialog(g)) }, "🔒 Chiffrement et empreinte"),
      h("button.danger", {
        type: "button",
        onclick: () => {
          if (!confirm(`Quitter « ${g.name} » ? La clé sera effacée de cet appareil : il vous faudra un nouveau lien pour revenir.`)) return;
          d.close();
          forget(g, `Vous avez quitté « ${g.name} ».`);
        },
      }, "🚪 Quitter le serveur"),
    ),
    [h("button.ghost", { value: "cancel" }, "Fermer")],
  );
}

function securityDialog(g) {
  dialog(
    "Chiffrement de bout en bout",
    h(
      "div.stack",
      h("p", "Messages, photos, noms de salons et pseudos sont chiffrés sur votre appareil en AES-256-GCM avant d'être envoyés. Le serveur ne garde que des jetons illisibles."),
      h("div.fp-big", h("small", "Empreinte du serveur"), h("code", g.fingerprint)),
      h("p.muted.small", "Comparez-la de vive voix avec les autres membres : si elle est identique pour tout le monde, personne n'a remplacé la clé."),
      h(
        "details",
        h("summary", "Clé au format Brocolis 🥦"),
        h("p.muted.small", "Alley utilise le protocole de la messagerie Brocolis : avec cette clé (base64), l'app Brocolis déchiffre les jetons [[BROCOLIS:v1:…]] d'Alley. Elle donne accès à tout le serveur : gardez-la secrète."),
        h("button", { type: "button", onclick: () => copy(B.toB64(g.raw), "Clé copiée") }, "Copier la clé"),
      ),
    ),
    [h("button.ghost", { value: "cancel" }, "Fermer")],
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
        h("button.danger", {
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
        }, "Supprimer le serveur"),
      ),
    ),
    [
      h("button.ghost", { value: "cancel" }, "Annuler"),
      h("button.primary", {
        type: "button",
        onclick: async () => {
          const n = name.value.trim();
          if (!n) return;
          try {
            await api(g, "PATCH", `/api/v1/guilds/${g.gid}`, { name: await enc(g, n) });
            d.close();
          } catch (err) {
            toast(err.message, "error");
          }
        },
      }, "Enregistrer"),
    ],
  );
}

function channelDialog(g, c = null) {
  const name = h("input", { value: c?.name || "", placeholder: "nouveau-salon", maxlength: 60, required: true });
  const topic = h("input", { value: c?.topic || "", placeholder: "De quoi parle-t-on ici ?", maxlength: 200 });
  const slug = (v) => v.trim().toLowerCase().replace(/\s+/g, "-");
  const d = dialog(
    c ? `Modifier #${c.name}` : "Créer un salon",
    h(
      "div.stack",
      field("Nom du salon", name),
      field("Sujet", topic, "Facultatif. Chiffré, comme le nom."),
      c && g.channels.length > 1
        ? h("button.danger", {
            type: "button",
            onclick: async () => {
              if (!confirm(`Supprimer #${c.name} et tous ses messages ?`)) return;
              try {
                await api(g, "DELETE", `/api/v1/guilds/${g.gid}/channels/${c.id}`);
                d.close();
              } catch (err) {
                toast(err.message, "error");
              }
            },
          }, "Supprimer le salon")
        : null,
    ),
    [
      h("button.ghost", { value: "cancel" }, "Annuler"),
      h("button.primary", {
        type: "button",
        onclick: async () => {
          const n = slug(name.value);
          if (!n) return name.focus();
          const body = { name: await enc(g, n), topic: topic.value.trim() ? await enc(g, topic.value.trim()) : "" };
          try {
            if (c) await api(g, "PATCH", `/api/v1/guilds/${g.gid}/channels/${c.id}`, body);
            else {
              const created = await api(g, "POST", `/api/v1/guilds/${g.gid}/channels`, body);
              g.entry.channel = created.id;
            }
            d.close();
            if (!c) await refreshGuild(g).then(() => selectChannel(g, g.entry.channel));
          } catch (err) {
            toast(err.message, "error");
          }
        },
      }, c ? "Enregistrer" : "Créer"),
    ],
  );
  setTimeout(() => name.focus(), 50);
}

function settingsDialog() {
  const name = h("input", { value: saved.name, maxlength: 32, required: true });
  const canNotify = "Notification" in window;
  const notif = h("input", { type: "checkbox", checked: saved.notify && canNotify && Notification.permission === "granted", disabled: !canNotify });
  const d = dialog(
    "Réglages",
    h(
      "div.stack",
      field("Pseudo", name, "Mis à jour (chiffré) sur tous vos serveurs."),
      h(
        "label.check",
        notif,
        h("span", "Notifications des nouveaux messages"),
      ),
      canNotify ? null : h("p.muted.small", "Sur iPhone et iPad, installez d'abord Alley sur l'écran d'accueil (Partager → Sur l'écran d'accueil) pour recevoir des notifications."),
      h("p.muted.small", `Alley garde ${state.guilds.size} serveur(s) et leurs clés sur cet appareil uniquement.`),
    ),
    [
      h("button.ghost", { value: "cancel" }, "Annuler"),
      h("button.primary", {
        type: "button",
        onclick: async () => {
          const n = name.value.trim();
          if (!n) return name.focus();
          if (notif.checked && canNotify && Notification.permission !== "granted") {
            const p = await Notification.requestPermission();
            if (p !== "granted") toast("Notifications refusées par le navigateur.", "error");
            saved.notify = p === "granted";
          } else saved.notify = notif.checked;
          const changed = n !== saved.name;
          saved.name = n;
          save();
          d.close();
          if (changed) {
            for (const g of state.guilds.values()) if (g.loaded) announce(g).catch(() => {});
            render();
          }
        },
      }, "Enregistrer"),
    ],
  );
}

function lightbox(src) {
  const d = h("dialog.lightbox", h("img", { src, alt: "Photo" }), h("button.icon-btn.close", { "aria-label": "Fermer", onclick: () => d.close() }, "✕"));
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
    toast(err.status === 404 ? "Serveur introuvable : il a peut-être été supprimé." : err.message, "error");
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

async function start() {
  document.body.append(h("div#toasts", { "aria-live": "polite" }));
  trackViewport();
  if ("serviceWorker" in navigator) navigator.serviceWorker.register("/sw.js").catch(() => {});

  // Le lien d'invitation : la clé est dans le fragment, qu'on efface aussitôt
  // de la barre d'adresse et de l'historique.
  if (location.hash.includes("rejoindre=")) {
    pendingInvite = location.href;
    history.replaceState(null, "", "/app");
  }
  const wantsNew = location.hash === "#nouveau";
  if (wantsNew) history.replaceState(null, "", "/app");

  for (const entry of saved.guilds) {
    try {
      const g = await hydrate(entry);
      state.guilds.set(g.gid, g);
    } catch {
      // Une entrée abîmée : on l'ignore.
    }
  }
  if (!saved.name) return render();
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
      .then(() => connect(g))
      .then(renderRail)
      .catch((err) => err.status === 404 && forget(g, `« ${g.name} » n'existe plus.`));
  }
}

addEventListener("online", () => {
  for (const g of state.guilds.values()) if (!g.socket) connect(g);
});

matchMedia("(max-width: 860px)").addEventListener("change", () => render());

start().catch((err) => {
  console.error(err);
  root().replaceChildren(h("div.welcome", h("div.welcome-card", h("h1", "Oups"), h("p", "Alley n'a pas pu démarrer : " + err.message))));
});
