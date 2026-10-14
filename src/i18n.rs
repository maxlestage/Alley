//! Les pages d'Alley en trois langues : français (`/`), anglais (`/en/`) et
//! espagnol (`/es/`). Chaque texte existe dans les trois : un champ oublié ne
//! compile pas.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Fr,
    En,
    Es,
}

pub const LANGS: [Lang; 3] = [Lang::Fr, Lang::En, Lang::Es];

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::Fr => "fr",
            Lang::En => "en",
            Lang::Es => "es",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Lang::Fr => "Français",
            Lang::En => "English",
            Lang::Es => "Español",
        }
    }

    /// Le préfixe des adresses : `""`, `"/en"`, `"/es"`.
    pub fn prefix(self) -> &'static str {
        match self {
            Lang::Fr => "",
            Lang::En => "/en",
            Lang::Es => "/es",
        }
    }

    /// Une adresse dans cette langue : `/app/` → `/en/app/`.
    pub fn path(self, path: &str) -> String {
        format!("{}{path}", self.prefix())
    }

    pub fn texts(self) -> &'static Texts {
        match self {
            Lang::Fr => &FR,
            Lang::En => &EN,
            Lang::Es => &ES,
        }
    }
}

pub type Card = (&'static str, &'static str, &'static str);
pub type Pair = (&'static str, &'static str);

pub struct Texts {
    pub locale: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub language: &'static str,
    pub home_label: &'static str,
    pub nav_features: &'static str,
    pub nav_security: &'static str,
    pub nav_install: &'static str,
    pub open_app: &'static str,
    pub eyebrow: &'static str,
    pub headline: &'static str,
    pub lead: &'static str,
    pub cta_enter: &'static str,
    pub cta_how: &'static str,
    pub hero_hint: &'static str,
    pub demo_you: &'static str,
    pub demo_messages: [&'static str; 3],
    pub demo_server: &'static str,
    pub features_title: &'static str,
    pub features_lead: &'static str,
    pub features: [Card; 8],
    pub security_title: &'static str,
    pub steps: [Pair; 5],
    pub tech: [Card; 3],
    pub platforms_title: &'static str,
    pub platforms_lead: &'static str,
    pub install: &'static str,
    pub platforms: [Pair; 6],
    pub faq_title: &'static str,
    pub faq: [Pair; 7],
    pub footer_made: &'static str,
    pub footer_server: &'static str,
    pub app_loading: &'static str,
    pub noscript: &'static str,
    pub notfound_title: &'static str,
    pub notfound_lead: &'static str,
    pub notfound_back: &'static str,
    pub manifest_name: &'static str,
    pub manifest_description: &'static str,
}

pub static FR: Texts = Texts {
    locale: "fr_FR",
    title: "Alley — salons éphémères, chiffrés de bout en bout",
    description: "Alley : des salons de discussion éphémères, chiffrés de bout en bout. Un pseudonyme suffit ; aucun message n'est enregistré, et tout disparaît à minuit. Messages, photos, vocaux et appels vidéo, sur iPhone, Android, Windows, macOS et Linux.",
    language: "Langue",
    home_label: "Alley, accueil",
    nav_features: "Fonctionnalités",
    nav_security: "Sécurité",
    nav_install: "Installer",
    open_app: "Ouvrir Alley",
    eyebrow: "🔒 Chiffré et signé de bout en bout · Aucun message enregistré · Gratuit",
    headline: "On parle. À minuit, tout s'efface.",
    lead: "Alley, ce sont des salons de discussion légers : un pseudonyme et c'est parti. Un salon reste ouvert jusqu'à minuit, et vous y revenez d'un geste ; personne ne voit ce qui s'est dit avant son arrivée, et à minuit il ne reste rien — ni sur le serveur, ni sur votre téléphone.",
    cta_enter: "Entrer dans Alley",
    cta_how: "Comment ça marche",
    hero_hint: "Dans le navigateur, et installable comme une app sur iPhone, iPad, Android, Windows, macOS, Linux et ChromeOS.",
    demo_you: "# soirée-jeux — ce que vous voyez",
    demo_messages: [
        "On se retrouve à 20 h pour la session ?",
        "Ok ! Et à minuit, plus rien 🌙",
        "Parfait, personne d'autre ne lira ça 😌",
    ],
    demo_server: "ce que voit le serveur (et oublie aussitôt)",
    features_title: "Simple, vivant, sans traces",
    features_lead: "Des salons où l'on se retrouve, sans historique et sans rien qui traîne.",
    features: [
        (
            "🎭",
            "Un pseudonyme suffit",
            "Pas d'e-mail, pas de numéro. Entrez avec un pseudonyme, ou ajoutez un mot de passe que votre navigateur retient.",
        ),
        (
            "🔑",
            "Vos salons vous suivent",
            "Les salons que vous créez ou visitez restent dans votre hall jusqu'à minuit, avec leur serveur : vous y revenez d'un geste. Connecté, ils vous suivent sur tous vos appareils ; votre mot de passe ne quitte jamais le vôtre.",
        ),
        (
            "🔎",
            "Cherchez ou créez",
            "La liste des salons ouverts, une barre de recherche, et si votre salon n'existe pas encore, il est créé d'un geste. Ou privé, seulement par lien. Vous en êtes l'administrateur 👑 : vous excluez, vous modérez, et avec un compte vous le restez.",
        ),
        (
            "🌱",
            "Des salons où revenir",
            "Un salon reste ouvert jusqu'à minuit, même quand tout le monde est parti : revenez quand vous voulez. Les messages, eux, ne sont jamais gardés.",
        ),
        (
            "🙈",
            "Pas d'historique",
            "En arrivant, vous ne voyez rien de ce qui s'est dit avant vous : le serveur ne garde aucun message, il les relaie et les oublie.",
        ),
        (
            "🌙",
            "Tout s'efface à minuit",
            "À minuit dans votre pays, l'écran se vide ; à minuit dans le pays de son créateur, le salon disparaît.",
        ),
        (
            "🎙️",
            "Photos, vocaux, appels",
            "Envoyez des photos et des messages vocaux, appelez en audio ou en vidéo, partagez votre écran — chiffré, de pair à pair.",
        ),
        (
            "🛡️",
            "Un lieu sain",
            "Le contenu sexuel et la drogue sont interdits. Les photos sont vérifiées avant d'être relayées : une image interdite bloque l'adresse IP de son auteur pendant 30 jours.",
        ),
    ],
    security_title: "Chiffré sur votre appareil, oublié partout",
    steps: [
        (
            "Une clé par salon",
            "Le premier arrivant tire une clé AES-256 au hasard, sur son appareil. Elle n'en sort jamais en clair.",
        ),
        (
            "Remise de la main à la main",
            "Chaque nouvel arrivant présente une clé X25519 jetable ; les présents lui remettent la clé du salon, chiffrée pour lui seul (l'appairage de la messagerie Brocolis).",
        ),
        (
            "Tout est chiffré avant de partir",
            "Messages, pseudonymes, photos, vocaux et mise en relation des appels deviennent des jetons [[BROCOLIS:v1:…]], chiffrés et signés par leur auteur (Ed25519) : le serveur les relaie sans pouvoir ni les lire ni les modifier, et un envoi altéré, rejoué ou attribué à un autre est écarté. Seules les photos lui sont montrées un instant, le temps de vérifier qu'elles sont permises.",
        ),
        (
            "Aucun message gardé",
            "Le serveur ne stocke aucun message : les absents ne reçoivent rien, les retardataires ne voient rien d'avant. Votre mot de passe, lui, ne quitte jamais votre appareil.",
        ),
        (
            "Vérifiez l'empreinte",
            "Chaque salon affiche l'empreinte de sa clé (ex. 4BB0-6F8E-4E3A-7715) : identique chez tous les présents, personne ne s'est intercalé. Survolez un membre pour voir l'empreinte de sa clé de signature.",
        ),
    ],
    tech: [
        (
            "🥦",
            "Le protocole Brocolis",
            "AES-256-GCM et jetons de la messagerie Brocolis, clés remises par X25519 + HKDF-SHA256, empreintes identiques à l'app Brocolis. En plus, chaque envoi est signé (Ed25519) et numéroté.",
        ),
        (
            "📦",
            "Un serveur Paquets",
            "Écrit en Rust avec Paquets : WAF, limites de débit, CSP stricte, HTTPS imposé, HSTS, blocage des adresses. Aucune base de données : tout vit en mémoire, le temps d'un salon.",
        ),
        (
            "🦀",
            "100 % Rust",
            "Le navigateur, les styles, la vérification des images et les outils de construction : tout est écrit en Rust.",
        ),
    ],
    platforms_title: "Sur tous vos appareils",
    platforms_lead: "Alley est une application web installable : la même app, à jour partout, sans passer par un magasin d'applications.",
    install: "Installer Alley",
    platforms: [
        (
            "iPhone et iPad",
            "Dans Safari, touchez Partager puis « Sur l'écran d'accueil ». Alley s'ouvre en plein écran, comme une app, avec ses notifications.",
        ),
        (
            "Android",
            "Dans Chrome, touchez « Installer l'application » (ou ⋮ → Ajouter à l'écran d'accueil).",
        ),
        (
            "Windows",
            "Dans Edge ou Chrome, cliquez sur l'icône d'installation dans la barre d'adresse. Alley rejoint le menu Démarrer.",
        ),
        (
            "macOS",
            "Dans Safari : Fichier → Ajouter au Dock. Dans Chrome ou Edge : l'icône d'installation de la barre d'adresse.",
        ),
        (
            "Linux",
            "Dans Chrome, Chromium ou Edge : l'icône d'installation. Dans Firefox, Alley marche directement dans un onglet.",
        ),
        (
            "ChromeOS",
            "Cliquez sur l'icône d'installation dans la barre d'adresse : Alley rejoint le lanceur d'applications.",
        ),
    ],
    faq_title: "Questions fréquentes",
    faq: [
        (
            "Faut-il créer un compte ?",
            "Non : un pseudonyme suffit. Ajoutez un mot de passe pour retrouver vos salons sur tous vos appareils : votre navigateur le retient, et il ne quitte jamais votre appareil. Le serveur ne connaît ni votre pseudonyme ni votre mot de passe.",
        ),
        (
            "Pourquoi je ne vois pas les anciens messages ?",
            "Parce qu'ils n'existent plus nulle part. Le serveur relaie chaque message aux personnes présentes, puis l'oublie. C'est voulu : ce qui se dit dans Alley reste entre ceux qui étaient là.",
        ),
        (
            "Quand un salon disparaît-il ?",
            "Dès que la dernière personne en sort, et au plus tard à minuit dans le pays de celui qui l'a créé. De votre côté, l'écran se vide aussi à minuit chez vous.",
        ),
        (
            "Que voit le serveur ?",
            "Le nom des salons publics (pour qu'on puisse les chercher) et combien de personnes y sont. Ni les pseudonymes, ni les messages, ni la clé. Les photos font exception : il les vérifie au passage, en mémoire, puis les oublie. Comme tout serveur web, il voit les adresses IP qui se connectent.",
        ),
        (
            "Qu'est-ce qui est interdit ?",
            "Le contenu sexuel et la drogue, en texte comme en image. Un message interdit ne part pas et est masqué à la réception ; une photo à caractère sexuel ou montrant de la drogue bloque l'adresse IP de son auteur pendant 30 jours.",
        ),
        (
            "Les appels sont-ils chiffrés ?",
            "Oui : ils passent de pair à pair en WebRTC (DTLS-SRTP), et leur mise en relation est chiffrée avec la clé du salon.",
        ),
        (
            "Comment inviter quelqu'un dans un salon privé ?",
            "Touchez 🔗 dans le salon : le lien marche tant que le salon existe, c'est-à-dire jusqu'à minuit, même s'il est vide.",
        ),
    ],
    footer_made: "Écrit avec Paquets 📦 · chiffré avec Brocolis 🥦",
    footer_server: "Ce que voit le serveur",
    app_loading: "Ouverture d'Alley…",
    noscript: "Alley chiffre vos messages dans le navigateur : activez JavaScript.",
    notfound_title: "Introuvable",
    notfound_lead: "Cette ruelle ne mène nulle part : la page demandée n'existe pas.",
    notfound_back: "Retour à l'accueil",
    manifest_name: "Alley — messagerie chiffrée",
    manifest_description: "Salons éphémères chiffrés de bout en bout : aucun message enregistré, tout s'efface à minuit.",
};

pub static EN: Texts = Texts {
    locale: "en_US",
    title: "Alley — ephemeral, end-to-end encrypted chat rooms",
    description: "Alley: ephemeral chat rooms, end-to-end encrypted. A nickname is all you need; no message is stored, and everything disappears at midnight. Messages, photos, voice notes and video calls on iPhone, Android, Windows, macOS and Linux.",
    language: "Language",
    home_label: "Alley, home",
    nav_features: "Features",
    nav_security: "Security",
    nav_install: "Install",
    open_app: "Open Alley",
    eyebrow: "🔒 End-to-end encrypted and signed · No message stored · Free",
    headline: "Talk. At midnight, it's all gone.",
    lead: "Alley gives you lightweight chat rooms: pick a nickname and you're in. A room stays open until midnight, and you go back to it in one tap; nobody sees what was said before they arrived, and at midnight nothing is left — not on the server, not on your phone.",
    cta_enter: "Enter Alley",
    cta_how: "How it works",
    hero_hint: "In your browser, and installable as an app on iPhone, iPad, Android, Windows, macOS, Linux and ChromeOS.",
    demo_you: "# game-night — what you see",
    demo_messages: [
        "Shall we meet at 8 pm for the session?",
        "Sure! And at midnight, it's all gone 🌙",
        "Perfect, nobody else will read this 😌",
    ],
    demo_server: "what the server sees (and forgets right away)",
    features_title: "Simple, lively, traceless",
    features_lead: "Rooms where people meet, with no history and nothing left lying around.",
    features: [
        (
            "🎭",
            "A nickname is enough",
            "No email, no phone number. Join with a nickname, or add a password your browser remembers.",
        ),
        (
            "🔑",
            "Your rooms follow you",
            "The rooms you create or visit stay in your lobby until midnight, with their server: you go back in one tap. Signed in, they follow you on all your devices; your password never leaves yours.",
        ),
        (
            "🔎",
            "Search or create",
            "The list of open rooms, a search bar, and if your room doesn't exist yet, it's created in one tap. Or private, by link only. You are its admin 👑: you remove people, you moderate, and with an account you stay admin.",
        ),
        (
            "🌱",
            "Living rooms",
            "A room stays open until midnight, even when everyone has left: come back whenever you like. Messages, however, are never kept.",
        ),
        (
            "🙈",
            "No history",
            "When you arrive, you see nothing of what was said before you: the server keeps no messages, it relays them and forgets them.",
        ),
        (
            "🌙",
            "Everything is erased at midnight",
            "At midnight in your country, your screen is cleared; at midnight in its creator's country, the room disappears.",
        ),
        (
            "🎙️",
            "Photos, voice notes, calls",
            "Send photos and voice messages, make audio or video calls, share your screen — encrypted, peer to peer.",
        ),
        (
            "🛡️",
            "A healthy place",
            "Sexual content and drugs are forbidden. Photos are checked before they are relayed: a forbidden image blocks its author's IP address for 30 days.",
        ),
    ],
    security_title: "Encrypted on your device, forgotten everywhere",
    steps: [
        (
            "One key per room",
            "The first person to arrive draws a random AES-256 key on their device. It never leaves it unencrypted.",
        ),
        (
            "Handed over in person",
            "Each newcomer presents a throwaway X25519 key; the people already there hand over the room key, encrypted for them alone (the pairing of the Brocolis messenger).",
        ),
        (
            "Everything is encrypted before it leaves",
            "Messages, nicknames, photos, voice notes and call setup become [[BROCOLIS:v1:…]] tokens, encrypted and signed by their author (Ed25519): the server relays them without being able to read or alter them, and anything altered, replayed or attributed to someone else is discarded. Only photos are shown to it for a moment, just long enough to check they are allowed.",
        ),
        (
            "No message is kept",
            "The server stores no messages: absent people receive nothing, latecomers see nothing from before. Your password never leaves your device.",
        ),
        (
            "Check the fingerprint",
            "Each room shows its key fingerprint (e.g. 4BB0-6F8E-4E3A-7715): identical for everyone present means nobody is listening in between. Hover a member to see their signing key fingerprint.",
        ),
    ],
    tech: [
        (
            "🥦",
            "The Brocolis protocol",
            "AES-256-GCM and the Brocolis messenger's tokens, keys handed over with X25519 + HKDF-SHA256, fingerprints identical to the Brocolis app. On top of that, everything sent is signed (Ed25519) and numbered.",
        ),
        (
            "📦",
            "A Paquets server",
            "Written in Rust with Paquets: WAF, rate limits, strict CSP, enforced HTTPS, HSTS, address blocking. No database: everything lives in memory, for the lifetime of a room.",
        ),
        (
            "🦀",
            "100% Rust",
            "The browser app, the styles, the image checks and the build tools: everything is written in Rust.",
        ),
    ],
    platforms_title: "On all your devices",
    platforms_lead: "Alley is an installable web app: the same app, up to date everywhere, without going through an app store.",
    install: "Install Alley",
    platforms: [
        (
            "iPhone and iPad",
            "In Safari, tap Share then “Add to Home Screen”. Alley opens full screen, like an app, with notifications.",
        ),
        (
            "Android",
            "In Chrome, tap “Install app” (or ⋮ → Add to Home screen).",
        ),
        (
            "Windows",
            "In Edge or Chrome, click the install icon in the address bar. Alley joins the Start menu.",
        ),
        (
            "macOS",
            "In Safari: File → Add to Dock. In Chrome or Edge: the install icon in the address bar.",
        ),
        (
            "Linux",
            "In Chrome, Chromium or Edge: the install icon. In Firefox, Alley works right in a tab.",
        ),
        (
            "ChromeOS",
            "Click the install icon in the address bar: Alley joins the app launcher.",
        ),
    ],
    faq_title: "Frequently asked questions",
    faq: [
        (
            "Do I need an account?",
            "No: a nickname is enough. Add a password to find your rooms on all your devices: your browser remembers it, and it never leaves your device. The server knows neither your nickname nor your password.",
        ),
        (
            "Why can't I see older messages?",
            "Because they no longer exist anywhere. The server relays each message to the people present, then forgets it. That's the point: what is said in Alley stays between those who were there.",
        ),
        (
            "When does a room disappear?",
            "As soon as the last person leaves, and at the latest at midnight in the country of the person who created it. On your side, your screen is also cleared at your own midnight.",
        ),
        (
            "What does the server see?",
            "The names of public rooms (so people can search for them) and how many people are in them. Not nicknames, not messages, not the key. Photos are the exception: it checks them in passing, in memory, then forgets them. Like any web server, it sees the IP addresses that connect.",
        ),
        (
            "What is forbidden?",
            "Sexual content and drugs, in text as in images. A forbidden message isn't sent and is hidden on arrival; a sexual photo or a photo showing drugs blocks its author's IP address for 30 days.",
        ),
        (
            "Are calls encrypted?",
            "Yes: they go peer to peer over WebRTC (DTLS-SRTP), and their setup is encrypted with the room key.",
        ),
        (
            "How do I invite someone to a private room?",
            "Tap 🔗 in the room: the link works as long as the room exists, that is until midnight, even when it is empty.",
        ),
    ],
    footer_made: "Built with Paquets 📦 · encrypted with Brocolis 🥦",
    footer_server: "What the server sees",
    app_loading: "Opening Alley…",
    noscript: "Alley encrypts your messages in the browser: please enable JavaScript.",
    notfound_title: "Not found",
    notfound_lead: "This alley leads nowhere: the page you asked for doesn't exist.",
    notfound_back: "Back to the home page",
    manifest_name: "Alley — encrypted chat",
    manifest_description: "Ephemeral end-to-end encrypted chat rooms: no message stored, everything is erased at midnight.",
};

pub static ES: Texts = Texts {
    locale: "es_ES",
    title: "Alley — salas efímeras, cifradas de extremo a extremo",
    description: "Alley: salas de chat efímeras, cifradas de extremo a extremo. Basta con un seudónimo; no se guarda ningún mensaje y todo desaparece a medianoche. Mensajes, fotos, notas de voz y videollamadas en iPhone, Android, Windows, macOS y Linux.",
    language: "Idioma",
    home_label: "Alley, inicio",
    nav_features: "Funciones",
    nav_security: "Seguridad",
    nav_install: "Instalar",
    open_app: "Abrir Alley",
    eyebrow: "🔒 Cifrado y firmado de extremo a extremo · Ningún mensaje guardado · Gratis",
    headline: "Hablamos. A medianoche, todo se borra.",
    lead: "Alley son salas de chat ligeras: un seudónimo y listo. Una sala sigue abierta hasta medianoche y vuelves a ella con un toque; nadie ve lo que se dijo antes de su llegada, y a medianoche no queda nada — ni en el servidor, ni en tu teléfono.",
    cta_enter: "Entrar en Alley",
    cta_how: "Cómo funciona",
    hero_hint: "En el navegador, e instalable como una app en iPhone, iPad, Android, Windows, macOS, Linux y ChromeOS.",
    demo_you: "# noche-de-juegos — lo que ves tú",
    demo_messages: [
        "¿Nos vemos a las 20 h para la partida?",
        "¡Vale! Y a medianoche, ya no queda nada 🌙",
        "Perfecto, nadie más leerá esto 😌",
    ],
    demo_server: "lo que ve el servidor (y olvida al instante)",
    features_title: "Sencillo, vivo, sin rastro",
    features_lead: "Salas donde encontrarse, sin historial y sin nada que quede por ahí.",
    features: [
        (
            "🎭",
            "Basta con un seudónimo",
            "Sin correo, sin número de teléfono. Entra con un seudónimo, o añade una contraseña que tu navegador recuerda.",
        ),
        (
            "🔑",
            "Tus salas te siguen",
            "Las salas que creas o visitas se quedan en tu vestíbulo hasta medianoche, con su servidor: vuelves con un toque. Con la sesión iniciada, te siguen en todos tus dispositivos; tu contraseña nunca sale del tuyo.",
        ),
        (
            "🔎",
            "Busca o crea",
            "La lista de salas abiertas, una barra de búsqueda, y si tu sala aún no existe, se crea con un gesto. O privada, solo con enlace. Eres su administrador 👑: expulsas, moderas, y con una cuenta lo sigues siendo.",
        ),
        (
            "🌱",
            "Salas a las que volver",
            "Una sala sigue abierta hasta medianoche, aunque todos se hayan ido: vuelve cuando quieras. Los mensajes, en cambio, nunca se guardan.",
        ),
        (
            "🙈",
            "Sin historial",
            "Al llegar, no ves nada de lo que se dijo antes: el servidor no guarda ningún mensaje, los transmite y los olvida.",
        ),
        (
            "🌙",
            "Todo se borra a medianoche",
            "A medianoche en tu país, la pantalla se vacía; a medianoche en el país de quien la creó, la sala desaparece.",
        ),
        (
            "🎙️",
            "Fotos, notas de voz, llamadas",
            "Envía fotos y mensajes de voz, llama en audio o vídeo, comparte tu pantalla — cifrado, de igual a igual.",
        ),
        (
            "🛡️",
            "Un lugar sano",
            "El contenido sexual y las drogas están prohibidos. Las fotos se verifican antes de transmitirse: una imagen prohibida bloquea la dirección IP de su autor durante 30 días.",
        ),
    ],
    security_title: "Cifrado en tu dispositivo, olvidado en todas partes",
    steps: [
        (
            "Una clave por sala",
            "La primera persona en llegar genera al azar una clave AES-256 en su dispositivo. Nunca sale de él sin cifrar.",
        ),
        (
            "Entregada de mano en mano",
            "Cada recién llegado presenta una clave X25519 desechable; los presentes le entregan la clave de la sala, cifrada solo para él (el emparejamiento de la mensajería Brocolis).",
        ),
        (
            "Todo se cifra antes de salir",
            "Mensajes, seudónimos, fotos, notas de voz y el establecimiento de las llamadas se convierten en fichas [[BROCOLIS:v1:…]], cifradas y firmadas por su autor (Ed25519): el servidor las transmite sin poder ni leerlas ni modificarlas, y todo envío alterado, repetido o atribuido a otro se descarta. Solo las fotos se le muestran un instante, el tiempo de comprobar que están permitidas.",
        ),
        (
            "Ningún mensaje guardado",
            "El servidor no almacena ningún mensaje: los ausentes no reciben nada, los que llegan tarde no ven nada de antes. Tu contraseña nunca sale de tu dispositivo.",
        ),
        (
            "Comprueba la huella",
            "Cada sala muestra la huella de su clave (p. ej. 4BB0-6F8E-4E3A-7715): si es idéntica para todos los presentes, nadie se ha interpuesto. Pasa el cursor sobre un miembro para ver la huella de su clave de firma.",
        ),
    ],
    tech: [
        (
            "🥦",
            "El protocolo Brocolis",
            "AES-256-GCM y las fichas de la mensajería Brocolis, claves entregadas con X25519 + HKDF-SHA256, huellas idénticas a las de la app Brocolis. Además, cada envío va firmado (Ed25519) y numerado.",
        ),
        (
            "📦",
            "Un servidor Paquets",
            "Escrito en Rust con Paquets: WAF, límites de tráfico, CSP estricta, HTTPS obligatorio, HSTS, bloqueo de direcciones. Sin base de datos: todo vive en memoria, lo que dura una sala.",
        ),
        (
            "🦀",
            "100 % Rust",
            "La app del navegador, los estilos, la verificación de imágenes y las herramientas de compilación: todo está escrito en Rust.",
        ),
    ],
    platforms_title: "En todos tus dispositivos",
    platforms_lead: "Alley es una aplicación web instalable: la misma app, actualizada en todas partes, sin pasar por una tienda de aplicaciones.",
    install: "Instalar Alley",
    platforms: [
        (
            "iPhone y iPad",
            "En Safari, toca Compartir y luego «Añadir a pantalla de inicio». Alley se abre a pantalla completa, como una app, con notificaciones.",
        ),
        (
            "Android",
            "En Chrome, toca «Instalar aplicación» (o ⋮ → Añadir a pantalla de inicio).",
        ),
        (
            "Windows",
            "En Edge o Chrome, haz clic en el icono de instalación de la barra de direcciones. Alley se une al menú Inicio.",
        ),
        (
            "macOS",
            "En Safari: Archivo → Añadir al Dock. En Chrome o Edge: el icono de instalación de la barra de direcciones.",
        ),
        (
            "Linux",
            "En Chrome, Chromium o Edge: el icono de instalación. En Firefox, Alley funciona directamente en una pestaña.",
        ),
        (
            "ChromeOS",
            "Haz clic en el icono de instalación de la barra de direcciones: Alley se une al lanzador de aplicaciones.",
        ),
    ],
    faq_title: "Preguntas frecuentes",
    faq: [
        (
            "¿Hace falta crear una cuenta?",
            "No: basta con un seudónimo. Añade una contraseña para encontrar tus salas en todos tus dispositivos: tu navegador la recuerda y nunca sale de tu dispositivo. El servidor no conoce ni tu seudónimo ni tu contraseña.",
        ),
        (
            "¿Por qué no veo los mensajes antiguos?",
            "Porque ya no existen en ningún sitio. El servidor transmite cada mensaje a las personas presentes y luego lo olvida. Es a propósito: lo que se dice en Alley queda entre quienes estaban allí.",
        ),
        (
            "¿Cuándo desaparece una sala?",
            "En cuanto sale la última persona, y como muy tarde a medianoche en el país de quien la creó. Por tu parte, la pantalla también se vacía a tu medianoche.",
        ),
        (
            "¿Qué ve el servidor?",
            "El nombre de las salas públicas (para poder buscarlas) y cuántas personas hay en ellas. Ni los seudónimos, ni los mensajes, ni la clave. Las fotos son la excepción: las verifica de paso, en memoria, y luego las olvida. Como cualquier servidor web, ve las direcciones IP que se conectan.",
        ),
        (
            "¿Qué está prohibido?",
            "El contenido sexual y las drogas, tanto en texto como en imagen. Un mensaje prohibido no se envía y se oculta al recibirlo; una foto de carácter sexual o que muestra drogas bloquea la dirección IP de su autor durante 30 días.",
        ),
        (
            "¿Las llamadas están cifradas?",
            "Sí: van de igual a igual por WebRTC (DTLS-SRTP), y su establecimiento se cifra con la clave de la sala.",
        ),
        (
            "¿Cómo invito a alguien a una sala privada?",
            "Toca 🔗 en la sala: el enlace funciona mientras la sala exista, es decir, hasta medianoche, aunque esté vacía.",
        ),
    ],
    footer_made: "Hecho con Paquets 📦 · cifrado con Brocolis 🥦",
    footer_server: "Lo que ve el servidor",
    app_loading: "Abriendo Alley…",
    noscript: "Alley cifra tus mensajes en el navegador: activa JavaScript.",
    notfound_title: "No encontrado",
    notfound_lead: "Este callejón no lleva a ninguna parte: la página que buscas no existe.",
    notfound_back: "Volver al inicio",
    manifest_name: "Alley — mensajería cifrada",
    manifest_description: "Salas efímeras cifradas de extremo a extremo: ningún mensaje guardado, todo se borra a medianoche.",
};
