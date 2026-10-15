import XCTest

@testable import AlleyKit

/// Les mêmes cas que le filtre Rust (`moderation/src/lib.rs`).
final class ModerationTests: XCTestCase {
    func testBlocksExplicitWordsInThreeLanguages() {
        for text in [
            "envoie des nudes", "regarde ce PORNO", "check my onlyfans", "vídeo pornográfico",
            "p o r n", "p0rn gratuit", "poooorn", "una orgía",
        ] {
            XCTAssertEqual(Moderation.check(text), .sexual, text)
        }
        for text in [
            "qui a de la cocaïne ?", "c.o.c.a.i.n.e", "vendo marihuana", "selling MDMA and LSD",
            "un peu de beuh ce soir", "fentanilo barato", "c0ca1ne", "cannabis!", "MDMA!!!",
            "co\u{AD}caine", "c\u{200B}annabis", "c@nnabis",
        ] {
            XCTAssertEqual(Moderation.check(text), .drugs, text)
        }
    }

    func testLeavesOrdinaryWordsAlone() {
        for text in [
            "On se retrouve à 20 h ?", "L'héroïne du film est géniale", "la heroína de la película",
            "un hachis parmentier", "Je fais du sport et de la méthode", "the hero of the day",
            "a photo of my garden", "Le Mexique, c'est loin", "crack the code", "Bonne nuit 🌙",
            "x y z", "Week-end à Pornic !", "Pornichet, la plage", "100 % d'accord !!!",
        ] {
            XCTAssertNil(Moderation.check(text), text)
        }
    }
}
