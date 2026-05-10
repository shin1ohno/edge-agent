import SwiftUI
import WeaveDesign

/// Home tab root. Phase 1 shows the FiringHeroCard preview only; full
/// "Firing now" + Recent activity sections land in Phase 3 — see
/// `#ios-tab-home`.
public struct HomeRootView: View {
    public init() {}

    public var body: some View {
        ScrollView {
            VStack(spacing: 12) {
                ForEach(FiringVariants.all, id: \.id) { variant in
                    FiringHeroCard(variant: variant)
                        .background(Color.weaveCardBackground)
                        .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
                }
            }
            .padding()
        }
        .background(Color.weaveGroupedBackground)
    }
}
