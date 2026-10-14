//! Le filtre de texte d'Alley : le contenu sexuel explicite et la drogue sont
//! interdits, en français, en anglais et en espagnol.
//!
//! Il sert des deux côtés : le navigateur refuse d'envoyer un message
//! interdit et masque ceux qu'il reçoit (les messages sont chiffrés, le
//! serveur ne les lit pas) ; le serveur refuse les noms de salon interdits.
//!
//! Le texte est d'abord ramené à une forme simple (minuscules, sans accents,
//! chiffres déguisés en lettres, lettres répétées ou espacées recollées),
//! puis comparé à des listes de mots choisis pour ne pas viser de mots
//! courants : « héroïne » (aussi un personnage) ou « hachis » (parmentier)
//! n'y sont pas.

/// Pourquoi un texte est refusé.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Sexual,
    Drugs,
}

impl Category {
    pub fn code(self) -> &'static str {
        match self {
            Category::Sexual => "sexual",
            Category::Drugs => "drugs",
        }
    }
}

/// Un mot interdit : exact, ou préfixe quand il finit par `*`.
const SEXUAL: &[&str] = &[
    "porn*",
    "pr0n",
    "xxx",
    "hentai*",
    "nude",
    "nudes",
    "nudity",
    "nsfw",
    "onlyfans",
    "sextape*",
    "blowjob*",
    "handjob*",
    "cumshot*",
    "creampie*",
    "gangbang*",
    "bukkake",
    "masturb*",
    "orgy",
    "orgie*",
    "orgia*",
    "dildo*",
    "fellation*",
    "felacion*",
    "sodomi*",
    "camgirl*",
    "pussy",
    "follar*",
    "lolicon",
    "zoophil*",
    "zoofil*",
    "bestialit*",
    "pedophil*",
    "pedofil*",
];

const DRUGS: &[&str] = &[
    "cocaine*",
    "cocaina*",
    "heroin",
    "meth",
    "methamphetamine*",
    "metanfetamina*",
    "amphetamine*",
    "anfetamina*",
    "mdma",
    "ecstasy",
    "lsd",
    "ketamine*",
    "ketamina*",
    "fentanyl*",
    "fentanilo*",
    "cannabis",
    "marijuana*",
    "marihuana*",
    "haschich*",
    "haschisch*",
    "hashish",
    "beuh",
    "ganja",
    "porro*",
    "psilocyb*",
    "psilocib*",
    "opium",
    "ghb",
    "crackhead*",
];

/// Des mots courants qui commencent comme un mot interdit (des villes).
const ALLOWED: &[&str] = &["pornic", "pornichet"];

/// Ramène un caractère à sa lettre de base (sans accent, sans déguisement).
/// Les symboles (`@`, `$`, `!`…) ne comptent comme des lettres que si
/// `symbols` est vrai : on essaie les deux lectures.
fn fold(c: char, symbols: bool) -> &'static str {
    match c {
        'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' | 'ā' | '4' => "a",
        'ç' | 'ć' | 'č' => "c",
        'è' | 'é' | 'ê' | 'ë' | 'ē' | '3' => "e",
        'ì' | 'í' | 'î' | 'ï' | '1' => "i",
        'ñ' => "n",
        'ò' | 'ó' | 'ô' | 'ö' | 'õ' | 'ø' | '0' => "o",
        'ù' | 'ú' | 'û' | 'ü' => "u",
        'ý' | 'ÿ' => "y",
        '5' => "s",
        '7' => "t",
        'œ' => "oe",
        'æ' => "ae",
        'ß' => "ss",
        '@' if symbols => "a",
        '€' if symbols => "e",
        '!' | '|' if symbols => "i",
        '$' if symbols => "s",
        _ => "",
    }
}

/// Un caractère invisible glissé dans un mot (trait d'union conditionnel,
/// espace de largeur nulle…) : ignoré.
fn invisible(c: char) -> bool {
    matches!(c, '\u{AD}' | '\u{200B}'..='\u{200F}' | '\u{2060}'..='\u{2064}' | '\u{FEFF}' | '\u{034F}')
}

