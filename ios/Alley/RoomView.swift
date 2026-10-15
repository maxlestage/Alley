// Un salon : les messages (texte, photos, vocaux), les réponses, les
// réactions, le menu de chaque message, la saisie (emoji, photo), les
// présents et les réglages du salon.

import AlleyKit
import PhotosUI
import SwiftUI
import UIKit

struct RoomView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.scenePhase) private var scenePhase
    let room: RoomSession

    @State private var draft = ""
    @State private var replyTo: String?
    @State private var editing: String?
    @State private var showMembers = false
    @State private var showSettings = false
    @State private var atBottom = true
    @State private var confirmDelete: String?
    @FocusState private var inputFocused: Bool

    var body: some View {
        let t = model.t
        VStack(spacing: 0) {
            CallPanel(room: room)
            messages
            if !room.typing.isEmpty {
                Text(fill(room.typing.count == 1 ? t.typingOne : t.typingMany, room.typing.joined(separator: ", ")))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 4)
            }
            Composer(
                room: room, draft: $draft, replyTo: $replyTo, editing: $editing, focused: $inputFocused)
        }
        .navigationTitle(room.info.name)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarLeading) {
                Button {
                    model.leave()
                    Task { await model.loadHall() }
                } label: {
                    Label(t.leave, systemImage: "chevron.backward")
                }
                .accessibilityIdentifier("leave")
            }
            ToolbarItem(placement: .principal) {
                VStack(spacing: 0) {
                    HStack(spacing: 4) {
                        Text(room.info.isPrivate ? "🔒" : "#").foregroundStyle(.secondary)
                        Text(room.info.name).font(.headline).lineLimit(1)
                    }
                    Text("\(fill(t.people, String(room.present))) · \(fill(t.closesAt, RoomRow.when(room.info.expires)))")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
                .accessibilityElement(children: .combine)
            }
            ToolbarItemGroup(placement: .topBarTrailing) {
                Button {
                    if model.call == nil { Task { await model.joinCall() } } else { model.leaveCall() }
                } label: {
                    Label(model.call == nil ? t.joinCall : t.leaveCall, systemImage: model.call == nil ? "phone" : "phone.down.fill")
                }
                .tint(model.call == nil ? Color.accentColor : Color.red)
                .accessibilityIdentifier("call-btn")
                Button { showMembers = true } label: { Label(t.members, systemImage: "person.2") }
                    .accessibilityIdentifier("members-btn")
                Button { showSettings = true } label: { Label(t.roomSettings, systemImage: "gearshape") }
                    .accessibilityIdentifier("room-settings")
            }
        }
        .sheet(isPresented: $showMembers) { MembersSheet(room: room) }
        .sheet(isPresented: $showSettings) { RoomSettingsSheet(room: room) }
        .confirmationDialog(t.confirmDelete, isPresented: Binding(get: { confirmDelete != nil }, set: { if !$0 { confirmDelete = nil } }), titleVisibility: .visible) {
            Button(t.delete, role: .destructive) {
                if let mid = confirmDelete { room.delete(mid) }
                confirmDelete = nil
            }
        }
        .onChange(of: room.notice) { _, notice in
            guard let notice else { return }
            let (text, error) = model.text(for: notice)
            model.show(text, error: error)
            room.notice = nil
        }
        .onChange(of: room.state) { _, state in
            if case .closed(let why) = state { model.closed(why) }
        }
        .onChange(of: scenePhase) { _, phase in
            // Revenu au premier plan : l'iOS a pu couper la connexion.
            if phase == .active, room.state == .closed(.connectionLost) { Task { await model.join(room.info.id) } }
        }
    }

    var messages: some View {
        let t = model.t
        return ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 2) {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("#").font(.system(size: 40, weight: .bold)).foregroundStyle(.secondary)
                        Text(fill(t.roomWelcome, room.info.name)).font(.title2.bold())
                        Text(t.roomWelcomeBody).font(.subheadline).foregroundStyle(.secondary)
                        if !room.hasKey {
                            Label(t.receivingKey, systemImage: "key")
                                .font(.footnote)
                                .foregroundStyle(.secondary)
                                .padding(.top, 8)
                        }
                    }
                    .padding(16)
                    ForEach(Array(room.messages.enumerated()), id: \.element.id) { index, m in
                        let previous = index > 0 ? room.messages[index - 1] : nil
                        let grouped = previous.map { $0.from == m.from && m.date.timeIntervalSince($0.date) < 300 && m.reply.isEmpty } ?? false
                        MessageRow(
                            room: room, message: m, grouped: grouped,
                            onReply: { replyTo = m.id; editing = nil; inputFocused = true },
                            onEdit: { editing = m.id; replyTo = nil; draft = m.text; inputFocused = true },
                            onDelete: { confirmDelete = m.id },
                            onJump: { mid in withAnimation { proxy.scrollTo(mid, anchor: .center) } }
                        )
                        .id(m.id)
                    }
                    Color.clear.frame(height: 1).id("bottom")
                        .onAppear { atBottom = true }
                        .onDisappear { atBottom = false }
                }
                .padding(.bottom, 8)
            }
            .scrollDismissesKeyboard(.interactively)
            .defaultScrollAnchor(.bottom)
            .onChange(of: room.messages.last?.id) { _, _ in
                // Mon message, ou déjà en bas : on suit la conversation.
                if atBottom || room.messages.last?.mine == true {
                    withAnimation { proxy.scrollTo("bottom", anchor: .bottom) }
                }
            }
            .overlay(alignment: .bottomTrailing) {
                if !atBottom {
                    Button {
                        withAnimation { proxy.scrollTo("bottom", anchor: .bottom) }
                    } label: {
                        Image(systemName: "arrow.down")
                            .font(.headline)
                            .foregroundStyle(.white)
                            .frame(width: 44, height: 44)
                            .background(Color.alleyAccent, in: .circle)
                            .shadow(radius: 6, y: 3)
                    }
                    .padding(14)
                    .accessibilityLabel(t.toBottom)
                    .transition(.scale.combined(with: .opacity))
                }
            }
        }
    }
}

