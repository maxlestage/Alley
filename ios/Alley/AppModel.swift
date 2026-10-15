// L'état de l'application : qui l'on est, le hall (salons ouverts, salons
// de l'appareil, salons du compte), le salon où l'on est. Rien n'est gardé
// des conversations : quitter un salon, c'est tout oublier.

import AlleyKit
import Foundation
import Observation

@MainActor
@Observable
final class AppModel {
    /// Le serveur Alley (Info.plist `AlleyServer`).
    let server: URL
    let api: API
    let vault: Vault

    var lang: Lang
    var t: L10n { lang.t }
    var prefs: Prefs {
        didSet { vault.prefs = prefs }
    }

    /// Le pseudonyme (vide : l'écran d'accueil).
    var pseudonym = ""
    var account: Account?

    var query = ""
    var rooms: [RoomInfo] = []
    var recents: [RecentRoom] = []
    var mine: [RoomInfo] = []
    var loading = false
    var room: RoomSession?
    /// L'appel en cours dans ce salon.
    var call: CallEngine?
    var banned = false
    /// Le bandeau du moment.
    var toast: Toast?

    struct Toast: Equatable, Identifiable {
        let id = UUID()
        let text: String
        let error: Bool
    }

    /// Un salon d'un lien reçu avant d'avoir choisi son pseudonyme.
    var invited: String?
    @ObservationIgnored var serverKey: String?

    init() {
        let configured = Bundle.main.object(forInfoDictionaryKey: "AlleyServer") as? String
        server = URL(string: configured ?? "https://alley-app-6af9b14c3489.herokuapp.com")!
        api = API(base: server)
        vault = Vault(deviceKey: Keychain.deviceKey())
        let prefs = vault.prefs
        self.prefs = prefs
        lang = prefs.lang.flatMap(Lang.init(rawValue:)) ?? .device
        recents = vault.recents()
        warnIfTampered()
    }

    var serverName: String { server.absoluteString.trimmingCharacters(in: CharacterSet(charactersIn: "/")) }

    func show(_ text: String, error: Bool = false) {
        toast = Toast(text: text, error: error)
    }

    func warnIfTampered() {
        if vault.takeTampered() { show(t.storageTampered, error: true) }
    }

    func setLang(_ l: Lang) {
        lang = l
        prefs.lang = l.rawValue
    }

    // MARK: Entrer, sortir

    /// Entre, avec ou sans compte (le mot de passe ne quitte jamais l'iPhone :
    /// seule sa marque est envoyée).
    func enter(pseudonym: String, password: String) async -> String? {
        let name = pseudonym.trimmingCharacters(in: .whitespacesAndNewlines)
        guard (2...32).contains(name.count) else { return t.pseudonymTooShort }
        if Moderation.check(name) != nil { return t.forbiddenName }
        if !password.isEmpty {
            guard password.count >= 8 else { return t.passwordTooShort }
            // 310 000 tours de PBKDF2 : hors du fil principal.
            let account = await Task.detached(priority: .userInitiated) {
                Account(pseudonym: name, password: password)
            }.value
            self.account = account
            vault.accountKey = account.vaultKey
        } else {
            account = nil
            vault.accountKey = nil
        }
        self.pseudonym = name
        recents = vault.recents()
        warnIfTampered()
        if let id = invited {
            invited = nil
            await join(id)
        } else {
            await loadHall()
        }
        return nil
    }

    func logout() {
        leave()
        pseudonym = ""
        account = nil
        mine = []
        vault.accountKey = nil
        recents = vault.recents()
    }

    // MARK: Le hall

    func loadHall() async {
        loading = true
        defer { loading = false }
        do {
            rooms = try await api.list(query: query)
        } catch let e as APIError {
            handle(e)
        } catch {}
        if let account, let list = try? await api.mine(account) {
            mine = list
        }
        await loadKnown()
    }

    /// Les salons de l'appareil : ceux que le serveur connaît encore (il
    /// rouvre ceux qu'il a oubliés, d'après leur description signée).
    func loadKnown() async {
        let mineHere = vault.recents().filter { $0.server == serverName }
        guard !mineHere.isEmpty else {
            recents = vault.recents()
            return
        }
        if let open = try? await api.known(mineHere.map(\.info)) {
            let ids = Set(open.map(\.id))
            var kept = vault.recents().filter { $0.server != serverName || ids.contains($0.info.id) }
            for i in kept.indices {
                if let fresh = open.first(where: { $0.id == kept[i].info.id }) {
                    kept[i].info.people = fresh.people
                }
            }
            vault.saveRecents(kept)
        }
        if serverKey == nil { serverKey = try? await api.serverKey() }
        if let serverKey { vault.verify(server: serverName, serverKey: serverKey) }
        recents = vault.recents()
        warnIfTampered()
    }

    func forget(_ r: RecentRoom) {
        vault.forget(r.info.id, server: r.server)
        recents = vault.recents()
    }

    func forgetAll() {
        vault.forgetAll()
        recents = []
        show(t.forgottenAll)
    }

    func handle(_ e: APIError) {
        if e.banned {
            banned = true
            return
        }
        show(text(for: e), error: true)
    }

    func text(for e: APIError) -> String {
        switch (e.status, e.code) {
        case (0, _): t.errNetwork
        case (_, "forbidden_name"): t.forbiddenName
        case (_, "bad_name"): t.badName
        case (_, "too_many_rooms"): t.errTooManyRooms
        case (_, "room_gone"), (404, _): t.closedGone
        case (429, _): t.errRate
        default: fill(t.errGeneric, String(e.status))
        }
    }

    // MARK: Les salons