/// Les mots du texte, en forme simple.
fn words(text: &str, symbols: bool) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    for c in text
        .chars()
        .filter(|c| !invisible(*c))
        .flat_map(char::to_lowercase)
    {
        let folded = fold(c, symbols);
        if !folded.is_empty() {
            current.push_str(folded);
        } else if c.is_alphanumeric() {
            current.push(c);
        } else if !current.is_empty() {
            words.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

/// « poooorn » → « porn ».
fn squeeze(word: &str) -> String {
    let mut out = String::with_capacity(word.len());
    for c in word.chars() {
        if !out.ends_with(c) {
            out.push(c);
        }
    }
    out
}

fn matches(word: &str, list: &[&str]) -> bool {
    if ALLOWED.contains(&word) {
        return false;
    }
    list.iter().any(|entry| match entry.strip_suffix('*') {
        Some(stem) => word.starts_with(stem),
        None => word == *entry,
    })
}

/// Les formes à comparer : chaque mot, ses lettres répétées réduites, et les
/// lettres isolées recollées (« p o r n », « c.o.c.a.i.n.e »).
fn candidates(text: &str) -> Vec<String> {
    let mut out = candidates_with(text, false);
    out.extend(candidates_with(text, true));
    out
}

fn candidates_with(text: &str, symbols: bool) -> Vec<String> {
    let words = words(text, symbols);
    let mut out = Vec::new();
    let mut letters = String::new();
    for w in &words {
        if w.chars().count() == 1 {
            letters.push_str(w);
            continue;
        }
        if letters.chars().count() > 2 {
            out.push(std::mem::take(&mut letters));
        }
        letters.clear();
        out.push(w.clone());
    }
    if letters.chars().count() > 2 {
        out.push(letters);
    }
    let squeezed: Vec<String> = out.iter().map(|w| squeeze(w)).collect();
    out.extend(squeezed);
    out
}

/// `Some(catégorie)` si le texte est interdit.
pub fn check_text(text: &str) -> Option<Category> {
    let candidates = candidates(text);
    if candidates.iter().any(|w| matches(w, SEXUAL)) {
        return Some(Category::Sexual);
    }
    if candidates.iter().any(|w| matches(w, DRUGS)) {
        return Some(Category::Drugs);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_explicit_words_in_three_languages() {
        for text in [
            "envoie des nudes",
            "regarde ce PORNO",
            "check my onlyfans",
            "vídeo pornográfico",
            "p o r n",
            "p0rn gratuit",
            "poooorn",
            "una orgía",
        ] {
            assert_eq!(check_text(text), Some(Category::Sexual), "{text}");
        }
        for text in [
            "qui a de la cocaïne ?",
            "c.o.c.a.i.n.e",
            "vendo marihuana",
            "selling MDMA and LSD",
            "un peu de beuh ce soir",
            "fentanilo barato",
            "c0ca1ne",
            "cannabis!",
            "MDMA!!!",
            "co\u{AD}caine",
            "c\u{200B}annabis",
            "c@nnabis",
        ] {
            assert_eq!(check_text(text), Some(Category::Drugs), "{text}");
        }
    }

    #[test]
    fn leaves_ordinary_words_alone() {
        for text in [
            "On se retrouve à 20 h ?",
            "L'héroïne du film est géniale",
            "la heroína de la película",
            "un hachis parmentier",
            "Je fais du sport et de la méthode",
            "the hero of the day",
            "a photo of my garden",
            "Le Mexique, c'est loin",
            "crack the code",
            "Bonne nuit 🌙",
            "x y z",
            "Week-end à Pornic !",
            "Pornichet, la plage",
            "100 % d'accord !!!",
        ] {
            assert_eq!(check_text(text), None, "{text}");
        }
    }
}
