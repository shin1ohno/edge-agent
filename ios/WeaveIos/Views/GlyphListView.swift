@preconcurrency import SwiftUI

/// Minimal glyph list: a single "Try the editor" shortcut for now. In
/// Phase 4 (UiClient wiring), this list is populated from the
/// server-side glyph registry via `/api/glyphs`.
struct GlyphListView: View {
    var body: some View {
        NavigationStack {
            List {
                Section {
                    NavigationLink("New glyph") {
                        GlyphEditorView(name: "new_glyph")
                    }
                    NavigationLink("Editor (\"play\" seeded)") {
                        GlyphEditorView(
                            name: "play",
                            initialRows: [
                                0b000010000,
                                0b000110000,
                                0b001110000,
                                0b011110000,
                                0b111110000,
                                0b011110000,
                                0b001110000,
                                0b000110000,
                                0b000010000,
                            ]
                        )
                    }
                } footer: {
                    Text("Server glyph list wires in Phase 4 (UiClient /api/glyphs).")
                        .font(.caption)
                }
            }
            .navigationTitle("Glyphs")
        }
    }
}

#Preview {
    GlyphListView()
}
