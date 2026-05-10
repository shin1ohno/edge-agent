// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "WeaveCore",
    platforms: [
        .iOS(.v17),
        .watchOS(.v10),
        .macOS(.v14),
    ],
    products: [
        .library(name: "WeaveCore", targets: ["WeaveCore"]),
    ],
    targets: [
        .target(name: "WeaveCore"),
        .testTarget(name: "WeaveCoreTests", dependencies: ["WeaveCore"]),
    ]
)
