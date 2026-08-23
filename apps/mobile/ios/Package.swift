// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "TelekinesisMobile",
    platforms: [.iOS(.v17), .macOS(.v14)],
    products: [
        .library(name: "TelekinesisMobile", targets: ["TelekinesisMobile"]),
    ],
    targets: [
        .target(
            name: "TelekinesisMobile",
            path: "Sources/TelekinesisMobile",
            resources: [.copy("fixture.json")]
        ),
    ]
)
