import SwiftUI
import WeaveCore
import WeaveDesign

/// Services tab — Connected list + "Add a service" inset group +
/// "More integrations…" footer row. Mirrors the hi-fi mockup
/// `IOSServicesList` + `SERVICES_CATALOG` (`#ios-tab-services`). Phase
/// 3 reads from `PlaceholderData.services`; Phase 5+ wires real
/// service-adapter status polling via `WeaveServices`.
public struct ServicesRootView: View {
    public init() {}

    private var connectedServices: [ServiceCatalogEntry] {
        PlaceholderData.services.filter(\.isConnected)
    }

    private var pendingServices: [ServiceCatalogEntry] {
        PlaceholderData.services.filter { !$0.isConnected }
    }

    public var body: some View {
        Form {
            connectedSection
            addSection
        }
        .navigationDestination(for: ServicesRoute.self) { route in
            switch route {
            case .service(let id):
                Text("Service detail (\(id)) — Phase 4")
                    .foregroundStyle(.secondary)
                    .navigationTitle("Service")
            }
        }
    }

    private var connectedSection: some View {
        Section {
            ForEach(connectedServices) { service in
                NavigationLink(value: ServicesRoute.service(service.id)) {
                    IOSInsetRow(
                        title: service.displayName,
                        subtitle: service.addressLine,
                        value: targetCountValue(for: service),
                        symbol: service.sfSymbol,
                        color: Color(weaveHex: service.accentHex)
                    )
                }
            }
        } header: {
            Text("Connected").textCase(nil)
        }
    }

    private var addSection: some View {
        Section {
            ForEach(pendingServices) { service in
                IOSInsetRow(
                    title: service.displayName,
                    subtitle: service.addressLine,
                    value: nil,
                    symbol: service.sfSymbol,
                    color: Color(weaveHex: service.accentHex)
                ) {
                    HStack(spacing: 6) {
                        if service.isNew {
                            Text("NEW")
                                .font(.caption2.weight(.semibold))
                                .foregroundStyle(Color.firingOrange)
                                .padding(.horizontal, 4)
                                .padding(.vertical, 1)
                                .background(Color.firingOrange.opacity(0.15))
                                .clipShape(RoundedRectangle(cornerRadius: 4, style: .continuous))
                        }
                        Button("Connect") {
                            // TODO(Phase 4): launch service connect flow.
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
            Button {
                // TODO(Phase 4): browse the weave gallery.
            } label: {
                IOSInsetRow(
                    title: "More integrations…",
                    subtitle: "Browse the weave gallery",
                    symbol: "plus.circle.fill",
                    color: Color(weaveHex: "8E8E93")
                )
                .foregroundStyle(.primary)
            }
            .buttonStyle(.plain)
        } header: {
            Text("Add a service").textCase(nil)
        }
    }

    private func targetCountValue(for service: ServiceCatalogEntry) -> AnyView? {
        guard service.targetCount > 0, !service.targetUnit.isEmpty else { return nil }
        return AnyView(
            Text("\(service.targetCount) \(service.targetUnit)")
                .font(.footnote)
                .foregroundStyle(.secondary)
        )
    }
}

enum ServicesRoute: Hashable {
    case service(String)
}

#Preview {
    NavigationStack {
        ServicesRootView()
            .navigationTitle("Services")
            .navigationDestination(for: ServicesRoute.self) { route in
                switch route {
                case .service(let id):
                    Text("Service detail (\(id)) — Phase 4")
                        .foregroundStyle(.secondary)
                }
            }
    }
}
