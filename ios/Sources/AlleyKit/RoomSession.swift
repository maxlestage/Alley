// Un salon, en direct : la connexion WebSocket, la remise de la clé du
// salon (X25519), les envois chiffrés et signés (Brocolis + Ed25519), les
// messages, réactions, modifications, suppressions, l'administration.
// Le même protocole que le navigateur (`client/src/app.rs`) : un iPhone et
// un navigateur sont dans le même salon. Rien n'est gardé : quitter le
// salon, c'est tout oublier.

import Foundation
import Observation

#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

/// Quelqu'un d'autre dans le salon.
public struct Peer: Sendable {
    public var x: String
    public var s: String?
    public var order: UInt64
    public var name: String?
    /// Dans l'appel, micro coupé, caméra (ou écran) partagée.
    public var voice = false
    public var muted = false
    public var video = false
    public var typingUntil: Date = .distantPast
    var seen = Replay()
    var warned = false
}

/// Un message reçu (ou envoyé) depuis qu'on est arrivé.
public struct Message: Identifiable, Equatable, Sendable {
    public enum Kind: String, Sendable { case text, image, audio }

    public var id: String
    public var from: String
    public var name: String
    public var date: Date
    public var kind: Kind
    public var text: String
    /// La photo (JPEG) ou le message vocal.
    public var media: Data?
    public var mime: String
    public var reply: String
    public var edited = false
    public var mine = false
    /// Une photo à moi que le serveur vérifie encore.
    public var pending = false
    /// Pour chaque emoji, qui l'a mis (identifiants de connexion).
    public var reactions: [String: [String]] = [:]
}

/// Les six réactions permises.
public let reactionEmojis = ["👍", "❤️", "😂", "😮", "😢", "🙏"]

/// Ce que le salon a à dire à l'écran (un bandeau).
public enum Notice: Equatable, Sendable {
    case nowAdmin
    case renamed(String, byMe: Bool)
    case renameRefused(String)
    case signatureRejected(String)
    case photoRefused(String)
    case refused(Category)
    case keyNotArrived
    case noKeyRetry
    case reconnecting
}

/// Pourquoi on a quitté le salon.
public enum Closed: Equatable, Sendable {
    case left, midnight, expelled, banned, gone, connectionLost
}

public enum RoomState: Equatable, Sendable {
    case connecting, waitingForKey, ready, closed(Closed)
}

/// Le transport des messages du salon (une WebSocket, ou un faux pour les tests).
public protocol RoomTransport: AnyObject {
    func send(_ text: String)
    func close()
}

@MainActor
@Observable
public final class RoomSession {
    public private(set) var info: RoomInfo
    public private(set) var state: RoomState = .connecting
    public private(set) var me = ""
    public private(set) var order: UInt64 = 0
    public private(set) var admin: String?
    public private(set) var fingerprint = ""
    public private(set) var peers: [String: Peer] = [:]
    public private(set) var messages: [Message] = []
    /// Le dernier bandeau à montrer.
    public var notice: Notice?
    /// Un nouveau message d'un autre (pour les notifications).
    @ObservationIgnored public var onIncoming: ((Message) -> Void)?
    /// Le salon a été renommé (pour la liste des salons de l'appareil).
    @ObservationIgnored public var onRenamed: ((RoomInfo) -> Void)?
    /// Un message de l'appel (offre, réponse, candidat ICE) de `from`.
    @ObservationIgnored public var onSignal: ((String, [String: Any]) -> Void)?
    /// Qui est dans l'appel a changé.
    @ObservationIgnored public var onCallChanged: (() -> Void)?

    /// Mon état d'appel, annoncé à tous (`hello`).
    public private(set) var inCall = false
    public private(set) var muted = false
    public private(set) var video = false

    @ObservationIgnored let api: API
    @ObservationIgnored let account: Account?
    @ObservationIgnored public var pseudonym: String
    @ObservationIgnored var identity = Identity()
    @ObservationIgnored var signer = Signer()
    @ObservationIgnored var key: Key?
    @ObservationIgnored var sent: UInt64 = 0
    @ObservationIgnored var pending: [[String: Any]] = []
    @ObservationIgnored var transport: RoomTransport?
    @ObservationIgnored var retries = 0
    @ObservationIgnored var lastTyping = Date.distantPast
    @ObservationIgnored var keyTimer: Task<Void, Never>?
    /// Le nom d'un invité sans pseudonyme : « Invité ab12 ».
    @ObservationIgnored public var guestName: (String) -> String = { "Invité \($0.prefix(4))" }

