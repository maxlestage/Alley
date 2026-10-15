// swift-tools-version:5.9
// AlleyKit : le cœur de l'application iOS d'Alley — le chiffrement Brocolis
// (AES-256-GCM, X25519 + HKDF-SHA256, Ed25519), le protocole des salons,
// l'API, le filtre de texte. Sans interface : testé aussi sous Linux.
import PackageDescription

let package = Package(
    name: "AlleyKit",
    platforms: [.iOS(.v17), .macOS(.v14)],
    products: [
        .library(name: "AlleyKit", targets: ["AlleyKit"]),
    ],
    dependencies: [
        // CryptoKit sur iPhone ; la même API (swift-crypto) ailleurs.
        .package(url: "https://github.com/apple/swift-crypto.git", "3.0.0"..<"4.0.0"),
    ],
    targets: [
        .target(
            name: "AlleyKit",
            dependencies: [
                .target(name: "LinuxShims", condition: .when(platforms: [.linux])),
                .product(
                    name: "Crypto", package: "swift-crypto",
                    condition: .when(platforms: [.linux])),
            ]),
        .target(name: "LinuxShims"),
        .testTarget(
            name: "AlleyKitTests", dependencies: ["AlleyKit"], exclude: ["vectors.json", "bridge.mjs"]),
    ]
)
