import SwiftUI

/// Red destructive row used at the bottom of detail screens (Forget
/// Nuimo / Delete connection / Disconnect service). Tapping presents a
/// system confirmation alert before invoking the destructive action.
public struct IOSDestructiveRow: View {
    public let title: String
    public let symbol: String
    public let alertMessage: String
    public let confirmLabel: String
    public let onConfirm: () -> Void

    @State private var showAlert: Bool = false

    public init(
        title: String,
        symbol: String = "xmark.circle.fill",
        alertMessage: String,
        confirmLabel: String = "Delete",
        onConfirm: @escaping () -> Void
    ) {
        self.title = title
        self.symbol = symbol
        self.alertMessage = alertMessage
        self.confirmLabel = confirmLabel
        self.onConfirm = onConfirm
    }

    public var body: some View {
        Button(role: .destructive) {
            showAlert = true
        } label: {
            HStack(spacing: 12) {
                IOSIconBadge(symbol: symbol, color: .firingRed)
                Text(title)
                    .foregroundStyle(Color.firingRed)
                Spacer(minLength: 0)
            }
            .padding(.vertical, 2)
        }
        .buttonStyle(.plain)
        .alert(title, isPresented: $showAlert) {
            Button("Cancel", role: .cancel) {}
            Button(confirmLabel, role: .destructive, action: onConfirm)
        } message: {
            Text(alertMessage)
        }
    }
}

#Preview {
    Form {
        Section {
            IOSDestructiveRow(
                title: "Forget this Nuimo",
                alertMessage: "Forget sofa? Pair it again to restore.",
                confirmLabel: "Forget"
            ) {}
        }
    }
}