    public init(info: RoomInfo, api: API, pseudonym: String, account: Account?) {
        self.info = info
        self.api = api
        self.pseudonym = pseudonym
        self.account = account
    }

    public var hasKey: Bool { key != nil }
    public var iAmAdmin: Bool { !me.isEmpty && admin == me }
    public var present: Int { peers.count + 1 }

    public func name(of id: String) -> String {
        if id == me { return pseudonym }
        return peers[id]?.name ?? guestName(id)
    }

    /// Ceux qui écrivent en ce moment.
    public var typing: [String] {
        let now = Date()
        return peers.filter { $0.value.typingUntil > now }.map { name(of: $0.key) }.sorted()
    }

    // MARK: Connexion

    /// L'adresse de la connexion en direct (clés publiques jetables en base64url).
    public var liveURL: URL {
        var s = "\(api.socketBase)/api/v1/rooms/\(info.id)/live?x=\(Brocolis.b64url(identity.publicRaw))&s=\(Brocolis.b64url(signer.publicRaw))"
        if let account { s += "&t=\(account.tag)" }
        return URL(string: s)!
    }

    /// Ouvre la connexion (une WebSocket, sauf transport donné).
    public func connect() {
        state = .connecting
        transport = makeTransport?(liveURL, self) ?? WebSocketTransport(url: liveURL, session: self)
    }

    /// Un autre transport que la WebSocket d'URLSession (tests sous Linux).
    @ObservationIgnored public var makeTransport: ((URL, RoomSession) -> RoomTransport)?

    /// Quitte le salon : tout ce qu'on y a vu est oublié.
    public func leave(_ reason: Closed = .left) {
        guard !isClosed else { return }
        state = .closed(reason)
        keyTimer?.cancel()
        let t = transport
        transport = nil
        t?.close()
        messages.removeAll()
        key = nil
        pending.removeAll()
        inCall = false
        onCallChanged?()
    }

    var isClosed: Bool {
        if case .closed = state { return true }
        return false
    }

    /// La connexion s'est coupée (code WebSocket).
    public func lost(code: Int) {
        guard !isClosed, transport != nil else { return }
        transport = nil
        switch code {
        case 4403: return leave(.banned)
        case 4404: return leave(.gone)
        case 4410: return leave(.expelled)
        case 4000: return leave(.midnight)
        default: break
        }
        retries += 1
        if retries > 3 { return leave(.connectionLost) }
        notice = .reconnecting
        let wait = UInt64(retries) * 1_000_000_000
        Task { [weak self] in
            try? await Task.sleep(nanoseconds: wait)
            self?.reconnect()
        }
    }

    /// On revient comme un nouvel arrivant : rien n'a été gardé.
    func reconnect() {
        guard !isClosed else { return }
        identity = Identity()
        signer = Signer()
        key = nil
        sent = 0
        peers.removeAll()
        pending.removeAll()
        me = ""
        onCallChanged?()
        connect()
    }

    // MARK: Envois

    func sendRaw(_ value: [String: Any]) {
        guard let data = try? JSONSerialization.data(withJSONObject: value),
              let text = String(data: data, encoding: .utf8)
        else { return }
        transport?.send(text)
    }

    func encode(_ payload: [String: Any]) -> String? {
        guard let data = try? JSONSerialization.data(withJSONObject: payload) else { return nil }
        return String(data: data, encoding: .utf8)
    }

    /// Chiffre et signe un envoi (pour `to` seul, ou pour tout le salon).
    func enc(_ payload: [String: Any], to: String? = nil) -> String? {
        guard let key, let body = encode(payload) else { return nil }
        sent += 1
        return Envelope.seal(key: key, signer: signer, room: info.id, to: to, n: sent, body: body)
    }

