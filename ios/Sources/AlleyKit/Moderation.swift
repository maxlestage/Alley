// Le filtre de texte d'Alley, le même que celui du serveur et du navigateur
// (`moderation/src/lib.rs`) : le contenu sexuel explicite et la drogue sont
// interdits, en français, en anglais et en espagnol. Les messages étant
// chiffrés, c'est l'application qui refuse d'en envoyer un interdit et
// masque ceux qu'elle reçoit.

import Foundation

public enum Category: String, Sendable {
    case sexual, drugs
}

public enum Moderation {
    static let sexual = [
        "porn*", "pr0n", "xxx", "hentai*", "nude", "nudes", "nudity", "nsfw", "onlyfans",
        "sextape*", "blowjob*", "handjob*", "cumshot*", "creampie*", "gangbang*", "bukkake",
        "masturb*", "orgy", "orgie*", "orgia*", "dildo*", "fellation*", "felacion*", "sodomi*",
        "camgirl*", "pussy", "follar*", "lolicon", "zoophil*", "zoofil*", "bestialit*",
        "pedophil*", "pedofil*",
    ]

    static let drugs = [
        "cocaine*", "cocaina*", "heroin", "meth", "methamphetamine*", "metanfetamina*",
        "amphetamine*", "anfetamina*", "mdma", "ecstasy", "lsd", "ketamine*", "ketamina*",
        "fentanyl*", "fentanilo*", "cannabis", "marijuana*", "marihuana*", "haschich*",
        "haschisch*", "hashish", "beuh", "ganja", "porro*", "psilocyb*", "psilocib*", "opium",
        "ghb", "crackhead*",
    ]

    /// Des mots courants qui commencent comme un mot interdit (des villes).
    static let allowed: Set<String> = ["pornic", "pornichet"]

    static func fold(_ c: Unicode.Scalar, symbols: Bool) -> String {
        switch c {
        case "à", "á", "â", "ä", "ã", "å", "ā", "4": return "a"
        case "ç", "ć", "č": return "c"
        case "è", "é", "ê", "ë", "ē", "3": return "e"
        case "ì", "í", "î", "ï", "1": return "i"
        case "ñ": return "n"
        case "ò", "ó", "ô", "ö", "õ", "ø", "0": return "o"
        case "ù", "ú", "û", "ü": return "u"
        case "ý", "ÿ": return "y"
        case "5": return "s"
        case "7": return "t"
        case "œ": return "oe"
        case "æ": return "ae"
        case "ß": return "ss"
        case "@" where symbols: return "a"
        case "€" where symbols: return "e"
        case "!" where symbols, "|" where symbols: return "i"
        case "$" where symbols: return "s"
        default: return ""
        }
    }

    static func invisible(_ c: Unicode.Scalar) -> Bool {
        switch c.value {
        case 0xAD, 0x200B...0x200F, 0x2060...0x2064, 0xFEFF, 0x034F: return true
        default: return false
        }
    }

    static func alphanumeric(_ c: Unicode.Scalar) -> Bool {
        c.properties.isAlphabetic || c.properties.numericType != nil
    }

    static func words(_ text: String, symbols: Bool) -> [String] {
        var words: [String] = []
        var current = ""
        let scalars = text.precomposedStringWithCanonicalMapping.unicodeScalars
            .filter { !invisible($0) }
            .flatMap { $0.properties.lowercaseMapping.unicodeScalars }
        for c in scalars {
            let folded = fold(c, symbols: symbols)
            if !folded.isEmpty {
                current += folded
            } else if alphanumeric(c) {
                current.unicodeScalars.append(c)
            } else if !current.isEmpty {
                words.append(current)
                current = ""
            }
        }
        if !current.isEmpty { words.append(current) }
        return words
    }

    /// « poooorn » → « porn ».
    static func squeeze(_ word: String) -> String {
        var out = ""
        for c in word where out.last != c { out.append(c) }
        return out
    }

    static func matches(_ word: String, _ list: [String]) -> Bool {
        if allowed.contains(word) { return false }
        return list.contains { entry in
            entry.hasSuffix("*") ? word.hasPrefix(String(entry.dropLast())) : word == entry
        }
    }

    static func candidates(_ text: String, symbols: Bool) -> [String] {
        var out: [String] = []
        var letters = ""
        for w in words(text, symbols: symbols) {
            if w.count == 1 {
                letters += w
                continue
            }
            if letters.count > 2 { out.append(letters) }
            letters = ""
            out.append(w)
        }
        if letters.count > 2 { out.append(letters) }
        return out + out.map(squeeze)
    }

    /// La catégorie si le texte est interdit, sinon `nil`.
    public static func check(_ text: String) -> Category? {
        let all = candidates(text, symbols: false) + candidates(text, symbols: true)
        if all.contains(where: { matches($0, sexual) }) { return .sexual }
        if all.contains(where: { matches($0, drugs) }) { return .drugs }
        return nil
    }
}
