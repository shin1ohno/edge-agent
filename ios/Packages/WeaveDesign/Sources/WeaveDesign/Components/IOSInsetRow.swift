import SwiftUI

/// Composable row primitive matching the hi-fi mockup's `Chrome.Row`
/// component. Use inside a SwiftUI `Form` / `List` `Section` block.
///
/// Layout: `[leading icon] [title + optional subtitle] [optional value
/// + optional accessory]`.
///
/// Accessory is a generic `View` so callers can drop in `EmptyView`,
/// `Image(systemName: "chevron.right")`, a `Toggle`, or a custom button.
public struct IOSInsetRow<Leading: View, Accessory: View>: View {
    public let leading: Leading
    public let title: String
    public let subtitle: String?
    public let value: AnyView?
    public let accessory: Accessory

    public init(
        title: String,
        subtitle: String? = nil,
        value: AnyView? = nil,
        @ViewBuilder leading: () -> Leading,
        @ViewBuilder accessory: () -> Accessory
    ) {
        self.title = title
        self.subtitle = subtitle
        self.value = value
        self.leading = leading()
        self.accessory = accessory()
    }

    public var body: some View {
        HStack(spacing: 12) {
            leading
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(.body)
                    .foregroundStyle(.primary)
                if let subtitle, !subtitle.isEmpty {
                    Text(subtitle)
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }
            }
            Spacer(minLength: 0)
            if let value {
                value
            }
            accessory
        }
        .padding(.vertical, 2)
    }
}

public extension IOSInsetRow where Leading == IOSIconBadge, Accessory == EmptyView {
    /// Convenience: icon-badge leading with no accessory.
    init(
        title: String,
        subtitle: String? = nil,
        value: AnyView? = nil,
        symbol: String,
        color: Color
    ) {
        self.init(title: title, subtitle: subtitle, value: value) {
            IOSIconBadge(symbol: symbol, color: color)
        } accessory: {
            EmptyView()
        }
    }
}

public extension IOSInsetRow where Leading == IOSIconBadge {
    /// Convenience: icon-badge leading with a custom accessory.
    init(
        title: String,
        subtitle: String? = nil,
        value: AnyView? = nil,
        symbol: String,
        color: Color,
        @ViewBuilder accessory: () -> Accessory
    ) {
        self.init(title: title, subtitle: subtitle, value: value) {
            IOSIconBadge(symbol: symbol, color: color)
        } accessory: accessory
    }
}
