@preconcurrency import SwiftUI

// UniFFI types (Glyph, DisplayOptions, DisplayTransition, buildLedPayload)
// live in Bundle/WeaveIosCore.swift, same target.

/// 9×9 tap-to-toggle editor for Nuimo LED glyphs. Save persists to the
/// weave-server `/api/glyphs/:name` endpoint — wired when `UiClient` lands
/// in Phase 4; for now the editor is offline and just lets the user see
/// and preview shapes.
struct GlyphEditorView: View {
    /// Each row is a 9-bit bitmask; bit 0 = leftmost pixel.
    @State private var rows: [UInt16] = Array(repeating: 0, count: 9)
    @State private var glyphName: String

    init(name: String = "glyph", initialRows: [UInt16]? = nil) {
        self._glyphName = State(initialValue: name)
        if let initialRows, initialRows.count == 9 {
            self._rows = State(initialValue: initialRows)
        }
    }

    var body: some View {
        Form {
            Section("Name") {
                TextField("Glyph name", text: $glyphName)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
            }

            Section("Pattern") {
                grid
                    .padding(.vertical, 8)
                    .frame(maxWidth: .infinity, alignment: .center)
            }

            Section {
                HStack {
                    Button("Clear")  { rows = Array(repeating: 0, count: 9) }
                    Spacer()
                    Button("Invert") { rows = rows.map { $0 ^ 0x1FF } }
                    Spacer()
                    Button("Fill")   { rows = Array(repeating: 0x1FF, count: 9) }
                }
                .buttonStyle(.bordered)
                .controlSize(.small)
            }

            Section("LED payload preview (13 bytes)") {
                Text(payloadPreview)
                    .font(.system(.caption, design: .monospaced))
                    .lineLimit(2)
                    .foregroundStyle(.secondary)
            }
        }
        .navigationTitle("Glyph")
        .navigationBarTitleDisplayMode(.inline)
    }

    // MARK: - Grid

    private var grid: some View {
        VStack(spacing: 2) {
            ForEach(0..<9, id: \.self) { r in
                HStack(spacing: 2) {
                    ForEach(0..<9, id: \.self) { c in
                        Rectangle()
                            .fill(isOn(r, c) ? Color.accentColor : Color.secondary.opacity(0.2))
                            .frame(width: 28, height: 28)
                            .contentShape(Rectangle())
                            .onTapGesture { toggle(r, c) }
                            .accessibilityLabel("row \(r) col \(c)")
                            .accessibilityValue(isOn(r, c) ? "on" : "off")
                    }
                }
            }
        }
    }

    private func isOn(_ row: Int, _ col: Int) -> Bool {
        (rows[row] >> col) & 1 == 1
    }

    private func toggle(_ row: Int, _ col: Int) {
        rows[row] ^= 1 << col
    }

    // MARK: - Payload preview

    private var payloadPreview: String {
        let glyph = Glyph(rows: rows)
        let opts = DisplayOptions(brightness: 1.0, timeoutMs: 2000, transition: .crossFade)
        guard let bytes = try? buildLedPayload(glyph: glyph, opts: opts) else {
            return "(invalid glyph)"
        }
        return bytes.map { String(format: "%02x", $0) }.joined(separator: " ")
    }
}

#Preview {
    NavigationStack {
        GlyphEditorView(name: "play")
    }
}
