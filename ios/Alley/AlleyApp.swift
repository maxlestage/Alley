// Alley pour iPhone et iPad : des salons de discussion chiffrés de bout en
// bout, où rien n'est gardé. Le même protocole que le navigateur : on se
// retrouve dans les mêmes salons, depuis un iPhone ou un ordinateur.

import AlleyKit
import SwiftUI

@main
struct AlleyApp: App {
    @State private var model = AppModel()

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(model)
                .tint(.alleyAccent)
                .preferredColorScheme(model.prefs.theme.colorScheme)
                .onOpenURL { model.open($0) }
                .onContinueUserActivity(NSUserActivityTypeBrowsingWeb) { activity in
                    if let url = activity.webpageURL { model.open(url) }
                }
        }
    }
}

extension Prefs.Theme {
    var colorScheme: ColorScheme? {
        switch self {
        case .auto: nil
        case .light: .light
        case .dark: .dark
        }
    }
}

extension Color {
    static let alleyAccent = Color(red: 124 / 255, green: 92 / 255, blue: 1)
    static let alleyMint = Color(red: 34 / 255, green: 211 / 255, blue: 166 / 255)
}

/// L'accueil, le hall ou le salon ; et les bandeaux par-dessus.
struct RootView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        ZStack(alignment: .top) {
            Group {
                if model.banned {
                    BannedView()
                } else if model.pseudonym.isEmpty {
                    WelcomeView()
                } else if let room = model.room {
                    NavigationStack { RoomView(room: room) }
                        .id(ObjectIdentifier(room))
                } else {
                    NavigationStack { HallView() }
                }
            }
            if let toast = model.toast {
                ToastView(toast: toast)
                    .padding(.horizontal, 16)
                    .transition(.move(edge: .top).combined(with: .opacity))
                    .task(id: toast.id) {
                        try? await Task.sleep(for: .seconds(toast.error ? 5 : 3))
                        withAnimation { if model.toast == toast { model.toast = nil } }
                    }
                    .onTapGesture { withAnimation { model.toast = nil } }
                    .zIndex(1)
            }
        }
        .animation(.spring(duration: 0.3), value: model.toast)
    }
}

struct ToastView: View {
    let toast: AppModel.Toast

    var body: some View {
        Text(toast.text)
            .font(.subheadline.weight(.medium))
            .foregroundStyle(.white)
            .padding(.horizontal, 16)
            .padding(.vertical, 12)
            .frame(maxWidth: 520)
            .background(toast.error ? Color.red.opacity(0.92) : Color.black.opacity(0.85), in: .rect(cornerRadius: 14))
            .shadow(radius: 10, y: 4)
            .accessibilityAddTraits(.isStaticText)
    }
}

struct BannedView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        ContentUnavailableView {
            Label("Alley", systemImage: "hand.raised.fill")
        } description: {
            Text(model.t.closedBanned)
        }
    }
}

/// Le logo d'Alley : la porte et sa serrure, sur un dégradé.
struct Logo: View {
    var size: CGFloat = 72

    var body: some View {
        Image("Logo")
            .resizable()
            .frame(width: size, height: size)
            .clipShape(.rect(cornerRadius: size * 0.23))
            .shadow(color: .alleyAccent.opacity(0.35), radius: 12, y: 6)
            .accessibilityHidden(true)
    }
}
