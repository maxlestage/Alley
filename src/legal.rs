//! Les pages légales et d'aide de la vitrine, en trois langues :
//!
//! - `/legal/` : mentions légales ;
//! - `/terms/` : conditions d'utilisation et de vente (les périodes payées) ;
//! - `/privacy/` : confidentialité (ce qui est traité, et ce qui ne l'est pas) ;
//! - `/rules/` : règles de la communauté, et comment signaler un abus.
//!
//! Les coordonnées de l'éditeur ne sont pas écrites dans le code : elles
//! viennent de la configuration ([`Publisher`]), pour le serveur comme pour
//! l'export GitHub Pages. Sans adresse e-mail de contact, le contact passe par
//! le dépôt GitHub du projet.

use std::sync::LazyLock;

use crate::i18n::Lang;

/// La date de la dernière mise à jour des textes.
pub const UPDATED: [&str; 3] = [
    "15 octobre 2026",
    "October 15, 2026",
    "15 de octubre de 2026",
];

/// Le dépôt du code source (et, à défaut d'e-mail, le contact).
pub const SOURCE: &str = "https://github.com/maxlestage/Alley";

/// L'éditeur du service, d'après la configuration :
/// `ALLEY_PUBLISHER` (nom ou raison sociale), `ALLEY_PUBLISHER_ADDRESS`,
/// `ALLEY_PUBLISHER_ID` (SIREN/SIRET, RCS…), `ALLEY_CONTACT_EMAIL`.
#[derive(Clone, Debug, Default)]
pub struct Publisher {
    pub name: String,
    pub address: String,
    pub id: String,
    pub email: String,
}

impl Publisher {
    pub fn from_env() -> Publisher {
        let var = |name: &str| std::env::var(name).unwrap_or_default().trim().to_owned();
        let email = var("ALLEY_CONTACT_EMAIL");
        let valid = email.contains('@') && !email.contains(['<', '>', '"', ' ', '?']);
        Publisher {
            name: var("ALLEY_PUBLISHER"),
            address: var("ALLEY_PUBLISHER_ADDRESS"),
            id: var("ALLEY_PUBLISHER_ID"),
            email: if valid { email } else { String::new() },
        }
    }

    /// Où écrire : l'e-mail de contact, sinon le dépôt GitHub.
    pub fn contact_href(&self) -> String {
        if self.email.is_empty() {
            format!("{SOURCE}/issues")
        } else {
            format!("mailto:{}", self.email)
        }
    }

    /// Où signaler un abus (jamais en public : seulement par e-mail).
    pub fn report_href(&self, subject: &str) -> Option<String> {
        (!self.email.is_empty()).then(|| {
            let subject: String = subject
                .bytes()
                .map(|b| match b {
                    b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' => {
                        (b as char).to_string()
                    }
                    _ => format!("%{b:02X}"),
                })
                .collect();
            format!("mailto:{}?subject={subject}", self.email)
        })
    }
}

pub static PUBLISHER: LazyLock<Publisher> = LazyLock::new(Publisher::from_env);

/// Une page : son adresse, son titre, son chapeau et ses parties (un titre,
/// des paragraphes). `{publisher}`, `{address}`, `{id}`, `{contact}` sont
/// remplacés par les coordonnées de l'éditeur.
pub struct Doc {
    pub slug: &'static str,
    pub title: &'static str,
    pub lead: &'static str,
    pub sections: &'static [(&'static str, &'static [&'static str])],
}

/// Les libellés du pied de page et des pages.
pub struct Labels {
    pub product: &'static str,
    pub help: &'static str,
    pub legal: &'static str,
    pub pricing: &'static str,
    pub faq: &'static str,
    pub contact: &'static str,
    pub report: &'static str,
    pub source: &'static str,
    pub status: &'static str,
    pub updated: &'static str,
    pub tagline: &'static str,
    pub rights: &'static str,
    pub back: &'static str,
    pub unknown_publisher: &'static str,
    pub pricing_title: &'static str,
    pub pricing_lead: &'static str,
    pub pricing_once: &'static str,
    pub pricing_days: &'static str,
    pub pricing_month_one: &'static str,
    pub pricing_months: &'static str,
    pub pricing_free: &'static str,
    pub pricing_free_text: &'static str,
    pub pricing_paid_text: &'static str,
    pub pricing_note: &'static str,
}

pub fn labels(lang: Lang) -> &'static Labels {
    match lang {
        Lang::Fr => &LABELS_FR,
        Lang::En => &LABELS_EN,
        Lang::Es => &LABELS_ES,
    }
}

pub fn docs(lang: Lang) -> &'static [Doc; 4] {
    match lang {
        Lang::Fr => &DOCS_FR,
        Lang::En => &DOCS_EN,
        Lang::Es => &DOCS_ES,
    }
}

pub fn updated(lang: Lang) -> &'static str {
    match lang {
        Lang::Fr => UPDATED[0],
        Lang::En => UPDATED[1],
        Lang::Es => UPDATED[2],
    }
}

