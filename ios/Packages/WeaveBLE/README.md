# WeaveBLE

CoreBluetooth driver for Nuimo controllers. Consumed by the iOS and
watchOS app targets via local SPM path.

## Status: Phase 1 placeholder

This package currently only exposes the `NuimoDriver` protocol and a
`MockNuimoDriver` so the rest of the app graph compiles. The real
CoreBluetooth implementation will be ported from
`../../WeaveIos/Core/{BleBridge,NuimoDevice}.swift` in Phase 5 of the
handoff. The existing `WeaveIos` app keeps using that code unchanged
until the migration lands.
