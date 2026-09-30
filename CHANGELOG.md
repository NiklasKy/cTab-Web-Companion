# Changelog

All notable changes to cTab Web Companion are documented in this file.

## 1.0.3 - 2026-09-30

- Add read-only adapter support for the cTab provider integrated into the
  60th Solar Detachment AuxMod through `solar_60th_equipment_cTab`.
- Recognize the AuxMod's cTab marker texture paths and resolve those textures
  from the player's loaded local mod without packaging third-party assets.
- Expose the 60th Solar provider in the protocol, browser diagnostics, and
  adapter regression tests while retaining original and Devastator support.

## 1.0.2 - 2026-09-26

- Load satellite-only Atlas terrains such as Vidda when no supported topographic
  layer is available, while retaining topographic preference on other terrains.
- Place the pause-menu browser action directly below Continue, including after
  expanding or collapsing Configure.
- Keep concurrent Arma exports ordered and periodically reconcile the complete
  marker state to repair missed edits and deletions.
- Retry busy Windows named pipes when consecutive tactical updates arrive.
- Skip markers deleted during collection and identify rejected marker fields
  in diagnostics without logging their contents.
- Reconnect interrupted or stalled browser sessions automatically and replace
  stale state with the current authoritative snapshot.
- Split large reconciled snapshots into bounded browser messages and keep
  slow browser connections from holding the tactical-state lock.
- Batch map updates per animation frame, avoid rebuilding the unit panel for
  marker-only changes, and reduce label collision layout work.

## 1.0.1 - 2026-09-08

- Keep player-follow mode active while zooming with the mouse wheel.
- Add an `OPEN cWEB` action to the Arma pause menu for reopening the running
  companion in the default browser.
- Preserve manual browser-open requests during companion recovery without
  opening an additional automatic tab or replaying old browser requests.
- Reject unexpected fields in authenticated browser-control messages.

## 1.0.0 - 2026-09-06

- Add a protected localhost browser display for locally visible cTab and Arma
  map data.
- Support original cTab 2.2.2.1 and cTab Devastator Edition 2.3.0.0.
- Display the local player, friendly BFT groups and vehicles, cTab user
  markers, and regular Arma map markers.
- Extract loaded mod marker icons and colors on demand without packaging
  third-party assets.
- Resolve supported terrain maps through PlanOps Atlas with a bounded local
  cache and coordinate-grid fallback.
- Add player following, label collision handling, map muting, and independent
  visibility controls.
- Add automatic companion lifecycle handling and privacy-safe diagnostics.
- Publish the cWEB launcher branding and signed `cweb_1_0_0` PBO release.
- Target 64-bit Arma 3 without shipping the deprecated 32-bit extension.
