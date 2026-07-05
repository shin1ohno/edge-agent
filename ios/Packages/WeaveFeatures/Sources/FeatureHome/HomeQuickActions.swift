import SwiftUI
import WeaveDesign

/// 2x2 grid of card buttons under the Home FiringHero. Phase 3 stubs
/// the actions; Phase 4 wires sheets / alerts for each.
struct HomeQuickActions: View {
    struct Action: Hashable {
        let id: String
        let symbol: String
        let title: String
        let subtitle: String
        let accentHex: String
    }

    let actions: [Action] = [
        Action(id: "pair",      symbol: "plus.circle.fill", title: "Pair Nuimo",     subtitle: "Bluetooth scan",      accentHex: "0A84FF"),
        Action(id: "new-conn",  symbol: "square.and.pencil", title: "New connection", subtitle: "rotate→volume…",       accentHex: "FF9F0A"),
        Action(id: "led-test",  symbol: "lightbulb.fill",   title: "Test LEDs",      subtitle: "Show \u{201C}A\u{201D} on sofa", accentHex: "FFB340"),
        Action(id: "reset",     symbol: "arrow.clockwise",  title: "Reset routes",   subtitle: "sofa default",         accentHex: "FF3B30"),
    ]

    let onTap: (Action) -> Void

    private let columns: [GridItem] = [
        GridItem(.flexible(), spacing: 8),
        GridItem(.flexible(), spacing: 8),
    ]

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("QUICK ACTIONS")
                .font(.caption.weight(.semibold))
                .tracking(0.5)
                .foregroundStyle(.secondary)
            LazyVGrid(columns: columns, spacing: 8) {
                ForEach(actions, id: \.id) { action in
                    Button {
                        onTap(action)
                    } label: {
                        actionCard(action)
                    }
                    .buttonStyle(.plain)
                }
            }
        }
    }

    private func actionCard(_ action: Action) -> some View {
        HStack(alignment: .top, spacing: 10) {
            IOSIconBadge(symbol: action.symbol, color: Color(weaveHex: action.accentHex), size: 32)
            VStack(alignment: .leading, spacing: 2) {
                Text(action.title)
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(.primary)
                Text(action.subtitle)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.weaveCardBackground)
        .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .stroke(Color.black.opacity(0.06), lineWidth: 0.5)
        }
    }
}

#Preview {
    HomeQuickActions(onTap: { _ in })
        .padding()
        .background(Color.weaveGroupedBackground)
}
