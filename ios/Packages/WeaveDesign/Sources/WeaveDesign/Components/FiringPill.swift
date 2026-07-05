import SwiftUI

/// Status pill used to flag a mapping (or device) as currently
/// "firing" — animated dot + lowercase text, accent `firingOrange`.
public struct FiringPill: View {
    public enum Tone {
        case firing
        case idle(String)
    }

    public let tone: Tone

    public init(tone: Tone = .firing) {
        self.tone = tone
    }

    public var body: some View {
        switch tone {
        case .firing:
            HStack(spacing: 4) {
                Circle()
                    .fill(Color.firingOrange)
                    .frame(width: 6, height: 6)
                Text("firing")
                    .font(.footnote.weight(.medium))
                    .foregroundStyle(Color.firingOrange)
            }
        case .idle(let label):
            Text(label)
                .font(.footnote)
                .foregroundStyle(.secondary)
        }
    }
}

#Preview {
    VStack(alignment: .leading, spacing: 8) {
        FiringPill(tone: .firing)
        FiringPill(tone: .idle("12m ago"))
        FiringPill(tone: .idle("just now"))
    }
    .padding()
}
