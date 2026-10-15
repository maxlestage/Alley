// Les textes de l'application, en français, anglais et espagnol — les mêmes
// que ceux du navigateur (`client/src/i18n.rs`). `{}` marque une valeur.

import Foundation

enum Lang: String, CaseIterable, Identifiable {
    case fr, en, es

    var id: String { rawValue }

    var name: String {
        switch self {
        case .fr: "Français"
        case .en: "English"
        case .es: "Español"
        }
    }

    /// Celle de l'iPhone si Alley la parle, sinon le français.
    static var device: Lang {
        for code in Locale.preferredLanguages {
            if let l = Lang(rawValue: String(code.prefix(2))) { return l }
        }
        return .fr
    }

    var t: L10n {
        switch self {
        case .fr: .fr
        case .en: .en
        case .es: .es
        }
    }
}

/// Remplace chaque `{}` par la valeur suivante.
func fill(_ text: String, _ values: String...) -> String {
    var out = text
    for v in values {
        guard let r = out.range(of: "{}") else { break }
        out.replaceSubrange(r, with: v)
    }
    return out
}

struct L10n {
    var tagline, pseudonym, pseudonymPlaceholder, password, passwordHint, signIn, guest, accept, terms, rules: String
    var pseudonymTooShort, passwordTooShort, signingIn: String
    var lobbyTitle, lobbyIntro, search, myRooms, myRoomsEmpty, noRooms, forgetAll, forget, forgottenAll: String
    var newRoom, joinRoom, settings, logout, changePseudonym, roomName, newRoomPlaceholder, newRoomHint: String
    var privateRoom, privateRoomHint, create, join, joinLabel, joinPlaceholder, joinHint, joinInvalid, paste: String
    var people, closesAt, cancel, close, done: String
    var roomWelcome, roomWelcomeBody, receivingKey, messagePlaceholder, send, photo, voice: String
    var reply, copy, edit, delete, react, report, reportMessage, confirmDelete, edited, beforeArrival, typingOne, typingMany: String
    var members, you, admin, expel, confirmExpel, fingerprint, fingerprintHint: String
    var roomSettings, invite, copyLink, linkCopied, share, mute, muteHint, renameTitle, rename, about, closes, role, roleAdmin, roleMember, leave: String
    var roomPrivateNote, roomPublicNote: String
    var account, signedInNote, guestNote, display, theme, themeAuto, themeLight, themeDark, language: String
    var alerts, notifications, notificationsHint, sound, soundHint, device, deviceNote, help, helpLink, privacy, legal, contact, confirmForgetAll: String
    var guestName, hiddenMessage, nowAdmin, renamedTo, renamedByMe, nameTaken, forbiddenName, badName: String
    var signatureRejected, refusedSexual, refusedDrugs, keyNotArrived, noKeyRetry, reconnecting: String
    var photoRefusedSexual, photoRefusedDrugs, photoInvalid, photoUnavailable, photoSlowDown, checking: String
    var closedMidnight, closedExpelled, closedBanned, closedGone, connectionLost: String
    var errNetwork, errGeneric, errRate, errTooManyRooms, storageTampered, banned: String
    var encrypted, reportSubject, reportBody, reportMessageBody, toBottom: String
}

