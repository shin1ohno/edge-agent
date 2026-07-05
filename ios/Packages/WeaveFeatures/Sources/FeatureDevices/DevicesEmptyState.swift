import SwiftUI
import WeaveDesign

/// Empty state for the Devices tab when no Nuimos are paired. Mirrors
/// the hi-fi mockup `IOSEmptyDevices` lines 6525-6538. Phase 4 ships
/// the view; Phase 5 wires it to `PlaceholderData.devices.isEmpty`
/// once SwiftData drives the device roster.
public struct DevicesEmptyState: View {
    public var onPair: () -> Void

    public init(onPair: @escaping () -> Void = {}) {
        self.onPair = onPair
    }

    public var body: some View {
        VStack(spacing: 0) {
            Spacer()

            RoundedRectangle(cornerRadius: 24, style: .continuous)
                .fill(Color.black.opacity(0.04))
                .frame(width: 96, height: 96)
                .overlay {
                    Image(systemName: "square.grid.2x2")
                        .font(.system(size: 44, weight: .semibold))
                        .foregroundStyle(.secondary)
                }

            Text("No Nuimos paired yet")
                .font(.title2.bold())
                .padding(.top, 20)

            Text("Nuimo を長押しして pairing モードにすると、ここに表示されます。")
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .padding(.top, 8)
                .padding(.horizontal, 32)

            Button("Pair a Nuimo", action: onPair)
                .weaveCTA()
                .frame(maxWidth: 220)
                .padding(.top, 24)

            Spacer()
            Spacer()
        }
        .frame(maxWidth: .infinity)
        .background(Color.weaveGroupedBackground)
    }
}

#Preview {
    DevicesEmptyState(onPair: {})
}
