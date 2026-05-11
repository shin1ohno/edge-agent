import Foundation

/// One mDNS-discovered candidate for the optional weave-server. Phase 2
/// uses static placeholder rows; Phase 5+ replaces these with live
/// discovery results.
public struct DiscoveredServer: Identifiable, Codable, Hashable, Sendable {
    public let id: UUID
    public var hostname: String
    public var ipAddress: String
    public var latencyMillis: Int
    public var version: String

    public init(
        id: UUID = UUID(),
        hostname: String,
        ipAddress: String,
        latencyMillis: Int,
        version: String
    ) {
        self.id = id
        self.hostname = hostname
        self.ipAddress = ipAddress
        self.latencyMillis = latencyMillis
        self.version = version
    }
}

/// Snapshot of what the system *would* know after onboarding finishes —
/// list of paired Nuimos and discovered servers. Phase 2 uses a static
/// `.placeholder` factory for UI scaffolding so the screens render
/// without BLE / mDNS plumbing. Phase 5 replaces this with a live model
/// fed by `WeaveBLE` and the network discovery layer.
public struct OnboardingSnapshot: Codable, Hashable, Sendable {
    public var foundDevices: [Device]
    public var discoveredServers: [DiscoveredServer]

    public init(
        foundDevices: [Device] = [],
        discoveredServers: [DiscoveredServer] = []
    ) {
        self.foundDevices = foundDevices
        self.discoveredServers = discoveredServers
    }

    /// Static placeholder content used by Phase 2 views. Three Nuimo
    /// candidates (one already paired, two unpaired) and two mDNS
    /// servers (one preferred, one slower fallback) — shape mirrors the
    /// hi-fi mockup `#ios-onboard-3` and `#ios-onboard-4`.
    public static let placeholder = OnboardingSnapshot(
        foundDevices: [
            Device(
                name: "Nuimo · AA:BB:CC",
                nickname: "sofa",
                room: "living",
                battery: 82,
                firmware: "2.3.0",
                isOnline: true
            ),
            Device(
                name: "Nuimo · DD:EE:FF",
                nickname: "puck",
                room: "kitchen",
                battery: 64,
                firmware: "2.3.0",
                isOnline: false
            ),
            Device(
                name: "Nuimo · 11:22:33",
                nickname: "bedside",
                room: "bedroom",
                battery: 41,
                firmware: "2.2.4",
                isOnline: false
            ),
        ],
        discoveredServers: [
            DiscoveredServer(
                hostname: "weave-server.local",
                ipAddress: "192.168.1.42",
                latencyMillis: 12,
                version: "0.4.2"
            ),
            DiscoveredServer(
                hostname: "mac-mini.local",
                ipAddress: "192.168.1.18",
                latencyMillis: 28,
                version: "0.4.0"
            ),
        ]
    )
}
