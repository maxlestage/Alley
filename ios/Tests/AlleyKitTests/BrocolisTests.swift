import Foundation
import XCTest

@testable import AlleyKit

/// Ce que produit le navigateur (Rust) doit s'ouvrir ici, et inversement.
final class BrocolisTests: XCTestCase {
    var v: [String: String] = [:]

    override func setUpWithError() throws {
        let url = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
            .appendingPathComponent("vectors.json")
        v = try JSONSerialization.jsonObject(with: Data(contentsOf: url)) as! [String: String]
    }

    var key: Key { Brocolis.unb64(v["key"]!)! }

    func testOpensATokenSealedByTheBrowser() {
        XCTAssertEqual(Brocolis.openToken(key, v["token"]!), "Bonjour 🌍 de Rust")
        XCTAssertNil(Brocolis.openToken(Brocolis.generateKey(), v["token"]!))
        let mine = Brocolis.sealToken(key, "Bonjour d'iPhone")
        XCTAssertTrue(mine.hasPrefix("[[BROCOLIS:v1:"))
        XCTAssertEqual(Brocolis.openToken(key, mine), "Bonjour d'iPhone")
    }

    func testFingerprintsAndCommitments() {
        XCTAssertEqual(Brocolis.fingerprint(key), v["fingerprint"])
        XCTAssertEqual(Brocolis.keyCommitment(key), v["commitment"])
    }

    func testAccountDerivations() {
        let a = Account(pseudonym: "Ada", seed: Data(repeating: 1, count: 32))
        XCTAssertEqual(a.tag, v["tag"])
        XCTAssertEqual(a.billing, v["billing"])
        XCTAssertEqual(Brocolis.b64(a.vaultKey), v["vault"])
        // RFC 7914 (PBKDF2-HMAC-SHA256, « passwd » / « salt », 1 tour).
        let p = Account.pbkdf2(password: Data("passwd".utf8), salt: Data("salt".utf8), iterations: 1)
        XCTAssertEqual(Brocolis.hex(p), "55ac046e56e3089fec1691c22544b605f94185216dde0465e68b9d57c20dacbc")
        XCTAssertEqual(Account.salt("  Ada "), Data("alley:v1:ada".utf8))
    }

    func testTheAppleTokenMatchesTheServer() {
        // Le même vecteur que `apple::tests` côté serveur.
        let a = Account(pseudonym: "Ada", tag: "", billing: String(repeating: "5", count: 64))
        XCTAssertEqual(Account.billingID(a.billing), "d05593d48ed176ac17657955fb34d060100d61a24286e56674dae32b2486bc87")
        XCTAssertEqual(a.appleToken.uuidString.lowercased(), "c223ad81-82f3-482f-8871-c3fe9bde14e2")
    }

    func testUnwrapsTheRoomKeyHandedByTheBrowser() {
        let b = Identity(raw: Data(repeating: 2, count: 32))!
        XCTAssertEqual(Brocolis.b64(b.publicRaw), v["b_public"])
        XCTAssertEqual(b.unwrapGroupKey(v["wrapped_for_b"]!, from: v["a_public"]!), key)
        // Et l'iPhone la remet à son tour.
        let a = Identity(raw: Data(repeating: 1, count: 32))!
        let blob = b.wrapGroupKey(key, for: v["a_public"]!)!
        XCTAssertEqual(a.unwrapGroupKey(blob, from: v["b_public"]!), key)
        XCTAssertNil(Identity().unwrapGroupKey(blob, from: v["b_public"]!))
    }

    func testOpensASignedEnvelopeAndRejectsTheRest() {
        let open = { (signer: String?, room: String, me: String, seen: Replay) in
            Envelope.open(key: self.key, token: self.v["envelope"]!, signer: signer, room: room, me: me, seen: seen)
        }
        let pub = v["signer_public"]!
        guard case .success(let (body, n)) = open(pub, "room1", "me42", Replay()) else {
            return XCTFail("enveloppe du navigateur refusée")
        }
        XCTAssertEqual(body, #"{"k":"msg","x":"salut"}"#)
        XCTAssertEqual(n, 5)
        XCTAssertEqual(open(Brocolis.b64(Signer().publicRaw), "room1", "me42", Replay()).failure, .forged)
        XCTAssertEqual(open(pub, "room2", "me42", Replay()).failure, .forged)
        XCTAssertEqual(open(pub, "room1", "bob", Replay()).failure, .misdirected)
        XCTAssertEqual(open(nil, "room1", "me42", Replay()).failure, .unsigned)
        var seen = Replay()
        seen.mark(5)
        XCTAssertEqual(open(pub, "room1", "me42", seen).failure, .replayed)
    }

    func testSealsAnEnvelopeTheBrowserLayoutAccepts() {
        let signer = Signer()
        let token = Envelope.seal(key: key, signer: signer, room: "r", to: nil, n: 9, body: "corps")
        let opened = Envelope.open(
            key: key, token: token, signer: Brocolis.b64(signer.publicRaw), room: "r", me: "x", seen: Replay())
        XCTAssertEqual(try? opened.get().0, "corps")
    }

    func testRoomStatementMatchesTheServer() {
        let s = ServerSignature.roomStatement(id: "ab12cd", name: "Café", isPrivate: true, expiresAt: 1_791_936_000_000)
        XCTAssertEqual(Brocolis.b64(s), v["statement"])
    }

    func testReplayWindow() {
        var r = Replay()
        XCTAssertFalse(r.fresh(0))
        r.mark(1)
        XCTAssertFalse(r.fresh(1))
        r.mark(10_000)
        XCTAssertFalse(r.fresh(2))
        XCTAssertTrue(r.fresh(9_999))
    }
}

extension Result {
    var failure: Failure? {
        if case .failure(let e) = self { return e }
        return nil
    }
}
