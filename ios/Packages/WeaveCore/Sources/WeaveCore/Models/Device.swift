import Foundation

public struct Device: Identifiable, Codable, Hashable, Sendable {
    public let id: UUID
    public var name: String
    public var nickname: String?
    public var room: String?
    public var battery: Int?
    public var firmware: String?
    public var isOnline: Bool

    public init(
        id: UUID = UUID(),
        name: String = "Nuimo",
        nickname: String? = nil,
        room: String? = nil,
        battery: Int? = nil,
        firmware: String? = nil,
        isOnline: Bool = false
    ) {
        self.id = id
        self.name = name
        self.nickname = nickname
        self.room = room
        self.battery = battery
        self.firmware = firmware
        self.isOnline = isOnline
    }
}
