fn main() {
    // Proc-macro scaffolding is handled at compile time by `uniffi::setup_scaffolding!`
    // in `src/lib.rs`. No UDL file is used. This build.rs is intentionally empty so
    // `cargo build` still re-runs this crate's build when its manifest changes.
}
