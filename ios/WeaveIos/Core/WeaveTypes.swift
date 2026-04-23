import Foundation

/// Swift `Codable` mirrors for the types in the `weave-contracts` Rust
/// crate. We keep the JSON field names (snake_case) as `CodingKeys` so the
/// source of truth stays on the Rust side.
///
/// Only the subset the iOS app consumes from `/ws/ui` and `/api/*` is
/// modeled here. Edge-side frames (`EdgeToServer`, etc.) are not needed.

// MARK: - UiSnapshot + constituents

struct UiSnapshot: Codable, Sendable {
    var edges: [EdgeInfo]
    var serviceStates: [ServiceStateEntry]
    var deviceStates: [DeviceStateEntry]
    var mappings: [MappingRecord]
    var glyphs: [GlyphRecord]

    private enum CodingKeys: String, CodingKey {
        case edges
        case serviceStates = "service_states"
        case deviceStates = "device_states"
        case mappings
        case glyphs
    }

    static let empty = UiSnapshot(
        edges: [], serviceStates: [], deviceStates: [], mappings: [], glyphs: []
    )
}

struct EdgeInfo: Codable, Identifiable, Sendable, Hashable {
    var edgeId: String
    var online: Bool
    var version: String
    var capabilities: [String]
    var lastSeen: String

    var id: String { edgeId }

    private enum CodingKeys: String, CodingKey {
        case edgeId = "edge_id"
        case online, version, capabilities
        case lastSeen = "last_seen"
    }
}

struct ServiceStateEntry: Codable, Identifiable, Sendable, Hashable {
    var edgeId: String
    var serviceType: String
    var target: String
    var property: String
    var outputId: String?
    /// Raw JSON value; kept as a string so the Swift side doesn't need to
    /// model every possible shape. Use the typed accessors below for
    /// the values LiveConsole actually renders.
    var valueJSON: String
    var updatedAt: String

    var id: String { "\(edgeId)/\(serviceType)/\(target)/\(property)" }

    // Only one consumer today — the Zones panel — needs to read display_name
    // and volume. When more shapes show up, add more accessors.
    var stringValue: String? {
        guard let data = valueJSON.data(using: .utf8) else { return nil }
        return try? JSONDecoder().decode(String.self, from: data)
    }

    var doubleValue: Double? {
        guard let data = valueJSON.data(using: .utf8) else { return nil }
        return try? JSONDecoder().decode(Double.self, from: data)
    }

    var boolValue: Bool? {
        guard let data = valueJSON.data(using: .utf8) else { return nil }
        return try? JSONDecoder().decode(Bool.self, from: data)
    }

    private enum CodingKeys: String, CodingKey {
        case edgeId = "edge_id"
        case serviceType = "service_type"
        case target, property
        case outputId = "output_id"
        case value
        case updatedAt = "updated_at"
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        self.edgeId = try c.decode(String.self, forKey: .edgeId)
        self.serviceType = try c.decode(String.self, forKey: .serviceType)
        self.target = try c.decode(String.self, forKey: .target)
        self.property = try c.decode(String.self, forKey: .property)
        self.outputId = try c.decodeIfPresent(String.self, forKey: .outputId)
        // Re-encode the raw JSON value as a string for later typed decoding.
        let nested = try c.decode(AnyCodable.self, forKey: .value)
        self.valueJSON = nested.encodedString()
        self.updatedAt = try c.decode(String.self, forKey: .updatedAt)
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(edgeId, forKey: .edgeId)
        try c.encode(serviceType, forKey: .serviceType)
        try c.encode(target, forKey: .target)
        try c.encode(property, forKey: .property)
        try c.encodeIfPresent(outputId, forKey: .outputId)
        try c.encode(AnyCodable(jsonString: valueJSON), forKey: .value)
        try c.encode(updatedAt, forKey: .updatedAt)
    }
}

struct DeviceStateEntry: Codable, Identifiable, Sendable, Hashable {
    var edgeId: String
    var deviceType: String
    var deviceId: String
    var property: String
    var valueJSON: String
    var updatedAt: String

    var id: String { "\(edgeId)/\(deviceType)/\(deviceId)/\(property)" }

    private enum CodingKeys: String, CodingKey {
        case edgeId = "edge_id"
        case deviceType = "device_type"
        case deviceId = "device_id"
        case property, value
        case updatedAt = "updated_at"
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        self.edgeId = try c.decode(String.self, forKey: .edgeId)
        self.deviceType = try c.decode(String.self, forKey: .deviceType)
        self.deviceId = try c.decode(String.self, forKey: .deviceId)
        self.property = try c.decode(String.self, forKey: .property)
        let nested = try c.decode(AnyCodable.self, forKey: .value)
        self.valueJSON = nested.encodedString()
        self.updatedAt = try c.decode(String.self, forKey: .updatedAt)
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(edgeId, forKey: .edgeId)
        try c.encode(deviceType, forKey: .deviceType)
        try c.encode(deviceId, forKey: .deviceId)
        try c.encode(property, forKey: .property)
        try c.encode(AnyCodable(jsonString: valueJSON), forKey: .value)
        try c.encode(updatedAt, forKey: .updatedAt)
    }
}

