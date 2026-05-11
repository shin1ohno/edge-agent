import Foundation

/// Display-side summary of a Connection between a Nuimo and a service
/// target. Phase 3 uses static instances from `PlaceholderData.mappings`;
/// Phase 5+ derives this from the real Connection store fed by BLE /
/// service adapters.
public struct Mapping: Identifiable, Codable, Hashable, Sendable {
    public let id: UUID
    public var deviceId: Device.ID
    public var serviceType: String
    public var serviceTarget: String
    public var routes: [Route]
    public var firing: Bool
    public var lastEvent: String

    public init(
        id: UUID = UUID(),
        deviceId: Device.ID,
        serviceType: String,
        serviceTarget: String,
        routes: [Route],
        firing: Bool = false,
        lastEvent: String = "—"
    ) {
        self.id = id
        self.deviceId = deviceId
        self.serviceType = serviceType
        self.serviceTarget = serviceTarget
        self.routes = routes
        self.firing = firing
        self.lastEvent = lastEvent
    }

    /// Display label combining serviceType + target (e.g. "Roon · Living",
    /// "Hue · Sofa lamp"). Used by Connections list rows.
    public var serviceDisplayLabel: String {
        let kind = serviceType.capitalized
        return "\(kind) · \(serviceTarget)"
    }
}

/// One edge agent (Mac / iPhone / Watch) capable of relaying BLE events
/// to weave-server. Devices tab "Edge agents" section row data.
public struct EdgeAgent: Identifiable, Codable, Hashable, Sendable {
    public let id: String
    public var displayName: String
    public var hostLabel: String
    public var isConnected: Bool
    public var eventsPerFiveMin: Int?
    public var lastSeen: String?

    public init(
        id: String,
        displayName: String,
        hostLabel: String,
        isConnected: Bool,
        eventsPerFiveMin: Int? = nil,
        lastSeen: String? = nil
    ) {
        self.id = id
        self.displayName = displayName
        self.hostLabel = hostLabel
        self.isConnected = isConnected
        self.eventsPerFiveMin = eventsPerFiveMin
        self.lastSeen = lastSeen
    }
}

/// Static row data for the Services tab. `sfSymbol` + `accentHex` feed
/// SwiftUI rendering without coupling WeaveCore to SwiftUI.
public struct ServiceCatalogEntry: Identifiable, Codable, Hashable, Sendable {
    public let id: String
    public var kind: ServiceKind
    public var displayName: String
    public var sfSymbol: String
    public var accentHex: String
    public var isConnected: Bool
    public var targetCount: Int
    public var targetUnit: String
    public var addressLine: String
    public var isNew: Bool

    public init(
        id: String,
        kind: ServiceKind,
        displayName: String,
        sfSymbol: String,
        accentHex: String,
        isConnected: Bool,
        targetCount: Int,
        targetUnit: String,
        addressLine: String,
        isNew: Bool = false
    ) {
        self.id = id
        self.kind = kind
        self.displayName = displayName
        self.sfSymbol = sfSymbol
        self.accentHex = accentHex
        self.isConnected = isConnected
        self.targetCount = targetCount
        self.targetUnit = targetUnit
        self.addressLine = addressLine
        self.isNew = isNew
    }
}

/// One row in the Home tab "Recent activity" inset group.
public struct ActivityEntry: Identifiable, Codable, Hashable, Sendable {
    public let id: UUID
    public var title: String
    public var subtitle: String
    public var when: String
    public var sfSymbol: String
    public var accentHex: String

    public init(
        id: UUID = UUID(),
        title: String,
        subtitle: String,
        when: String,
        sfSymbol: String,
        accentHex: String
    ) {
        self.id = id
        self.title = title
        self.subtitle = subtitle
        self.when = when
        self.sfSymbol = sfSymbol
        self.accentHex = accentHex
    }
}

/// Static UI scaffolding data. Every collection here is intentionally
/// fake and is replaced by live model objects in Phase 5+:
///
/// - `mappings` ← `WeaveStore` SwiftData query
/// - `edgeAgents` ← weave-server `/api/edges` snapshot
/// - `services` ← `WeaveServices` adapter status polling
/// - `recentActivity` ← WS `/ws/ui` event tail
public enum PlaceholderData {
    /// Reuses `OnboardingSnapshot.placeholder.foundDevices` for shared
    /// row data so Devices tab + Connections tab stay consistent.
    public static var devices: [Device] {
        OnboardingSnapshot.placeholder.foundDevices
    }

