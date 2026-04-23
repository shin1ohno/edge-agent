#!/usr/bin/env bash
# Build WeaveIos from the CLI, install on a USB-connected device, launch.
#
# Prereqs (one-time):
#   - Apple ID signed into Xcode.app (Settings → Accounts) at least once so
#     the Personal Team can auto-provision via xcodebuild
#   - Device trusted on this Mac ("Trust This Computer")
#   - DEV_TEAM env var: 10-char Personal Team ID
#     (get via `security find-identity -v -p codesigning | grep "Apple Development"`)
#   - UDID env var: from `xcrun devicectl list devices`
#   - xcodegen has generated WeaveIos.xcodeproj
#   - build-xcframework.sh has produced Frameworks/WeaveIosCore.xcframework

set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE"

: "${DEV_TEAM:?Set DEV_TEAM=<10-char Personal Team ID>}"
: "${UDID:?Set UDID=<device UUID from xcrun devicectl list devices>}"

BUNDLE_ID="com.shin1ohno.weave.WeaveIos"
SCHEME="WeaveIos"

echo "[deploy] xcodebuild build"
xcodebuild \
    -project WeaveIos.xcodeproj \
    -scheme "$SCHEME" \
    -configuration Debug \
    -destination "generic/platform=iOS" \
    -allowProvisioningUpdates \
    DEVELOPMENT_TEAM="$DEV_TEAM" \
    clean build | xcbeautify 2>/dev/null || \
xcodebuild \
    -project WeaveIos.xcodeproj \
    -scheme "$SCHEME" \
    -configuration Debug \
    -destination "generic/platform=iOS" \
    -allowProvisioningUpdates \
    DEVELOPMENT_TEAM="$DEV_TEAM" \
    clean build

echo "[deploy] locating .app"
APP_PATH=$(xcodebuild \
    -project WeaveIos.xcodeproj \
    -scheme "$SCHEME" \
    -configuration Debug \
    -destination "generic/platform=iOS" \
    -showBuildSettings 2>/dev/null \
    | awk -F= '/ CONFIGURATION_BUILD_DIR /{gsub(/^[ \t]+|[ \t]+$/,"",$2); print $2"/WeaveIos.app"}' \
    | head -n1)

if [[ ! -d "$APP_PATH" ]]; then
    echo "[deploy] ERROR: .app not found at: $APP_PATH" >&2
    exit 1
fi
echo "[deploy] app: $APP_PATH"

echo "[deploy] installing to device $UDID"
xcrun devicectl device install app --device "$UDID" "$APP_PATH"

echo "[deploy] launching $BUNDLE_ID"
xcrun devicectl device process launch --device "$UDID" "$BUNDLE_ID"

echo "[deploy] done. To tail logs:"
echo "   log stream --device --predicate 'subsystem == \"com.shin1ohno.weave.WeaveIos\"'"
