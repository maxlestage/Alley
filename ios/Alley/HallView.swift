// Le hall : créer un salon, rejoindre par un lien, ses salons (gardés sur
// l'iPhone, et ceux du compte), les salons publics ouverts.

import AlleyKit
import SwiftUI

struct HallView: View {
    @Environment(AppModel.self) private var model
    @State private var sheet: Sheet?
    @State private var confirmForget = false

    enum Sheet: String, Identifiable {
        case create, join, settings
        var id: String { rawValue }
    }

    struct Mine: Identifiable {
        let info: RoomInfo
        let recent: RecentRoom?
        var id: String { info.id }
    }

    /// Mes salons : ceux de l'iPhone, puis ceux du compte qu'il ne connaît pas.
    var myRooms: [Mine] {
        let here = model.recents.filter { $0.server == model.serverName }
        var out = here.map { Mine(info: $0.info, recent: $0) }
        for r in model.mine where !out.contains(where: { $0.info.id == r.id }) {
            out.append(Mine(info: r, recent: nil))
        }
        return out
    }

    var body: some View {
        @Bindable var model = model
        let t = model.t
        List {
            Section {
                VStack(alignment: .leading, spacing: 10) {
                    Text(t.lobbyIntro).font(.subheadline).foregroundStyle(.secondary)
                    HStack(spacing: 10) {
                        QuickButton(title: t.newRoom, icon: "plus", prominent: true) { sheet = .create }
                            .accessibilityIdentifier("create-btn")
                        QuickButton(title: t.joinRoom, icon: "link") { sheet = .join }
                            .accessibilityIdentifier("join-btn")
                    }
                }
                .listRowInsets(EdgeInsets(top: 8, leading: 16, bottom: 12, trailing: 16))
            }
            Section {
                if myRooms.isEmpty {
                    Text(t.myRoomsEmpty).font(.footnote).foregroundStyle(.secondary)
                }
                ForEach(myRooms) { item in
                    RoomRow(info: item.info, created: item.recent?.created == true)
                        .swipeActions {
                            if let recent = item.recent {
                                Button(t.forget, role: .destructive) { model.forget(recent) }
                            }
                        }
                }
            } header: {
                HStack {
                    Text(t.myRooms)
                    Spacer()
                    if !model.recents.isEmpty {
                        Button(t.forgetAll) { confirmForget = true }
                            .font(.caption.weight(.semibold))
                            .textCase(nil)
                    }
                }
            }
            Section(t.lobbyTitle) {
                if model.rooms.isEmpty && !model.loading {
                    Text(t.noRooms).font(.footnote).foregroundStyle(.secondary)
                }
                ForEach(model.rooms) { RoomRow(info: $0, created: false) }
            }
        }
        .listStyle(.insetGrouped)
        .navigationTitle("Alley")
        .searchable(text: $model.query, prompt: t.search)
        .task(id: model.query) {
            // Une recherche toutes les 250 ms au plus.
            try? await Task.sleep(for: .milliseconds(model.query.isEmpty ? 0 : 250))
            guard !Task.isCancelled else { return }
            await model.loadHall()
        }
        .task {
            // La liste se met à jour toute seule.
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(15))
                if !Task.isCancelled { await model.loadHall() }
            }
        }
        .refreshable { await model.loadHall() }
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button { sheet = .settings } label: {
                    Label(t.settings, systemImage: "gearshape")
                }
                .accessibilityIdentifier("settings-btn")
            }
            ToolbarItem(placement: .topBarLeading) {
                Menu {
                    Button { sheet = .create } label: { Label(t.newRoom, systemImage: "plus") }
                    Button { sheet = .join } label: { Label(t.joinRoom, systemImage: "link") }
                    Button { sheet = .settings } label: { Label(t.settings, systemImage: "gearshape") }
                    Divider()
                    Button(role: .destructive) { model.logout() } label: {
                        Label(model.account == nil ? t.changePseudonym : t.logout, systemImage: "rectangle.portrait.and.arrow.right")
                    }
                } label: {
                    HStack(spacing: 6) {
                        Avatar(id: "moi", name: model.pseudonym, size: 28)
                        if model.account != nil { Image(systemName: "key.fill").font(.caption2) }
                    }
                }
                .accessibilityLabel(model.pseudonym)
            }
        }
        .sheet(item: $sheet) { which in
            switch which {
            case .create: CreateRoomSheet()
            case .join: JoinSheet()
            case .settings: SettingsView()
            }
        }
        .confirmationDialog(t.confirmForgetAll, isPresented: $confirmForget, titleVisibility: .visible) {
            Button(t.forgetAll, role: .destructive) { model.forgetAll() }
        }
    }
}

struct QuickButton: View {
    let title: String
    let icon: String
    var prominent = false
    let action: () -> Void

    var body: some View {
        let label = Label(title, systemImage: icon)
            .font(.subheadline.weight(.semibold))
            .frame(maxWidth: .infinity, minHeight: 44)
        if prominent {
            Button(action: action) { label }
                .buttonStyle(.borderedProminent)
                .buttonBorderShape(.capsule)
        } else {
            Button(action: action) { label }
                .buttonStyle(.bordered)
                .buttonBorderShape(.capsule)
        }
    }
}

