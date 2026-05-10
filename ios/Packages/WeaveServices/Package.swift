// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "WeaveServices",
    platforms: [
        .iOS(.v17),
        .watchOS(.v10),
        .macOS(.v14),
    ],
    products: [
        .library(name: "WeaveServices", targets: ["WeaveServices"]),
    ],
    dependencies: [
        .package(path: "../WeaveCore"),
    ],
    targets: [
        .target(name: "WeaveServices", dependencies: ["WeaveCore"]),
    ]
)