extension L10n {
    static let fr = L10n(
        tagline: "Discutez en salons, chiffré de bout en bout. Rien n'est gardé : à minuit, tout disparaît.",
        pseudonym: "Pseudonyme", pseudonymPlaceholder: "Votre pseudonyme", password: "Mot de passe (facultatif)",
        passwordHint: "Avec un mot de passe, vos salons vous suivent sur tous vos appareils. Il ne quitte jamais cet iPhone : enregistrez-le dans votre trousseau.",
        signIn: "Se connecter", guest: "Entrer sans compte",
        accept: "En entrant, vous acceptez les conditions d'utilisation et les règles d'Alley.", terms: "Conditions", rules: "Règles",
        pseudonymTooShort: "Choisissez un pseudonyme (2 à 32 caractères).", passwordTooShort: "Le mot de passe doit faire 8 caractères au moins.",
        signingIn: "Connexion…",
        lobbyTitle: "Salons ouverts",
        lobbyIntro: "Aucun message n'est enregistré, et tout ce qui s'envoie est chiffré et signé. Un salon reste ouvert jusqu'à minuit, même vide.",
        search: "Chercher un salon", myRooms: "Vos salons",
        myRoomsEmpty: "Les salons que vous créez ou visitez s'afficheront ici jusqu'à leur minuit, pour y revenir d'un geste.",
        noRooms: "Aucun salon ouvert pour l'instant : créez le premier !", forgetAll: "Tout oublier", forget: "Oublier",
        forgottenAll: "Les salons gardés sur cet iPhone sont oubliés.",
        newRoom: "Créer un salon", joinRoom: "Rejoindre par lien", settings: "Réglages", logout: "Se déconnecter",
        changePseudonym: "Changer de pseudonyme", roomName: "Nom du salon", newRoomPlaceholder: "Ex. : Soirée jeux, Projet Atlas…",
        newRoomHint: "2 à 40 caractères. Le salon ferme à minuit, ou à la fin de votre période payée.",
        privateRoom: "Salon privé", privateRoomHint: "Invisible dans la liste, seulement par lien.", create: "Créer", join: "Rejoindre",
        joinLabel: "Lien d'invitation ou identifiant", joinPlaceholder: "https://…#salon=…",
        joinHint: "Collez le lien reçu : il contient « salon= ».", joinInvalid: "Ce n'est pas un lien de salon Alley.", paste: "Coller",
        people: "{} présent(s)", closesAt: "disparaît {}", cancel: "Annuler", close: "Fermer", done: "OK",
        roomWelcome: "Bienvenue dans #{}",
        roomWelcomeBody: "Vous ne voyez que ce qui se dit à partir de maintenant : aucun message n'est gardé. Tout est chiffré de bout en bout et signé par son auteur.",
        receivingKey: "🔑 Réception de la clé du salon…", messagePlaceholder: "Message chiffré", send: "Envoyer", photo: "📷 Photo", voice: "🎤 Message vocal",
        reply: "Répondre", copy: "Copier le texte", edit: "Modifier", delete: "Supprimer", react: "Réagir", report: "Signaler un abus",
        reportMessage: "Signaler ce message", confirmDelete: "Supprimer ce message pour tout le monde ?", edited: "(modifié)",
        beforeArrival: "un message d'avant votre arrivée", typingOne: "{} écrit…", typingMany: "{} écrivent…",
        members: "Présents", you: "vous", admin: "administrateur", expel: "Exclure", confirmExpel: "Exclure {} du salon ?",
        fingerprint: "Empreinte de la clé", fingerprintHint: "Identique chez tous les présents : personne ne s'est intercalé.",
        roomSettings: "Réglages du salon", invite: "Inviter", copyLink: "Copier le lien", linkCopied: "Lien copié", share: "Partager",
        mute: "Sourdine", muteHint: "Ni son ni notification pour ce salon sur cet iPhone.", renameTitle: "Renommer le salon", rename: "Renommer",
        about: "À propos", closes: "Ferme", role: "Votre rôle", roleAdmin: "Administrateur", roleMember: "Membre", leave: "Quitter le salon",
        roomPrivateNote: "Salon privé : seules les personnes qui ont ce lien peuvent entrer.",
        roomPublicNote: "Salon public : il apparaît aussi dans la recherche du hall.",
        account: "Compte", signedInNote: "Connecté : votre clé signe ce que garde cet iPhone.", guestNote: "Invité : rien n'est gardé à votre nom.",
        display: "Affichage", theme: "Thème", themeAuto: "Auto", themeLight: "Clair", themeDark: "Sombre", language: "Langue",
        alerts: "Notifications", notifications: "Notifications", notificationsHint: "Un aperçu des nouveaux messages quand Alley est en arrière-plan.",
        sound: "Son", soundHint: "Un son discret à chaque nouveau message.", device: "Cet iPhone",
        deviceNote: "Alley ne garde ici que la liste de vos salons et ces réglages, signés : jamais un message.",
        help: "Aide et informations", helpLink: "Aide", privacy: "Confidentialité", legal: "Mentions légales", contact: "Contact",
        confirmForgetAll: "Oublier tous les salons gardés sur cet iPhone ?",
        guestName: "Invité {}", hiddenMessage: "Message masqué : contenu interdit.", nowAdmin: "Vous êtes maintenant administrateur de ce salon 👑",
        renamedTo: "Le salon s'appelle maintenant « {} »", renamedByMe: "Salon renommé « {} »", nameTaken: "Un salon public porte déjà ce nom.",
        forbiddenName: "Ce nom n'est pas permis.", badName: "Nom de salon invalide (2 à 40 caractères).",
        signatureRejected: "Un envoi attribué à {} a été écarté : sa signature ne correspond pas.",
        refusedSexual: "Non envoyé : le contenu sexuel est interdit sur Alley.", refusedDrugs: "Non envoyé : la drogue est interdite sur Alley.",
        keyNotArrived: "La clé du salon n'est pas encore arrivée.", noKeyRetry: "Personne n'a remis la clé du salon : nouvelle tentative…",
        reconnecting: "Reconnexion…",
        photoRefusedSexual: "Photo refusée : le contenu sexuel est interdit sur Alley.", photoRefusedDrugs: "Photo refusée : la drogue est interdite sur Alley.",
        photoInvalid: "Photo illisible.", photoUnavailable: "La vérification des photos est indisponible : réessayez plus tard.",
        photoSlowDown: "Une photo à la fois : patientez un instant.", checking: "Vérification…",
        closedMidnight: "Minuit : le salon a disparu, et tout ce qui s'y est dit avec lui.",
        closedExpelled: "Vous avez été exclu de ce salon par son administrateur.",
        closedBanned: "Une image à caractère sexuel ou montrant de la drogue a été envoyée depuis cette connexion : son adresse est bloquée pendant 30 jours.",
        closedGone: "Ce salon n'existe plus, ou il est complet.", connectionLost: "Connexion perdue.",
        errNetwork: "Pas de réseau : vérifiez votre connexion.", errGeneric: "Une erreur est survenue ({}).",
        errRate: "Trop de demandes : patientez un instant.", errTooManyRooms: "Trop de salons ouverts en ce moment : réessayez plus tard.",
        storageTampered: "Des données gardées sur cet iPhone ont été modifiées hors d'Alley : elles ont été écartées.",
        banned: "Cette connexion est bloquée.", encrypted: "Chiffré de bout en bout",
        reportSubject: "Signalement — salon « {} »",
        reportBody: "Salon : {}\nIdentifiant : {}\nDate et heure : {}\n\nCe qui s'est passé :\n\n",
        reportMessageBody: "Message signalé — de {} :\n« {} »\n\n", toBottom: "Revenir aux derniers messages")