/// Un salon dans une liste : son nom, qui est là, quand il disparaît.
struct RoomRow: View {
    @Environment(AppModel.self) private var model
    let info: RoomInfo
    let created: Bool

    var body: some View {
        let t = model.t
        Button {
            Task { await model.join(info.id) }
        } label: {
            HStack(spacing: 12) {
                Text(info.isPrivate ? "🔒" : "#")
                    .font(.title3.weight(.bold))
                    .foregroundStyle(.secondary)
                    .frame(width: 28)
                VStack(alignment: .leading, spacing: 2) {
                    HStack(spacing: 6) {
                        Text(info.name).font(.body.weight(.semibold)).foregroundStyle(.primary).lineLimit(1)
                        if created { Image(systemName: "crown.fill").font(.caption2).foregroundStyle(.yellow) }
                    }
                    Text("\(fill(t.people, String(info.people))) · \(fill(t.closesAt, Self.when(info.expires)))")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Image(systemName: "chevron.right").font(.caption.weight(.semibold)).foregroundStyle(.tertiary)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    /// « 00:00 », « ven. 00:00 » ou « 12 nov. ».
    static func when(_ date: Date) -> String {
        let days = date.timeIntervalSinceNow / 86_400
        if days > 6 { return date.formatted(.dateTime.day().month(.abbreviated)) }
        if days > 1 { return date.formatted(.dateTime.weekday(.abbreviated).hour().minute()) }
        return date.formatted(date: .omitted, time: .shortened)
    }
}

/// Les initiales, sur une couleur tirée de l'identifiant.
struct Avatar: View {
    let id: String
    let name: String
    var size: CGFloat = 36

    var body: some View {
        let hue = Double(id.unicodeScalars.reduce(0) { ($0 &* 31 &+ Int($1.value)) % 360 }) / 360
        Circle()
            .fill(Color(hue: hue, saturation: 0.55, brightness: 0.7))
            .frame(width: size, height: size)
            .overlay {
                Text(Self.initials(name))
                    .font(.system(size: size * 0.4, weight: .bold))
                    .foregroundStyle(.white)
            }
            .accessibilityHidden(true)
    }

    static func initials(_ name: String) -> String {
        let parts = name.split(separator: " ").prefix(2)
        let letters = parts.compactMap(\.first).map(String.init).joined()
        return letters.isEmpty ? "?" : letters.uppercased()
    }
}

struct CreateRoomSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var isPrivate = false
    @FocusState private var focused: Bool

    var body: some View {
        let t = model.t
        NavigationStack {
            Form {
                Section {
                    TextField(t.newRoomPlaceholder, text: $name)
                        .focused($focused)
                        .submitLabel(.go)
                        .onSubmit(create)
                        .accessibilityIdentifier("new-room-name")
                } header: {
                    Text(t.roomName)
                } footer: {
                    Text(t.newRoomHint)
                }
                Section {
                    Toggle(isOn: $isPrivate) {
                        VStack(alignment: .leading) {
                            Text(t.privateRoom)
                            Text(t.privateRoomHint).font(.caption).foregroundStyle(.secondary)
                        }
                    }
                }
            }
            .navigationTitle(t.newRoom)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button(t.cancel) { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button(t.create, action: create).disabled(name.trimmingCharacters(in: .whitespaces).count < 2)
                }
            }
            .onAppear { focused = true }
        }
        .presentationDetents([.medium, .large])
    }

    func create() {
        let name = name
        let isPrivate = isPrivate
        guard name.trimmingCharacters(in: .whitespaces).count >= 2 else { return }
        dismiss()
        Task { await model.create(name: name, isPrivate: isPrivate) }
    }
}

struct JoinSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var link = ""
    @State private var invalid = false
    @FocusState private var focused: Bool

    var body: some View {
        let t = model.t
        NavigationStack {
            Form {
                Section {
                    TextField(t.joinPlaceholder, text: $link)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .keyboardType(.URL)
                        .focused($focused)
                        .submitLabel(.go)
                        .onSubmit(join)
                        .accessibilityIdentifier("join-link")
                    PasteButton(payloadType: String.self) { strings in
                        if let s = strings.first { link = s.trimmingCharacters(in: .whitespacesAndNewlines) }
                    }
                } header: {
                    Text(t.joinLabel)
                } footer: {
                    Text(invalid ? t.joinInvalid : t.joinHint).foregroundStyle(invalid ? .red : .secondary)
                }
            }
            .navigationTitle(t.joinRoom)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button(t.cancel) { dismiss() } }
                ToolbarItem(placement: .confirmationAction) { Button(t.join, action: join).disabled(link.isEmpty) }
            }
            .onAppear { focused = true }
        }
        .presentationDetents([.medium])
    }

    func join() {
        guard let id = Links.room(from: link) else {
            invalid = true
            return
        }
        dismiss()
        Task { await model.join(id) }
    }
}
