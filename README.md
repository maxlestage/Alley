# Alley 🔒

**L'équivalent de Discord, chiffré de bout en bout.** Des serveurs, des salons,
des discussions en temps réel, des photos, des réponses, la présence en ligne…
et un serveur qui ne voit **que des jetons illisibles**.

- 🥦 **Chiffré avec [Brocolis](https://github.com/maxlestage/Messagerie-)** : le
  protocole de la messagerie Brocolis (AES-256-GCM, jetons
  `[[BROCOLIS:v1:…]]`, photos `BRCM1:img:`, empreinte de clé). Un jeton Alley se
  déchiffre dans l'app Brocolis avec la même clé, et inversement.
- 📦 **Servi par [Paquets](https://github.com/maxlestage/Paquets)** : une seule
  `App` Rust — Vitesse (HTTP + WebSocket), Securite (WAF, limites de débit,
  CSP stricte, HSTS, anti-robots), Memoire (données), Active (pages SEO, PWA).
- 📱 **Sur tous les systèmes** : une application web installable (PWA) —
  iPhone et iPad (Safari → Partager → *Sur l'écran d'accueil*), Android,
  Windows, macOS, Linux, ChromeOS. Plein écran, encoche et clavier iOS gérés,
  ouverture hors ligne, badge de non-lus.

## Ce qu'il y a dedans

| Adresse | Contenu |
|---|---|
| `/` | la vitrine qui présente le produit (rendue par le serveur, SEO complet) |
| `/app` | l'application façon Discord |
| `/api/v1/…` | l'API (serveurs, salons, membres, messages) et le flux temps réel en WebSocket |
| `/manifest.webmanifest`, `/sw.js` | ce qui rend l'app installable et utilisable hors ligne |

**Fonctionnalités** : créer/rejoindre des serveurs par lien, salons avec sujet,
messages en temps réel, « en train d'écrire… », présence en ligne, non-lus
(badges, titre, badge d'icône), réponses, modification et suppression,
photos chiffrées, mise en forme (`**gras**`, `*italique*`, `~~barré~~`,
`` `code` ``, blocs ```` ``` ````, citations, liens), administration
(salons, nom du serveur, modération, suppression du serveur), notifications,
mode clair/sombre automatique.

## Comment le chiffrement fonctionne

1. En créant un serveur, le navigateur tire une **clé AES-256** au hasard.
2. Le lien d'invitation la porte **après le `#`** (`/app#rejoindre=<clé>`) :
   cette partie d'une adresse n'est jamais envoyée au serveur. L'app l'efface
   de la barre d'adresse dès qu'elle l'a lue.
3. De la clé, le navigateur dérive :
   - l'**identifiant du serveur** : `SHA-256("brocolis-conv-v1" ‖ clé)`
     (comme une conversation Brocolis), tronqué à 128 bits ;
   - le **jeton d'accès** : `SHA-256("alley-access-v1" ‖ clé)`. Le serveur
     n'en garde que l'empreinte et répond `404` à qui ne l'a pas ;
   - l'**empreinte** à comparer de vive voix (`4BB0-6F8E-4E3A-7715`).
4. Messages, photos, noms de serveurs et de salons, sujets et pseudos sont
   chiffrés **avant d'être envoyés**. Le serveur **refuse tout ce qui n'est pas
   un jeton Brocolis** (`422`).
5. Le créateur reçoit un **secret d'admin** ; chaque appareil tire un **secret
   de membre** (son identifiant public en est l'empreinte) : seul l'auteur
   modifie ses messages, seul l'admin gère les salons et modère.

Ce que voit le serveur : des identifiants aléatoires, des horaires, des tailles
et des jetons. Ni la clé, ni le contenu, ni les noms.

## Lancer

```sh
cargo run                                  # http://localhost:3000
DATABASE_URL=sqlite://alley.db cargo run   # données (chiffrées) gardées dans un fichier
```

`DATABASE_URL` accepte tout ce que gère Paquets : vide (en mémoire),
`memory://alley.memoire`, `sqlite://…`, `postgres://…` (Heroku Postgres).

Déployer : `docker build -t alley . && docker run -p 8080:8080 alley`, ou
Heroku (pile *container*, `heroku.yml` et `app.json` fournis). Le WebCrypto des
navigateurs exige **HTTPS** (ou `localhost`).

## Tests

```sh
cargo test                     # API de bout en bout, refus du clair, droits, temps réel
node --test web/*.test.mjs     # chiffrement du navigateur, recoupé avec les vecteurs de Brocolis
```

## Organisation

| Fichier | Rôle |
|---|---|
| `src/lib.rs` | l'`App` Paquets : bouclier, API, pages, fichiers |
| `src/protocol.rs` | ce que le serveur vérifie des jetons Brocolis et des secrets |
| `src/api.rs` | modèles (`#[model]`) et routes de l'API, WebSocket |
| `src/live.rs` | diffusion temps réel et présence |
| `src/pages.rs` | la vitrine et la coquille de l'app |
| `src/assets.rs` | fichiers du navigateur, manifeste et service worker |
| `web/brocolis.js` | le chiffrement (WebCrypto), compatible Brocolis |
| `web/app.js`, `web/app.css` | l'application |
| `web/site.js`, `web/site.css` | la vitrine |
