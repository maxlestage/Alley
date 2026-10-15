// Les présents, les réglages du salon, les réglages de l'application, et le
// lecteur des messages vocaux.

import AlleyKit
import Observation
import SwiftUI
import UIKit

/// Qui est là, l'administrateur (👑), l'empreinte de la clé ; exclure.
struct MembersSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let room: RoomSession
    @State private var expel: String?

    var body: some View {
        let t = model.t
        let others = room.peers.sorted { $0.value.order < $1.value.order }.map(\.key)
        NavigationStack {
            List {
                Section {
                    member(room.me, name: "\(model.pseudonym) (\(t.you))")
                    ForEach(others, id: \.self) { id in
                        member(id, name: room.name(of: id))
                            .swipeActions {
                                if room.iAmAdmin {
                                    Button(t.expel, role: .destructive) { expel = id }
                                }
                            }
                    }
                }
                Section {
                    Text(room.fingerprint.isEmpty ? t.receivingKey : room.fingerprint)
                        .font(.system(.body, design: .monospaced).weight(.semibold))
                } header: {
                    Text(t.fingerprint)
                } footer: {
                    Text(t.fingerprintHint)
                }
            }
            .navigationTitle(fill(t.people, String(room.present)))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button(t.done) { dismiss() } } }
            .confirmationDialog(
                fill(t.confirmExpel, expel.map { room.name(of: $0) } ?? ""),
                isPresented: Binding(get: { expel != nil }, set: { if !$0 { expel = nil } }),
                titleVisibility: .visible
            ) {
                Button(t.expel, role: .destructive) {
                    if let id = expel { room.expel(id) }
                    expel = nil
                }
            }
        }
        .presentationDetents([.medium, .large])
    }

    func member(_ id: String, name: String) -> some View {
        HStack(spacing: 10) {
            Avatar(id: id, name: name, size: 32)
            Text(name)
            if room.admin == id {
                Text("👑").accessibilityLabel(model.t.admin)
            }
            Spacer()
            if room.iAmAdmin && id != room.me {
                Button(model.t.expel) { expel = id }
                    .font(.caption.weight(.semibold))
                    .buttonStyle(.bordered)
                    .tint(.red)
            }
        }
    }
}

/// Les réglages du salon : inviter, sourdine, renommer (l'administrateur),
/// à propos, signaler, quitter.
struct RoomSettingsSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let room: RoomSession
    @State private var newName = ""
    @State private var copied = false

    var body: some View {
        let t = model.t
        let link = model.inviteLink(room.info.id)
        NavigationStack {
            Form {
                Section {
                    Text(link.absoluteString).font(.footnote.monospaced()).textSelection(.enabled).lineLimit(2)
                    HStack {
                        Button {
                            UIPasteboard.general.url = link
                            copied = true
                        } label: {
                            Label(copied ? t.linkCopied : t.copyLink, systemImage: copied ? "checkmark" : "doc.on.doc")
                        }
                        Spacer()
                        ShareLink(item: link) { Label(t.share, systemImage: "square.and.arrow.up") }
                    }
                } header: {
                    Text(t.invite)
                } footer: {
                    Text(room.info.isPrivate ? t.roomPrivateNote : t.roomPublicNote)
                }
                Section {
                    Toggle(isOn: Binding(
                        get: { model.prefs.isMuted(room.info.id) },
                        set: { model.prefs.setMuted(room.info.id, $0) }
                    )) {
                        VStack(alignment: .leading) {
                            Text(t.mute)
                            Text(t.muteHint).font(.caption).foregroundStyle(.secondary)
                        }
                    }
                }
                if room.iAmAdmin {
                    Section(t.renameTitle) {
                        TextField(t.roomName, text: $newName)
                            .submitLabel(.done)
                            .onSubmit(rename)
                        Button(t.rename, action: rename)
                            .disabled(newName.trimmingCharacters(in: .whitespaces).count < 2 || newName == room.info.name)
                    }
                }
                Section(t.about) {
                    LabeledContent(t.roomName, value: room.info.name)
                    LabeledContent(t.closes, value: room.info.expires.formatted(date: .complete, time: .shortened))
                    LabeledContent(t.fingerprint, value: room.fingerprint.isEmpty ? "…" : room.fingerprint)
                    LabeledContent(t.role, value: room.iAmAdmin ? t.roleAdmin : t.roleMember)
                }
                Section {
                    if let url = model.reportURL(room: room.info) {
                        Link(destination: url) { Label(t.report, systemImage: "flag") }
                    }
                    Button(role: .destructive) {
                        dismiss()
                        model.leave()
                        Task { await model.loadHall() }
                    } label: {
                        Label(t.leave, systemImage: "rectangle.portrait.and.arrow.right")
                    }
                }
            }
            .navigationTitle(t.roomSettings)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button(t.done) { dismiss() } } }
            .onAppear { newName = room.info.name }
        }
    }

    func rename() {
        if room.rename(newName) { dismiss() }
    }
}