    func create(name: String, isPrivate: Bool) async {
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        if Moderation.check(name) != nil { return show(t.forbiddenName, error: true) }
        do {
            let info = try await api.create(name: name, isPrivate: isPrivate, account: account)
            vault.remember(info, server: serverName, created: true)
            await join(info.id)
        } catch let e as APIError {
            handle(e)
        } catch {}
    }

    /// Entre dans un salon (rouvert d'après sa description signée si le
    /// serveur l'a oublié).
    func join(_ id: String) async {
        leave()
        var info: RoomInfo
        do {
            info = try await api.room(id)
        } catch let e as APIError {
            if e.status == 404, let kept = vault.recents().first(where: { $0.info.id == id && $0.server == serverName }),
               !kept.info.sig.isEmpty, (try? await api.known([kept.info])) != nil,
               let again = try? await api.room(id)
            {
                info = again
            } else {
                handle(e)
                return await loadHall()
            }
        } catch {
            return
        }
        vault.remember(info, server: serverName, created: false)
        recents = vault.recents()
        let session = RoomSession(info: info, api: api, pseudonym: pseudonym, account: account)
        session.guestName = { [t] id in fill(t.guestName, String(id.prefix(4))) }
        session.hiddenText = t.hiddenMessage
        session.onIncoming = { [weak self] msg in self?.incoming(msg) }
        session.onRenamed = { [weak self] info in
            guard let self else { return }
            self.vault.remember(info, server: self.serverName, created: false)
            self.recents = self.vault.recents()
        }
        room = session
        session.connect()
    }

    func leave() {
        leaveCall()
        room?.leave()
        room = nil
    }

    /// Rejoint l'appel du salon (micro, puis caméra si on veut).
    func joinCall() async {
        guard let room, call == nil else { return }
        guard room.hasKey else { return show(t.keyNotArrived, error: true) }
        let engine = CallEngine(room: room, iceServers: await api.iceServers())
        call = engine
        if !(await engine.start()) {
            call = nil
            show(t.micDenied, error: true)
        }
    }

    func leaveCall() {
        call?.stop()
        call = nil
    }

    /// Le salon s'est fermé de lui-même : on dit pourquoi, et retour au hall.
    func closed(_ why: Closed) {
        leaveCall()
        room = nil
        switch why {
        case .left: break
        case .midnight: show(t.closedMidnight)
        case .expelled: show(t.closedExpelled, error: true)
        case .banned: banned = true
        case .gone: show(t.closedGone, error: true)
        case .connectionLost: show(t.connectionLost, error: true)
        }
        Task { await loadHall() }
    }

    func incoming(_ msg: Message) {
        guard let room, !prefs.isMuted(room.info.id) else { return }
        Notifier.message(
            from: msg.name, room: room.info.name,
            body: msg.kind == .image ? t.photo : msg.kind == .audio ? t.voice : msg.text,
            sound: prefs.sound, enabled: prefs.notify)
    }

    /// Le texte d'un bandeau du salon.
    func text(for notice: Notice) -> (String, Bool) {
        switch notice {
        case .nowAdmin: (t.nowAdmin, false)
        case .renamed(let name, let byMe): (fill(byMe ? t.renamedByMe : t.renamedTo, name), false)
        case .renameRefused(let reason):
            (reason == "name_taken" ? t.nameTaken : reason == "forbidden_name" ? t.forbiddenName : t.badName, true)
        case .signatureRejected(let who): (fill(t.signatureRejected, who), true)
        case .photoRefused(let reason): (photoRefusal(reason), true)
        case .refused(let c): (c == .sexual ? t.refusedSexual : t.refusedDrugs, true)
        case .keyNotArrived: (t.keyNotArrived, true)
        case .noKeyRetry: (t.noKeyRetry, true)
        case .reconnecting: (t.reconnecting, false)
        }
    }

    func photoRefusal(_ reason: String) -> String {
        switch reason {
        case "sexual": t.photoRefusedSexual
        case "drugs": t.photoRefusedDrugs
        case "slow_down": t.photoSlowDown
        case "unavailable": t.photoUnavailable
        default: t.photoInvalid
        }
    }

    /// Un lien `…#salon=<id>` ouvert depuis ailleurs (Safari, Messages…).
    func open(_ url: URL) {
        guard let id = Links.room(from: url.absoluteString) else { return }
        if pseudonym.isEmpty {
            invited = id
        } else {
            Task { await join(id) }
        }
    }

    func inviteLink(_ id: String) -> URL {
        Links.invite(server: server, room: id, lang: lang.rawValue)
    }

    func siteLink(_ slug: String) -> URL {
        let prefix = lang == .fr ? "" : "/\(lang.rawValue)"
        return URL(string: "\(serverName)\(prefix)/\(slug == "faq" ? "#faq" : "\(slug)/")")!
    }

    /// L'adresse de contact de l'éditeur (Info.plist `AlleyContact`).
    var contactEmail: String? {
        (Bundle.main.object(forInfoDictionaryKey: "AlleyContact") as? String).flatMap { $0.isEmpty ? nil : $0 }
    }

    /// Un e-mail de signalement prérempli (le salon, l'heure, le message).
    func reportURL(room: RoomInfo, message: Message? = nil) -> URL? {
        guard let email = contactEmail else { return nil }
        let when = Date().formatted(date: .abbreviated, time: .shortened)
        var body = fill(t.reportBody, room.name, room.id, when)
        if let message {
            let excerpt = message.kind == .text ? String(message.text.prefix(500)) : (message.kind == .image ? t.photo : t.voice)
            body = fill(t.reportMessageBody, message.name, excerpt) + body
        }
        var c = URLComponents()
        c.scheme = "mailto"
        c.path = email
        c.queryItems = [
            URLQueryItem(name: "subject", value: fill(t.reportSubject, room.name)),
            URLQueryItem(name: "body", value: body),
        ]
        return c.url
    }
}
