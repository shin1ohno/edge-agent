# Installing Weave on iPhone / iPad

Weave's iOS app runs with a **free Apple ID** via Xcode's Personal Team. That means provisioning profiles are valid for **7 days**; plan for a weekly re-install.

This doc is the runbook for getting Weave onto a device for the first time and keeping it running.

---

## One-time setup (Mac)

1. **Xcode + command-line tools**

    ```bash
    xcode-select -p                                # should print /Applications/Xcode.app/Contents/Developer
    sudo xcodebuild -runFirstLaunch                # if Xcode was just installed / updated
    ```

2. **Rust iOS targets**

    ```bash
    rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
    ```

3. **XcodeGen**

    ```bash
    brew install xcodegen
    ```

4. **Xcode Apple ID sign-in** (enables Personal Team auto-provisioning)

    Open Xcode.app → Settings → Accounts → `+` → Apple ID → sign in with your free account. This is required once; the CLI can't do this step.

5. **Device trust + Developer Mode**

    - Plug the iPhone / iPad into the Mac with a **data** USB cable (some cables are charge-only).
    - On the device, tap **Trust This Computer** when prompted.
    - iOS 16+ requires Developer Mode:
        - If `/Settings → Privacy & Security → Developer Mode` is visible, enable it and reboot.
        - If it's missing, open the Xcode project on Mac (`open ios/WeaveIos.xcodeproj`), plug in the device, Cmd+R once — iOS shows the "Enable Developer Mode" prompt; accept, reboot, done.

---

## First install (Mac + device connected)

From the repo root:

```bash
cd ios
./build-xcframework.sh     # Rust → xcframework (1–2 min first time, seconds after)
xcodegen                    # generates WeaveIos.xcodeproj
open WeaveIos.xcodeproj     # Xcode opens
```

In Xcode:

1. Target **WeaveIos** → **Signing & Capabilities** → **Team**: pick your Personal Team.
2. Top-left scheme picker → pick the connected device.
3. **Cmd+R**.

On first launch the device asks you to trust the developer:

- Settings → General → VPN & Device Management → your Apple ID → **Trust**.

Then tap the Weave icon to launch.

---

## CLI-only rebuild loop (after first install)

Once the Apple ID is signed in and the device trusted, everything else runs from the shell. Two env vars are needed:

```bash
# 10-char Personal Team ID
export DEV_TEAM=$(security find-identity -v -p codesigning | awk -F'[()]' '/Apple Development/{print $2; exit}')

# UUID of the connected device
export UDID=$(xcrun devicectl list devices | awk '/Connected/{print $NF; exit}')
```

Then:

```bash
cd ios
./build-xcframework.sh     # re-run only when Rust sources change
./deploy.sh                 # build + sign + install + launch on device
```

`./deploy.sh` runs `xcodebuild … -allowProvisioningUpdates` and `xcrun devicectl device install app / process launch`. See the script header for details.

---

## 7-day re-sign cycle

Personal Team provisioning profiles last 7 days. When the app refuses to launch:

```bash
cd ~/ManagedProjects/edge-agent/ios
./deploy.sh
```

That re-signs and reinstalls in ~30 seconds. The app's **UserDefaults survive** (server URL, edge_id, paired Nuimo UUIDs are preserved).

If the device is not at hand on day 7, the app stops launching — no data is lost; the next `./deploy.sh` restores it.

> Plan a recurring calendar reminder on day 6 so the re-sign is predictable rather than reactive.

---

## Simulator loop (UI-only verification, no BLE)

The iOS simulator has no Bluetooth stack. Use it to verify layout, WS `/ws/ui`, and REST `/api/*` — not Nuimo features.

```bash
# pick a booted simulator
UDID=$(xcrun simctl list devices booted | awk -F'[()]' '/Booted/{print $2; exit}')

cd ios
./build-xcframework.sh
xcodegen

xcodebuild \
    -project WeaveIos.xcodeproj \
    -scheme WeaveIos \
    -configuration Debug \
    -destination "id=$UDID" \
    build

APP=$(xcodebuild -project WeaveIos.xcodeproj -scheme WeaveIos -configuration Debug \
    -destination "id=$UDID" -showBuildSettings 2>/dev/null \
    | awk -F= '/ CONFIGURATION_BUILD_DIR /{gsub(/^[ \t]+|[ \t]+$/,"",$2); print $2"/WeaveIos.app"}' | head -n1)

xcrun simctl install "$UDID" "$APP"
xcrun simctl launch "$UDID" com.shin1ohno.weave.WeaveIos

# Optional: pre-fill the server URL so Connections tab wires up on first open.
xcrun simctl spawn "$UDID" defaults write \
    com.shin1ohno.weave.WeaveIos \
    weave.server_url "http://pro.home.local:3100"
```

The Bluetooth tab will say `State: unsupported on this device` — that's expected.

---

## Troubleshooting

**`xcrun devicectl list devices` is empty**

The Mac doesn't see the device. Check:

1. `system_profiler SPUSBDataType | grep -i iphone` — if empty, cable / port issue (try a data cable, different port).
2. Device is unlocked.
3. "Trust This Computer" was tapped.
4. Developer Mode is on (see setup step 5).

**`xcodebuild` fails with "`Unable to load plug-in com.apple.dt.IDESimulatorFoundation`"**

Xcode CLT got out of sync with the app (common after an OS update):

```bash
sudo xcode-select -s /Applications/Xcode.app/Contents/Developer
sudo xcodebuild -runFirstLaunch
```

**Swift compile fails with "Unable to resolve module dependency: 'WeaveIosCore'"**

You have a leftover `import WeaveIosCore` from an older revision. The generated bindings are included as a loose source file in the target, not as a framework module — remove the import. The types (`NuimoEvent`, `Glyph`, `parseNuimoNotification`, …) are accessible without it.

**`error[E0463]: can't find crate for core`**

Missing rustup target:

```bash
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
```

**Build warnings about "object file was built for newer iOS-simulator version"**

Harmless. The Rust crates build against the installed iOS SDK (e.g. 26.4) while we link against deployment target 18.0. The linker tolerates the gap for simulator-arm64.

---

## What's not yet covered

- **RoutesEditor** (Mapping create / edit / delete via UI): Phase 6 v2. For now, create mappings from weave-web.
- **Server-side glyph persistence**: the in-app Glyph editor is offline. Save-to-server lands with the full UiClient glyph API wiring.
- **Background BLE**: the app has `bluetooth-central` background mode enabled, but state preservation and reconnect semantics under backgrounding haven't been validated yet.
- **App Store / TestFlight**: out of scope by design (Personal Team).
