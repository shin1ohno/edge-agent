import SwiftUI
import WeaveCore
import WeaveDesign

/// Gradient-orange hero card on top of the Home tab, mirroring the
/// hi-fi mockup `IOSHome` lines 6115-6142. Phase 3 reads from
/// `PlaceholderData.firingMapping`; Phase 5 swaps to live state.
struct HomeFiringHero: View {
    let mapping: Mapping?
    let device: Device?

    var body: some View {
        if let mapping, let device {
            firingCard(mapping: mapping, device: device)
        } else {
            idleCard
        }
    }

    private func firingCard(mapping: Mapping, device: Device) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            topRow(eventLabel: mapping.lastEvent)
            mainRow(device: device, mapping: mapping)
            statStrip(device: device)
        }
        .padding(16)
        .background(
            LinearGradient(
                colors: [Color(weaveHex: "FFB340"), .firingOrange],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            )
        )
        .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
        .shadow(color: Color.firingOrange.opacity(0.3), radius: 24, x: 0, y: 8)
    }

    private var idleCard: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Nothing firing")
                .font(.title3.weight(.semibold))
            Text("Pair a Nuimo and add a connection to start firing.")
                .font(.subheadline)
                .foregroundStyle(.secondary)
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.weaveCardBackground)
        .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
    }

    private func topRow(eventLabel: String) -> some View {
        HStack(spacing: 8) {
            ZStack {
                Circle()
                    .fill(Color.white.opacity(0.25))
                    .frame(width: 28, height: 28)
                Image(systemName: "bolt.fill")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.white)
            }
            Text("FIRING NOW")
                .font(.caption.weight(.semibold))
                .tracking(0.8)
                .foregroundStyle(.white.opacity(0.9))
            Spacer(minLength: 0)
            Text(eventLabel)
                .font(.caption.monospacedDigit())
                .foregroundStyle(.white.opacity(0.8))
        }
    }

    private func mainRow(device: Device, mapping: Mapping) -> some View {
        HStack(alignment: .bottom) {
            VStack(alignment: .leading, spacing: 4) {
                Text(device.nickname ?? device.name)
                    .font(.system(size: 28, weight: .bold))
                    .foregroundStyle(.white)
                Text("rotate → \(mapping.serviceDisplayLabel) · volume")
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(0.9))
            }
            Spacer(minLength: 0)
            NuimoGlyph(size: 56, tone: .firing)
                .foregroundStyle(.white)
        }
    }

    private func statStrip(device: Device) -> some View {
        HStack(spacing: 0) {
            statCell(value: "55", label: "Volume")
            statCell(value: "22 Hz", label: "Tick rate")
            statCell(value: "\(device.battery ?? 0)%", label: "Battery")
        }
        .padding(10)
        .background(Color.black.opacity(0.15))
        .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
    }

    private func statCell(value: String, label: String) -> some View {
        VStack(spacing: 2) {
            Text(value)
                .font(.system(size: 18, weight: .bold, design: .monospaced))
                .foregroundStyle(.white)
            Text(label.uppercased())
                .font(.caption2.weight(.medium))
                .tracking(0.6)
                .foregroundStyle(.white.opacity(0.75))
        }
        .frame(maxWidth: .infinity)
    }
}

#Preview("Firing") {
    HomeFiringHero(
        mapping: PlaceholderData.firingMapping,
        device: PlaceholderData.devices.first
    )
    .padding()
    .background(Color.weaveGroupedBackground)
}

#Preview("Idle") {
    HomeFiringHero(mapping: nil, device: nil)
        .padding()
        .background(Color.weaveGroupedBackground)
}
