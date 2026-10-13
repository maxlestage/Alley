# Alley 🔒

**Des salons de discussion éphémères, chiffrés de bout en bout.** Un
pseudonyme, et c'est tout : on cherche un salon ou on en crée un, on parle, on
s'appelle en vidéo… et rien ne reste.

🌐 <https://maxlestage.github.io/Alley/> (GitHub Pages) et l'API sur Heroku —
voir [Déploiement](#déploiement).

## Le principe

- 🎭 **Un pseudonyme, c'est tout** : pas de compte, et **rien n'est enregistré**,
  ni sur l'appareil (aucun `localStorage`), ni sur le serveur (aucune base de
  données). Fermer la page, c'est tout oublier.
- 🔎 **Chercher ou choisir un salon** : le hall liste les salons ouverts, une
  recherche les filtre (sans tenir compte des accents ni des majuscules), et un
  nom qui n'existe pas encore se crée d'un geste. Un salon peut être **privé** :
  absent de la liste, joignable seulement par son lien.
- 🌱 **Un salon ne vit que si quelqu'un y est** : le dernier qui part l'emporte
  avec lui.
- 🙈 **Pas d'historique** : le serveur ne garde aucun message, il relaie chaque
  jeton chiffré aux présents puis l'oublie. Qui arrive ne voit rien de ce qui
  s'est dit avant.
- 🌙 **Tout disparaît à minuit** : à minuit dans le pays de son créateur, le
  salon est fermé pour tout le monde ; à minuit chez soi, l'écran se vide.
- 🎙️ Messages (gras, italique, code, liens, réponses, modification,
  suppression), photos, messages vocaux, **appels audio et vidéo**, partage
  d'écran.
- 🥦 **Chiffré avec [Brocolis](https://github.com/maxlestage/Messagerie-)** :
  AES-256-GCM et jetons `[[BROCOLIS:v1:…]]`, clé du salon remise par **X25519 +
  HKDF-SHA256** exactement comme l'appairage Brocolis (recoupé avec ses
  vecteurs de test).
- 📦 **Servi par [Paquets](https://github.com/maxlestage/Paquets)** : Vitesse
  (HTTP + WebSocket), Securite (WAF, limites de débit, CSP stricte, HTTPS
  imposé, HSTS, CORS), Active (pages SEO, PWA).
- 🦀 **100 % Rust** : le serveur, tout le navigateur compilé en
  **WebAssembly** ([`client/`](client/)), les styles ([`src/style/`](src/style/))
  et la compilation elle-même ([`xtask/`](xtask/)). Ni JavaScript, ni CSS, ni
  script shell écrits à la main : le seul JavaScript livré est la colle que
  génère wasm-bindgen pour charger le module (un navigateur ne sait pas
  démarrer du WebAssembly sans elle).
- 📱 **Sur tous les systèmes** : application web installable (iPhone et iPad
  via Safari → Partager → *Sur l'écran d'accueil*, Android, Windows, macOS,
  Linux, ChromeOS).

## Le chiffrement

1. Le premier arrivant d'un salon tire une **clé AES-256** sur son appareil.
2. Chaque arrivant présente une **clé X25519 jetable** ; les trois plus anciens
   présents lui remettent la clé du salon, **emballée pour lui seul** (format
   `wrapGroupKey` de Brocolis). Le serveur ne voit que des clés publiques et du
   chiffré.
3. Pseudonymes, messages, photos, vocaux et mise en relation des appels
   deviennent des jetons que le serveur relaie sans pouvoir les lire ; il
   refuse tout ce qui n'en est pas un.
4. Le serveur indique qui envoie chaque jeton : personne ne peut modifier ou
   supprimer le message d'un autre.
5. Chaque salon affiche l'**empreinte** de sa clé (`4BB0-6F8E-4E3A-7715`, comme
   dans Brocolis) : identique chez tous les présents, personne ne s'est
   intercalé.
6. Les **appels** passent de pair à pair (WebRTC, DTLS-SRTP).

**Ce que voit le serveur** : le nom des salons publics (pour qu'on puisse les
chercher), combien de personnes y sont, et des jetons qu'il oublie aussitôt.
Pas les pseudonymes, pas les messages, pas la clé. Comme tout serveur web, il
voit les adresses IP qui se connectent.

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

Sans API configurée, l'app GitHub Pages demande l'adresse d'un serveur Alley
sur l'écran d'accueil (elle n'est jamais enregistrée).

## Lancer en local

```sh
rustup target add wasm32-unknown-unknown
cargo xtask web                            # le navigateur : client/ → client/pkg/ (WebAssembly)
cargo run                                  # http://localhost:3000
BASE_PATH=/Alley ALLEY_API_URL=https://mon-api.herokuapp.com cargo run -- export dist
```

| Variable | Rôle |
|---|---|
| `PORT` | le port (3000 par défaut) |
| `ALLEY_ALLOWED_ORIGINS` | les sites autorisés à appeler l'API (`https://maxlestage.github.io` par défaut) |
| `ALLEY_ICE_SERVERS` | serveurs STUN/TURN en plus (JSON `RTCIceServer[]`) |

Le chiffrement du navigateur (WebCrypto) n'existe qu'en HTTPS (ou sur
`localhost`) : derrière Heroku, le serveur redirige HTTP vers HTTPS, et l'app
renvoie vers la version sécurisée si on l'ouvre en HTTP.

## Tests

```sh
cargo test --workspace    # salons (minuit, départ du dernier, recherche), vrais salons en WebSocket
                          # (remise de la clé, clair refusé, rien de gardé), CORS, HTTPS, export Pages,
                          # et le chiffrement du navigateur recoupé avec les vecteurs de Brocolis
```

Les tests du serveur chiffrent avec le code même du navigateur
(`alley_web::crypto`, compilé ici sans WebAssembly).

Testé aussi dans de vrais navigateurs (Chromium, ordinateur et iPhone émulé) :
hall, recherche, création, aucun historique pour qui arrive, même empreinte de
clé, appel audio et vidéo, photo, vocal, rien dans le `localStorage`, salon
disparu au départ du dernier, et écran vidé à minuit (horloge simulée).

## Organisation

| Fichier | Rôle |
|---|---|
| `src/lib.rs` | l'`App` Paquets : bouclier (HTTPS, CORS, CSP…), API, pages, fichiers |
| `src/protocol.rs` | ce que le serveur vérifie des jetons et des clés |
| `src/rooms.rs` | les salons, en mémoire : présence, relais, minuit |
| `src/api.rs` | les routes de l'API et la WebSocket des salons |
| `src/pages.rs` | la vitrine et la coquille de l'app |
| `src/style/` | les styles, écrits en Rust et rendus en CSS au démarrage |
| `src/assets.rs` | fichiers du navigateur (WebAssembly compris, adresses versionnées) et manifeste |
| `src/export.rs` | l'export statique pour GitHub Pages |
| `build.rs` | range `client/pkg/` dans le binaire |
| `client/src/crypto.rs` | AES-256-GCM, X25519 + HKDF (format Brocolis) |
| `client/src/app.rs`, `model.rs`, `net.rs` | l'application : hall, salons, remise de la clé, messages, minuit |
| `client/src/ui.rs`, `dom.rs` | l'affichage, construit en Rust (aucun HTML injecté) |
| `client/src/call.rs` | les appels WebRTC |
| `client/src/site.rs` | la vitrine (installation selon l'appareil) |
| `web/icons/`, `web/favicon.svg` | les icônes |
| `xtask/` | `cargo xtask web` : compile le navigateur et génère sa colle avec la bibliothèque de wasm-bindgen |
