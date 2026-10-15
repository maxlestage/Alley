# Alley pour iPhone et iPad

L'application iOS d'Alley, en Swift et SwiftUI (iOS 17 et plus). Elle parle le même protocole que le navigateur : un iPhone et un ordinateur se retrouvent dans les mêmes salons, avec la même empreinte de clé.

## Ce qu'elle fait

- **L'accueil** : un pseudonyme et un mot de passe facultatif. Le mot de passe ne quitte jamais l'iPhone (PBKDF2-SHA256, 310 000 tours, puis HKDF) : seule la marque du compte est envoyée, la même que dans le navigateur. Le trousseau d'iOS peut le retenir.
- **Le hall** :
  - *Créer un salon* et *Rejoindre par lien* (bouton Coller) ;
  - vos salons : ceux que garde l'iPhone, jusqu'à leur fin, et ceux du compte ;
  - les salons publics, avec recherche et tirer pour rafraîchir ;
  - les réglages et la déconnexion.
- **Le salon** :
  - messages chiffrés de bout en bout et signés, en Markdown (gras, italique, code, liens) ;
  - réponses, modification, suppression ;
  - **réactions** 👍 ❤️ 😂 😮 😢 🙏 et sélecteur d'emoji ;
  - **photos** : réduites à 1600 px et sans leurs métadonnées, vérifiées par le serveur comme dans le navigateur ;
  - lecture des messages vocaux.
  - **Menu de chaque message** (appui long) : réagir, répondre, copier, modifier, supprimer, signaler.
  - **Présents** : l'administrateur 👑, exclure, l'empreinte de la clé.
  - **Réglages du salon** : lien d'invitation (copier, partager), sourdine, renommer (administrateur), à propos, signaler, quitter.
  - Un bouton pour revenir aux derniers messages.
- **Les réglages** : thème (auto, clair, sombre), langue (français, anglais, espagnol), notifications, son, tout oublier, aide et pages légales.
- **Ce que garde l'iPhone** : la liste des salons et les réglages, jamais un message. Ces données sont signées (HMAC-SHA256) par une clé de l'appareil, gardée dans le trousseau, et par celle du compte. Chaque salon garde aussi la signature du serveur. Ce qui a été modifié hors d'Alley est écarté.
- **Le filtre de texte**, le même que celui du serveur et du navigateur : un message interdit n'est pas envoyé, et un message reçu interdit est masqué.