/// Un message : l'auteur, l'heure, le contenu, la réponse citée, les
/// réactions ; et son menu (appui long).
struct MessageRow: View {
    @Environment(AppModel.self) private var model
    let room: RoomSession
    let message: Message
    let grouped: Bool
    let onReply: () -> Void
    let onEdit: () -> Void
    let onDelete: () -> Void
    let onJump: (String) -> Void

    var body: some View {
        let t = model.t
        let m = message
        VStack(alignment: .leading, spacing: 4) {
            if !m.reply.isEmpty {
                replyQuote
            }
            HStack(alignment: .top, spacing: 10) {
                if grouped {
                    Color.clear.frame(width: 36, height: 1)
                } else {
                    Avatar(id: m.from, name: m.name)
                }
                VStack(alignment: .leading, spacing: 3) {
                    if !grouped {
                        HStack(alignment: .firstTextBaseline, spacing: 6) {
                            Text(m.name).font(.subheadline.weight(.bold))
                                .foregroundStyle(Color(hue: Double(m.from.unicodeScalars.reduce(0) { ($0 &* 31 &+ Int($1.value)) % 360 }) / 360, saturation: 0.6, brightness: 0.75))
                            Text(m.date, style: .time).font(.caption2).foregroundStyle(.secondary)
                        }
                    }
                    content
                    if m.edited {
                        Text(t.edited).font(.caption2).foregroundStyle(.secondary)
                    }
                    if !m.reactions.isEmpty {
                        reactions
                    }
                }
                Spacer(minLength: 0)
            }
        }
        .padding(.horizontal, 16)
        .padding(.top, grouped ? 1 : 8)
        .contentShape(Rectangle())
        .contextMenu { menu }
        .accessibilityElement(children: .combine)
        .accessibilityActions { menu }
    }

