import SwiftUI

/// Static accent palette referenced by `FiringHeroCard` and other hero
/// surfaces. The values mirror the hi-fi mockup's `FIRING_VARIANTS` table.
/// We use literal hex constants rather than asset lookups for the four
/// firing accents because SwiftUI Color literals tree-shake cleanly
/// across the iOS / watchOS / macOS targets.
public extension Color {
    /// `#FF9F0A` — rotate accent (firing orange).
    static let firingOrange = Color(red: 1.0, green: 0.624, blue: 0.039)

    /// `#0A84FF` — press accent (system blue).
    static let firingBlue = Color(red: 0.039, green: 0.518, blue: 1.0)

    /// `#5856D6` — swipe accent (system purple).
    static let firingPurple = Color(red: 0.345, green: 0.337, blue: 0.839)

    /// `#FF3B30` — long-press accent (system red).
    static let firingRed = Color(red: 1.0, green: 0.231, blue: 0.188)

    /// Cross-platform stand-in for `UIColor.systemGroupedBackground`.
    /// iOS / iPadOS / Mac Catalyst use the UIKit token directly; watchOS
    /// and AppKit macOS fall back to a neutral zinc-50 / zinc-900 pair so
    /// the `WeaveDesign` package stays buildable on every target.
    static var weaveGroupedBackground: Color {
        #if os(iOS)
        return Color(uiColor: .systemGroupedBackground)
        #else
        return Color(red: 0.949, green: 0.949, blue: 0.961)
        #endif
    }

    /// Cross-platform stand-in for `UIColor.secondarySystemGroupedBackground`
    /// (used for inset card surfaces). Falls back to system background on
    /// non-UIKit targets.
    static var weaveCardBackground: Color {
        #if os(iOS)
        return Color(uiColor: .secondarySystemGroupedBackground)
        #else
        return Color.white
        #endif
    }
}
