// Les notifications locales : un aperçu d'un nouveau message quand Alley
// est en arrière-plan. Rien ne passe par Apple ni par le serveur : c'est
// l'iPhone qui, ayant déchiffré le message, se prévient lui-même.

import Foundation

#if canImport(UserNotifications)
import UserNotifications
#endif
#if canImport(UIKit)
import UIKit
#endif

@MainActor
enum Notifier {
    /// Demande la permission (au premier réglage « Notifications »).
    static func requestPermission() async -> Bool {
        #if canImport(UserNotifications)
        return (try? await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge])) ?? false
        #else
        return false
        #endif
    }

    static func message(from name: String, room: String, body: String, sound: Bool, enabled: Bool) {
        #if canImport(UIKit) && canImport(UserNotifications)
        guard UIApplication.shared.applicationState != .active else {
            return
        }
        guard enabled else { return }
        let content = UNMutableNotificationContent()
        content.title = "\(name) · #\(room)"
        content.body = String(body.prefix(140))
        content.threadIdentifier = room
        if sound { content.sound = .default }
        let request = UNNotificationRequest(identifier: UUID().uuidString, content: content, trigger: nil)
        UNUserNotificationCenter.current().add(request)
        #endif
    }
}
