#!/usr/bin/env bash
# Build WeaveIosCore.xcframework from `weave-ios-core` for the iOS app.
#
# Produces a universal xcframework with:
#   - device slice (aarch64-apple-ios)
#   - simulator slice (universal2: aarch64-apple-ios-sim + x86_64-apple-ios)
#
# Swift bindings are generated into ios/WeaveIos/Bundle/. Symlinks in the
# Xcode project point at these outputs.
#
# Must be run on macOS with Xcode command-line tools installed. Before first
# run:
#   rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios

set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$HERE/.." && pwd)"
CRATE_DIR="$REPO_ROOT/crates/weave-ios-core"
TARGET_DIR="$REPO_ROOT/target"
OUT_XCFRAMEWORK="$HERE/Frameworks/WeaveIosCore.xcframework"
OUT_SWIFT_DIR="$HERE/WeaveIos/Bundle"

LIB_NAME="weave_ios_core"
STATICLIB="lib${LIB_NAME}.a"

PROFILE="release"
CARGO_FLAGS=(--release -p weave-ios-core)

log() { printf '[build-xcframework] %s\n' "$*"; }

if [[ "$(uname)" != "Darwin" ]]; then
    echo "error: this script only runs on macOS (iOS SDK + xcodebuild required)" >&2
    exit 1
fi

log "building Rust staticlib for device + simulator targets"
cargo build --target aarch64-apple-ios       "${CARGO_FLAGS[@]}"
cargo build --target aarch64-apple-ios-sim   "${CARGO_FLAGS[@]}"
cargo build --target x86_64-apple-ios        "${CARGO_FLAGS[@]}"

DEVICE_LIB="$TARGET_DIR/aarch64-apple-ios/$PROFILE/$STATICLIB"
SIM_ARM_LIB="$TARGET_DIR/aarch64-apple-ios-sim/$PROFILE/$STATICLIB"
SIM_X86_LIB="$TARGET_DIR/x86_64-apple-ios/$PROFILE/$STATICLIB"
SIM_FAT_LIB="$TARGET_DIR/sim-universal/$PROFILE/$STATICLIB"

mkdir -p "$(dirname "$SIM_FAT_LIB")"
log "combining simulator slices into a fat universal2 staticlib"
lipo -create "$SIM_ARM_LIB" "$SIM_X86_LIB" -output "$SIM_FAT_LIB"

log "generating Swift bindings + module map"
BINDINGS_TMP="$(mktemp -d)"
trap 'rm -rf "$BINDINGS_TMP"' EXIT
(cd "$CRATE_DIR" && cargo run --bin uniffi-bindgen -- generate \
    --library "$DEVICE_LIB" \
    --language swift \
    --out-dir "$BINDINGS_TMP")

mkdir -p "$OUT_SWIFT_DIR"
cp "$BINDINGS_TMP/${LIB_NAME}.swift" "$OUT_SWIFT_DIR/WeaveIosCore.swift"

HEADERS_DIR="$TARGET_DIR/ios-headers"
rm -rf "$HEADERS_DIR"
mkdir -p "$HEADERS_DIR"
cp "$BINDINGS_TMP/${LIB_NAME}FFI.h"        "$HEADERS_DIR/"
cp "$BINDINGS_TMP/${LIB_NAME}FFI.modulemap" "$HEADERS_DIR/module.modulemap"

log "assembling xcframework"
rm -rf "$OUT_XCFRAMEWORK"
mkdir -p "$(dirname "$OUT_XCFRAMEWORK")"
xcodebuild -create-xcframework \
    -library "$DEVICE_LIB"  -headers "$HEADERS_DIR" \
    -library "$SIM_FAT_LIB" -headers "$HEADERS_DIR" \
    -output  "$OUT_XCFRAMEWORK"

log "done. artifacts:"
log "  xcframework: $OUT_XCFRAMEWORK"
log "  swift binding: $OUT_SWIFT_DIR/WeaveIosCore.swift"