/// Un texte avec les coordonnées de l'éditeur.
pub fn fill(text: &str, lang: Lang) -> String {
    let p = &*PUBLISHER;
    let or = |v: &str| {
        if v.is_empty() {
            labels(lang).unknown_publisher.to_owned()
        } else {
            v.to_owned()
        }
    };
    let contact = if p.email.is_empty() {
        format!("{SOURCE}/issues")
    } else {
        p.email.clone()
    };
    text.replace("{publisher}", &or(&p.name))
        .replace("{address}", &or(&p.address))
        .replace("{id}", &or(&p.id))
        .replace("{contact}", &contact)
}

static LABELS_FR: Labels = Labels {
    product: "Produit",
    help: "Aide",
    legal: "Légal",
    pricing: "Tarifs",
    faq: "Questions fréquentes",
    contact: "Nous contacter",
    report: "Signaler un abus",
    source: "Code source",
    status: "Ce que voit le serveur",
    updated: "Mis à jour le",
    tagline: "Des salons éphémères, chiffrés et signés de bout en bout. Aucun message enregistré.",
    rights: "Tous droits réservés.",
    back: "Retour à l'accueil",
    unknown_publisher: "communiqué sur simple demande",
    pricing_title: "Tarifs",
    pricing_lead: "Alley est gratuit. Si vous voulez que vos salons durent plus longtemps qu'une journée, payez une période, une seule fois.",
    pricing_once: "en une fois",
    pricing_days: "{} jours",
    pricing_month_one: "1 mois",
    pricing_months: "{} mois",
    pricing_free: "Gratuit",
    pricing_free_text: "Salons illimités, messages, photos, vocaux, appels audio et vidéo, partage d'écran. Chaque salon ferme à minuit.",
    pricing_paid_text: "Pendant {}, les salons que vous créez restent ouverts jusqu'à la fin de la période au lieu de disparaître à minuit.",
    pricing_note: "Pas d'abonnement, rien ne se renouvelle, rien à résilier. Paiement sécurisé par Stripe ; Alley ne voit jamais votre carte. Prix TTC.",
};

static LABELS_EN: Labels = Labels {
    product: "Product",
    help: "Help",
    legal: "Legal",
    pricing: "Pricing",
    faq: "FAQ",
    contact: "Contact us",
    report: "Report abuse",
    source: "Source code",
    status: "What the server sees",
    updated: "Last updated",
    tagline: "Ephemeral rooms, end-to-end encrypted and signed. No message stored.",
    rights: "All rights reserved.",
    back: "Back to home",
    unknown_publisher: "available on request",
    pricing_title: "Pricing",
    pricing_lead: "Alley is free. If you want your rooms to last longer than a day, pay for a period, once.",
    pricing_once: "one-time",
    pricing_days: "{} days",
    pricing_month_one: "1 month",
    pricing_months: "{} months",
    pricing_free: "Free",
    pricing_free_text: "Unlimited rooms, messages, photos, voice notes, audio and video calls, screen sharing. Each room closes at midnight.",
    pricing_paid_text: "For {}, the rooms you create stay open until the period ends instead of disappearing at midnight.",
    pricing_note: "No subscription, nothing renews, nothing to cancel. Secure payment by Stripe; Alley never sees your card. VAT included.",
};

static LABELS_ES: Labels = Labels {
    product: "Producto",
    help: "Ayuda",
    legal: "Legal",
    pricing: "Precios",
    faq: "Preguntas frecuentes",
    contact: "Contacto",
    report: "Denunciar un abuso",
    source: "Código fuente",
    status: "Lo que ve el servidor",
    updated: "Actualizado el",
    tagline: "Salas efímeras, cifradas y firmadas de extremo a extremo. Ningún mensaje guardado.",
    rights: "Todos los derechos reservados.",
    back: "Volver al inicio",
    unknown_publisher: "disponible a petición",
    pricing_title: "Precios",
    pricing_lead: "Alley es gratis. Si quieres que tus salas duren más de un día, paga un periodo, una sola vez.",
    pricing_once: "un solo pago",
    pricing_days: "{} días",
    pricing_month_one: "1 mes",
    pricing_months: "{} meses",
    pricing_free: "Gratis",
    pricing_free_text: "Salas ilimitadas, mensajes, fotos, notas de voz, llamadas de audio y vídeo, compartir pantalla. Cada sala cierra a medianoche.",
    pricing_paid_text: "Durante {}, las salas que creas siguen abiertas hasta el final del periodo en lugar de desaparecer a medianoche.",
    pricing_note: "Sin suscripción, nada se renueva, nada que cancelar. Pago seguro con Stripe; Alley nunca ve tu tarjeta. IVA incluido.",
};

