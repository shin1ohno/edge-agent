//! UniFFI bindgen CLI shim. Run via
//! `cargo run --bin uniffi-bindgen -- generate --library <staticlib> --language swift --out-dir <dir>`.

fn main() {
    uniffi::uniffi_bindgen_main()
}