    public static let mappings: [Mapping] = {
        let sofa = OnboardingSnapshot.placeholder.foundDevices[0]
        let puck = OnboardingSnapshot.placeholder.foundDevices[1]
        return [
            Mapping(
                deviceId: sofa.id,
                serviceType: "roon",
                serviceTarget: "Living",
                routes: [
                    Route(input: .rotate, device: sofa.id, service: UUID(), action: .volume(delta: 1), smoothness: 0.7),
                    Route(input: .press, device: sofa.id, service: UUID(), action: .toggle, smoothness: 0.0),
                    Route(input: .swipeLeft, device: sofa.id, service: UUID(), action: .skip(.prev), smoothness: 0.0),
                ],
                firing: true,
                lastEvent: "+3 just now"
            ),
            Mapping(
                deviceId: sofa.id,
                serviceType: "hue",
                serviceTarget: "Sofa lamp",
                routes: [
                    Route(input: .rotate, device: sofa.id, service: UUID(), action: .volume(delta: 1), smoothness: 0.6),
                    Route(input: .longPress, device: sofa.id, service: UUID(), action: .toggle, smoothness: 0.0),
                ],
                firing: false,
                lastEvent: "12m ago"
            ),
            Mapping(
                deviceId: puck.id,
                serviceType: "hue",
                serviceTarget: "Kitchen",
                routes: [
                    Route(input: .rotate, device: puck.id, service: UUID(), action: .volume(delta: 1), smoothness: 0.5),
                ],
                firing: false,
                lastEvent: "1h ago"
            ),
        ]
    }()

    public static let edgeAgents: [EdgeAgent] = [
        EdgeAgent(
            id: "edge-living",
            displayName: "edge-living",
            hostLabel: "this Mac",
            isConnected: true,
            eventsPerFiveMin: 247
        ),
        EdgeAgent(
            id: "edge-bedroom",
            displayName: "edge-bedroom",
            hostLabel: "Mac mini",
            isConnected: false,
            lastSeen: "2d ago"
        ),
    ]

    public static let services: [ServiceCatalogEntry] = [
        ServiceCatalogEntry(
            id: "roon",
            kind: .roon,
            displayName: "Roon",
            sfSymbol: "play.fill",
            accentHex: "FF9F0A",
            isConnected: true,
            targetCount: 3,
            targetUnit: "zones",
            addressLine: "Roon Core · 192.168.1.18"
        ),
        ServiceCatalogEntry(
            id: "hue",
            kind: .hue,
            displayName: "Hue",
            sfSymbol: "lightbulb.fill",
            accentHex: "FFB340",
            isConnected: true,
            targetCount: 7,
            targetUnit: "lights",
            addressLine: "Hue Bridge · 192.168.1.5"
        ),
        ServiceCatalogEntry(
            id: "midi",
            kind: .webhook,
            displayName: "MIDI",
            sfSymbol: "waveform",
            accentHex: "5856D6",
            isConnected: true,
            targetCount: 0,
            targetUnit: "",
            addressLine: "on this Mac"
        ),
        ServiceCatalogEntry(
            id: "http",
            kind: .webhook,
            displayName: "HTTP",
            sfSymbol: "link",
            accentHex: "34C759",
            isConnected: true,
            targetCount: 0,
            targetUnit: "",
            addressLine: "Custom integration"
        ),
        ServiceCatalogEntry(
            id: "sonos",
            kind: .sonos,
            displayName: "Sonos",
            sfSymbol: "speaker.wave.3.fill",
            accentHex: "1C1C1E",
            isConnected: false,
            targetCount: 0,
            targetUnit: "",
            addressLine: "Discover Sonos speakers on the network",
            isNew: true
        ),
    ]

    public static let recentActivity: [ActivityEntry] = [
        ActivityEntry(
            title: "rotate +3",
            subtitle: "sofa → Roon volume",
            when: "just now",
            sfSymbol: "arrow.up.arrow.down",
            accentHex: "FF9F0A"
        ),
        ActivityEntry(
            title: "press",
            subtitle: "sofa → Roon play_pause",
            when: "2m ago",
            sfSymbol: "hand.tap",
            accentHex: "0A84FF"
        ),
        ActivityEntry(
            title: "swipe \u{2190}",
            subtitle: "sofa → Roon prev_track",
            when: "5m ago",
            sfSymbol: "arrow.left",
            accentHex: "5856D6"
        ),
        ActivityEntry(
            title: "rotate \u{2212}2",
            subtitle: "kitchen → Hue brightness",
            when: "18m ago",
            sfSymbol: "arrow.up.arrow.down",
            accentHex: "FFB340"
        ),
    ]

    /// Currently-firing mapping for the Home FiringHero card. Returns
    /// the first mapping flagged `firing` (or nil for empty state —
    /// Phase 4 wires the empty hero variant).
    public static var firingMapping: Mapping? {
        mappings.first(where: { $0.firing })
    }
}
