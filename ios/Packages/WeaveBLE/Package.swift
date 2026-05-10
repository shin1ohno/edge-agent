// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "WeaveBLE",
    platforms: [
        .iOS(.v17),
        .watchOS(.v10),
        .macOS(.v14),
    ],
    products: [
        .library(name: "WeaveBLE", targets: ["WeaveBLE"]),
    ],
    dependencies: [
        .package(path: "../WeaveCore"),
    ],
    targets: [
        .target(name: "WeaveBLE", dependencies: ["WeaveCore"]),
    ]
)