    @ViewBuilder var content: some View {
        let t = model.t
        switch message.kind {
        case .text:
            Text(Self.markdown(message.text))
                .font(Self.onlyEmoji(message.text) ? .system(size: 40) : .body)
                .textSelection(.enabled)
                .tint(.alleyAccent)
        case .image:
            if let data = message.media, let image = UIImage(data: data) {
                Image(uiImage: image)
                    .resizable()
                    .scaledToFit()
                    .frame(maxWidth: 280, maxHeight: 320, alignment: .leading)
                    .clipShape(.rect(cornerRadius: 12))
                    .opacity(message.pending ? 0.5 : 1)
                    .overlay {
                        if message.pending {
                            Label(t.checking, systemImage: "hourglass").font(.caption.weight(.semibold))
                                .padding(8).background(.ultraThinMaterial, in: .capsule)
                        }
                    }
                    .accessibilityLabel(t.photo)
            }
        case .audio:
            VoiceNote(data: message.media, mime: message.mime, label: t.voice)
        }
    }

    var replyQuote: some View {
        let t = model.t
        let target = room.messages.first { $0.id == message.reply }
        let what: String = switch target?.kind {
        case .image: t.photo
        case .audio: t.voice
        case .text: String(target!.text.prefix(90))
        case nil: t.beforeArrival
        }
        return Button { onJump(message.reply) } label: {
            HStack(spacing: 6) {
                Image(systemName: "arrowshape.turn.up.left").font(.caption2)
                if let target { Text("@\(target.name)").font(.caption.weight(.semibold)) }
                Text(what).font(.caption).lineLimit(1)
            }
            .foregroundStyle(.secondary)
            .padding(.leading, 46)
        }
        .buttonStyle(.plain)
    }

    var reactions: some View {
        HStack(spacing: 4) {
            ForEach(reactionEmojis.filter { message.reactions[$0] != nil }, id: \.self) { e in
                let people = message.reactions[e] ?? []
                let mine = people.contains(room.me)
                Button {
                    room.react(message.id, e)
                } label: {
                    HStack(spacing: 3) {
                        Text(e)
                        Text("\(people.count)").font(.caption.weight(.bold)).foregroundStyle(mine ? .primary : .secondary)
                    }
                    .padding(.horizontal, 8)
                    .frame(minHeight: 28)
                    .background(mine ? Color.alleyAccent.opacity(0.22) : Color(.secondarySystemBackground), in: .capsule)
                    .overlay { Capsule().strokeBorder(mine ? Color.alleyAccent : .clear) }
                }
                .buttonStyle(.plain)
                .accessibilityLabel("\(e) \(people.map { $0 == room.me ? model.t.you : room.name(of: $0) }.joined(separator: ", "))")
            }
        }
    }

    @ViewBuilder var menu: some View {
        let t = model.t
        let m = message
        ControlGroup {
            ForEach(reactionEmojis.prefix(4), id: \.self) { e in
                Button(e) { room.react(m.id, e) }
            }
        }
        Menu {
            ForEach(reactionEmojis, id: \.self) { e in
                Button(e) { room.react(m.id, e) }
            }
        } label: {
            Label(t.react, systemImage: "face.smiling")
        }
        Button(action: onReply) { Label(t.reply, systemImage: "arrowshape.turn.up.left") }
        if m.kind == .text {
            Button {
                UIPasteboard.general.string = m.text
            } label: {
                Label(t.copy, systemImage: "doc.on.doc")
            }
        }
        if m.mine && m.kind == .text {
            Button(action: onEdit) { Label(t.edit, systemImage: "pencil") }
        }
        if m.mine || room.iAmAdmin {
            Button(role: .destructive, action: onDelete) { Label(t.delete, systemImage: "trash") }
        }
        if !m.mine, let url = model.reportURL(room: room.info, message: m) {
            Link(destination: url) { Label(t.reportMessage, systemImage: "flag") }
        }
    }

