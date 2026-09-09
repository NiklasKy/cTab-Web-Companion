# cTab Web Companion Local Test v36

This is an unsigned local development build. It is not a Workshop release and has not been approved by BattlEye.

## Before Starting

1. Close Arma 3 and any previous `ctab-web-companion.exe` process.
2. Keep BattlEye disabled for this local editor smoke test.
3. In the Arma 3 Launcher, add the complete `@cTab Web Companion Test v36` folder as a local mod.
4. Confirm the Arma 3 Launcher displays the 512-pixel cWEB logo without a missing-picture warning.
5. Enable either original `cTab` 2.2.2.1 or cTab Devastator Edition 2.3.0.0 together with this test mod, then start the 64-bit Arma 3 client.

## Live Mission Test

1. Open the Eden Editor on Altis, place one playable unit, and start the scenario.
2. Wait up to ten seconds for the browser to open on a random `127.0.0.1` port.
3. Confirm the page shows the real mission name, Altis, the local player plus locally available cTab BFT contacts, and `Live - localhost`.
4. Confirm that the local player uses the blue own-position dot with a soft glow. Teleport or move the player; its position should update within approximately 250 milliseconds without refreshing.
5. Enable player following, zoom in and out with the mouse wheel, and keep moving the player. Following must stay enabled and the map must continue tracking the player. Drag the map or press an arrow key over the map and confirm following turns off.
6. Create several icon markers with text on the Arma map. Their original Arma symbols should appear in the browser within approximately one second. Black outlines and internal black details must remain black while white icon areas receive the selected marker color.
7. Set markers to BLUFOR, OPFOR, Independent, Civilian, and Unknown side colors. Confirm that the browser shows blue, red, green, purple, and yellow instead of white.
8. Edit a marker's position, direction, color, alpha, text, or size and confirm the browser follows the change.
9. Create rectangle and ellipse markers. If the mission supports Arma polyline markers, create one as well.
10. Delete every test marker and confirm none remains in the browser after approximately one second.
11. Enable a Workshop marker pack, place one of its custom `CfgMarkers` icons, and select one of its custom `CfgMarkerColors` entries. Confirm the browser uses the mod icon and the same RGBA color as Arma instead of a fallback symbol or vanilla color. Also leave several custom markers on their default color and confirm inherited `CfgMarkers` colors, multitone black details, and white flag areas match Arma.
12. Refresh the browser tab and confirm the latest player position and marker state is restored.
13. Add at least one other cTab-equipped unit, one friendly group, and one occupied friendly vehicle. Compare their browser symbols, labels, colors, and movement with the original cTab display.
14. Move a BFT contact, change its group, enter or leave a vehicle, respawn it, and delete it. Membership should reconcile within a few seconds and movement should update at approximately 4 Hz.
15. Change a tracked group's size across the cTab team/squad/section thresholds. The black size dots must update within approximately two seconds without refreshing the page.
16. Create cTab user markers with a group-size overlay and reported movement direction. Confirm creation, update, deletion, and browser refresh behavior.
17. Confirm the floating top controls keep the GRP9 logo and application name on the left, equal-width map-control buttons beside them, and the local-link status on the right.
18. Toggle **Unit panel**, **Units**, **Markers**, **Grid**, **BFT names**, and **Muted map** independently. Confirm **Center player** and **Fit terrain** remain available on wide displays.
19. Confirm the left unit panel lists friendly groups and tracked vehicles, excludes individual squad members, and fits each consistently sized tactical symbol without clipping. Its narrow dark custom scrollbar and right content gutter must remain visible while the list scrolls, and system source text must wrap instead of widening the panel. Per-row coordinates and presence dots must not appear.
20. Collapse the left panel and confirm the map uses the released space. Reopen it with the floating **Units** control at the bottom-left map edge.
21. Confirm BFT symbols use bright blue while their names remain readable in compact dark labels over both bright and dark terrain. Disabling **BFT names** must keep the symbols visible.
22. Confirm **Muted map** darkens and desaturates only the terrain tiles without muting the grid, unit symbols, marker symbols, or labels.
23. Place several labels at nearly the same position. Symbols must remain visible while substantially overlapping lower-priority labels are suppressed in this order: groups, vehicles, cTab user markers, regular map markers, and individual BFT contacts. Labels that only touch or overlap by a narrow edge must remain visible.
24. Confirm normal map-marker labels have a visible gap from their symbols.
25. Confirm the system panel shows the detected cTab edition, terrain, bounded tile-cache usage, and last heartbeat age. Use **Copy diagnostics** and verify the copied text contains no mission name, unit names, marker text, coordinates, or session token.
26. End the mission. The browser may remain open, but its localhost server must stop accepting connections after the 15-second heartbeat grace period.
27. Start another mission without restarting Arma. The bridge must restart the companion and open the new protected localhost session automatically.

## Pause Menu Browser Reopen