    static let en = L10n(
        tagline: "Chat in rooms, end-to-end encrypted. Nothing is kept: at midnight, everything disappears.",
        pseudonym: "Pseudonym", pseudonymPlaceholder: "Your pseudonym", password: "Password (optional)",
        passwordHint: "With a password, your rooms follow you on all your devices. It never leaves this iPhone: save it in your keychain.",
        signIn: "Sign in", guest: "Enter without an account",
        accept: "By entering, you accept Alley's terms of use and rules.", terms: "Terms", rules: "Rules",
        pseudonymTooShort: "Choose a pseudonym (2 to 32 characters).", passwordTooShort: "The password must be at least 8 characters.",
        signingIn: "Signing in…",
        lobbyTitle: "Open rooms",
        lobbyIntro: "No message is stored, and everything sent is encrypted and signed. A room stays open until midnight, even when empty.",
        search: "Search a room", myRooms: "Your rooms",
        myRoomsEmpty: "Rooms you create or visit will show up here until their midnight, so you can go back in one tap.",
        noRooms: "No open rooms right now: create the first one!", forgetAll: "Forget all", forget: "Forget",
        forgottenAll: "Rooms kept on this iPhone are forgotten.",
        newRoom: "Create a room", joinRoom: "Join with a link", settings: "Settings", logout: "Sign out",
        changePseudonym: "Change pseudonym", roomName: "Room name", newRoomPlaceholder: "E.g. Game night, Project Atlas…",
        newRoomHint: "2 to 40 characters. The room closes at midnight, or at the end of your paid period.",
        privateRoom: "Private room", privateRoomHint: "Hidden from the list, only reachable by link.", create: "Create", join: "Join",
        joinLabel: "Invitation link or identifier", joinPlaceholder: "https://…#salon=…",
        joinHint: "Paste the link you received: it contains “salon=”.", joinInvalid: "This isn't an Alley room link.", paste: "Paste",
        people: "{} here", closesAt: "disappears {}", cancel: "Cancel", close: "Close", done: "OK",
        roomWelcome: "Welcome to #{}",
        roomWelcomeBody: "You only see what is said from now on: no message is kept. Everything is end-to-end encrypted and signed by its author.",
        receivingKey: "🔑 Receiving the room key…", messagePlaceholder: "Encrypted message", send: "Send", photo: "📷 Photo", voice: "🎤 Voice message",
        reply: "Reply", copy: "Copy text", edit: "Edit", delete: "Delete", react: "React", report: "Report abuse",
        reportMessage: "Report this message", confirmDelete: "Delete this message for everyone?", edited: "(edited)",
        beforeArrival: "a message from before you arrived", typingOne: "{} is typing…", typingMany: "{} are typing…",
        members: "Here now", you: "you", admin: "admin", expel: "Remove", confirmExpel: "Remove {} from the room?",
        fingerprint: "Key fingerprint", fingerprintHint: "The same for everyone here: nobody has slipped in between.",
        roomSettings: "Room settings", invite: "Invite", copyLink: "Copy link", linkCopied: "Link copied", share: "Share",
        mute: "Mute", muteHint: "No sound or notification for this room on this iPhone.", renameTitle: "Rename the room", rename: "Rename",
        about: "About", closes: "Closes", role: "Your role", roleAdmin: "Admin", roleMember: "Member", leave: "Leave the room",
        roomPrivateNote: "Private room: only people with this link can get in.",
        roomPublicNote: "Public room: it also shows up in the lobby search.",
        account: "Account", signedInNote: "Signed in: your key signs what this iPhone keeps.", guestNote: "Guest: nothing is kept under your name.",
        display: "Display", theme: "Theme", themeAuto: "Auto", themeLight: "Light", themeDark: "Dark", language: "Language",
        alerts: "Notifications", notifications: "Notifications", notificationsHint: "A preview of new messages when Alley is in the background.",
        sound: "Sound", soundHint: "A subtle sound for each new message.", device: "This iPhone",
        deviceNote: "Alley only keeps your room list and these settings here, signed: never a message.",
        help: "Help and information", helpLink: "Help", privacy: "Privacy", legal: "Legal notice", contact: "Contact",
        confirmForgetAll: "Forget all the rooms kept on this iPhone?",
        guestName: "Guest {}", hiddenMessage: "Message hidden: forbidden content.", nowAdmin: "You are now the admin of this room 👑",
        renamedTo: "The room is now called “{}”", renamedByMe: "Room renamed “{}”", nameTaken: "A public room already has this name.",
        forbiddenName: "This name isn't allowed.", badName: "Invalid room name (2 to 40 characters).",
        signatureRejected: "Something attributed to {} was discarded: its signature doesn't match.",
        refusedSexual: "Not sent: sexual content is not allowed on Alley.", refusedDrugs: "Not sent: drugs are not allowed on Alley.",
        keyNotArrived: "The room key hasn't arrived yet.", noKeyRetry: "Nobody handed over the room key: trying again…",
        reconnecting: "Reconnecting…",
        photoRefusedSexual: "Photo refused: sexual content is not allowed on Alley.", photoRefusedDrugs: "Photo refused: drugs are not allowed on Alley.",
        photoInvalid: "Unreadable photo.", photoUnavailable: "Photo checking is unavailable right now: please try again later.",
        photoSlowDown: "One photo at a time: please wait a moment.", checking: "Checking…",
        closedMidnight: "Midnight: the room is gone, along with everything said in it.",
        closedExpelled: "You were removed from this room by its admin.",
        closedBanned: "A sexual image or an image showing drugs was sent from this connection: its address is blocked for 30 days.",
        closedGone: "This room no longer exists, or it is full.", connectionLost: "Connection lost.",
        errNetwork: "No network: check your connection.", errGeneric: "Something went wrong ({}).",
        errRate: "Too many requests: please wait a moment.", errTooManyRooms: "Too many rooms open right now: try again later.",
        storageTampered: "Data kept on this iPhone was modified outside Alley: it was discarded.",
        banned: "This connection is blocked.", encrypted: "End-to-end encrypted",
        reportSubject: "Report — room “{}”",
        reportBody: "Room: {}\nIdentifier: {}\nDate and time: {}\n\nWhat happened:\n\n",
        reportMessageBody: "Reported message — from {}:\n“{}”\n\n", toBottom: "Back to the latest messages")

