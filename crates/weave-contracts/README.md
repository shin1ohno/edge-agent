# weave-contracts

WebSocket protocol types shared between [`edge-agent`](https://github.com/shin1ohno/edge-agent)
and `weave-server`.

Wire format is JSON text frames. Each frame is a single `ServerToEdge` or
`EdgeToServer` value. Nested types include `EdgeConfig`, `Mapping`, `Glyph`,
and related enums.

This crate is consumed by edge-side binaries (e.g. `nuimo-mqtt`) that need
to interoperate with a weave server without depending on the full edge-agent
runtime.

## License

MIT.
