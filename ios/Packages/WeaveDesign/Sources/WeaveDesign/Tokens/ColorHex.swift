import SwiftUI

public extension Color {
    /// Initialize from a `#RRGGBB` or `RRGGBB` hex string. Invalid input
    /// falls back to `.secondary`. Used by `WeaveCore.PlaceholderData`
    /// rows that hold accent colors as plain strings to stay
    /// SwiftUI-free.
    init(weaveHex: String) {
        let trimmed = weaveHex.hasPrefix("#") ? String(weaveHex.dropFirst()) : weaveHex
        guard trimmed.count == 6, let value = UInt32(trimmed, radix: 16) else {
            self = .secondary
            return
        }
        let r = Double((value >> 16) & 0xFF) / 255
        let g = Double((value >> 8) & 0xFF) / 255
        let b = Double(value & 0xFF) / 255
        self = Color(.sRGB, red: r, green: g, blue: b, opacity: 1)
    }
}
