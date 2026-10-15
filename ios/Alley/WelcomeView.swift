// L'accueil : un pseudonyme, et un mot de passe si l'on veut retrouver ses
// salons sur tous ses appareils (il ne quitte jamais l'iPhone).

import AlleyKit
import SwiftUI

struct WelcomeView: View {
    @Environment(AppModel.self) private var model
    @State private var pseudonym = ""
    @State private var password = ""
    @State private var error: String?
    @State private var busy = false
    @FocusState private var focus: Field?

    enum Field { case pseudonym, password }

    var body: some View {
        let t = model.t
        ScrollView {
            VStack(spacing: 22) {
                HStack {
                    Spacer()
                    LanguageMenu()
                }
                Logo(size: 84)
                VStack(spacing: 8) {
                    Text("Alley").font(.largeTitle.bold())
                    Text(t.tagline)
                        .font(.callout)
                        .foregroundStyle(.secondary)
                        .multilineTextAlignment(.center)
                }
                VStack(alignment: .leading, spacing: 14) {
                    TextField(t.pseudonymPlaceholder, text: $pseudonym)
                        .textContentType(.username)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .submitLabel(.next)
                        .focused($focus, equals: .pseudonym)
                        .onSubmit { focus = .password }
                        .fieldStyle()
                        .accessibilityIdentifier("pseudonym")
                    SecureField(t.password, text: $password)
                        .textContentType(.password)
                        .submitLabel(.go)
                        .focused($focus, equals: .password)
                        .onSubmit { go() }
                        .fieldStyle()
                        .accessibilityIdentifier("password")
                    Text(t.passwordHint).font(.footnote).foregroundStyle(.secondary)
                    if let error {
                        Text(error).font(.footnote.weight(.medium)).foregroundStyle(.red)
                    }
                }
                Button(action: go) {
                    HStack {
                        if busy { ProgressView().tint(.white) }
                        Text(busy ? t.signingIn : (password.isEmpty ? t.guest : t.signIn))
                    }
                    .frame(maxWidth: .infinity, minHeight: 50)
                }
                .buttonStyle(.borderedProminent)
                .buttonBorderShape(.roundedRectangle(radius: 14))
                .font(.headline)
                .disabled(busy)
                .accessibilityIdentifier("enter")
                VStack(spacing: 6) {
                    Text(t.accept).font(.footnote).foregroundStyle(.secondary).multilineTextAlignment(.center)
                    HStack(spacing: 18) {
                        Link(t.terms, destination: model.siteLink("terms"))
                        Link(t.rules, destination: model.siteLink("rules"))
                        Link(t.privacy, destination: model.siteLink("privacy"))
                    }
                    .font(.footnote.weight(.medium))
                }
                Label(t.encrypted, systemImage: "lock.shield")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
            .padding(24)
            .frame(maxWidth: 480)
            .frame(maxWidth: .infinity)
        }
        .scrollDismissesKeyboard(.interactively)
        .onAppear { focus = .pseudonym }
    }

    func go() {
        guard !busy else { return }
        busy = true
        error = nil
        Task {
            error = await model.enter(pseudonym: pseudonym, password: password)
            busy = false
        }
    }
}

struct LanguageMenu: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        Menu {
            ForEach(Lang.allCases) { l in
                Button {
                    model.setLang(l)
                } label: {
                    if l == model.lang { Label(l.name, systemImage: "checkmark") } else { Text(l.name) }
                }
            }
        } label: {
            Label(model.lang.name, systemImage: "globe")
                .font(.subheadline)
        }
        .accessibilityLabel(model.t.language)
    }
}

extension View {
    /// Un champ de saisie arrondi, à la manière d'Alley.
    func fieldStyle() -> some View {
        padding(.horizontal, 14)
            .frame(minHeight: 50)
            .background(Color(.secondarySystemBackground), in: .rect(cornerRadius: 14))
    }
}
