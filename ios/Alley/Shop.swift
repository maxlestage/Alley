// Les périodes payantes sur iPhone : des achats intégrés (StoreKit 2), un
// paiement unique pour 3 jours, 7 jours, 1 mois ou 3 mois — rien ne se
// renouvelle. L'achat est lié au compte (son `appAccountToken`) : l'App
// Store remet une transaction signée, que le serveur vérifie lui-même
// (jusqu'à l'autorité racine d'Apple) avant d'ouvrir des salons qui durent.

import AlleyKit
import Foundation
import Observation
import StoreKit
import SwiftUI

@MainActor
@Observable
final class Shop {
    static let prefix = "app.alley.ios.period."
    static let plans = ["3d", "7d", "1m", "3m"]

    var products: [Product] = []
    var loaded = false
    var busy = false
    /// La fin de la période en cours du compte connecté.
    var until: Date?
    @ObservationIgnored var updates: Task<Void, Never>?

    init() {
        // Les achats terminés ailleurs (validation parentale, autre appareil).
        updates = Task.detached {
            for await update in Transaction.updates {
                if case .verified(let t) = update { await t.finish() }
            }
        }
    }

    func load() async {
        guard !loaded else { return }
        let found = (try? await Product.products(for: Self.plans.map { Self.prefix + $0 })) ?? []
        products = found.sorted { $0.price < $1.price }
        loaded = true
    }

    static func plan(of product: Product) -> String {
        String(product.id.dropFirst(prefix.count))
    }

    /// La transaction la plus récente de ce compte, encore valable, telle que
    /// le serveur la vérifie (`apple:<JWS>`).
    func payment(for account: Account) async -> String? {
        var best: (Date, String)?
        for await result in Transaction.all {
            guard case .verified(let t) = result, t.productID.hasPrefix(Self.prefix),
                  t.appAccountToken == account.appleToken, t.revocationDate == nil
            else { continue }
            if best == nil || t.purchaseDate > best!.0 {
                best = (t.purchaseDate, "apple:" + result.jwsRepresentation)
            }
        }
        return best?.1
    }

    /// Achète une période pour ce compte. Rend l'erreur à afficher, ou nil.
    func buy(_ product: Product, account: Account) async -> String? {
        busy = true
        defer { busy = false }
        do {
            switch try await product.purchase(options: [.appAccountToken(account.appleToken)]) {
            case .success(let result):
                guard case .verified(let t) = result else { return "unverified" }
                await t.finish()
                return nil
            case .userCancelled, .pending:
                return ""
            @unknown default:
                return ""
            }
        } catch {
            return error.localizedDescription
        }
    }

    /// « Restaurer les achats » : l'App Store renvoie les transactions de
    /// ce compte Apple.
    func restore() async {
        try? await AppStore.sync()
    }
}

/// Les périodes, dans le hall : le prix de l'App Store, l'accord pour que la
/// période commence tout de suite, acheter, restaurer.
struct PlansSection: View {
    @Environment(AppModel.self) private var model
    @State private var consent = false

    var body: some View {
        let t = model.t
        let shop = model.shop
        Section {
            if let until = model.periodEnd {
                Label(fill(t.periodActive, until.formatted(date: .complete, time: .shortened)), systemImage: "checkmark.seal.fill")
                    .foregroundStyle(Color.alleyMint)
                    .font(.subheadline.weight(.semibold))
            } else {
                Text(t.plansIntro).font(.footnote).foregroundStyle(.secondary)
                if shop.loaded && shop.products.isEmpty {
                    Text(t.storeUnavailable).font(.footnote).foregroundStyle(.secondary)
                }
                if model.account != nil && !shop.products.isEmpty {
                    Toggle(isOn: $consent) {
                        Text(t.consentBuy).font(.footnote)
                    }
                }
                ForEach(shop.products, id: \.id) { product in
                    HStack {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(name(Shop.plan(of: product))).font(.headline)
                            Text("\(product.displayPrice), \(t.oneTime)").font(.caption).foregroundStyle(.secondary)
                        }
                        Spacer()
                        if model.account == nil {
                            Button(t.signInToBuy) { model.logout() }
                                .font(.caption.weight(.semibold))
                                .buttonStyle(.bordered)
                        } else {
                            Button(t.buy) {
                                guard consent else { return model.show(t.consentRequired, error: true) }
                                Task { await model.buy(product) }
                            }
                            .buttonStyle(.borderedProminent)
                            .disabled(shop.busy)
                        }
                    }
                }
                if model.account != nil {
                    Button(t.restore) {
                        Task {
                            await shop.restore()
                            await model.loadPeriod()
                        }
                    }
                    .font(.footnote)
                }
            }
        } header: {
            Text(t.plansTitle)
        } footer: {
            HStack(spacing: 14) {
                Link(t.terms, destination: model.siteLink("terms"))
                Link(t.privacy, destination: model.siteLink("privacy"))
            }
            .font(.caption)
        }
        .task { await shop.load() }
    }

    func name(_ plan: String) -> String {
        let t = model.t
        switch plan {
        case "3d": return t.plan3d
        case "7d": return t.plan7d
        case "1m": return t.plan1m
        default: return t.plan3m
        }
    }
}
