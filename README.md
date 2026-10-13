# Alley 🔒

**L'équivalent de Discord, chiffré de bout en bout.** Des serveurs, des salons
textuels et vocaux, des appels audio et vidéo, des photos, des messages
vocaux, des réponses, la présence en ligne… et un serveur qui ne voit **que
des jetons illisibles** — pas même qui a écrit quoi, où, ni quand.

🌐 **En ligne (HTTPS)** : <https://maxlestage.github.io/Alley/> (GitHub Pages)
et l'API sur Heroku — voir [Déploiement](#déploiement).

- 🥦 **Chiffré avec [Brocolis](https://github.com/maxlestage/Messagerie-)** :
  AES-256-GCM et jetons `[[BROCOLIS:v1:…]]` de la messagerie Brocolis, clés
  redistribuées par **X25519 + HKDF-SHA256** exactement comme l'appairage
  Brocolis (recoupé avec ses vecteurs de test), même empreinte de clé.
- 📦 **Servi par [Paquets](https://github.com/maxlestage/Paquets)** : une seule
  `App` Rust — Vitesse (HTTP + WebSocket), Securite (WAF, limites de débit,
  CSP stricte, HTTPS imposé, HSTS, CORS, anti-robots), Memoire (données),
  Active (pages SEO, PWA).
- 🦀 **100 % Rust** : le serveur, et tout le navigateur — chiffrement,
  application, appels WebRTC, vitrine — compilé en **WebAssembly**
  ([`client/`](client/)). Aucune ligne de JavaScript écrite à la main : le
  navigateur ne reçoit que la colle générée par wasm-bindgen et une ligne de
  démarrage générée par le serveur (comme les îlots de Paquets).
- 📱 **Sur tous les systèmes** : une application web installable (PWA) —
  iPhone et iPad (Safari → Partager → *Sur l'écran d'accueil*), Android,
  Windows, macOS, Linux, ChromeOS. Plein écran, encoche et clavier iOS gérés,
  ouverture hors ligne, badge de non-lus, notifications.

## Fonctionnalités

| | |
|---|---|
| 🏰 Serveurs et salons | créer, rejoindre par lien, salons textuels et vocaux, sujets, renommer, supprimer |
| 💬 Messages | temps réel, « en train d'écrire… », réponses, modification, suppression, mise en forme (gras, italique, barré, `code`, blocs, citations, liens) |
| 🖼️ Médias | photos (réduites puis chiffrées), messages vocaux (2 min, chiffrés) |
| 🎙️ Appels | salons vocaux : voix, caméra, partage d'écran, qui parle, micro coupé — de pair à pair |
| 👥 Membres | présence en ligne, qui est en appel, empreinte du serveur |
| 🛡️ Modération | l'admin **exclut** un membre : la clé est renouvelée et redistribuée aux autres, l'exclu est banni ; **renouveler la clé** si un lien a fui |
| 🔔 Confort | non-lus par salon, badge d'icône, notifications, mode clair/sombre, hors ligne |

## Le chiffrement

1. En créant un serveur, le navigateur tire une **clé AES-256**. Le lien
   d'invitation la porte **après le `#`** (`/app/#rejoindre=<clé>&g=<id>`) :
   les navigateurs n'envoient jamais cette partie, et l'app l'efface aussitôt
   de la barre d'adresse.
2. De la clé dérivent le **jeton d'accès** (`SHA-256("alley-access-v1" ‖ clé)`,
   dont le serveur ne garde que l'empreinte) et l'**empreinte** à comparer de
   vive voix (`4BB0-6F8E-4E3A-7715`, la même que dans Brocolis).
3. **Tout est chiffré avant de partir** : contenu, photos, vocaux, noms de
   serveur et de salons, pseudos — et aussi l'**auteur**, le **salon** et
   l'**heure** de chaque message, rangés *dans* l'enveloppe chiffrée. La
   taille est arrondie au demi-kilo-octet. Le serveur refuse tout ce qui n'est
   pas un jeton (`422`).
4. **Chaque message est signé** (ECDSA P-256) : un membre, qui a pourtant la
   clé, ne peut pas parler au nom d'un autre. Les profils (pseudo, clés
   publiques) sont chiffrés et liés à leur identifiant.
5. **Modifier ou supprimer** son message demande une preuve que seul l'auteur
   peut recalculer, différente pour chaque message : le serveur ne peut pas
   relier deux messages au même auteur.
6. **Exclure ou renouveler** : l'admin tire une nouvelle clé et l'emballe pour
   chaque membre restant avec sa clé publique X25519 (format `wrapGroupKey` de
   Brocolis). Le nouvel état chiffré contient les anciennes clés : un membre
   n'accepte une clé que si elle ouvre un état qui contient la sienne — seul
   quelqu'un qui la connaissait, jamais le serveur, a pu la fabriquer.
   L'ancien jeton d'accès cesse de marcher, les connexions sont coupées,
   l'exclu est banni.
7. **Appels** : WebRTC de pair à pair (DTLS-SRTP). Les offres, réponses et
   candidats passent par un relais **chiffré et signé** : le serveur ne peut
   ni écouter ni s'intercaler. Un TURN éventuel (`ALLEY_ICE_SERVERS`) ne voit
   que du chiffré.
8. **Présence, « j'écris », signalisation** : relayés chiffrés et signés ; la
   connexion temps réel ne présente que le jeton d'accès, le serveur ne sait
   pas qui est en ligne.

**Ce que voit le serveur** : qu'un serveur Alley existe, son nombre de membres
(des identifiants aléatoires) et de messages (des numéros et des jetons), et,
comme tout serveur web, les adresses IP qui s'y connectent — passez par un VPN
ou Tor si cela compte pour vous.

## Déploiement

Chaque push est testé ; **une branche verte est fusionnée automatiquement dans
`master`** (sauf `wip/...`), puis l'API part sur Heroku et le site sur GitHub
Pages, les deux en HTTPS.

À faire une fois, depuis GitHub (même sur téléphone) :

1. **GitHub Pages** : *Settings → Pages → Build and deployment → Source :
   GitHub Actions*.
2. **L'API sur Heroku** : *Settings → Secrets and variables → Actions* :
   - secret `HEROKU_API_KEY` (Heroku → *Account settings* → *API Key*) ;
   - variable `HEROKU_APP_NAME` (l'app est créée si besoin, et réglée pour
     accepter la version GitHub Pages).
   Une autre API ? Mettez son adresse dans la variable `ALLEY_API_URL`.
3. *Actions → CI → Run workflow* (ou n'importe quel push).

| Workflow | Rôle |
|---|---|
| [`ci.yml`](.github/workflows/ci.yml) | formatage, clippy, tests Rust et JS, puis fusion automatique dans `master` et déploiements |
| [`heroku.yml`](.github/workflows/heroku.yml) | construit l'image Docker sur Heroku, règle `ALLEY_ALLOWED_ORIGINS`, puis republie Pages |
| [`pages.yml`](.github/workflows/pages.yml) | `alley export` sous `/<dépôt>` avec l'adresse de l'API, publication sur GitHub Pages |

Sans API configurée, l'app GitHub Pages demande l'adresse d'un serveur Alley au
premier lancement (modifiable ensuite dans *Réglages → Avancé*).

## Lancer en local

```sh
rustup target add wasm32-unknown-unknown
scripts/wasm-bindgen.sh                    # wasm-bindgen, à la version de Cargo.lock
scripts/web.sh                             # le navigateur : client/ → client/pkg/ (WebAssembly)
cargo run                                  # http://localhost:3000
DATABASE_URL=sqlite://alley.db cargo run   # données (chiffrées) gardées dans un fichier
BASE_PATH=/Alley ALLEY_API_URL=https://mon-api.herokuapp.com cargo run -- export dist
```

| Variable | Rôle |
|---|---|
| `PORT` | le port (3000 par défaut) |
| `DATABASE_URL` | vide (mémoire), `memory://alley.memoire`, `sqlite://…`, `postgres://…` (Heroku Postgres) |
| `ALLEY_ALLOWED_ORIGINS` | les sites autorisés à appeler l'API (`https://maxlestage.github.io` par défaut) |
| `ALLEY_ICE_SERVERS` | serveurs STUN/TURN en plus (JSON `RTCIceServer[]`) |

Le chiffrement du navigateur (WebCrypto) n'existe qu'en HTTPS (ou sur
`localhost`) : derrière Heroku, le serveur redirige HTTP vers HTTPS, et l'app
renvoie vers la version sécurisée si on l'ouvre en HTTP.

## Tests

```sh
cargo test --workspace    # API, chiffrement imposé, rotation et exclusion, CORS, HTTPS, export Pages,
                          # et le chiffrement du navigateur recoupé avec les vecteurs de Brocolis
```

Les tests du serveur chiffrent avec le code même du navigateur
(`alley_web::crypto`, compilé ici sans WebAssembly).

Testé aussi dans de vrais navigateurs (Chromium, ordinateur, iPhone et Android
émulés) : discussion, appel audio et vidéo, exclusion d'un membre et
redistribution de la clé, message vocal — depuis le serveur comme depuis
l'export GitHub Pages en cross-origin.

## Organisation

| Fichier | Rôle |
|---|---|
| `src/lib.rs` | l'`App` Paquets : bouclier (HTTPS, CORS, CSP…), API, pages, fichiers |
| `src/protocol.rs` | ce que le serveur vérifie des jetons et des secrets |
| `src/api.rs` | modèles (`#[model]`) et routes de l'API, rotation de clé, WebSocket |
| `src/live.rs` | diffusion temps réel (sans savoir qui est connecté) |
| `src/pages.rs` | la vitrine et la coquille de l'app, la ligne de démarrage du WebAssembly |
| `src/assets.rs` | fichiers du navigateur (WebAssembly compris), manifeste et service worker |
| `src/export.rs` | l'export statique pour GitHub Pages |
| `build.rs` | range `client/pkg/` dans le binaire |
| `client/src/crypto.rs` | AES-256-GCM, X25519 + HKDF (format Brocolis), ECDSA P-256 |
| `client/src/app.rs`, `model.rs`, `net.rs` | l'application : serveurs, clés, messages, temps réel |
| `client/src/ui.rs`, `dom.rs` | l'affichage, construit en Rust (aucun HTML injecté) |
| `client/src/call.rs` | les appels WebRTC |
| `client/src/site.rs` | la vitrine (installation selon l'appareil) |
| `web/*.css`, `web/icons/` | les styles et les icônes |
| `scripts/web.sh`, `scripts/wasm-bindgen.sh` | compiler le navigateur |