    func send(_ kind: String, _ fields: [String: Any], to: String? = nil) {
        var payload = fields
        payload["k"] = kind
        guard let data = enc(payload, to: to) else { return }
        var msg: [String: Any] = ["type": "relay", "data": data]
        if let to { msg["to"] = to }
        sendRaw(msg)
    }

    /// Mon pseudonyme, pour tous ou pour un nouvel arrivant.
    func hello(to: String? = nil) {
        send("hello", ["n": pseudonym, "voice": inCall, "muted": muted, "video": video], to: to)
    }

    // MARK: L'appel

    /// Entre dans l'appel, ou en sort, coupe le micro, partage la caméra :
    /// annoncé à tous.
    public func setCall(inCall: Bool, muted: Bool = false, video: Bool = false) {
        self.inCall = inCall
        self.muted = inCall && muted
        self.video = inCall && video
        if key != nil { hello() }
        onCallChanged?()
    }

    /// Ceux qui sont dans l'appel (moi compris, si j'y suis).
    public var callMembers: [String] {
        var ids = inCall ? [me] : []
        ids += peers.filter(\.value.voice).map(\.key).sorted()
        return ids
    }

    /// Un message de l'appel pour un seul membre (chiffré et signé).
    public func signal(to: String, _ data: [String: Any]) {
        send("signal", ["d": data], to: to)
    }

    static func newMid() -> String { Brocolis.hex(Brocolis.randomBytes(8)) }