struct MappingRecord: Codable, Identifiable, Sendable, Hashable {
    var mappingId: String
    var edgeId: String
    var deviceType: String
    var deviceId: String
    var serviceType: String
    var serviceTarget: String
    var active: Bool

    var id: String { mappingId }

    private enum CodingKeys: String, CodingKey {
        case mappingId = "mapping_id"
        case edgeId = "edge_id"
        case deviceType = "device_type"
        case deviceId = "device_id"
        case serviceType = "service_type"
        case serviceTarget = "service_target"
        case active
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        self.mappingId = try c.decode(String.self, forKey: .mappingId)
        self.edgeId = try c.decode(String.self, forKey: .edgeId)
        self.deviceType = try c.decode(String.self, forKey: .deviceType)
        self.deviceId = try c.decode(String.self, forKey: .deviceId)
        self.serviceType = try c.decode(String.self, forKey: .serviceType)
        self.serviceTarget = try c.decode(String.self, forKey: .serviceTarget)
        self.active = try c.decodeIfPresent(Bool.self, forKey: .active) ?? true
    }
}

struct GlyphRecord: Codable, Identifiable, Sendable, Hashable {
    var name: String
    var pattern: String
    var builtin: Bool

    var id: String { name }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        self.name = try c.decode(String.self, forKey: .name)
        self.pattern = try c.decodeIfPresent(String.self, forKey: .pattern) ?? ""
        self.builtin = try c.decodeIfPresent(Bool.self, forKey: .builtin) ?? false
    }

    private enum CodingKeys: String, CodingKey {
        case name, pattern, builtin
    }
}

// MARK: - UiFrame dispatcher

/// The top-level `/ws/ui` frame. Rust uses serde internal tag `type`
/// with `rename_all = "snake_case"`.
enum UiFrame: Sendable {
    case snapshot(UiSnapshot)
    case edgeOnline(EdgeInfo)
    case edgeOffline(edgeId: String)
    case serviceState(ServiceStateEntry)
    case deviceState(DeviceStateEntry)
    case mappingChanged(id: String, op: String, mapping: MappingRecord?)
    case glyphsChanged(glyphs: [GlyphRecord])
    // `Command` and `Error` frames are transient; not modeled yet.
    case unknown(String)

    private enum TopKeys: String, CodingKey {
        case type
        case snapshot
        case edge
        case edge_id
        // service_state / device_state have their fields inlined — we decode
        // the full ServiceStateEntry from the outer object below.
        case mapping_id
        case op
        case mapping
        case glyphs
    }

    static func decode(from data: Data) throws -> UiFrame {
        let container = try JSONDecoder().decode(Peek.self, from: data)
        switch container.type {
        case "snapshot":
            let wrap = try JSONDecoder().decode(SnapshotWrap.self, from: data)
            return .snapshot(wrap.snapshot)
        case "edge_online":
            let wrap = try JSONDecoder().decode(EdgeOnlineWrap.self, from: data)
            return .edgeOnline(wrap.edge)
        case "edge_offline":
            let wrap = try JSONDecoder().decode(EdgeOfflineWrap.self, from: data)
            return .edgeOffline(edgeId: wrap.edge_id)
        case "service_state":
            let entry = try JSONDecoder().decode(ServiceStateInline.self, from: data)
            return .serviceState(entry.toEntry())
        case "device_state":
            let entry = try JSONDecoder().decode(DeviceStateInline.self, from: data)
            return .deviceState(entry.toEntry())
        case "mapping_changed":
            let wrap = try JSONDecoder().decode(MappingChangedWrap.self, from: data)
            return .mappingChanged(id: wrap.mapping_id, op: wrap.op, mapping: wrap.mapping)
        case "glyphs_changed":
            let wrap = try JSONDecoder().decode(GlyphsChangedWrap.self, from: data)
            return .glyphsChanged(glyphs: wrap.glyphs)
        default:
            return .unknown(container.type)
        }
    }

    private struct Peek: Decodable { let type: String }
    private struct SnapshotWrap: Decodable { let snapshot: UiSnapshot }
    private struct EdgeOnlineWrap: Decodable { let edge: EdgeInfo }
    private struct EdgeOfflineWrap: Decodable { let edge_id: String }
    private struct MappingChangedWrap: Decodable {
        let mapping_id: String
        let op: String
        let mapping: MappingRecord?
    }
    private struct GlyphsChangedWrap: Decodable { let glyphs: [GlyphRecord] }