1. Close the browser tab during a running mission, press Escape, and select **OPEN cWEB**. The default browser must reopen the current localhost session with live data.
2. Close and reopen the pause menu several times. It must contain exactly one **OPEN cWEB** button, with no overlap with the vanilla controls at the tested UI scale.
3. Repeat the browser reopen action. Each click must open one tab without resetting the mission state or revealing the browser token in the RPT.
4. During a mission, terminate only the test companion process. Allow recovery, wait at least five seconds, terminate it again, and immediately select **OPEN cWEB**. If this action triggers recovery, it must still open exactly one tab even after the automatic replacement-tab allowance has been used.
5. Repeat the menu checks with original cTab and Devastator Edition, including a multiplayer mission. Record any UI-scale or other-mod conflicts.

## Multiplayer Reconciliation

1. Join a mission after other players have already placed regular Arma and cTab markers. Existing markers must appear within two seconds.
2. Let two players create and delete cTab markers at the same time. The browser must converge to the state currently visible in cTab without stale duplicates.
3. Rename an ACE group and move a player between groups without reopening cTab. Labels and membership must update within two seconds.
4. Let another group enter and drive a vehicle. Its marker must continue moving, and superseded individual or group markers must disappear within two seconds.
5. Join a mission whose PlanOps or Metis setup creates several thousand regular map markers. Existing BFT entities must be present as soon as the browser authenticates.
6. Keep that mission running for at least four hours. The browser must remain stable, marker changes must stay current, and the RPT must not contain repeating `uptime_ms` publish failures.

## Navigation Equipment Access

Run these checks while the browser remains open. Each equipment change should take effect within approximately one second without refreshing the page.

1. Remove every map, GPS, cTab device, and supported vehicle terminal from the local player. Confirm that terrain tiles, the coordinate grid, own position, regular markers, cTab markers, and BFT contacts are all hidden.
2. Equip only an `ItemMap`. Confirm that terrain tiles, the grid, and regular Arma markers are visible, while own position and BFT data remain hidden.
3. Equip only an `ItemGPS`. Confirm that the grid and own position are visible, while terrain tiles, regular Arma markers, and BFT data remain hidden.
4. Equip `ItemcTab`, `ItemAndroid`, or `ItemMicroDAGR`. Confirm that terrain, own position, regular markers, cTab markers, and permitted BFT contacts are visible.
5. Remove the handheld cTab device and occupy a cTab-enabled FBCB2 or TAD vehicle seat. Confirm that full cTab access remains available only while the player occupies the enabled seat.
6. Remove access again and confirm that already-rendered tactical entities and markers disappear instead of remaining cached in the browser.

## Terrain Catalog Resolution

1. Start Tanoa or a terrain variant that reports the Arma world name `Tanoa`. Confirm the browser loads the PlanOps `tanoa` map.
2. Start a community terrain represented in PlanOps Atlas by its exact world name. Confirm the terrain loads without a Companion code change.
3. If available, start a terrain whose Arma world name is a PlanOps alias. Confirm the canonical catalog map loads only when its world size matches the active Arma terrain.
4. Start a terrain absent from PlanOps Atlas. Confirm the browser keeps the coordinate-grid fallback and does not request tiles from an unrelated host.
5. On Kamino, Jabiim, and G.O.S N'Djenahoud, confirm the terrain image, coordinate grid, player, BFT entities, and markers share the same Arma-local `0..worldSize` coordinate space despite the non-zero PlanOps raster origin.

Only validated heartbeat messages keep the companion alive. A repeated same-session failure can open at most one replacement tab; subsequent rate-limited recovery attempts remain silent. If a browser falls behind the live stream, it receives the current reconciled snapshot instead of silently dropping updates.

Only values already present in the local client's cTab lists and `allMapMarkers` result are exported. The adapter does not reconstruct server-only or encrypted marker data.

Base-game, BFT, and currently loaded mod marker textures are read from the player's own Arma 3 installation and converted on demand. A mod texture is searched only in the `Addons` directory reported for its loaded Steam Workshop item or local mod folder. Converted PNG files are stored only in `%LOCALAPPDATA%\cTabWeb\marker-cache\v1`. No extracted Arma, cTab, or third-party mod texture is included in this test package. Original-cTab-only `\cTab\img` texture paths retain a visible fallback in this build.

## RPT Check

Open the newest RPT below `%LOCALAPPDATA%\Arma 3\` and search for `cTab Web Companion`.

Expected lines:

```text
[cTab Web Companion] Native bridge accepted the start request.
[cTab Web Companion] Original cTab 2.2.2.1 adapter detected.
[cTab Web Companion] Initial live snapshot queued (... tracked entities, ... markers).
```

Record any nearby `callExtension`, missing-extension, script, or companion error exactly as written.

## Shutdown and Recovery

Normal mission shutdown is automatic after the 15-second heartbeat grace period. `Stop-TestCompanion.ps1` remains available only as a manual recovery tool if a test process becomes stuck.
If no mission becomes ready, the unused companion exits after 60 seconds and is restarted automatically when a later mission publishes its first snapshot.