    /// Envoie un texte (réponse à `reply` s'il y en a une). Faux s'il est refusé.
    @discardableResult
    public func sendText(_ text: String, reply: String? = nil) -> Bool {
        let text = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return false }
        guard key != nil else {
            notice = .keyNotArrived
            return false
        }
        if let category = Moderation.check(text) {
            notice = .refused(category)
            return false
        }
        let mid = RoomSession.newMid()
        send("msg", ["mid": mid, "kind": "text", "x": text, "m": "", "r": reply ?? ""])
        messages.append(
            Message(
                id: mid, from: me, name: pseudonym, date: Date(), kind: .text, text: text, media: nil,
                mime: "", reply: reply ?? "", mine: true))
        trim()
        return true
    }

    /// Envoie une photo (JPEG) : chiffrée avec sa propre clé, confiée au
    /// serveur le temps qu'il la vérifie, et aux présents dans le message.
    public func sendImage(_ jpeg: Data, reply: String? = nil) {
        guard key != nil else {
            notice = .keyNotArrived
            return
        }
        let imageKey = Brocolis.generateKey()
        let c = Brocolis.sealBytesToken(imageKey, jpeg)
        let mid = RoomSession.newMid()
        guard let data = enc([
            "k": "msg", "mid": mid, "kind": "image", "ik": Brocolis.b64(imageKey), "r": reply ?? "",
        ]) else { return }
        sendRaw(["type": "image", "id": mid, "key": Brocolis.b64(imageKey), "c": c, "data": data])
        messages.append(
            Message(
                id: mid, from: me, name: pseudonym, date: Date(), kind: .image, text: "", media: jpeg,
                mime: "image/jpeg", reply: reply ?? "", mine: true, pending: true))
        trim()
    }

    /// Envoie un message vocal (`mime` : `audio/mp4` depuis l'iPhone).
    public func sendAudio(_ audio: Data, mime: String, reply: String? = nil) {
        guard key != nil else {
            notice = .keyNotArrived
            return
        }
        let mid = RoomSession.newMid()
        send("msg", ["mid": mid, "kind": "audio", "x": Brocolis.b64(audio), "m": mime, "r": reply ?? ""])
        messages.append(
            Message(
                id: mid, from: me, name: pseudonym, date: Date(), kind: .audio, text: "", media: audio,
                mime: mime, reply: reply ?? "", mine: true))
        trim()
    }

    /// Modifie un de mes messages.
    @discardableResult
    public func edit(_ mid: String, text: String) -> Bool {
        let text = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return false }
        if let category = Moderation.check(text) {
            notice = .refused(category)
            return false
        }
        send("edit", ["mid": mid, "x": text])
        if let i = messages.firstIndex(where: { $0.id == mid && $0.mine }) {
            messages[i].text = text
            messages[i].edited = true
        }
        return true
    }

    /// Supprime un message (le mien, ou n'importe lequel si j'administre).
    public func delete(_ mid: String) {
        send("del", ["mid": mid])
        let admin = iAmAdmin
        messages.removeAll { $0.id == mid && ($0.mine || admin) }
    }

    /// Met (ou retire, si je l'avais mise) une réaction.
    public func react(_ mid: String, _ emoji: String) {
        guard reactionEmojis.contains(emoji), let i = messages.firstIndex(where: { $0.id == mid }) else {
            return
        }
        let on = !(messages[i].reactions[emoji] ?? []).contains(me)
        RoomSession.apply(&messages[i].reactions, emoji, who: me, on: on)
        send("react", ["mid": mid, "e": emoji, "on": on])
    }

    nonisolated static func apply(_ reactions: inout [String: [String]], _ emoji: String, who: String, on: Bool) {
        var people = (reactions[emoji] ?? []).filter { $0 != who }
        if on { people.append(who) }
        reactions[emoji] = people.isEmpty ? nil : people
    }

    /// « … écrit » (au plus toutes les 3 secondes).
    public func typingNow() {
        guard Date().timeIntervalSince(lastTyping) > 3, key != nil else { return }
        lastTyping = Date()
        send("typing", [:])
    }

    /// L'administrateur renomme le salon (le serveur vérifie et prévient tous).
    @discardableResult
    public func rename(_ name: String) -> Bool {
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard iAmAdmin, !name.isEmpty else { return false }
        if let category = Moderation.check(name) {
            notice = .refused(category)
            return false
        }
        sendRaw(["type": "rename", "name": name])
        return true
    }

    /// L'administrateur exclut un membre.
    public func expel(_ id: String) {
        guard iAmAdmin else { return }
        sendRaw(["type": "expel", "to": id])
    }

    func trim() {
        if messages.count > 500 { messages.removeFirst(messages.count - 500) }
    }

    // MARK: Ce qui arrive

    /// Un événement du serveur (texte JSON).
    public func receive(_ text: String) {
        guard let obj = (try? JSONSerialization.jsonObject(with: Data(text.utf8))) as? [String: Any] else {
            return
        }
        handle(obj)
    }

    func handle(_ event: [String: Any]) {
        let str = { (k: String) in event[k] as? String ?? "" }
        switch str("type") {
        case "welcome":
            me = str("you")
            admin = event["admin"] as? String
            order = (event["order"] as? NSNumber)?.uint64Value ?? 0
            if let room = event["room"] as? [String: Any], let at = room["expires_at"] as? NSNumber {
                info.expiresAt = at.int64Value
            }
            for p in event["peers"] as? [[String: Any]] ?? [] {
                guard let id = p["id"] as? String else { continue }
                peers[id] = Peer(
                    x: p["x"] as? String ?? "", s: p["s"] as? String,
                    order: (p["order"] as? NSNumber)?.uint64Value ?? 0)
            }
            if peers.isEmpty {
                // Le premier arrivant tire la clé du salon.
                setKey(Brocolis.generateKey())
                hello()
            } else {
                state = .waitingForKey
                // Les présents vont nous la remettre ; sinon, on réessaie.
                keyTimer?.cancel()
                keyTimer = Task { [weak self] in
                    try? await Task.sleep(nanoseconds: 8_000_000_000)
                    guard let self, !Task.isCancelled, self.key == nil, !self.isClosed else { return }
                    self.notice = .noKeyRetry
                    self.retries += 1
                    if self.retries > 3 { return self.leave(.connectionLost) }
                    self.transport?.close()
                    self.transport = nil
                    self.reconnect()
                }
            }
        case "joined":
            let id = str("id")
            let x = str("x")
            peers[id] = Peer(x: x, s: event["s"] as? String, order: (event["order"] as? NSNumber)?.uint64Value ?? 0)
            // Les trois plus anciens remettent la clé, emballée pour lui seul.
            if let key, keyHolder(except: id), let blob = identity.wrapGroupKey(key, for: x) {
                sendRaw(["type": "key", "to": id, "blob": blob])
            }
            hello(to: id)
        case "key":
            let from = str("from")
            // Seuls les trois plus anciens présents remettent la clé.
            guard key == nil, mayHandKey(from: from), let x = peers[from]?.x,
                  let k = identity.unwrapGroupKey(str("blob"), from: x)
            else { return }
            setKey(k)
            retries = 0
            hello()
            let waiting = pending
            pending.removeAll()
            for e in waiting { relay(e) }
        case "left":
            let wasInCall = peers[str("id")]?.voice == true
            peers[str("id")] = nil
            if wasInCall { onCallChanged?() }
        case "relay":
            if key == nil {
                pending.append(event)
            } else {
                relay(event)
            }
        case "sent":
            if let i = messages.firstIndex(where: { $0.id == str("id") && $0.mine }) {
                messages[i].pending = false
            }
        case "moderated":
            messages.removeAll { $0.id == str("id") && $0.mine }
            if event["banned"] as? Bool == true { return leave(.banned) }
            notice = .photoRefused(str("reason"))
        case "admin":
            let was = iAmAdmin
            admin = event["id"] as? String
            if !was && iAmAdmin { notice = .nowAdmin }
        case "renamed":
            guard let room = event["room"] as? [String: Any], (room["id"] as? String) == info.id,
                  let name = room["name"] as? String
            else { return }
            info.name = name
            if let sig = room["sig"] as? String { info.sig = sig }
            if let at = room["expires_at"] as? NSNumber { info.expiresAt = at.int64Value }
            notice = .renamed(name, byMe: iAmAdmin)
            onRenamed?(info)
        case "rename_refused":
            notice = .renameRefused(str("reason"))
        case "closed":
            switch str("reason") {
            case "banned": leave(.banned)
            case "expelled": leave(.expelled)
            default: leave(.midnight)
            }
        default:
            break
        }
    }

    func setKey(_ k: Key) {
        key = k
        fingerprint = Brocolis.fingerprint(k)
        state = .ready
        keyTimer?.cancel()
    }

    /// Suis-je parmi les trois plus anciens (ceux qui remettent la clé) ?
    func keyHolder(except: String) -> Bool {
        var orders = peers.filter { $0.key != except }.map(\.value.order)
        orders.append(order)
        return orders.sorted().prefix(3).contains(order)
    }

    /// `from` peut-il me remettre la clé ? Seulement s'il est parmi les trois
    /// plus anciens arrivés avant moi.
    func mayHandKey(from: String) -> Bool {
        peers.filter { $0.value.order < order }
            .sorted { ($0.value.order, $0.key) < ($1.value.order, $1.key) }
            .prefix(3).contains { $0.key == from }
    }

    /// Un texte reçu : masqué s'il est interdit.
    func screen(_ text: String) -> String {
        Moderation.check(text) == nil ? text : hiddenText
    }

    @ObservationIgnored public var hiddenText = "Message masqué : contenu interdit sur Alley."

    func relay(_ event: [String: Any]) {
        let from = event["from"] as? String ?? ""
        let data = event["data"] as? String ?? ""
        guard let key, var peer = peers[from] else { return }
        let opened = Envelope.open(key: key, token: data, signer: peer.s, room: info.id, me: me, seen: peer.seen)
        let body: String
        switch opened {
        case .success(let (b, n)):
            body = b
            peer.seen.mark(n)
            peers[from] = peer
        case .failure(.unreadable):
            return
        case .failure:
            // Pas signé comme il faut : on prévient, une fois par membre.
            if !peer.warned {
                peer.warned = true
                peers[from] = peer
                notice = .signatureRejected(name(of: from))
            }
            return
        }
        guard let p = (try? JSONSerialization.jsonObject(with: Data(body.utf8))) as? [String: Any] else { return }
        let str = { (k: String) in p[k] as? String ?? "" }
        switch str("k") {
        case "hello":
            var name = String(str("n").prefix(32))
            if Moderation.check(name) != nil { name = "" }
            peers[from]?.name = name.trimmingCharacters(in: .whitespaces).isEmpty ? guestName(from) : name
            let voice = p["voice"] as? Bool ?? false
            let callChanged = peers[from]?.voice != voice || peers[from]?.video != (p["video"] as? Bool ?? false)
            peers[from]?.voice = voice
            peers[from]?.muted = p["muted"] as? Bool ?? false
            peers[from]?.video = p["video"] as? Bool ?? false
            if callChanged { onCallChanged?() }
            // Les messages déjà là prennent son nom.
            for i in messages.indices where messages[i].from == from { messages[i].name = self.name(of: from) }
        case "msg":
            let kind = Message.Kind(rawValue: str("kind")) ?? .text
            var media: Data?
            var mime = str("m")
            switch kind {
            case .image:
                // Une photo n'est montrée que si le serveur l'a vérifiée.
                guard let jpeg = verifiedImage(event, ik: str("ik")) else { return }
                media = jpeg
                mime = "image/jpeg"
            case .audio:
                media = Brocolis.unb64(str("x"))
                if !mime.hasPrefix("audio/") { mime = "audio/mp4" }
            case .text:
                break
            }
            let msg = Message(
                id: str("mid"), from: from, name: name(of: from), date: Date(), kind: kind,
                text: kind == .text ? screen(str("x")) : "", media: media, mime: mime, reply: str("r"))
            peers[from]?.typingUntil = .distantPast
            messages.append(msg)
            trim()
            onIncoming?(msg)
        case "edit":
            // Seul l'auteur d'un message peut le modifier.
            if let i = messages.firstIndex(where: { $0.id == str("mid") && $0.from == from }) {
                messages[i].text = screen(str("x"))
                messages[i].edited = true
            }
        case "del":
            let byAdmin = admin == from
            messages.removeAll { $0.id == str("mid") && ($0.from == from || byAdmin) }
        case "react":
            let e = str("e")
            guard reactionEmojis.contains(e), let i = messages.firstIndex(where: { $0.id == str("mid") }) else {
                return
            }
            RoomSession.apply(&messages[i].reactions, e, who: from, on: p["on"] as? Bool ?? false)
        case "typing":
            peers[from]?.typingUntil = Date().addingTimeInterval(6)
        case "signal":
            if let d = p["d"] as? [String: Any] { onSignal?(from, d) }
        default:
            break
        }
    }

    /// La photo : sa clé (dans le message chiffré) doit avoir l'empreinte que
    /// le serveur a relayée, et ouvrir l'image qu'il a vue.
    func verifiedImage(_ event: [String: Any], ik: String) -> Data? {
        guard let k = Brocolis.unb64(ik), k.count == 32,
              let kc = event["kc"] as? String, Brocolis.keyCommitment(k) == kc,
              let c = event["c"] as? String, let jpeg = Brocolis.openBytesToken(k, c),
              jpeg.starts(with: [0xFF, 0xD8, 0xFF])
        else { return nil }
        return jpeg
    }
}