    /// Gras, italique, code et liens (Markdown en ligne).
    static func markdown(_ text: String) -> AttributedString {
        (try? AttributedString(
            markdown: text,
            options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace, failurePolicy: .returnPartiallyParsedIfPossible)))
            ?? AttributedString(text)
    }

    static func onlyEmoji(_ text: String) -> Bool {
        let t = text.trimmingCharacters(in: .whitespaces)
        return !t.isEmpty && t.count <= 3
            && t.unicodeScalars.allSatisfy { $0.properties.isEmojiPresentation || $0.properties.isEmoji && $0.value > 0x238C || $0.value == 0xFE0F || $0.value == 0x200D || $0.properties.generalCategory == .spaceSeparator }
    }
}

/// Un message vocal reçu.
struct VoiceNote: View {
    let data: Data?
    let mime: String
    let label: String
    @State private var player = VoicePlayer()

    var body: some View {
        Button {
            player.toggle(data, mime: mime)
        } label: {
            Label(label, systemImage: player.playing ? "pause.circle.fill" : "play.circle.fill")
                .font(.subheadline.weight(.medium))
                .padding(.horizontal, 12)
                .frame(minHeight: 40)
                .background(Color(.secondarySystemBackground), in: .capsule)
        }
        .buttonStyle(.plain)
        .disabled(data == nil)
    }
}

/// La saisie : réponse ou modification en cours, emoji, photo, envoi.
struct Composer: View {
    @Environment(AppModel.self) private var model
    let room: RoomSession
    @Binding var draft: String
    @Binding var replyTo: String?
    @Binding var editing: String?
    var focused: FocusState<Bool>.Binding
    @State private var photo: PhotosPickerItem?
    @State private var showEmoji = false
    @State private var recorder = VoiceRecorder()

    static let emojis = [
        "😀", "😂", "🥲", "😊", "😍", "🥰", "😘", "😉", "😎", "🤩", "🤔", "🤨", "😐", "🙄", "😴", "😮",
        "😢", "😭", "😡", "🤯", "🥳", "😇", "🤗", "🤫", "👍", "👎", "👏", "🙌", "🙏", "💪", "👋", "🤝",
        "❤️", "🧡", "💛", "💚", "💙", "💜", "🔥", "✨", "🎉", "🎂", "☕", "🍕", "⚽", "🎵", "✅", "❌",
    ]