static DOCS_FR: [Doc; 4] = [
    Doc {
        slug: "legal",
        title: "Mentions légales",
        lead: "Qui édite Alley, qui l'héberge, et comment nous joindre.",
        sections: &[
            (
                "Éditeur",
                &[
                    "Alley est édité par {publisher}.",
                    "Adresse : {address}.",
                    "Immatriculation : {id}.",
                    "Directeur de la publication : {publisher}.",
                    "Contact : {contact}.",
                ],
            ),
            (
                "Hébergement",
                &[
                    "Le serveur d'Alley est hébergé par Heroku, une société de Salesforce, Inc., 415 Mission Street, Suite 300, San Francisco, CA 94105, États-Unis (région Europe).",
                    "La version GitHub Pages du site est hébergée par GitHub, Inc., 88 Colin P. Kelly Jr. Street, San Francisco, CA 94107, États-Unis.",
                ],
            ),
            (
                "Paiements",
                &[
                    "Les paiements sont traités par Stripe Payments Europe, Ltd., 1 Grand Canal Street Lower, Grand Canal Dock, Dublin, D02 H210, Irlande. Alley ne voit ni ne conserve vos données de carte.",
                ],
            ),
            (
                "Propriété intellectuelle",
                &[
                    "Le nom, le logo et les textes d'Alley appartiennent à leur éditeur. Le code source est publié sur GitHub.",
                ],
            ),
            (
                "Signaler un contenu illicite",
                &[
                    "Les messages sont chiffrés de bout en bout : l'éditeur ne peut pas les lire. Pour signaler un contenu ou un comportement illicite, écrivez à {contact} en indiquant le nom du salon, la date et l'heure, et ce qui s'est passé. Voir aussi les règles de la communauté.",
                ],
            ),
        ],
    },
    Doc {
        slug: "terms",
        title: "Conditions d'utilisation et de vente",
        lead: "Les règles du service, et ce que vous achetez quand vous payez une période.",
        sections: &[
            (
                "1. Le service",
                &[
                    "Alley propose des salons de discussion éphémères, chiffrés de bout en bout : messages, photos, messages vocaux, appels audio et vidéo. Un salon ferme à minuit (dans le fuseau de celui qui l'a créé), ou à la fin de la période payée par son créateur. Aucun message n'est enregistré : ni avant votre arrivée, ni après votre départ.",
                    "Utiliser Alley, c'est accepter ces conditions et les règles de la communauté.",
                ],
            ),
            (
                "2. Accès et compte",
                &[
                    "Un pseudonyme suffit. Vous pouvez y ajouter un mot de passe : il ne quitte jamais votre appareil, et il sert à retrouver vos salons et votre période payée sur vos autres appareils. L'éditeur ne connaît ni votre pseudonyme ni votre mot de passe et ne peut pas les récupérer : un mot de passe oublié, c'est l'accès au compte perdu (y compris à une période en cours).",
                    "Alley est réservé aux personnes de 15 ans ou plus.",
                ],
            ),
            (
                "3. Vos contenus et la modération",
                &[
                    "Vous êtes responsable de ce que vous envoyez. Le contenu sexuel, la drogue, le harcèlement et tout contenu illicite sont interdits (voir les règles de la communauté).",
                    "Les photos sont vérifiées automatiquement par le serveur avant d'être relayées, puis oubliées. Une photo à caractère sexuel ou montrant de la drogue entraîne le blocage de l'adresse IP de son auteur pendant 30 jours. L'administrateur d'un salon peut supprimer des messages et exclure un membre.",
                ],
            ),
            (
                "4. Les périodes payantes",
                &[
                    "Une période coûte 4,99 € pour 3 jours, 9,99 € pour 7 jours, 14,99 € pour 1 mois (30 jours) ou 34,99 € pour 3 mois (90 jours), toutes taxes comprises. Elle se paie une seule fois : par carte via Stripe sur le site, ou par un achat intégré de l'App Store dans l'application iPhone (au prix affiché par l'App Store, qui peut varier selon le pays). Ce n'est pas un abonnement : rien ne se renouvelle et rien n'est jamais reprélevé. Un achat sur l'App Store est traité, et le cas échéant remboursé, par Apple, selon ses conditions.",
                    "La période commence au paiement et se termine au minuit qui clôt son dernier jour (le 3ᵉ, le 7ᵉ, le 30ᵉ ou le 90ᵉ), dans le fuseau horaire de l'acheteur. Pendant la période, les salons que l'acheteur crée restent ouverts jusqu'à la fin de celle-ci au lieu de fermer à minuit. Elle n'ajoute rien d'autre : en particulier, aucun message n'est conservé.",
                    "La période est liée au compte (pseudonyme et mot de passe) avec lequel elle a été payée. Une seule période peut être en cours à la fois.",
                ],
            ),
            (
                "5. Droit de rétractation",
                &[
                    "Une période est un service numérique fourni immédiatement. Avant de payer, vous demandez expressément qu'elle commence tout de suite et vous reconnaissez perdre votre droit de rétractation de 14 jours dès qu'elle a commencé (article L221-28 du Code de la consommation).",
                    "Si un dysfonctionnement d'Alley vous empêche de profiter de votre période, écrivez-nous à {contact} : nous vous rembourserons.",
                ],
            ),
            (
                "6. Disponibilité et responsabilité",
                &[
                    "Alley est fourni tel quel. L'éditeur fait de son mieux pour qu'il fonctionne, sans garantir une disponibilité permanente. Comme rien n'est enregistré, rien ne peut être restauré : un salon fermé, un message effacé ou une clé perdue ne se récupèrent pas.",
                    "L'éditeur peut bloquer l'accès d'une personne qui enfreint ces conditions, sans remboursement de la période en cours.",
                ],
            ),
            (
                "7. Droit applicable et litiges",
                &[
                    "Ces conditions sont régies par le droit français. En cas de difficulté, écrivez-nous d'abord à {contact}. Si nous ne trouvons pas de solution, vous pouvez recourir gratuitement à un médiateur de la consommation, dont les coordonnées vous seront communiquées sur demande, ou saisir les tribunaux compétents.",
                ],
            ),
        ],
    },
    Doc {
        slug: "privacy",
        title: "Confidentialité",
        lead: "Alley est conçu pour en savoir le moins possible. Voici exactement ce qui est traité, et ce qui ne l'est pas.",
        sections: &[
            (
                "Ce que le serveur ne voit jamais",
                &[
                    "Vos messages, vocaux, pseudonymes et la mise en relation de vos appels sont chiffrés dans votre navigateur : le serveur relaie des jetons qu'il ne peut pas lire et n'en garde aucun. Il ne connaît ni votre pseudonyme, ni votre mot de passe, ni votre e-mail, ni votre carte. Les appels passent de pair à pair.",
                ],
            ),
            (
                "Ce que le serveur traite",
                &[
                    "Votre adresse IP, en mémoire, pour la sécurité (limites de débit, protection contre les abus) ; une adresse bloquée pour une image interdite l'est pendant 30 jours.",
                    "Les photos, un instant, en mémoire, le temps de vérifier qu'elles sont permises ; elles sont ensuite oubliées.",
                    "Le nom des salons publics, pour qu'on puisse les chercher, et le nombre de présents.",
                    "Si vous avez un compte : une empreinte tirée de votre mot de passe dans votre navigateur, pour retrouver vos salons. Elle ne permet de retrouver ni le pseudonyme ni le mot de passe.",
                    "Pour les salons de plusieurs jours : leur identifiant, leur nom, leur type et leur fin, le temps qu'ils sont ouverts.",
                    "L'hébergeur (Heroku) tient des journaux techniques des requêtes, qui contiennent les adresses IP, pendant une durée limitée.",
                ],
            ),
            (
                "Ce que garde votre appareil",
                &[
                    "Votre navigateur garde, signés pour qu'on ne puisse pas les modifier : les salons que vous avez créés ou visités (jusqu'à leur fin), le serveur choisi, votre langue et votre période payée. « Tout oublier » les efface. Votre gestionnaire de mots de passe peut retenir votre pseudonyme et votre mot de passe si vous l'acceptez.",
                    "Alley n'utilise ni cookie, ni traceur, ni publicité, ni mesure d'audience.",
                ],
            ),
            (
                "Paiements",
                &[
                    "Stripe traite votre e-mail et vos données de paiement, selon sa propre politique de confidentialité. Alley ne les voit jamais ; Stripe ne reçoit d'Alley qu'une empreinte de votre compte, qui ne révèle ni votre pseudonyme ni votre mot de passe. Sur iPhone, Apple traite l'achat ; Alley ne reçoit que la transaction signée par l'App Store (produit, date, identifiant de transaction) et un identifiant tiré de l'empreinte de votre compte.",
                ],
            ),
            (
                "Transferts et base légale",
                &[
                    "Le serveur est hébergé en Europe par une société américaine (Heroku, Salesforce) ; la version GitHub Pages par GitHub. Ces transferts sont encadrés par les clauses contractuelles types de la Commission européenne et le Data Privacy Framework.",
                    "Ces traitements reposent sur l'exécution du service que vous demandez et sur notre intérêt légitime à le protéger contre les abus.",
                ],
            ),
            (
                "Vos droits",
                &[
                    "Vous disposez d'un droit d'accès, de rectification, d'effacement, d'opposition et de limitation. Comme presque rien n'est conservé, la plupart de ces données disparaissent d'elles-mêmes. Pour exercer vos droits : {contact}. Vous pouvez aussi saisir la CNIL (cnil.fr).",
                ],
            ),
        ],
    },
    Doc {
        slug: "rules",
        title: "Règles de la communauté",
        lead: "Alley est un lieu pour parler librement, à condition de respecter les autres.",
        sections: &[
            (
                "Ce qui est interdit",
                &[
                    "Le contenu sexuel, et tout contenu sexuel impliquant des mineurs, qui sera signalé aux autorités.",
                    "La vente, l'achat ou la promotion de drogues.",
                    "Le harcèlement, les menaces, les appels à la haine ou à la violence.",
                    "Les contenus illicites, l'usurpation d'identité, l'arnaque, le spam, les logiciels malveillants.",
                    "Partager les informations personnelles de quelqu'un sans son accord.",
                ],
            ),
            (
                "Ce qui se passe en cas d'abus",
                &[
                    "Les messages qui enfreignent ces règles sont masqués et ne partent pas. Les photos sont vérifiées avant d'être relayées : une photo interdite bloque l'adresse IP de son auteur pendant 30 jours.",
                    "L'administrateur d'un salon (👑) peut supprimer n'importe quel message et exclure un membre, qui ne peut plus revenir tant que le salon existe.",
                ],
            ),
            (
                "Signaler",
                &[
                    "Dans un salon, le bouton « Signaler » prépare un message à l'éditeur. Les messages étant chiffrés, l'éditeur ne peut pas les lire : décrivez ce qui s'est passé, le nom du salon et l'heure.",
                    "En cas de danger immédiat, appelez le 112. Pour un enfant en danger, le 119. Vous pouvez aussi signaler un contenu illicite sur internet-signalement.gouv.fr (Pharos).",
                ],
            ),
        ],
    },
];

