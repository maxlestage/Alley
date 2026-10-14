# Alley 🔒

**Des salons de discussion éphémères, chiffrés de bout en bout.** Un
pseudonyme suffit : on cherche un salon ou on en crée un, on parle, on
s'appelle en vidéo… et rien ne reste. En français, en anglais et en espagnol.

🌐 <https://maxlestage.github.io/Alley/> (GitHub Pages) et l'API sur Heroku —
voir [Déploiement](#déploiement).

## Le principe

- 🎭 **Un pseudonyme suffit**, et **aucun message n'est enregistré** : ni sur
  l'appareil, ni dans une base de données sur le serveur. Fermer la page, c'est
  oublier messages, pseudonyme et clés.
- 🔁 **On revient dans ses salons** ([`client/src/recent.rs`](client/src/recent.rs)) :
  l'appareil retient les salons créés ou visités, avec leur serveur, jusqu'à
  leur minuit — jamais un message, une clé ou un pseudonyme. Le hall les
  affiche (« Vos salons ») ; un geste pour y revenir, « Oublier » ou « Tout
  oublier » pour les effacer. Le serveur saisi sur GitHub Pages est retenu
  aussi.
- 🔏 **Ce que garde l'appareil est signé** ([`client/src/vault.rs`](client/src/vault.rs)) :
  n'importe qui peut modifier le `localStorage` d'un navigateur (outils de
  développement, extension, programme sur l'ordinateur). Chaque donnée gardée
  (salons retenus, serveur, langue) est donc signée par plusieurs points
  indépendants, dont aucune clé n'est dans le `localStorage` :
  - **l'appareil** : une clé HMAC-SHA256 tirée par WebCrypto, **non
    exportable**, rangée dans IndexedDB : la page s'en sert, personne ne peut
    la lire ni la copier ;
  - **le compte**, si l'on est connecté : une clé tirée du mot de passe,
    jamais enregistrée ; ce qui a été écrit connecté est revérifié à chaque
    connexion ;
  - **le serveur**, pour chaque salon retenu : il signe (Ed25519, clé tirée à
    son démarrage, `GET /api/v1/key`) l'identifiant, le nom, le type et la
    fin de chaque salon qu'il décrit.

  Une donnée dont une signature ne correspond pas est écartée et effacée, et
  un avertissement s'affiche. Même un script qui se servirait de la clé de
  l'appareil ne peut ni renommer un salon (signature du serveur) ni réécrire
  ce qu'a écrit un compte (signature du compte). Sans IndexedDB, rien n'est
  gardé : mieux vaut ne rien retenir que retenir sans pouvoir vérifier.
- ✍️ **Tout est signé** ([`client/src/crypto.rs`](client/src/crypto.rs)) :
  chaque membre tire, en entrant, une clé Ed25519 propre au salon et
  l'annonce avec sa clé X25519. Tout ce qu'il envoie (messages,
  modifications, suppressions, pseudonyme, photos, vocaux, signalisation
  d'appel) est signé avec elle, en plus d'être chiffré : la signature couvre
  le salon, le destinataire et un numéro d'envoi qui ne fait que croître.
  Un envoi modifié, rejoué, détourné vers un autre salon ou un autre
  destinataire, ou attribué à quelqu'un d'autre est écarté (et signalé).
  L'empreinte de la clé de chacun s'affiche en survolant son nom, pour la
  comparer de vive voix.
- 🔑 **Se connecter avec un mot de passe**, que le navigateur retient (son
  gestionnaire de mots de passe, synchronisé entre appareils) : on retrouve
  ses salons sur tous ses appareils, jusqu'à leur minuit. Le mot de passe ne
  quitte jamais l'appareil : WebCrypto en tire une graine (PBKDF2-SHA256,
  310 000 tours, salée par le pseudonyme), dont on tire une **marque** ; le
  serveur ne voit que cette marque, que chaque salon retient tant qu'il vit.
- 👑 **Un administrateur par salon** : son créateur. Connecté avec un compte,
  il le reste — de retour avec le même pseudonyme et mot de passe, il retrouve
  son rôle ; sans compte, il redevient simple membre en partant. Tant qu'il
  est absent, le plus ancien présent tient le rôle. L'administrateur exclut
  un membre (qui ne peut plus revenir tant que le salon existe) et supprime
  n'importe quel message.
- 🌍 **Français, anglais, espagnol** : la vitrine (`/`, `/en/`, `/es/`),
  l'application (`/app/`, `/en/app/`, `/es/app/`), les messages d'erreur, tout
  est traduit ; on change de langue sans quitter son salon.
- 🛡️ **Ni contenu sexuel, ni drogue** ([`src/moderation/`](src/moderation/)) :
  - le texte reste chiffré de bout en bout ; c'est le navigateur qui le filtre
    (en trois langues, chiffres déguisés et lettres espacées compris) : un
    message interdit ne part pas et est masqué à la réception. Le serveur
    refuse les noms de salon interdits ;
  - une **photo** est vérifiée par le serveur avant d'être relayée :
    l'expéditeur lui confie la clé de cette seule photo, il la déchiffre en
    mémoire, la juge avec deux modèles exécutés en Rust (un classifieur
    d'images sexuelles, et CLIP, qui la compare à des descriptions de drogues,
    de nudité et d'images ordinaires), puis l'oublie. Les autres reçoivent
    l'empreinte de la clé vue par le serveur et vérifient que c'est bien
    cette photo ;
  - une photo à caractère sexuel ou montrant de la drogue **bloque l'adresse IP**
    de son auteur pendant 30 jours (`ALLEY_BAN_DAYS`) : ses connexions sont
    fermées, toutes ses requêtes refusées.
- 🔎 **Chercher ou choisir un salon** : le hall liste les salons ouverts, une
  recherche les filtre (sans tenir compte des accents ni des majuscules), et un
  nom qui n'existe pas encore se crée d'un geste. Un salon peut être **privé** :
  absent de la liste, joignable seulement par son lien.
- 🌱 **Un salon reste ouvert jusqu'à minuit**, même quand tout le monde est
  parti : on peut y revenir (une nouvelle clé est tirée, l'ancienne est
  partie avec le dernier présent). Un salon jamais rejoint disparaît au bout
  d'une minute ; quand la place manque (5 000 salons), le salon vide depuis
  le plus longtemps cède la sienne.
- 🙈 **Pas d'historique** : le serveur ne garde aucun message, il relaie chaque
  jeton chiffré aux présents puis l'oublie. Qui arrive ne voit rien de ce qui
  s'est dit avant.
- 🌙 **Tout disparaît à minuit** : à minuit dans le pays de son créateur, le
  salon est fermé pour tout le monde ; à minuit chez soi, l'écran se vide.
- 🎙️ Messages (gras, italique, code, liens, réponses, modification,
  suppression), photos, messages vocaux, **appels audio et vidéo**, partage
  d'écran.
- 🧑‍💻 **Le code, mis en forme et coloré** ([`client/src/highlight.rs`](client/src/highlight.rs),
  en Rust) : 28 langages (Rust, JavaScript, TypeScript, Python, Go, Java,
  Kotlin, Swift, C, C++, C#, PHP, Ruby, SQL, Shell, HTML, CSS, JSON, YAML,
  TOML, Dockerfile, diff…), langage deviné quand le bloc ne le nomme pas,
  lignes numérotées, bouton « Copier ». Dans la saisie : le bouton `</>`,
  le code collé mis tout seul dans un bloc, Entrée qui va à la ligne (en
  gardant l'indentation) et Tab qui indente à l'intérieur d'un bloc.
- 🥦 **Chiffré avec [Brocolis](https://github.com/maxlestage/Messagerie-)** :
  AES-256-GCM et jetons `[[BROCOLIS:v1:…]]`, clé du salon remise par **X25519 +
  HKDF-SHA256** exactement comme l'appairage Brocolis (recoupé avec ses
  vecteurs de test).
- 📦 **Servi par [Paquets](https://github.com/maxlestage/Paquets)** : Vitesse
  (HTTP + WebSocket), Securite (WAF, limites de débit, CSP stricte, HTTPS
  imposé, HSTS, CORS), Active (pages SEO, PWA).
- 🦀 **100 % Rust** : le serveur, tout le navigateur compilé en
  **WebAssembly** ([`client/`](client/)), les styles ([`src/style/`](src/style/)),
  la modération des images, et les outils ([`xtask/`](xtask/)) : compilation,
  modèles, fusion, déploiements (les workflows ne font qu'appeler
  `cargo xtask`). Ni JavaScript, ni CSS, ni script shell écrits à la main : le
  seul JavaScript livré est la colle que génère wasm-bindgen pour charger le
  module (un navigateur ne sait pas démarrer du WebAssembly sans elle).
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
chercher), combien de personnes y sont, la marque des comptes connectés, et
des jetons qu'il oublie aussitôt. Pas les pseudonymes, pas les mots de passe,
pas les messages, pas la clé. Les photos font exception : il les vérifie au
passage, en mémoire, puis les oublie. Comme tout serveur web, il voit les
adresses IP qui se connectent.

## Mettre Alley sur Heroku

[![Deploy to Heroku](https://www.herokucdn.com/deploy/button.svg)](https://heroku.com/deploy?template=https://github.com/maxlestage/Alley)

Heroku compile Alley avec le buildpack Rust
[`emk/rust`](https://github.com/emk/heroku-buildpack-rust) (pile
`heroku-24`). Ce buildpack ne lance qu'un `cargo build --release` ;
[`RustConfig`](RustConfig), qu'il lit d'abord, ajoute les deux étapes
d'avant : le navigateur en WebAssembly (`cargo xtask web`) et les modèles
de modération (`cargo xtask models`, vérifiés par empreinte, gardés dans le
cache de Heroku). [`Procfile`](Procfile) lance le serveur. Le site, les
icônes et le navigateur sont rangés dans le binaire.

Vérifié avec le vrai script du buildpack dans l'image `heroku/heroku:24-build`,
depuis un cache vide : la première construction prend une dizaine de minutes
(sous la limite de 15 de Heroku, même sur 2 cœurs), les suivantes beaucoup
moins grâce au cache. En marche, environ 300 Mo, modèles compris : une dyno
**Basic** (512 Mo) suffit ; évitez Eco, qui s'endort et coupe les salons.

**À la main** (une app existante) : *Settings → Buildpacks → Add buildpack* →
`emk/rust`. Si l'app était réglée pour Docker (pile `container`), repassez-la
d'abord sur `heroku-24` : `heroku stack:set heroku-24 -a <votre-app>`. Puis
déployez la branche `master` (*Deploy → GitHub → Deploy Branch*, ou
`git push heroku master`).

**En un clic** : le bouton ci-dessus → choisissez un nom d'app et la région
(Europe) → *Deploy app*. [`app.json`](app.json) règle déjà le buildpack.
Puis, pour que la version GitHub Pages utilise ce serveur :
*Settings → Secrets and variables → Actions → Variables* du dépôt →
`ALLEY_API_URL` = `https://<votre-app>.herokuapp.com`, et *Actions → Pages →
Run workflow*.

**Ou automatiquement à chaque fusion dans `master`** (*Settings → Secrets and
variables → Actions*) :

1. le secret `HEROKU_API_KEY` : Heroku → *Account settings* → *API Key* →
   *Reveal* ;
2. la variable `HEROKU_APP_NAME` : le nom de l'app (créée si elle n'existe
   pas) ; facultatif : `HEROKU_REGION` (`eu` par défaut, ou `us`).

Le workflow [`heroku.yml`](.github/workflows/heroku.yml) (`cargo xtask
heroku`) crée l'app, règle la pile et le buildpack, compile, autorise la
version GitHub Pages (`ALLEY_ALLOWED_ORIGINS`), puis republie Pages avec
l'adresse de l'API : rien d'autre à régler.

L'image Docker ([`Dockerfile`](Dockerfile)) reste là pour les autres
hébergeurs (Render, Fly.io, un serveur à vous).

Sur Heroku, le serveur lit l'adresse des clients derrière le routeur
(`X-Forwarded-For`, la dernière, que le routeur ajoute) : le blocage par IP
vise le vrai client. Le HTTP est redirigé vers le HTTPS (HSTS), et la
WebSocket des salons reçoit un ping toutes les 25 s (le routeur coupe une
connexion muette au bout de 55 s).

## Déploiement

Chaque push est testé ; **une branche verte est fusionnée automatiquement dans
`master`** (sauf `wip/...`), puis l'API part sur Heroku et le site sur GitHub
Pages, les deux en HTTPS.

À faire une fois, depuis GitHub (même sur téléphone) :

1. **GitHub Pages** : *Settings → Pages → Build and deployment → Source :
   GitHub Actions*.
2. **Le serveur**, au choix :
   - **Heroku** : *Settings → Secrets and variables → Actions* : secret
     `HEROKU_API_KEY` (Heroku → *Account settings* → *API Key*) et variable
     `HEROKU_APP_NAME` (l'app est créée si besoin, et réglée pour accepter la
     version GitHub Pages) ;
   - **Render** (offre gratuite) : *New → Blueprint* sur ce dépôt
     ([`render.yaml`](render.yaml)), puis l'adresse
     `https://<service>.onrender.com` dans la variable `ALLEY_API_URL` ;
   - un autre hébergeur Docker : son adresse dans la variable `ALLEY_API_URL`.
3. *Actions → CI → Run workflow* (ou n'importe quel push).

Le serveur a besoin d'environ 400 Mo de mémoire (les modèles de modération).

| Workflow | Rôle |
|---|---|
| [`ci.yml`](.github/workflows/ci.yml) | formatage, clippy, tests, puis `cargo xtask merge` : fusion automatique dans `master` et déploiements |
| [`heroku.yml`](.github/workflows/heroku.yml) | `cargo xtask heroku` : compile sur Heroku (buildpack emk/rust), règle `ALLEY_ALLOWED_ORIGINS`, puis republie Pages |
| [`pages.yml`](.github/workflows/pages.yml) | `cargo xtask api-url`, puis `alley export` sous `/<dépôt>`, publication sur GitHub Pages |

Sans API configurée, l'app GitHub Pages demande l'adresse d'un serveur Alley
sur l'écran d'accueil (elle n'est jamais enregistrée).

## Lancer en local

```sh
rustup target add wasm32-unknown-unknown
cargo xtask web                            # le navigateur : client/ → client/pkg/ (WebAssembly)
cargo xtask models                         # les modèles de modération → models/ (empreintes vérifiées)
cargo run                                  # http://localhost:3000
cargo run -- judge photo.jpg               # ce que la modération voit dans une image
BASE_PATH=/Alley ALLEY_API_URL=https://mon-api.herokuapp.com cargo run -- export dist
```

| Variable | Rôle |
|---|---|
| `PORT` | le port (3000 par défaut) |
| `ALLEY_ALLOWED_ORIGINS` | les sites autorisés à appeler l'API (`https://maxlestage.github.io` par défaut) |
| `ALLEY_ICE_SERVERS` | serveurs STUN/TURN en plus (JSON `RTCIceServer[]`) |
| `ALLEY_MODELS` | le dossier des modèles (`models/` par défaut) ; sans eux, les photos sont refusées |
| `ALLEY_BAN_DAYS` | durée d'un blocage d'adresse (30 jours par défaut) |
| `ALLEY_BEHIND_PROXY` | lire l'adresse du client dans `X-Forwarded-For` (automatique sur Heroku, Render, Fly.io) |

Le chiffrement du navigateur (WebCrypto) n'existe qu'en HTTPS (ou sur
`localhost`) : derrière Heroku, le serveur redirige HTTP vers HTTPS, et l'app
renvoie vers la version sécurisée si on l'ouvre en HTTP.

## Tests

```sh
cargo test --workspace    # salons (minuit, départ du dernier, recherche, marques des comptes),
                          # vrais salons en WebSocket (remise de la clé, clair refusé, rien de gardé,
                          # photo vérifiée puis relayée, photo de drogue → adresse bloquée),
                          # filtre de texte en trois langues, pages en trois langues, CORS, HTTPS,
                          # export Pages, et le chiffrement recoupé avec les vecteurs de Brocolis
```

Les tests du serveur chiffrent avec le code même du navigateur
(`alley_web::crypto`, compilé ici sans WebAssembly).

Testé aussi dans de vrais navigateurs (Chromium, ordinateur et iPhone émulé) :
connexion, salons retrouvés depuis un autre appareil (et pas avec un autre mot
de passe), app en anglais et en espagnol, changement de langue, hall,
recherche, création, aucun historique pour qui arrive, même empreinte de clé,
message interdit refusé, photo vérifiée puis reçue, photo de drogue → écran
« adresse bloquée » et requêtes refusées, appel audio et vidéo, vocal, salon
disparu au départ du dernier, et écran vidé à minuit (horloge simulée).

## Organisation

| Fichier | Rôle |
|---|---|
| `src/lib.rs` | l'`App` Paquets : bouclier (HTTPS, CORS, CSP…), API, pages, fichiers |
| `src/protocol.rs` | ce que le serveur vérifie des jetons et des clés |
| `src/rooms.rs` | les salons, en mémoire : présence, relais, minuit |
| `src/api.rs` | les routes de l'API et la WebSocket des salons |
| `src/pages.rs`, `src/i18n.rs` | la vitrine et la coquille de l'app, en trois langues |
| `src/moderation/` | la vérification des images (modèles ONNX exécutés par tract), le blocage des adresses |
| `moderation/` | le filtre de texte, partagé par le serveur et le navigateur |
| `src/style/` | les styles, écrits en Rust et rendus en CSS au démarrage |
| `src/assets.rs` | fichiers du navigateur (WebAssembly compris, adresses versionnées) et manifeste |
| `src/export.rs` | l'export statique pour GitHub Pages |
| `build.rs` | range `client/pkg/` dans le binaire |
| `client/src/crypto.rs` | AES-256-GCM, X25519 + HKDF (format Brocolis) |
| `client/src/app.rs`, `model.rs`, `net.rs` | l'application : hall, salons, remise de la clé, messages, minuit |
| `client/src/account.rs` | la connexion : graine PBKDF2 (WebCrypto), marque, gestionnaire de mots de passe |
| `client/src/i18n.rs` | les textes de l'application, en trois langues |
| `client/src/ui.rs`, `dom.rs` | l'affichage, construit en Rust (aucun HTML injecté) |
| `client/src/call.rs` | les appels WebRTC |
| `client/src/site.rs` | la vitrine (installation selon l'appareil) |
| `web/icons/`, `web/favicon.svg` | les icônes |
| `xtask/` | `cargo xtask web` (le navigateur), `models` (les modèles), `merge`, `heroku`, `pages-enabled`, `api-url` (GitHub Actions), `prompts` (les empreintes CLIP) |
| `tests/fixtures/` | une image ordinaire et une image montrant de la drogue, pour les tests |
