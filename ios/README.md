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

Pas encore dans l'application : les appels audio et vidéo (WebRTC), l'enregistrement de messages vocaux et le paiement des périodes.

Une période payée sur le site s'applique bien aux salons créés depuis l'iPhone avec le même compte. Mais l'App Store impose son propre système d'achat pour vendre des contenus numériques dans une application : le paiement Stripe n'y est donc pas proposé.

## Organisation

| Dossier | Rôle |
|---|---|
| `Sources/AlleyKit/` | le cœur, sans interface, testé aussi sous Linux : `Brocolis.swift` (AES-256-GCM, X25519 + HKDF, Ed25519, enveloppes signées, anti-rejeu), `Account.swift`, `Moderation.swift`, `API.swift`, `RoomSession.swift` (la WebSocket, la remise de la clé, les messages), `Store.swift` (coffre signé, liens) |
| `Tests/AlleyKitTests/` | les tests. `vectors.json` est produit par le code Rust du navigateur (`cargo run -p alley-web --example vectors`) : AlleyKit doit l'ouvrir |
| `Alley/` | l'application SwiftUI : écrans, textes en trois langues, trousseau, notifications, icône |
| `project.yml` | le projet Xcode, pour [XcodeGen](https://github.com/yonaskolb/XcodeGen) |
| `interop.mjs` | un navigateur (Playwright) et AlleyKit dans le même salon |

## Ouvrir dans Xcode

```sh
brew install xcodegen
cd ios && xcodegen && open Alley.xcodeproj
```

Choisissez votre équipe dans *Signing & Capabilities*, puis lancez sur un simulateur ou sur votre iPhone. Le serveur et l'adresse de contact (signalements) se règlent dans `Alley/Info.plist` (`AlleyServer`, `AlleyContact`).

Avant de publier sur l'App Store :
- Changez l'identifiant `app.alley.ios` dans `project.yml` si besoin.
- Répondez à la question sur le chiffrement dans App Store Connect : Alley utilise des algorithmes standard (AES-GCM, X25519, Ed25519) pour chiffrer les messages de bout en bout.

## Tests

```sh
cd ios && swift test                     # chiffrement recoupé avec le navigateur, filtre, coffre
ALLEY_SERVER=http://localhost:3999 swift test   # + deux iPhones dans un salon, sur un vrai serveur
ALLEY_SERVER=http://localhost:3999 node interop.mjs   # + un navigateur et un iPhone ensemble
```

Le workflow GitHub `iOS` lance `swift test`, puis compile l'application pour le simulateur sur un Mac.