static DOCS_EN: [Doc; 4] = [
    Doc {
        slug: "legal",
        title: "Legal notice",
        lead: "Who publishes Alley, who hosts it, and how to reach us.",
        sections: &[
            (
                "Publisher",
                &[
                    "Alley is published by {publisher}.",
                    "Address: {address}.",
                    "Registration: {id}.",
                    "Publication director: {publisher}.",
                    "Contact: {contact}.",
                ],
            ),
            (
                "Hosting",
                &[
                    "Alley's server is hosted by Heroku, a Salesforce, Inc. company, 415 Mission Street, Suite 300, San Francisco, CA 94105, USA (Europe region).",
                    "The GitHub Pages version of the site is hosted by GitHub, Inc., 88 Colin P. Kelly Jr. Street, San Francisco, CA 94107, USA.",
                ],
            ),
            (
                "Payments",
                &[
                    "Payments are processed by Stripe Payments Europe, Ltd., 1 Grand Canal Street Lower, Grand Canal Dock, Dublin, D02 H210, Ireland. Alley never sees or keeps your card details.",
                ],
            ),
            (
                "Intellectual property",
                &[
                    "Alley's name, logo and texts belong to their publisher. The source code is published on GitHub.",
                ],
            ),
            (
                "Reporting illegal content",
                &[
                    "Messages are end-to-end encrypted: the publisher cannot read them. To report illegal content or behaviour, write to {contact} with the room name, the date and time, and what happened. See also the community rules.",
                ],
            ),
        ],
    },
    Doc {
        slug: "terms",
        title: "Terms of use and sale",
        lead: "The rules of the service, and what you buy when you pay for a period.",
        sections: &[
            (
                "1. The service",
                &[
                    "Alley offers ephemeral, end-to-end encrypted chat rooms: messages, photos, voice notes, audio and video calls. A room closes at midnight (in its creator's time zone), or at the end of the period paid for by its creator. No message is stored: not before you arrive, not after you leave.",
                    "Using Alley means accepting these terms and the community rules.",
                ],
            ),
            (
                "2. Access and account",
                &[
                    "A nickname is enough. You can add a password: it never leaves your device, and it lets you find your rooms and your paid period on your other devices. The publisher knows neither your nickname nor your password and cannot recover them: a forgotten password means losing access to the account (including a running period).",
                    "Alley is for people aged 15 or over.",
                ],
            ),
            (
                "3. Your content and moderation",
                &[
                    "You are responsible for what you send. Sexual content, drugs, harassment and any illegal content are forbidden (see the community rules).",
                    "Photos are checked automatically by the server before being relayed, then forgotten. A sexual photo or one showing drugs blocks its author's IP address for 30 days. A room's admin can delete messages and remove a member.",
                ],
            ),
            (
                "4. Paid periods",
                &[
                    "A period costs €4.99 for 3 days, €9.99 for 7 days, €14.99 for 1 month (30 days) or €34.99 for 3 months (90 days), all taxes included. It is paid once: by card through Stripe on the website, or as an App Store in-app purchase in the iPhone app (at the price shown by the App Store, which may vary by country). It is not a subscription: nothing renews and nothing is ever charged again. An App Store purchase is processed, and refunded where applicable, by Apple, under its own terms.",
                    "The period starts at payment and ends at the midnight closing its last day (the 3rd, 7th, 30th or 90th), in the buyer's time zone. During the period, the rooms the buyer creates stay open until it ends instead of closing at midnight. It adds nothing else: in particular, no message is kept.",
                    "The period is tied to the account (nickname and password) it was paid with. Only one period can run at a time.",
                ],
            ),
            (
                "5. Right of withdrawal",
                &[
                    "A period is a digital service supplied immediately. Before paying, you expressly ask for it to start right away and acknowledge that you lose your 14-day right of withdrawal once it has started.",
                    "If a malfunction of Alley prevents you from using your period, write to us at {contact}: we will refund you.",
                ],
            ),
            (
                "6. Availability and liability",
                &[
                    "Alley is provided as is. The publisher does its best to keep it running, without guaranteeing permanent availability. Since nothing is stored, nothing can be restored: a closed room, a deleted message or a lost key cannot be recovered.",
                    "The publisher may block anyone who breaches these terms, without refunding a running period.",
                ],
            ),
            (
                "7. Governing law and disputes",
                &[
                    "These terms are governed by French law. If there is a problem, write to us first at {contact}. If we cannot find a solution, you may use a consumer mediator free of charge, whose details are available on request, or go to the competent courts.",
                ],
            ),
        ],
    },
    Doc {
        slug: "privacy",
        title: "Privacy",
        lead: "Alley is designed to know as little as possible. Here is exactly what is processed, and what is not.",
        sections: &[
            (
                "What the server never sees",
                &[
                    "Your messages, voice notes, nicknames and call setup are encrypted in your browser: the server relays tokens it cannot read and keeps none of them. It knows neither your nickname, nor your password, nor your e-mail, nor your card. Calls go peer to peer.",
                ],
            ),
            (
                "What the server processes",
                &[
                    "Your IP address, in memory, for security (rate limits, abuse protection); an address blocked for a forbidden image stays blocked for 30 days.",
                    "Photos, for a moment, in memory, just long enough to check they are allowed; they are then forgotten.",
                    "The names of public rooms, so they can be searched, and the number of people present.",
                    "If you have an account: a fingerprint derived from your password in your browser, to find your rooms. It reveals neither the nickname nor the password.",
                    "For multi-day rooms: their identifier, name, type and end, for as long as they are open.",
                    "The host (Heroku) keeps technical request logs, which contain IP addresses, for a limited time.",
                ],
            ),
            (
                "What your device keeps",
                &[
                    "Your browser keeps, signed so they cannot be altered: the rooms you created or visited (until they end), the chosen server, your language and your paid period. \"Forget all\" erases them. Your password manager may remember your nickname and password if you allow it.",
                    "Alley uses no cookies, trackers, ads or analytics.",
                ],
            ),
            (
                "Payments",
                &[
                    "Stripe processes your e-mail and payment details under its own privacy policy. Alley never sees them; Stripe only receives from Alley a fingerprint of your account, which reveals neither your nickname nor your password. On iPhone, Apple processes the purchase; Alley only receives the transaction signed by the App Store (product, date, transaction identifier) and an identifier derived from your account's fingerprint.",
                ],
            ),
            (
                "Transfers and legal basis",
                &[
                    "The server is hosted in Europe by a US company (Heroku, Salesforce); the GitHub Pages version by GitHub. These transfers are covered by the European Commission's standard contractual clauses and the Data Privacy Framework.",
                    "This processing is based on providing the service you ask for and on our legitimate interest in protecting it from abuse.",
                ],
            ),
            (
                "Your rights",
                &[
                    "You have the right to access, rectify, erase, object and restrict. Since almost nothing is kept, most of this data disappears by itself. To exercise your rights: {contact}. You may also complain to your data protection authority.",
                ],
            ),
        ],
    },
    Doc {
        slug: "rules",
        title: "Community rules",
        lead: "Alley is a place to talk freely, as long as you respect others.",
        sections: &[
            (
                "What is forbidden",
                &[
                    "Sexual content, and any sexual content involving minors, which will be reported to the authorities.",
                    "Selling, buying or promoting drugs.",
                    "Harassment, threats, calls to hatred or violence.",
                    "Illegal content, impersonation, scams, spam, malware.",
                    "Sharing someone's personal information without their consent.",
                ],
            ),
            (
                "What happens in case of abuse",
                &[
                    "Messages that break these rules are hidden and not sent. Photos are checked before being relayed: a forbidden photo blocks its author's IP address for 30 days.",
                    "A room's admin (👑) can delete any message and remove a member, who cannot come back for as long as the room exists.",
                ],
            ),
            (
                "Reporting",
                &[
                    "In a room, the \"Report\" button prepares a message to the publisher. Since messages are encrypted, the publisher cannot read them: describe what happened, the room name and the time.",
                    "In immediate danger, call 112 (or your local emergency number).",
                ],
            ),
        ],
    },
];

