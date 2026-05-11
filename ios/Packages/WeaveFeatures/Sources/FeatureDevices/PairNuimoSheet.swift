import SwiftUI
import WeaveCore
import WeaveDesign

/// Modal sheet driving the pair-new-Nuimo flow. 4 phases mirror the
/// hi-fi mockup `IOSPairSheet` lines 6464-6520. Phase 5 wires the
/// scanning + naming phases to real CoreBluetooth + WeaveStore.add(:).
public struct PairNuimoSheet: View {
    @Environment(\.dismiss) private var dismiss

    enum Phase: Hashable {
        case scanning
        case found
        case naming
        case done
    }

    @State private var phase: Phase = .scanning
    @State private var nickname: String = "Living room"
    @State private var selectedEdgeId: String = PlaceholderData.edgeAgents.first?.id ?? "edge-living"

    public init() {}

    public var body: some View {
        NavigationStack {
            content
                .navigationTitle("Pair Nuimo")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .topBarLeading) {
                        Button("Cancel") { dismiss() }
                    }
                    ToolbarItem(placement: .topBarTrailing) {
                        Button("Save") {
                            advance(to: .done)
                        }
                        .font(.body.weight(.semibold))
                        .disabled(phase != .naming)
                        .opacity(phase == .naming ? 1 : 0.4)
                    }
                }
                .task(id: phase) { await advanceFromTimer() }
        }
    }

    @ViewBuilder
    private var content: some View {
        switch phase {
        case .scanning:
            scanningView
        case .found:
            foundView
        case .naming:
            namingView
        case .done:
            doneView
        }
    }

    private var scanningView: some View {
        VStack(spacing: 24) {
            ZStack {
                ForEach(0..<3, id: \.self) { i in
                    Circle()
                        .stroke(Color.firingBlue.opacity(0.6 - Double(i) * 0.2), lineWidth: 2)
                        .frame(width: 124 - CGFloat(i) * 32, height: 124 - CGFloat(i) * 32)
                }
                Image(systemName: "antenna.radiowaves.left.and.right")
                    .font(.system(size: 44, weight: .semibold))
                    .foregroundStyle(Color.firingBlue)
            }
            .frame(width: 132, height: 132)
            .padding(.top, 40)

            Text("Looking for Nuimos\u{2026}")
                .font(.system(size: 22, weight: .bold))

            Text("Nuimo を 3 秒以上長押しして pairing モードにしてください。")
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .padding(.horizontal, 32)

            Spacer()
        }
        .frame(maxWidth: .infinity)
    }

    private var foundView: some View {
        Form {
            Section {
                HStack(spacing: 12) {
                    NuimoGlyph(size: 32)
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Nuimo · DD:EE:FF")
                            .font(.body)
                        Text("-42 dBm · strong signal")
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                    }
                    Spacer(minLength: 0)
                    Button {
                        advance(to: .naming)
                    } label: {
                        Text("Pair")
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(.white)
                            .padding(.horizontal, 12)
                            .padding(.vertical, 5)
                            .background(Color.firingBlue)
                            .clipShape(RoundedRectangle(cornerRadius: 6, style: .continuous))
                    }
                    .buttonStyle(.plain)
                }
                .padding(.vertical, 2)
            } header: {
                Text("Found 1").textCase(nil)
            }
        }
    }

    private var namingView: some View {
        Form {
            Section {
                HStack {
                    Spacer()
                    NuimoGlyph(size: 88, tone: .firing)
                    Spacer()
                }
                .listRowBackground(Color.weaveGroupedBackground)
            }

            Section {
                TextField("Nickname", text: $nickname)
                    .textInputAutocapitalization(.sentences)
                    .submitLabel(.done)
            } header: {
                Text("Name").textCase(nil)
            }

            Section {
                ForEach(PlaceholderData.edgeAgents) { agent in
                    Button {
                        selectedEdgeId = agent.id
                    } label: {
                        IOSInsetRow(
                            title: "\(agent.displayName) (\(agent.hostLabel))",
                            symbol: "rectangle.connected.to.line.below",
                            color: agent.isConnected ? .green : Color(weaveHex: "8E8E93")
                        ) {
                            if selectedEdgeId == agent.id {
                                Image(systemName: "checkmark")
                                    .foregroundStyle(Color.firingBlue)
                            }
                        }
                    }
                    .buttonStyle(.plain)
                }
            } header: {
                Text("Edge agent").textCase(nil)
            } footer: {
                Text("この Nuimo を捌く端末。家にずっと居る端末を選んでください。")
            }

            Section {
                HStack {
                    IOSIconBadge(symbol: "lightbulb.fill", color: Color(weaveHex: "FFB340"))
                    Text("Show \u{201C}A\u{201D} on LEDs")
                    Spacer(minLength: 0)
                    Button("Test") {
                        // TODO(Phase 5): WeaveBLE.sendGlyph("A")
                    }
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.white)
                    .padding(.horizontal, 10)
                    .padding(.vertical, 4)
                    .background(Color.firingBlue)
                    .clipShape(RoundedRectangle(cornerRadius: 6, style: .continuous))
                    .buttonStyle(.plain)
                }
            }
        }
    }

    private var doneView: some View {
        VStack(spacing: 16) {
            Image(systemName: "checkmark.circle.fill")
                .font(.system(size: 80, weight: .bold))
                .foregroundStyle(.green)
            Text("Paired")
                .font(.title.bold())
            Text(nickname)
                .font(.body)
                .foregroundStyle(.secondary)
            Spacer()
        }
        .padding(.top, 80)
        .frame(maxWidth: .infinity)
    }

    private func advance(to next: Phase) {
        withAnimation(.easeInOut(duration: 0.22)) {
            phase = next
        }
    }

    private func advanceFromTimer() async {
        switch phase {
        case .scanning:
            try? await Task.sleep(for: .seconds(1.8))
            if phase == .scanning {
                advance(to: .found)
            }
        case .done:
            try? await Task.sleep(for: .seconds(0.6))
            if phase == .done {
                dismiss()
            }
        case .found, .naming:
            break
        }
    }
}

#Preview {
    PairNuimoSheet()
}
