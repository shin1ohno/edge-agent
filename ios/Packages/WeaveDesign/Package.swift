// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "WeaveDesign",
    platforms: [
        .iOS(.v17),
        .watchOS(.v10),
        .macOS(.v14),
    ],
    products: [
        .library(name: "WeaveDesign", targets: ["WeaveDesign"]),
    ],
    dependencies: [
        .package(path: "../WeaveCore"),
    ],
    targets: [
        .target(
            name: "WeaveDesign",
            dependencies: ["WeaveCore"]
        ),
    ]
)
