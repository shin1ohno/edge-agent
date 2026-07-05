import SwiftUI

/// 3-cell "at a glance" stat strip used by Device detail. Each cell
/// shows a monospaced value on top and a small-caps label below.
public struct IOSStatStrip: View {
    public struct Item: Hashable {
        public let label: String
        public let value: String
        public let valueColor: Color

        public init(label: String, value: String, valueColor: Color = .primary) {
            self.label = label
            self.value = value
            self.valueColor = valueColor
        }
    }

    public let items: [Item]

    public init(items: [Item]) {
        self.items = items
    }

    public var body: some View {
        HStack(spacing: 0) {
            ForEach(Array(items.enumerated()), id: \.element) { _, item in
                VStack(spacing: 4) {
                    Text(item.value)
                        .font(.system(size: 20, weight: .bold, design: .monospaced))
                        .foregroundStyle(item.valueColor)
                    Text(item.label.uppercased())
                        .font(.caption2.weight(.medium))
                        .tracking(0.6)
                        .foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity)
            }
        }
        .padding(12)
        .background(Color.weaveCardBackground)
        .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .stroke(Color.black.opacity(0.06), lineWidth: 0.5)
        }
    }
}

#Preview {
    IOSStatStrip(items: [
        .init(label: "Battery", value: "82%", valueColor: .green),
        .init(label: "Edge", value: "living"),
        .init(label: "Status", value: "live", valueColor: .green),
    ])
    .padding()
    .background(Color.weaveGroupedBackground)
}
