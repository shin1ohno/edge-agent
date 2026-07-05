import SwiftUI

/// Visual placeholder for the Nuimo controller across onboarding,
/// Settings, and tab roots. Phase 2 renders SF Symbol `dial.high.fill`;
/// Phase 5 swaps in a custom SF Symbol template when one is committed
/// to `Resources/Symbols/`.
public struct NuimoGlyph: View {
    public enum Tone {
        /// Plain inline rendering (Settings row icon).
        case plain
        /// Highlighted "firing" tint — orange foreground for active
        /// rotate / press / swipe visualizations.
        case firing
    }

    public let size: CGFloat
    public let tone: Tone

    public init(size: CGFloat = 28, tone: Tone = .plain) {
        self.size = size
        self.tone = tone
    }

    public var body: some View {
        Image(systemName: "dial.high.fill")
            .resizable()
            .aspectRatio(contentMode: .fit)
            .symbolRenderingMode(.hierarchical)
            .foregroundStyle(tone == .firing ? AnyShapeStyle(Color.firingOrange) : AnyShapeStyle(.secondary))
            .frame(width: size, height: size)
            .accessibilityLabel("Nuimo controller")
    }
}

#Preview("NuimoGlyph sizes", traits: .sizeThatFitsLayout) {
    HStack(spacing: 16) {
        NuimoGlyph(size: 28)
        NuimoGlyph(size: 56, tone: .firing)
        NuimoGlyph(size: 120, tone: .firing)
    }
    .padding()
}
