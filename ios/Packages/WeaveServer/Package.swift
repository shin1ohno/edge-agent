// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "WeaveServer",
    platforms: [
        .iOS(.v17),
        .watchOS(.v10),
        .macOS(.v14),
    ],
    products: [
        .library(name: "WeaveServer", targets: ["WeaveServer"]),
    ],
    dependencies: [
        .package(path: "../WeaveCore"),
    ],
    targets: [
        .target(name: "WeaveServer", dependencies: ["WeaveCore"]),
    ]
)
