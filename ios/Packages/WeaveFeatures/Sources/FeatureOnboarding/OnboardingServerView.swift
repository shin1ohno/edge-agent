import SwiftUI
import WeaveCore
import WeaveDesign

/// Step 4 / 4. weave-server is **optional** — both "Connect & finish"
/// and "Skip — set up later" complete onboarding (handoff §4 mandates
/// skippability). The chosen path is persisted to
/// `DefaultsKeys.useServer` so Settings can mirror the decision.
/// Mockup reference: `#ios-onboard-4` / `IOSOnboardingServer`.
struct OnboardingServerView: View {
    var onConnect: () -> Void
    var onSkip: () -> Void

    private let snapshot = OnboardingSnapshot.placeholder
    @State private var selectedServerId: UUID?

    var body: some View {
        VStack(spacing: 0) {
            OnboardingStepIndicator(current: 3, total: 4)

            ScrollView {
                VStack(spacing: 0) {
                    heroIcon
                        .padding(.top, 24)

                    HStack(spacing: 6) {
                        Text("weave Server")
                            .font(.system(size: 28, weight: .bold))
                        Text("(optional)")
                            .font(.system(size: 28, weight: .bold))
                            .foregroundStyle(.secondary)
                    }
                    .padding(.top, 20)

                    Text("Server なしでも Nuimo は使えます。複数デバイス間の同期や cloud 制御を使うときだけ必要です。")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                        .multilineTextAlignment(.center)
                        .padding(.top, 6)
                        .padding(.horizontal, 24)

                    capabilitiesCard
                        .padding(.top, 24)
                        .padding(.horizontal, 16)

                    discoveredHeader
                        .padding(.top, 24)
                        .padding(.horizontal, 16)

                    discoveredCard
                        .padding(.horizontal, 16)
                        .padding(.bottom, 24)
                }
            }

            VStack(spacing: 2) {
                Button("Connect & finish", action: onConnect)
                    .weaveCTA()
                Button("Skip — set up later", action: onSkip)
                    .font(.body.weight(.semibold))
                    .foregroundStyle(Color.firingBlue)
                    .padding(.vertical, 10)
                    .frame(maxWidth: .infinity)
            }
            .padding(.horizontal, 24)
            .padding(.bottom, 32)
        }
        .onAppear {
            if selectedServerId == nil {
                selectedServerId = snapshot.discoveredServers.first?.id
            }
        }
    }

    private var heroIcon: some View {
        RoundedRectangle(cornerRadius: 20, style: .continuous)
            .fill(Color.green)
            .frame(width: 80, height: 80)
            .overlay {
                Image(systemName: "rectangle.connected.to.line.below")
                    .font(.system(size: 40, weight: .semibold))
                    .foregroundStyle(.white)
            }
    }

    private var capabilitiesCard: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text("WHAT YOU GET WITHOUT A SERVER")
                .font(.caption2.weight(.semibold))
                .foregroundStyle(.secondary)
                .padding(.bottom, 4)
            capabilityRow(symbol: "checkmark.circle.fill", color: .green, text: "Direct BLE → Roon / Hue / MIDI")
            capabilityRow(symbol: "checkmark.circle.fill", color: .green, text: "Local routes & haptics")
            capabilityRow(symbol: "xmark.circle.fill", color: .secondary, text: "Cross-device sync between iPhone / Mac / Watch", muted: true)
            capabilityRow(symbol: "xmark.circle.fill", color: .secondary, text: "Remote control over the internet", muted: true)
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.weaveCardBackground)
        .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
    }

    private func capabilityRow(symbol: String, color: Color, text: String, muted: Bool = false) -> some View {
        HStack(spacing: 8) {
            Image(systemName: symbol)
                .font(.footnote)
                .foregroundStyle(color)
            Text(text)
                .font(.footnote)
                .foregroundStyle(muted ? .secondary : .primary)
            Spacer(minLength: 0)
        }
        .padding(.vertical, 2)
    }

    private var discoveredHeader: some View {
        HStack {
            Text("DISCOVERED ON THIS NETWORK")
                .font(.caption2.weight(.semibold))
                .foregroundStyle(.secondary)
            Spacer()
        }
        .padding(.bottom, 6)
    }

    private var discoveredCard: some View {
        VStack(spacing: 0) {
            ForEach(Array(snapshot.discoveredServers.enumerated()), id: \.element.id) { idx, server in
                Button {
                    selectedServerId = server.id
                } label: {
                    HStack(spacing: 12) {
                        RoundedRectangle(cornerRadius: 6, style: .continuous)
                            .fill(selectedServerId == server.id ? Color.green : Color.secondary)
                            .frame(width: 28, height: 28)
                            .overlay {
                                Image(systemName: "rectangle.connected.to.line.below")
                                    .font(.caption.weight(.semibold))
                                    .foregroundStyle(.white)
                            }
                        VStack(alignment: .leading, spacing: 2) {
                            Text(server.hostname)
                                .font(.subheadline.weight(.medium))
                                .foregroundStyle(.primary)
                            Text("\(server.ipAddress) · \(server.latencyMillis) ms · v\(server.version)")
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                        }
                        Spacer(minLength: 0)
                        if selectedServerId == server.id {
                            Image(systemName: "checkmark.circle.fill")
                                .foregroundStyle(.green)
                        }
                    }
                    .padding(.horizontal, 14)
                    .padding(.vertical, 10)
                }
                .buttonStyle(.plain)
                if idx < snapshot.discoveredServers.count - 1 {
                    Divider().padding(.leading, 54)
                }
            }
        }
        .background(Color.weaveCardBackground)
        .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
    }
}

#Preview {
    OnboardingServerView(onConnect: {}, onSkip: {})
}
