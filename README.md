# cTab Web Companion

cTab Web Companion is a read-only Arma 3 client mod that mirrors cTab and regular map data into a browser on the same computer. It is intended for players who want to use a second display as a practical cTab map.

## Status

Version 1.0.0 supports both original cTab 2.2.2.1 and cTab Devastator Edition 2.3.0.0 through one shared read-only adapter contract. It includes lifecycle heartbeats, automatic companion shutdown and bounded restart, safe cache limits, and non-tactical diagnostics alongside the live map and adapter implementation.

The first edition targets:

- cTab 2.2.2.1 (original edition)
- cTab 2.3.0.0 (Devastator Edition)
- Regular Arma map markers
- Vanilla and community terrains resolved by exact Arma world name or catalog alias through PlanOps Atlas and cached locally
- Steam Workshop-only installation and updates

The MVP is display-only. Marker editing, cTab messaging, video feeds, remote access, and LAN hosting are outside the initial scope.

Author: `[GRP9] NiklasKy`

## Intended Player Experience

1. Subscribe to the Workshop item.
2. Enable it together with a supported cTab edition in the Arma 3 Launcher.
3. Start Arma normally through Steam.
4. The bundled local companion starts automatically.
5. The browser opens when the player enters a mission.
6. Terrain tiles are downloaded from PlanOps Atlas on demand and cached for later use. Terrains not available there remain usable with the coordinate-grid fallback.

No separate shortcut, administrator installation, or manually started web server is intended.

## Developer Validation

Run the frontend build before Rust commands because the production web assets are embedded in the companion executable:

```powershell
npm ci
npm run typecheck
npm test
npm run build
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
hemtt check
hemtt build
```

The same validation can be run with `./tools/Test-Project.ps1`.

The frontend wrapper uses an isolated temporary source copy because Vite treats the `#` in this repository's parent path as a URL fragment. It removes the copy after each command and copies only generated `web/dist` assets back after a successful build.

## Local Test Mod

Build the complete unsigned local test mod with:

```powershell
.\tools\Build-TestMod.ps1
```

The command runs the project validation suite, then creates:

- `build\@cTab Web Companion Test v35` for the Arma Launcher **Local mod** action
- `build\cTab-Web-Companion-Test-v35.zip` as a portable copy of the same folder

The generated `README_TESTING.md` contains the editor, browser, RPT, and shutdown checklist. BattlEye must remain disabled for this unsigned development smoke test.

## Public Release

Build the signed Publisher folder and matching release archive with:

```powershell
.\tools\Build-PublicMod.ps1
```

The command expects the private `cweb_1_0_0.biprivatekey` and its public
`cweb_1_0_0.bikey` counterpart in the sibling `Arma 3 Mod BiKey` directory by
default. Use `-PrivateKeyPath` for another private location. The private key is
never copied into the repository, Publisher folder, or archive.

If the Arma signing utilities are not available on `PATH` or in Steam's
standard Program Files library, pass their directory explicitly:

```powershell
.\tools\Build-PublicMod.ps1 -SigningToolsDirectory "X:\SteamLibrary\steamapps\common\Arma 3 Tools\DSSignFile"
```

The generated Publisher folder is `build\public\1.0.0\@cTab Web Companion`.
PBO signature verification and BattlEye approval of the native binaries remain
separate release checks.

## Git Policy

Commits and pushes require the project owner's explicit approval immediately before each action.