static DOCS_ES: [Doc; 4] = [
    Doc {
        slug: "legal",
        title: "Aviso legal",
        lead: "Quién edita Alley, quién lo aloja y cómo contactarnos.",
        sections: &[
            (
                "Editor",
                &[
                    "Alley es editado por {publisher}.",
                    "Dirección: {address}.",
                    "Registro: {id}.",
                    "Director de la publicación: {publisher}.",
                    "Contacto: {contact}.",
                ],
            ),
            (
                "Alojamiento",
                &[
                    "El servidor de Alley está alojado por Heroku, una empresa de Salesforce, Inc., 415 Mission Street, Suite 300, San Francisco, CA 94105, EE. UU. (región Europa).",
                    "La versión GitHub Pages del sitio está alojada por GitHub, Inc., 88 Colin P. Kelly Jr. Street, San Francisco, CA 94107, EE. UU.",
                ],
            ),
            (
                "Pagos",
                &[
                    "Los pagos los procesa Stripe Payments Europe, Ltd., 1 Grand Canal Street Lower, Grand Canal Dock, Dublín, D02 H210, Irlanda. Alley nunca ve ni guarda los datos de tu tarjeta.",
                ],
            ),
            (
                "Propiedad intelectual",
                &[
                    "El nombre, el logotipo y los textos de Alley pertenecen a su editor. El código fuente está publicado en GitHub.",
                ],
            ),
            (
                "Denunciar un contenido ilícito",
                &[
                    "Los mensajes están cifrados de extremo a extremo: el editor no puede leerlos. Para denunciar un contenido o un comportamiento ilícito, escribe a {contact} indicando el nombre de la sala, la fecha y la hora, y lo que ocurrió. Consulta también las normas de la comunidad.",
                ],
            ),
        ],
    },
    Doc {
        slug: "terms",
        title: "Condiciones de uso y de venta",
        lead: "Las reglas del servicio, y lo que compras cuando pagas un periodo.",
        sections: &[
            (
                "1. El servicio",
                &[
                    "Alley ofrece salas de chat efímeras, cifradas de extremo a extremo: mensajes, fotos, notas de voz, llamadas de audio y vídeo. Una sala cierra a medianoche (en la zona horaria de quien la creó), o al final del periodo pagado por quien la creó. No se guarda ningún mensaje: ni antes de que llegues, ni después de que te vayas.",
                    "Usar Alley supone aceptar estas condiciones y las normas de la comunidad.",
                ],
            ),
            (
                "2. Acceso y cuenta",
                &[
                    "Basta con un seudónimo. Puedes añadir una contraseña: nunca sale de tu dispositivo y sirve para encontrar tus salas y tu periodo pagado en tus otros dispositivos. El editor no conoce ni tu seudónimo ni tu contraseña y no puede recuperarlos: olvidar la contraseña supone perder el acceso a la cuenta (incluido un periodo en curso).",
                    "Alley está reservado a personas de 15 años o más.",
                ],
            ),
            (
                "3. Tus contenidos y la moderación",
                &[
                    "Eres responsable de lo que envías. El contenido sexual, las drogas, el acoso y cualquier contenido ilícito están prohibidos (consulta las normas de la comunidad).",
                    "El servidor comprueba automáticamente las fotos antes de transmitirlas y luego las olvida. Una foto de carácter sexual o que muestre drogas bloquea la dirección IP de su autor durante 30 días. El administrador de una sala puede borrar mensajes y expulsar a un miembro.",
                ],
            ),
            (
                "4. Los periodos de pago",
                &[
                    "Un periodo cuesta 4,99 € por 3 días, 9,99 € por 7 días, 14,99 € por 1 mes (30 días) o 34,99 € por 3 meses (90 días), impuestos incluidos. Se paga una sola vez: con tarjeta a través de Stripe en el sitio web, o como compra integrada del App Store en la aplicación para iPhone (al precio que muestra el App Store, que puede variar según el país). No es una suscripción: nada se renueva y nunca se vuelve a cobrar. Una compra en el App Store la procesa, y en su caso la reembolsa, Apple, según sus propias condiciones.",
                    "El periodo empieza con el pago y termina a la medianoche que cierra su último día (el 3.º, el 7.º, el 30.º o el 90.º), en la zona horaria del comprador. Durante el periodo, las salas que crea el comprador siguen abiertas hasta su final en lugar de cerrar a medianoche. No añade nada más: en particular, no se guarda ningún mensaje.",
                    "El periodo está vinculado a la cuenta (seudónimo y contraseña) con la que se pagó. Solo puede haber un periodo en curso a la vez.",
                ],
            ),
            (
                "5. Derecho de desistimiento",
                &[
                    "Un periodo es un servicio digital que se presta de inmediato. Antes de pagar, pides expresamente que empiece en ese momento y reconoces que pierdes tu derecho de desistimiento de 14 días una vez que ha empezado.",
                    "Si un fallo de Alley te impide disfrutar de tu periodo, escríbenos a {contact}: te lo reembolsaremos.",
                ],
            ),
            (
                "6. Disponibilidad y responsabilidad",
                &[
                    "Alley se ofrece tal cual. El editor hace todo lo posible para que funcione, sin garantizar una disponibilidad permanente. Como no se guarda nada, nada puede restaurarse: una sala cerrada, un mensaje borrado o una clave perdida no se recuperan.",
                    "El editor puede bloquear el acceso a quien incumpla estas condiciones, sin reembolsar el periodo en curso.",
                ],
            ),
            (
                "7. Ley aplicable y litigios",
                &[
                    "Estas condiciones se rigen por la ley francesa. Si hay algún problema, escríbenos primero a {contact}. Si no encontramos una solución, puedes recurrir gratuitamente a un mediador de consumo, cuyos datos se facilitan a petición, o acudir a los tribunales competentes.",
                ],
            ),
        ],
    },
    Doc {
        slug: "privacy",
        title: "Privacidad",
        lead: "Alley está pensado para saber lo menos posible. Esto es exactamente lo que se trata, y lo que no.",
        sections: &[
            (
                "Lo que el servidor nunca ve",
                &[
                    "Tus mensajes, notas de voz, seudónimos y el establecimiento de tus llamadas se cifran en tu navegador: el servidor transmite fichas que no puede leer y no guarda ninguna. No conoce ni tu seudónimo, ni tu contraseña, ni tu correo, ni tu tarjeta. Las llamadas van de par a par.",
                ],
            ),
            (
                "Lo que trata el servidor",
                &[
                    "Tu dirección IP, en memoria, por seguridad (límites de uso, protección contra abusos); una dirección bloqueada por una imagen prohibida lo está durante 30 días.",
                    "Las fotos, un instante, en memoria, el tiempo de comprobar que están permitidas; después se olvidan.",
                    "El nombre de las salas públicas, para poder buscarlas, y el número de presentes.",
                    "Si tienes una cuenta: una huella obtenida de tu contraseña en tu navegador, para encontrar tus salas. No permite recuperar ni el seudónimo ni la contraseña.",
                    "Para las salas de varios días: su identificador, nombre, tipo y final, mientras están abiertas.",
                    "El proveedor de alojamiento (Heroku) guarda registros técnicos de las peticiones, que contienen direcciones IP, durante un tiempo limitado.",
                ],
            ),
            (
                "Lo que guarda tu dispositivo",
                &[
                    "Tu navegador guarda, firmados para que no puedan modificarse: las salas que creaste o visitaste (hasta su final), el servidor elegido, tu idioma y tu periodo pagado. «Olvidar todo» los borra. Tu gestor de contraseñas puede recordar tu seudónimo y tu contraseña si lo aceptas.",
                    "Alley no usa cookies, rastreadores, publicidad ni medición de audiencia.",
                ],
            ),
            (
                "Pagos",
                &[
                    "Stripe trata tu correo y tus datos de pago según su propia política de privacidad. Alley nunca los ve; Stripe solo recibe de Alley una huella de tu cuenta, que no revela ni tu seudónimo ni tu contraseña. En iPhone, Apple procesa la compra; Alley solo recibe la transacción firmada por el App Store (producto, fecha, identificador de transacción) y un identificador derivado de la huella de tu cuenta.",
                ],
            ),
            (
                "Transferencias y base jurídica",
                &[
                    "El servidor está alojado en Europa por una empresa estadounidense (Heroku, Salesforce); la versión GitHub Pages, por GitHub. Estas transferencias se rigen por las cláusulas contractuales tipo de la Comisión Europea y el Data Privacy Framework.",
                    "Estos tratamientos se basan en la prestación del servicio que solicitas y en nuestro interés legítimo en protegerlo contra abusos.",
                ],
            ),
            (
                "Tus derechos",
                &[
                    "Tienes derecho de acceso, rectificación, supresión, oposición y limitación. Como casi nada se conserva, la mayoría de estos datos desaparecen por sí solos. Para ejercer tus derechos: {contact}. También puedes reclamar ante tu autoridad de protección de datos.",
                ],
            ),
        ],
    },
    Doc {
        slug: "rules",
        title: "Normas de la comunidad",
        lead: "Alley es un lugar para hablar con libertad, siempre que respetes a los demás.",
        sections: &[
            (
                "Lo que está prohibido",
                &[
                    "El contenido sexual, y cualquier contenido sexual que implique a menores, que se denunciará a las autoridades.",
                    "Vender, comprar o promocionar drogas.",
                    "El acoso, las amenazas, la incitación al odio o a la violencia.",
                    "Los contenidos ilícitos, la suplantación de identidad, las estafas, el spam y el software malicioso.",
                    "Compartir datos personales de alguien sin su consentimiento.",
                ],
            ),
            (
                "Qué pasa en caso de abuso",
                &[
                    "Los mensajes que incumplen estas normas se ocultan y no se envían. Las fotos se comprueban antes de transmitirse: una foto prohibida bloquea la dirección IP de su autor durante 30 días.",
                    "El administrador de una sala (👑) puede borrar cualquier mensaje y expulsar a un miembro, que no podrá volver mientras exista la sala.",
                ],
            ),
            (
                "Denunciar",
                &[
                    "En una sala, el botón «Denunciar» prepara un mensaje al editor. Como los mensajes están cifrados, el editor no puede leerlos: describe lo que ocurrió, el nombre de la sala y la hora.",
                    "En caso de peligro inmediato, llama al 112.",
                ],
            ),
        ],
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::LANGS;

    #[test]
    fn every_page_exists_in_every_language() {
        for lang in LANGS {
            let slugs: Vec<&str> = docs(lang).iter().map(|d| d.slug).collect();
            assert_eq!(slugs, ["legal", "terms", "privacy", "rules"], "{lang:?}");
            for doc in docs(lang) {
                assert!(!doc.sections.is_empty());
                for (_, paragraphs) in doc.sections {
                    for p in *paragraphs {
                        let filled = fill(p, lang);
                        assert!(!filled.contains('{'), "{filled}");
                    }
                }
            }
        }
    }

    #[test]
    fn the_contact_is_an_email_or_the_repository() {
        let none = Publisher::default();
        assert_eq!(none.contact_href(), format!("{SOURCE}/issues"));
        assert_eq!(none.report_href("x"), None, "jamais un signalement public");
        let p = Publisher {
            email: "contact@exemple.fr".into(),
            ..Publisher::default()
        };
        assert_eq!(p.contact_href(), "mailto:contact@exemple.fr");
        assert_eq!(
            p.report_href("Signalement : salon").as_deref(),
            Some("mailto:contact@exemple.fr?subject=Signalement%20%3A%20salon")
        );
    }
}