    static let es = L10n(
        tagline: "Chatea en salas, cifrado de extremo a extremo. No se guarda nada: a medianoche, todo desaparece.",
        pseudonym: "Seudónimo", pseudonymPlaceholder: "Tu seudónimo", password: "Contraseña (opcional)",
        passwordHint: "Con una contraseña, tus salas te siguen en todos tus dispositivos. Nunca sale de este iPhone: guárdala en tu llavero.",
        signIn: "Iniciar sesión", guest: "Entrar sin cuenta",
        accept: "Al entrar, aceptas las condiciones de uso y las reglas de Alley.", terms: "Condiciones", rules: "Reglas",
        pseudonymTooShort: "Elige un seudónimo (de 2 a 32 caracteres).", passwordTooShort: "La contraseña debe tener 8 caracteres como mínimo.",
        signingIn: "Iniciando sesión…",
        lobbyTitle: "Salas abiertas",
        lobbyIntro: "No se guarda ningún mensaje, y todo lo que se envía va cifrado y firmado. Una sala sigue abierta hasta medianoche, aunque esté vacía.",
        search: "Buscar una sala", myRooms: "Tus salas",
        myRoomsEmpty: "Las salas que crees o visites aparecerán aquí hasta su medianoche, para volver con un toque.",
        noRooms: "No hay salas abiertas por ahora: ¡crea la primera!", forgetAll: "Olvidar todo", forget: "Olvidar",
        forgottenAll: "Las salas guardadas en este iPhone se han olvidado.",
        newRoom: "Crear una sala", joinRoom: "Unirse con un enlace", settings: "Ajustes", logout: "Cerrar sesión",
        changePseudonym: "Cambiar de seudónimo", roomName: "Nombre de la sala", newRoomPlaceholder: "Ej.: Noche de juegos, Proyecto Atlas…",
        newRoomHint: "De 2 a 40 caracteres. La sala cierra a medianoche, o al final de tu periodo pagado.",
        privateRoom: "Sala privada", privateRoomHint: "Invisible en la lista, solo con enlace.", create: "Crear", join: "Unirse",
        joinLabel: "Enlace de invitación o identificador", joinPlaceholder: "https://…#salon=…",
        joinHint: "Pega el enlace recibido: contiene «salon=».", joinInvalid: "Esto no es un enlace de sala de Alley.", paste: "Pegar",
        people: "{} presente(s)", closesAt: "desaparece {}", cancel: "Cancelar", close: "Cerrar", done: "OK",
        roomWelcome: "Bienvenido a #{}",
        roomWelcomeBody: "Solo ves lo que se dice a partir de ahora: no se guarda ningún mensaje. Todo está cifrado de extremo a extremo y firmado por su autor.",
        receivingKey: "🔑 Recibiendo la clave de la sala…", messagePlaceholder: "Mensaje cifrado", send: "Enviar", photo: "📷 Foto", voice: "🎤 Mensaje de voz",
        reply: "Responder", copy: "Copiar el texto", edit: "Editar", delete: "Eliminar", react: "Reaccionar", report: "Denunciar un abuso",
        reportMessage: "Denunciar este mensaje", confirmDelete: "¿Eliminar este mensaje para todos?", edited: "(editado)",
        beforeArrival: "un mensaje de antes de tu llegada", typingOne: "{} está escribiendo…", typingMany: "{} están escribiendo…",
        members: "Presentes", you: "tú", admin: "administrador", expel: "Expulsar", confirmExpel: "¿Expulsar a {} de la sala?",
        fingerprint: "Huella de la clave", fingerprintHint: "Idéntica para todos los presentes: nadie se ha interpuesto.",
        roomSettings: "Ajustes de la sala", invite: "Invitar", copyLink: "Copiar el enlace", linkCopied: "Enlace copiado", share: "Compartir",
        mute: "Silenciar", muteHint: "Ni sonido ni notificación de esta sala en este iPhone.", renameTitle: "Renombrar la sala", rename: "Renombrar",
        about: "Acerca de", closes: "Cierra", role: "Tu papel", roleAdmin: "Administrador", roleMember: "Miembro", leave: "Salir de la sala",
        roomPrivateNote: "Sala privada: solo quienes tienen este enlace pueden entrar.",
        roomPublicNote: "Sala pública: también aparece en la búsqueda del vestíbulo.",
        account: "Cuenta", signedInNote: "Conectado: tu clave firma lo que guarda este iPhone.", guestNote: "Invitado: no se guarda nada a tu nombre.",
        display: "Pantalla", theme: "Tema", themeAuto: "Auto", themeLight: "Claro", themeDark: "Oscuro", language: "Idioma",
        alerts: "Notificaciones", notifications: "Notificaciones", notificationsHint: "Un avance de los mensajes nuevos cuando Alley está en segundo plano.",
        sound: "Sonido", soundHint: "Un sonido discreto por cada mensaje nuevo.", device: "Este iPhone",
        deviceNote: "Alley solo guarda aquí tu lista de salas y estos ajustes, firmados: nunca un mensaje.",
        help: "Ayuda e información", helpLink: "Ayuda", privacy: "Privacidad", legal: "Aviso legal", contact: "Contacto",
        confirmForgetAll: "¿Olvidar todas las salas guardadas en este iPhone?",
        guestName: "Invitado {}", hiddenMessage: "Mensaje oculto: contenido prohibido.", nowAdmin: "Ahora eres el administrador de esta sala 👑",
        renamedTo: "La sala ahora se llama «{}»", renamedByMe: "Sala renombrada «{}»", nameTaken: "Ya hay una sala pública con este nombre.",
        forbiddenName: "Este nombre no está permitido.", badName: "Nombre de sala no válido (de 2 a 40 caracteres).",
        signatureRejected: "Se descartó un envío atribuido a {}: su firma no coincide.",
        refusedSexual: "No enviado: el contenido sexual está prohibido en Alley.", refusedDrugs: "No enviado: las drogas están prohibidas en Alley.",
        keyNotArrived: "La clave de la sala aún no ha llegado.", noKeyRetry: "Nadie entregó la clave de la sala: nuevo intento…",
        reconnecting: "Reconectando…",
        photoRefusedSexual: "Foto rechazada: el contenido sexual está prohibido en Alley.", photoRefusedDrugs: "Foto rechazada: las drogas están prohibidas en Alley.",
        photoInvalid: "Foto ilegible.", photoUnavailable: "La verificación de fotos no está disponible: inténtalo más tarde.",
        photoSlowDown: "Una foto a la vez: espera un momento.", checking: "Verificando…",
        closedMidnight: "Medianoche: la sala ha desaparecido, y todo lo que se dijo en ella también.",
        closedExpelled: "El administrador te ha expulsado de esta sala.",
        closedBanned: "Se envió desde esta conexión una imagen sexual o que muestra drogas: su dirección queda bloqueada 30 días.",
        closedGone: "Esta sala ya no existe, o está llena.", connectionLost: "Conexión perdida.",
        errNetwork: "Sin red: comprueba tu conexión.", errGeneric: "Se produjo un error ({}).",
        errRate: "Demasiadas solicitudes: espera un momento.", errTooManyRooms: "Demasiadas salas abiertas ahora: inténtalo más tarde.",
        storageTampered: "Se modificaron datos guardados en este iPhone fuera de Alley: se descartaron.",
        banned: "Esta conexión está bloqueada.", encrypted: "Cifrado de extremo a extremo",
        reportSubject: "Denuncia — sala «{}»",
        reportBody: "Sala: {}\nIdentificador: {}\nFecha y hora: {}\n\nLo que pasó:\n\n",
        reportMessageBody: "Mensaje denunciado — de {}:\n«{}»\n\n", toBottom: "Volver a los últimos mensajes")
}
