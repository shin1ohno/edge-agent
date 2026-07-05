import SwiftUI
import WeaveCore
import WeaveDesign

/// "Recent" inset group under Quick actions. 4 placeholder rows from
/// `PlaceholderData.recentActivity`; Phase 5 swaps to `/ws/ui` event
/// tail.
struct HomeRecentList: View {
    let entries: [ActivityEntry]
    var onSeeAll: () -> Void

    var body: some View {
        Section {
            ForEach(entries) { entry in
                IOSInsetRow(
                    title: entry.title,
                    subtitle: "\(entry.subtitle) · \(entry.when)",
                    symbol: entry.sfSymbol,
                    color: Color(weaveHex: entry.accentHex)
                )
            }
        } header: {
            HStack {
                Text("Recent")
                    .textCase(nil)
                Spacer()
                Button("See all", action: onSeeAll)
                    .textCase(nil)
                    .font(.body)
                    .foregroundStyle(Color.firingBlue)
            }
        }
    }
}