/// Les réglages : compte, affichage, notifications, l'iPhone, l'aide.
struct SettingsView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var confirmForget = false

    var body: some View {
        @Bindable var model = model
        let t = model.t
        NavigationStack {
            Form {
                Section(t.account) {
                    HStack(spacing: 12) {
                        Avatar(id: "moi", name: model.pseudonym, size: 44)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(model.pseudonym).font(.headline)
                            Text(model.account == nil ? t.guestNote : t.signedInNote).font(.caption).foregroundStyle(.secondary)
                        }
                    }
                    Button(model.account == nil ? t.changePseudonym : t.logout, role: .destructive) {
                        dismiss()
                        model.logout()
                    }
                    .accessibilityIdentifier("logout")
                }
                Section(t.display) {
                    Picker(t.theme, selection: $model.prefs.theme) {
                        Text(t.themeAuto).tag(Prefs.Theme.auto)
                        Text(t.themeLight).tag(Prefs.Theme.light)
                        Text(t.themeDark).tag(Prefs.Theme.dark)
                    }
                    .pickerStyle(.segmented)
                    Picker(t.language, selection: Binding(get: { model.lang }, set: { model.setLang($0) })) {
                        ForEach(Lang.allCases) { Text($0.name).tag($0) }
                    }
                }
                Section(t.alerts) {
                    Toggle(isOn: Binding(
                        get: { model.prefs.notify },
                        set: { on in
                            model.prefs.notify = on
                            if on {
                                Task {
                                    let granted = await Notifier.requestPermission()
                                    if !granted { model.prefs.notify = false }
                                }
                            }
                        }
                    )) {
                        VStack(alignment: .leading) {
                            Text(t.notifications)
                            Text(t.notificationsHint).font(.caption).foregroundStyle(.secondary)
                        }
                    }
                    Toggle(isOn: $model.prefs.sound) {
                        VStack(alignment: .leading) {
                            Text(t.sound)
                            Text(t.soundHint).font(.caption).foregroundStyle(.secondary)
                        }
                    }
                }
                Section {
                    Button(t.forgetAll, role: .destructive) { confirmForget = true }
                } header: {
                    Text(t.device)
                } footer: {
                    Text(t.deviceNote)
                }
                Section(t.help) {
                    Link(t.helpLink, destination: model.siteLink("faq"))
                    Link(t.rules, destination: model.siteLink("rules"))
                    Link(t.terms, destination: model.siteLink("terms"))
                    Link(t.privacy, destination: model.siteLink("privacy"))
                    Link(t.legal, destination: model.siteLink("legal"))
                    if let email = model.contactEmail, let url = URL(string: "mailto:\(email)") {
                        Link(t.contact, destination: url)
                    }
                }
                Section {
                    LabeledContent("Alley", value: Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "")
                    Label(model.server.host ?? "", systemImage: "server.rack")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
            }
            .navigationTitle(t.settings)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button(t.done) { dismiss() } } }
            .confirmationDialog(t.confirmForgetAll, isPresented: $confirmForget, titleVisibility: .visible) {
                Button(t.forgetAll, role: .destructive) { model.forgetAll() }
            }
        }
    }
}
