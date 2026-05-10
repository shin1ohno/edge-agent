// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "WeaveFeatures",
    platforms: [
        .iOS(.v17),
        .watchOS(.v10),
        .macOS(.v14),
    ],
    products: [
        .library(name: "FeatureOnboarding",  targets: ["FeatureOnboarding"]),
        .library(name: "FeatureHome",        targets: ["FeatureHome"]),
        .library(name: "FeatureDevices",     targets: ["FeatureDevices"]),
        .library(name: "FeatureConnections", targets: ["FeatureConnections"]),
        .library(name: "FeatureServices",    targets: ["FeatureServices"]),
        .library(name: "FeatureSettings",    targets: ["FeatureSettings"]),
        .library(name: "FeatureRoutesPad",   targets: ["FeatureRoutesPad"]),
        .library(name: "FeatureWatchEdge",   targets: ["FeatureWatchEdge"]),
    ],
    dependencies: [
        .package(path: "../WeaveCore"),
        .package(path: "../WeaveDesign"),
    ],
    targets: [
        .target(name: "FeatureOnboarding",  dependencies: ["WeaveCore", "WeaveDesign"]),
        .target(name: "FeatureHome",        dependencies: ["WeaveCore", "WeaveDesign"]),
        .target(name: "FeatureDevices",     dependencies: ["WeaveCore", "WeaveDesign"]),
        .target(name: "FeatureConnections", dependencies: ["WeaveCore", "WeaveDesign"]),
        .target(name: "FeatureServices",    dependencies: ["WeaveCore", "WeaveDesign"]),
        .target(name: "FeatureSettings",    dependencies: ["WeaveCore", "WeaveDesign"]),
        .target(name: "FeatureRoutesPad",   dependencies: ["WeaveCore", "WeaveDesign"]),
        .target(name: "FeatureWatchEdge",   dependencies: ["WeaveCore", "WeaveDesign"]),
    ]
)