/// La connexion en direct : une WebSocket (URLSession).
final class WebSocketTransport: RoomTransport, @unchecked Sendable {
    var task: URLSessionWebSocketTask!
    weak var session: RoomSession?
    var urlSession: URLSession!
    var closed = false

    @MainActor
    init(url: URL, session: RoomSession) {
        self.session = session
        urlSession = URLSession(configuration: .default)
        task = urlSession.webSocketTask(with: url)
        task.resume()
        listen()
    }

    func listen() {
        task.receive { [weak self] result in
            guard let self, !self.closed else { return }
            switch result {
            case .success(.string(let text)):
                Task { @MainActor [weak self] in self?.session?.receive(text) }
                self.listen()
            case .success:
                self.listen()
            case .failure(let error):
                if ProcessInfo.processInfo.environment["ALLEY_DEBUG"] != nil { print("websocket :", error) }
                self.finish(code: self.task.closeCode.rawValue)
            }
        }
    }

    func finish(code: Int) {
        guard !closed else { return }
        closed = true
        urlSession.invalidateAndCancel()
        Task { @MainActor [weak self] in self?.session?.lost(code: code == 0 ? 1006 : code) }
    }

    func send(_ text: String) {
        task.send(.string(text)) { _ in }
    }

    func close() {
        closed = true
        task.cancel(with: .normalClosure, reason: nil)
        urlSession.invalidateAndCancel()
    }
}
