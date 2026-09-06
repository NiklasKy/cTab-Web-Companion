# cTab Web Companion (cWEB)

cWEB is a read-only Arma 3 client mod that mirrors locally visible cTab and
regular Arma map data to a protected browser display on the same Windows PC.

## Requirements

- 64-bit Arma 3 on Windows
- cTab 2.2.2.1 or cTab Devastator Edition 2.3.0.0
- A modern default browser
- Internet access for uncached PlanOps Atlas terrain tiles

## Use

Load cWEB together with a supported cTab edition. The companion starts with
Arma and opens the browser after the player enters a mission. Unsupported
PlanOps terrains remain available through the coordinate-grid fallback.

The browser listens only on localhost and uses a temporary session token. The
display cannot edit the Arma or cTab map and does not expose a LAN service.

## Server signature key

Servers using signature verification can copy `keys/cweb_1_0_0.bikey` to the
server's `keys` directory. Never distribute a `.biprivatekey` file.

PBO signature verification does not establish BattlEye approval for the native
bridge and companion binaries. Validate those exact release binaries on a
BattlEye-protected test server before advertising protected-server support.

The deprecated 32-bit Arma extension is intentionally not included.

## Source and support

- Source: https://github.com/NiklasKy/cTab-Web-Companion
- Issues: https://github.com/NiklasKy/cTab-Web-Companion/issues

Author: `[GRP9] NiklasKy`
