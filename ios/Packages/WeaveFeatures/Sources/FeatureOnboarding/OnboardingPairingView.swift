import SwiftUI
import WeaveCore
import WeaveDesign

/// Step 3 / 4. Animated radar + growing list of placeholder Nuimo
/// candidates. Phase 5 swaps the dummy `OnboardingSnapshot.placeholder`
/// data for live BLE scan results.
/// Mockup reference: `#ios-onboard-3` / `IOSOnboardingPairing`.
struct OnboardingPairingView: View {
    var onNext: () -> Void
    var onSkip: () -> Void

    @State private var visibleCount: Int = 1
    @State private var pairedIds: Set<UUID> = []
    @State private var radarPhase: Double = 0

    private let snapshot = OnboardingSnapshot.placeholder
    private let cadenceSeconds: Double = 1.1

    var body: some View {
        VStack(spacing: 0) {
            OnboardingStepIndicator(current: 2, total: 4)

            radarHeader
                .padding(.top, 24)

            Text("Looking for Nuimos…")
                .font(.system(size: 28, weight: .bold))
                .padding(.top, 20)

            Text("長押しで Nuimo をペアリングモードに")
                .font(.footnote)
                .foregroundStyle(.secondary)
                .padding(.top, 4)

            foundList
                .padding(.top, 20)

            Spacer(minLength: 0)

            VStack(spacing: 2) {
                Button("Continue", action: onNext)
                    .weaveCTA()
                Button("Pair later in Settings", action: onSkip)
                    .weaveCTA(.tertiaryText)
            }
            .padding(.horizontal, 24)
            .padding(.bottom, 32)
        }
        .task {
            // Mark the first device as already paired to match the
            // mockup's initial state.
            if let first = snapshot.foundDevices.first {
                pairedIds.insert(first.id)
            }
            // Animate the discovery list growing 1 -> 2 -> 3.
            while visibleCount < snapshot.foundDevices.count {
                try? await Task.sleep(for: .seconds(cadenceSeconds))
                withAnimation(.easeOut(duration: 0.25)) {
                    visibleCount += 1
                }
            }
        }
    }

    private var radarHeader: some View {
        ZStack {
            ForEach(0..<3, id: \.self) { i in
                Circle()
                    .stroke(
                        Color.firingBlue.opacity(0.6 - Double(i) * 0.2),
                        lineWidth: 1.5
                    )
                    .frame(
                        width: 110 - CGFloat(i) * 28,
                        height: 110 - CGFloat(i) * 28
                    )
                    .scaleEffect(1 + 0.4 * radarPhase)
                    .opacity(1 - radarPhase)
            }
            NuimoGlyph(size: 56, tone: .firing)
        }
        .frame(width: 120, height: 120)
        .onAppear {
            withAnimation(.easeOut(duration: 1.6).repeatForever(autoreverses: false)) {
                radarPhase = 1
            }
        }
    }

    private var foundList: some View {
        let devices = Array(snapshot.foundDevices.prefix(visibleCount))
        return VStack(alignment: .leading, spacing: 0) {
            Text("FOUND \(devices.count)")
                .font(.caption.weight(.semibold))
                .foregroundStyle(.secondary)
                .padding(.horizontal, 16)
                .padding(.bottom, 6)

            VStack(spacing: 0) {
                ForEach(Array(devices.enumerated()), id: \.element.id) { idx, device in
                    deviceRow(device)
                    if idx < devices.count - 1 {
                        Divider().padding(.leading, 60)
                    }
                }
            }
            .background(Color.weaveCardBackground)
            .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
            .padding(.horizontal, 16)
        }
    }

    private func deviceRow(_ device: Device) -> some View {
        HStack(spacing: 12) {
            NuimoGlyph(size: 28)
            VStack(alignment: .leading, spacing: 2) {
                Text(device.name)
                    .font(.subheadline.weight(.medium))
                Text(rssiSummary(for: device))
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
            if pairedIds.contains(device.id) {
                HStack(spacing: 4) {
                    Image(systemName: "checkmark")
                        .font(.caption.weight(.semibold))
                    Text("Paired")
                        .font(.caption.weight(.medium))
                }
                .foregroundStyle(.green)
            } else {
                Button {
                    withAnimation { pairedIds.insert(device.id) }
                } label: {
                    Text("Pair")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.white)
                        .padding(.horizontal, 10)
                        .padding(.vertical, 4)
                        .background(Color.firingBlue)
                        .clipShape(RoundedRectangle(cornerRadius: 6, style: .continuous))
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
    }

    private func rssiSummary(for device: Device) -> String {
        let rssi = -40 - (device.firmware == "2.2.4" ? 30 : (device.isOnline ? 2 : 18))
        let signalPercent = max(0, 100 + rssi)
        return "\(rssi) dBm · \(signalPercent)% signal"
    }
}

#Preview {
    OnboardingPairingView(onNext: {}, onSkip: {})
}