    /// ServiceState frame has the entry fields inlined next to the tag.
    /// Reuse the same shape as `ServiceStateEntry` but allow missing
    /// `updated_at` (server stamps it on broadcast).
    private struct ServiceStateInline: Decodable {
        let edge_id: String
        let service_type: String
        let target: String
        let property: String
        let output_id: String?
        let value: AnyCodable
        let updated_at: String?

        func toEntry() -> ServiceStateEntry {
            var entry = ServiceStateEntry.stub()
            entry.edgeId = edge_id
            entry.serviceType = service_type
            entry.target = target
            entry.property = property
            entry.outputId = output_id
            entry.valueJSON = value.encodedString()
            entry.updatedAt = updated_at ?? ""
            return entry
        }
    }

    private struct DeviceStateInline: Decodable {
        let edge_id: String
        let device_type: String
        let device_id: String
        let property: String
        let value: AnyCodable
        let updated_at: String?

        func toEntry() -> DeviceStateEntry {
            var entry = DeviceStateEntry.stub()
            entry.edgeId = edge_id
            entry.deviceType = device_type
            entry.deviceId = device_id
            entry.property = property
            entry.valueJSON = value.encodedString()
            entry.updatedAt = updated_at ?? ""
            return entry
        }
    }
}

extension ServiceStateEntry {
    fileprivate static func stub() -> ServiceStateEntry {
        // Decode a throwaway object to get a fully-initialized struct.
        // Swift's memberwise init is internal-only when there's a custom init.
        let raw = #"{"edge_id":"","service_type":"","target":"","property":"","value":null,"updated_at":""}"#
        return try! JSONDecoder().decode(ServiceStateEntry.self, from: Data(raw.utf8))
    }
}

extension DeviceStateEntry {
    fileprivate static func stub() -> DeviceStateEntry {
        let raw = #"{"edge_id":"","device_type":"","device_id":"","property":"","value":null,"updated_at":""}"#
        return try! JSONDecoder().decode(DeviceStateEntry.self, from: Data(raw.utf8))
    }
}

// MARK: - AnyCodable (round-trips an arbitrary JSON value to/from a string)

struct AnyCodable: Codable, Sendable {
    let raw: String

    init(jsonString: String) { self.raw = jsonString }

    init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        // Decode the value into a JSONValue tree and re-serialize to string.
        if container.decodeNil() {
            self.raw = "null"
            return
        }
        if let v = try? container.decode(Bool.self) {
            self.raw = v ? "true" : "false"
            return
        }
        if let v = try? container.decode(Double.self) {
            self.raw = String(v)
            return
        }
        if let v = try? container.decode(String.self) {
            // Re-encode to get proper escaping.
            self.raw = (try? String(data: JSONEncoder().encode(v), encoding: .utf8)) ?? "\"\(v)\""
            return
        }
        if let v = try? container.decode([AnyCodable].self) {
            let inner = v.map(\.raw).joined(separator: ",")
            self.raw = "[\(inner)]"
            return
        }
        if let v = try? container.decode([String: AnyCodable].self) {
            let pairs = v.map { key, val in
                let keyJson = (try? String(data: JSONEncoder().encode(key), encoding: .utf8))
                    ?? "\"\(key)\""
                return "\(keyJson):\(val.raw)"
            }
            self.raw = "{\(pairs.joined(separator: ","))}"
            return
        }
        self.raw = "null"
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        // Round-trip via parsing back into Any.
        guard let data = raw.data(using: .utf8),
              let any = try? JSONSerialization.jsonObject(with: data, options: [.fragmentsAllowed])
        else {
            try container.encodeNil()
            return
        }
        let reencoded = try JSONSerialization.data(withJSONObject: any, options: [.fragmentsAllowed])
        if let s = String(data: reencoded, encoding: .utf8) {
            // We can't write raw JSON into a single-value container without a
            // custom JSONEncoder. Fall back to re-decoding via the appropriate
            // type.
            if let v = try? JSONDecoder().decode(Bool.self, from: Data(s.utf8)) {
                try container.encode(v)
            } else if let v = try? JSONDecoder().decode(Double.self, from: Data(s.utf8)) {
                try container.encode(v)
            } else if let v = try? JSONDecoder().decode(String.self, from: Data(s.utf8)) {
                try container.encode(v)
            } else {
                try container.encodeNil()
            }
        } else {
            try container.encodeNil()
        }
    }

    func encodedString() -> String { raw }
}