    var body: some View {
        let t = model.t
        VStack(spacing: 6) {
            if let mid = replyTo ?? editing, let target = room.messages.first(where: { $0.id == mid }) {
                HStack {
                    Image(systemName: editing != nil ? "pencil" : "arrowshape.turn.up.left")
                    Text(editing != nil ? t.edit : "\(t.reply) @\(target.name)").font(.caption.weight(.semibold))
                    Text(target.kind == .text ? target.text : t.photo).font(.caption).lineLimit(1).foregroundStyle(.secondary)
                    Spacer()
                    Button {
                        if editing != nil { draft = "" }
                        replyTo = nil
                        editing = nil
                    } label: {
                        Image(systemName: "xmark.circle.fill").foregroundStyle(.secondary)
                    }
                    .accessibilityLabel(t.cancel)
                }
                .padding(.horizontal, 16)
            }
            if showEmoji {
                ScrollView(.horizontal, showsIndicators: false) {
                    LazyHGrid(rows: [GridItem(.fixed(40)), GridItem(.fixed(40))], spacing: 2) {
                        ForEach(Self.emojis, id: \.self) { e in
                            Button(e) { draft += e }
                                .font(.system(size: 26))
                                .frame(width: 40, height: 40)
                        }
                    }
                    .padding(.horizontal, 12)
                }
                .frame(height: 86)
            }
            HStack(alignment: .bottom, spacing: 6) {
                PhotosPicker(selection: $photo, matching: .images) {
                    Image(systemName: "photo").font(.title3).frame(width: 40, height: 40)
                }
                .accessibilityLabel(t.photo)
                Button { showEmoji.toggle() } label: {
                    Image(systemName: showEmoji ? "keyboard" : "face.smiling").font(.title3).frame(width: 40, height: 40)
                }
                .accessibilityLabel("Emoji")
                if recorder.recording {
                    // L'enregistrement en cours : sa durée, annuler, envoyer.
                    Button {
                        recorder.cancel()
                    } label: {
                        Image(systemName: "trash").font(.title3).foregroundStyle(.red).frame(width: 40, height: 40)
                    }
                    .accessibilityLabel(t.cancelRecording)
                    TimelineView(.periodic(from: .now, by: 0.5)) { context in
                        let s = Int(context.date.timeIntervalSince(recorder.started))
                        Label(String(format: "%d:%02d", s / 60, s % 60), systemImage: "record.circle")
                            .foregroundStyle(.red)
                            .font(.body.monospacedDigit())
                            .frame(maxWidth: .infinity, minHeight: 40)
                            .background(Color(.secondarySystemBackground), in: .rect(cornerRadius: 20))
                    }
                    Button {
                        if let audio = recorder.stop() {
                            room.sendAudio(audio, mime: "audio/mp4", reply: replyTo)
                            replyTo = nil
                        }
                    } label: {
                        Image(systemName: "arrow.up")
                            .font(.headline)
                            .foregroundStyle(.white)
                            .frame(width: 40, height: 40)
                            .background(Color.alleyAccent, in: .circle)
                    }
                    .accessibilityLabel(t.stopRecording)
                    .accessibilityIdentifier("send-voice")
                } else {
                    TextField(t.messagePlaceholder, text: $draft, axis: .vertical)
                        .lineLimit(1...6)
                        .focused(focused)
                        .padding(.horizontal, 12)
                        .padding(.vertical, 9)
                        .background(Color(.secondarySystemBackground), in: .rect(cornerRadius: 20))
                        .onChange(of: draft) { _, new in if !new.isEmpty { room.typingNow() } }
                        .accessibilityIdentifier("input")
                    if draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && editing == nil {
                        Button {
                            Task {
                                if !(await recorder.start()) { model.show(t.micDenied, error: true) }
                            }
                        } label: {
                            Image(systemName: "mic.fill")
                                .font(.headline)
                                .foregroundStyle(.white)
                                .frame(width: 40, height: 40)
                                .background(Color.alleyAccent, in: .circle)
                        }
                        .accessibilityLabel(t.recordVoice)
                        .accessibilityIdentifier("mic")
                    } else {
                        Button(action: send) {
                            Image(systemName: "arrow.up")
                                .font(.headline)
                                .foregroundStyle(.white)
                                .frame(width: 40, height: 40)
                                .background(Color.alleyAccent, in: .circle)
                        }
                        .accessibilityLabel(t.send)
                        .accessibilityIdentifier("send")
                    }
                }
            }
            .padding(.horizontal, 10)
            .padding(.bottom, 8)
        }
        .padding(.top, 6)
        .background(.bar)
        .onChange(of: photo) { _, item in
            guard let item else { return }
            photo = nil
            Task {
                guard let data = try? await item.loadTransferable(type: Data.self),
                      let jpeg = ImageShrinker.jpeg(from: data)
                else { return model.show(t.photoInvalid, error: true) }
                room.sendImage(jpeg, reply: replyTo)
                replyTo = nil
            }
        }
    }

    func send() {
        let text = draft
        if let mid = editing {
            if room.edit(mid, text: text) {
                draft = ""
                editing = nil
            }
        } else if room.sendText(text, reply: replyTo) {
            draft = ""
            replyTo = nil
        }
    }
}

/// Une photo ramenée à 1600 pixels au plus, en JPEG, sans ses métadonnées
/// (lieu, appareil) : rien d'autre que l'image ne part.
enum ImageShrinker {
    static func jpeg(from data: Data) -> Data? {
        guard let image = UIImage(data: data) else { return nil }
        let side: CGFloat = 1600
        let scale = min(1, side / max(image.size.width, image.size.height))
        let size = CGSize(width: (image.size.width * scale).rounded(), height: (image.size.height * scale).rounded())
        let format = UIGraphicsImageRendererFormat()
        format.scale = 1
        let resized = UIGraphicsImageRenderer(size: size, format: format).image { _ in
            image.draw(in: CGRect(origin: .zero, size: size))
        }
        return resized.jpegData(compressionQuality: 0.82)
    }
}
