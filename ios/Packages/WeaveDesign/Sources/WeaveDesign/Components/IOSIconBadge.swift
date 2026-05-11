import SwiftUI

/// Colored rounded-square icon badge used by iOS Settings.app-style
/// rows. 28pt by default; consumers may scale via the `size` parameter.
public struct IOSIconBadge: View {
    public let symbol: String
    public let color: Color
    public let size: CGFloat

    public init(symbol: String, color: Color, size: CGFloat = 28) {
        self.symbol = symbol
        self.color = color
        self.size = size
    }

    public var body: some View {
        RoundedRectangle(cornerRadius: size * 0.214, style: .continuous)
            .fill(color)
            .frame(width: size, height: size)
            .overlay {
                Image(systemName: symbol)
                    .font(.system(size: size * 0.5, weight: .semibold))
                    .foregroundStyle(.white)
            }
            .accessibilityHidden(true)
    }
}

#Preview {
    HStack(spacing: 10) {
        IOSIconBadge(symbol: "bolt.fill", color: .firingOrange)
        IOSIconBadge(symbol: "link", color: .firingBlue)
        IOSIconBadge(symbol: "trash", color: .firingRed)
        IOSIconBadge(symbol: "waveform", color: Color(weaveHex: "5856D6"))
        IOSIconBadge(symbol: "plus.circle.fill", color: .firingBlue, size: 36)
    }
    .padding()
}