- **Appels audio et vidéo** (WebRTC, de pair à pair, chiffrés DTLS-SRTP), avec le même protocole que le navigateur : un iPhone appelle un ordinateur.
  - Ce qui transite par le serveur (l'offre, la réponse, les candidats ICE) est chiffré et signé pour un seul membre.
  - Micro, caméra avant ou arrière, haut-parleur, vidéos des membres, bandeau pour rejoindre un appel en cours.
- **Messages vocaux** : enregistrés en AAC (2 min au plus), lisibles partout. Ceux enregistrés par Chrome ou Firefox (WebM/Ogg Opus) sont lus par WebKit.
- **Périodes payantes** en achats intégrés (StoreKit 2) : 3 jours, 7 jours, 1 mois, 3 mois, payés une fois, aux mêmes prix que sur le site (4,99 €, 9,99 €, 14,99 €, 34,99 €).
  - L'achat est lié au compte Alley par son `appAccountToken` (même calcul que le serveur).
  - Le serveur vérifie lui-même la transaction signée par l'App Store, jusqu'à l'*Apple Root CA - G3* (`src/apple.rs`).
  - Une période achetée sur le site vaut aussi sur l'iPhone, et inversement.

Partager son écran depuis l'iPhone n'est pas proposé : il faudrait une extension ReplayKit. L'iPhone voit en revanche l'écran partagé par un ordinateur.

## Organisation

| Dossier | Rôle |
|---|---|
| `Sources/AlleyKit/` | le cœur, sans interface, testé aussi sous Linux : `Brocolis.swift` (AES-256-GCM, X25519 + HKDF, Ed25519, enveloppes signées, anti-rejeu), `Account.swift`, `Moderation.swift`, `API.swift`, `RoomSession.swift` (la WebSocket, la remise de la clé, les messages), `Store.swift` (coffre signé, liens) |
| `Tests/AlleyKitTests/` | les tests. `vectors.json` est produit par le code Rust du navigateur (`cargo run -p alley-web --example vectors`) : AlleyKit doit l'ouvrir |
| `Alley/` | l'application SwiftUI : écrans, textes en trois langues, trousseau, notifications, icône |
| `project.yml` | le projet Xcode, pour [XcodeGen](https://github.com/yonaskolb/XcodeGen) |
| `ci/` | l'envoi sur TestFlight depuis GitHub Actions (`asc.py` : App Store Connect ; `ExportOptions.plist`) |
| `interop.mjs` | un navigateur (Playwright) et AlleyKit dans le même salon |

## Sans ordinateur : TestFlight depuis l'iPhone

Un Mac de GitHub compile l'application, la signe et l'envoie sur TestFlight (workflow `TestFlight`). Vous l'installez ensuite avec l'app TestFlight. Tout se fait depuis l'iPhone, dans Safari.

1. **Compte Apple Developer** (99 € par an). Dans l'app *Apple Developer* de l'iPhone : *Compte* → *S'inscrire*.
2. **Clé d'API.** Sur [appstoreconnect.apple.com](https://appstoreconnect.apple.com), allez dans *Utilisateurs et accès* → *Intégrations* → *Clés API App Store Connect* → **+**. Donnez-lui un nom, l'accès *Admin*, puis *Générer*.
   - Notez l'**Issuer ID** (en haut) et l'**ID de la clé**.
   - Téléchargez la clé : un fichier `AuthKey_….p8`, dans l'app *Fichiers*. Ce téléchargement n'est possible qu'une fois.
   - Pour en copier le texte : dans *Fichiers*, renommez-le en `.txt`, ouvrez-le, puis *Tout sélectionner* → *Copier*.
3. **Secrets GitHub.** Sur github.com, dans le dépôt Alley : *Settings* → *Secrets and variables* → *Actions* → *New repository secret*. Créez-en trois :
   - `ASC_KEY_ID` : l'ID de la clé ;
   - `ASC_ISSUER_ID` : l'Issuer ID ;
   - `ASC_KEY` : tout le texte du fichier, de `-----BEGIN PRIVATE KEY-----` à `-----END PRIVATE KEY-----`.
4. **Premier lancement.** *Actions* → *TestFlight* → *Run workflow*. Il enregistre l'identifiant `com.maxlestage.alley`, puis s'arrête en demandant de créer l'app.
5. **L'app dans App Store Connect.** *Apps* → **+** → *Nouvelle app* :
   - plateforme iOS, nom « Alley », langue Français ;
   - identifiant de bundle `com.maxlestage.alley`, SKU `alley`.

   Puis relancez le workflow. Il compile, signe et envoie l'app, et ajoute toute l'équipe aux testeurs internes.
6. **Sur l'iPhone.** Installez l'app *TestFlight* depuis l'App Store. Après le traitement par Apple (10 à 30 minutes), Alley y apparaît : *Installer*.
   - Si App Store Connect pose la question du chiffrement sur la version : Alley chiffre les messages de bout en bout avec des algorithmes standard (AES-GCM, X25519, Ed25519).

Ensuite, chaque modification de `ios/` fusionnée dans `master` envoie automatiquement une nouvelle version sur TestFlight.

**Achats intégrés.** Le workflow crée lui-même les quatre périodes dans App Store Connect : prix, textes en trois langues, disponibilité et capture pour la revue d'Apple. Il reste deux choses à faire vous-même, depuis l'iPhone :
- signer l'accord *Apps payantes*, dans *Accords, taxes et banques* ;
- renseigner votre compte bancaire et vos informations fiscales.

Sur TestFlight, les achats sont gratuits : c'est le bac à sable d'Apple. Le serveur les accepte, pour pouvoir tester. Une fois l'app publiée, mettez `ALLEY_APPLE_SANDBOX=0` sur Heroku.

## Ouvrir dans Xcode

```sh
brew install xcodegen
cd ios && xcodegen && open Alley.xcodeproj
```

Choisissez votre équipe dans *Signing & Capabilities*, puis lancez sur un simulateur ou sur votre iPhone. Le serveur et l'adresse de contact (signalements) se règlent dans `Alley/Info.plist` (`AlleyServer`, `AlleyContact`).

Avant de publier sur l'App Store :
- Changez l'identifiant `com.maxlestage.alley` dans `project.yml` si besoin.
- Répondez à la question sur le chiffrement dans App Store Connect : Alley utilise des algorithmes standard (AES-GCM, X25519, Ed25519) pour chiffrer les messages de bout en bout.

## Tests

```sh
cd ios && swift test                     # chiffrement recoupé avec le navigateur, filtre, coffre
ALLEY_SERVER=http://localhost:3999 swift test   # + deux iPhones dans un salon, sur un vrai serveur
ALLEY_SERVER=http://localhost:3999 node interop.mjs   # + un navigateur et un iPhone ensemble
```

Le workflow GitHub `iOS` lance `swift test`, puis compile l'application pour le simulateur sur un Mac.
