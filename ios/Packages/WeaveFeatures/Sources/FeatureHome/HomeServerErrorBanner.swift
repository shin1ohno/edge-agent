import SwiftUI
import WeaveCore
import WeaveDesign

/// Surface shown above the Home tab when the optional weave-server is
/// unreachable. Mirrors the hi-fi mockup `IOSErrorServer` lines
/// 6540-6575. Phase 4 ships the view as a standalone component; Phase
/// 5 wires it to live server connectivity state (`@AppStorage`
/// `weave.use-server` + ping result).
public struct HomeServerErrorBanner: View {
    public var onRetry: () -> Void
    public var onContinueLocalOnly: () -> Void
    public var onSwitchToCloud: () -> Void

    public init(
        onRetry: @escaping () -> Void = {},
        onContinueLocalOnly: @escaping () -> Void = {},
        onSwitchToCloud: @escaping () -> Void = {}
    ) {
        self.onRetry = onRetry
        self.onContinueLocalOnly = onContinueLocalOnly
        self.onSwitchToCloud = onSwitchToCloud
    }

    public var body: some View {
        ScrollView {
            VStack(spacing: 16) {
                banner
                    .padding(.horizontal, 16)

                Form {
                    diagnosticsSection
                    devicesLocalSection
                }
                .scrollDisabled(true)
                .frame(minHeight: 480)
            }
            .padding(.vertical, 12)
        }
        .background(Color.weaveGroupedBackground)
    }

    private var banner: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 6) {
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundStyle(.white)
                Text("SERVER UNREACHABLE")
                    .font(.caption.weight(.semibold))
                    .tracking(0.6)
                    .foregroundStyle(.white)
            }
            Text("weave-server.local に届きません")
                .font(.title3.weight(.bold))
                .foregroundStyle(.white)
            Text("Local-only モードに切り替えて続行できます。Routes と haptics はそのまま動作します。同期と cloud 制御だけが一時的に止まります。")
                .font(.footnote)
                .foregroundStyle(.white.opacity(0.9))

            HStack(spacing: 8) {
                Button("Retry", action: onRetry)
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(Color.firingOrange)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 6)
                    .background(Color.white)
                    .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
                Button("Continue local-only", action: onContinueLocalOnly)
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(.white)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 6)
                    .background(Color.white.opacity(0.25))
                    .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
                Button("Switch to Cloud", action: onSwitchToCloud)
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(.white)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 6)
                    .background(Color.white.opacity(0.15))
                    .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
            }
            .buttonStyle(.plain)
            .padding(.top, 4)
        }
        .padding(16)
        .background(Color.firingOrange)
        .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
    }

    private var diagnosticsSection: some View {
        Section {
            IOSInsetRow(
                title: "Wi-Fi",
                value: AnyView(Text("HouseNet").foregroundStyle(.secondary)),
                symbol: "wifi",
                color: .firingBlue
            )
            IOSInsetRow(
                title: "Bluetooth",
                value: AnyView(
                    Text("OK · 3 devices")
                        .font(.footnote)
                        .foregroundStyle(.green)
                ),
                symbol: "antenna.radiowaves.left.and.right",
                color: .green
            )
            IOSInsetRow(
                title: "weave Server",
                value: AnyView(
                    Text("timeout")
                        .font(.footnote)
                        .foregroundStyle(Color.firingOrange)
                ),
                symbol: "rectangle.connected.to.line.below",
                color: .firingOrange
            )
            IOSInsetRow(
                title: "Last sync",
                value: AnyView(Text("47s ago").foregroundStyle(.secondary)),
                symbol: "square.grid.2x2",
                color: Color(weaveHex: "8E8E93")
            )
        } header: {
            Text("Diagnostics").textCase(nil)
        }
    }

    private var devicesLocalSection: some View {
        Section {
            ForEach(PlaceholderData.devices) { device in
                IOSInsetRow(
                    title: device.nickname ?? device.name,
                    subtitle: device.isOnline ? "Bluetooth で直接動作中" : "last beat 47s ago",
                    value: AnyView(
                        Text(device.isOnline ? "BLE" : "off")
                            .font(.footnote)
                            .foregroundStyle(device.isOnline ? .green : .secondary)
                    )
                ) {
                    NuimoGlyph(size: 28, tone: device.isOnline ? .firing : .plain)
                } accessory: {
                    EmptyView()
                }
            }
        } header: {
            Text("Devices · still local-controlled").textCase(nil)
        }
    }
}

#Preview {
    NavigationStack {
        HomeServerErrorBanner()
            .navigationTitle("Home")
    }
}
