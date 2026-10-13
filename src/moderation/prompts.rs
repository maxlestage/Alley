//! Les descriptions auxquelles le serveur compare chaque image avec CLIP.
//!
//! CLIP place une image et une phrase dans le même espace : l'image est
//! attribuée aux descriptions dont elle est la plus proche. Celles de drogue et
//! de nudité côtoient beaucoup de descriptions ordinaires (plantes, farine,
//! médicaments, maillots de bain…) pour qu'une image banale n'y ressemble
//! jamais par défaut.
//!
//! Ce fichier est partagé avec `cargo xtask prompts`, qui calcule une fois
//! pour toutes l'empreinte de chaque description (`prompts.bin`).

/// Ce que montre une image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Drugs,
    Sexual,
    Safe,
}

pub const PROMPTS: &[(Group, &str)] = &[
    (Group::Drugs, "a photo of lines of cocaine on a table"),
    (
        Group::Drugs,
        "a photo of white powder drugs and a rolled banknote",
    ),
    (Group::Drugs, "a photo of a bag of cocaine"),
    (Group::Drugs, "a photo of marijuana buds"),
    (Group::Drugs, "a photo of a cannabis leaf"),
    (Group::Drugs, "a photo of cannabis plants growing indoors"),
    (Group::Drugs, "a photo of someone rolling a joint"),
    (Group::Drugs, "a photo of someone smoking a joint"),
    (Group::Drugs, "a photo of a bong for smoking weed"),
    (Group::Drugs, "a photo of hashish"),
    (Group::Drugs, "a photo of heroin and a syringe"),
    (Group::Drugs, "a photo of a person injecting drugs"),
    (Group::Drugs, "a photo of crystal meth"),
    (Group::Drugs, "a photo of a crack pipe"),
    (Group::Drugs, "a photo of ecstasy pills with logos"),
    (Group::Drugs, "a photo of LSD blotter tabs"),
    (Group::Drugs, "a photo of magic mushrooms"),
    (Group::Drugs, "a photo of bags of illegal drugs for sale"),
    (Group::Sexual, "a pornographic photo"),
    (Group::Sexual, "a photo of a naked person"),
    (Group::Sexual, "a photo of naked breasts"),
    (Group::Sexual, "a photo of genitals"),
    (Group::Sexual, "a photo of people having sex"),
    (Group::Sexual, "an explicit nude selfie"),
    (Group::Sexual, "a pornographic drawing"),
    (Group::Safe, "a photo of a person"),
    (Group::Safe, "a selfie"),
    (Group::Safe, "a photo of a group of friends"),
    (Group::Safe, "a photo of a child"),
    (
        Group::Safe,
        "a photo of a person in a swimsuit at the beach",
    ),
    (Group::Safe, "a photo of a meal"),
    (Group::Safe, "a photo of a cup of coffee"),
    (Group::Safe, "a photo of a dog"),
    (Group::Safe, "a photo of a cat"),
    (Group::Safe, "a landscape photo"),
    (Group::Safe, "a photo of a city street"),
    (Group::Safe, "a photo of a house"),
    (Group::Safe, "a photo of a room"),
    (Group::Safe, "a photo of a car"),
    (Group::Safe, "a photo of flowers"),
    (Group::Safe, "a photo of green plants in a garden"),
    (Group::Safe, "a photo of a houseplant"),
    (Group::Safe, "a photo of fresh herbs for cooking"),
    (Group::Safe, "a photo of flour and sugar for baking"),
    (Group::Safe, "a photo of salt"),
    (Group::Safe, "a photo of medicine from a pharmacy"),
    (Group::Safe, "a photo of a doctor giving a vaccine"),
    (Group::Safe, "a medical syringe for a vaccine"),
    (Group::Safe, "a maple leaf"),
    (Group::Safe, "autumn leaves"),
    (Group::Safe, "a pizza"),
    (Group::Safe, "a photo of fruit"),
    (Group::Safe, "a plain colored background"),
    (Group::Safe, "a photo of a cigarette"),
    (Group::Safe, "a photo of a glass of wine"),
    (Group::Safe, "a photo of a sports game"),
    (Group::Safe, "a screenshot of a phone"),
    (Group::Safe, "a screenshot of a video game"),
    (Group::Safe, "a screenshot of text"),
    (Group::Safe, "a photo of a document"),
    (Group::Safe, "a meme"),
    (Group::Safe, "a cartoon"),
    (Group::Safe, "a drawing"),
    (Group::Safe, "an emoji"),
    (Group::Safe, "a logo"),
    (Group::Safe, "an abstract colorful image"),
    (Group::Safe, "a blank image"),
];

/// L'empreinte de la liste : `prompts.bin` doit avoir été calculé pour
/// exactement ces descriptions.
pub fn digest() -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    for (group, text) in PROMPTS {
        hasher.update(format!("{group:?}\t{text}\n"));
    }
    hasher.finalize().into()
}

/// En-tête de `prompts.bin`, suivi de l'empreinte de la liste, puis des
/// vecteurs (f32 petit-boutiste, normalisés).
pub const MAGIC: &[u8; 8] = b"ALLEYCL1";
pub const DIM: usize = 1024;
