// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "KivoVozBridge",
    platforms: [.macOS(.v15)],
    products: [.library(name: "KivoVozBridge", type: .static, targets: ["KivoVozBridge"])],
    dependencies: [
        .package(url: "https://github.com/Desert-Ant-Labs/desert-ant-core.git", exact: "3.5.0"),
    ],
    targets: [
        .target(name: "KivoVozBridge", dependencies: [
            .product(name: "Voz", package: "desert-ant-core"),
            .product(name: "Ear", package: "desert-ant-core"),
            .product(name: "DesertAnt", package: "desert-ant-core"),
        ]),
    ]
)
