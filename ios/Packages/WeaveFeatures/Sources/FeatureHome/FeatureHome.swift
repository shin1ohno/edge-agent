import SwiftUI
import WeaveCore
import WeaveDesign

/// Home tab — gradient FiringHero + Quick actions + Recent activity.
/// Mirrors the hi-fi mockup `IOSHome` (`#ios-tab-home`). Phase 3 reads
/// from `PlaceholderData`; Phase 5+ swaps in live state.
public struct HomeRootView: View {
    public init() {}

    private let mapping = PlaceholderData.firingMapping
    private let entries = PlaceholderData.recentActivity

    public var body: some View {
        ScrollView {
            VStack(spacing: 20) {
                HomeFiringHero(
                    mapping: mapping,
                    device: PlaceholderData.devices.first
                )
                .padding(.horizontal, 16)

                HomeQuickActions(onTap: { _ in
                    // TODO(Phase 4): present per-action sheet (pair / new
                    // connection / LED test / reset routes).
                })
                .padding(.horizontal, 16)

                Form {
                    HomeRecentList(entries: entries, onSeeAll: {
                        // TODO(Phase 4): push full activity log.
                    })
                }
                .scrollDisabled(true)
                .frame(height: CGFloat(entries.count) * 64 + 64)
            }
            .padding(.vertical, 12)
        }
        .background(Color.weaveGroupedBackground)
    }
}

#Preview("Home — firing") {
    NavigationStack {
        HomeRootView()
            .navigationTitle("Home")
    }
}
