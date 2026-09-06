import "leaflet/dist/leaflet.css";
import "./style.css";
import { appendText, replaceText } from "./dom";
import { applyEnvelope, type LiveState } from "./live-state";
import { sortTacticalEntitiesByLabel } from "./panel";
import {
  createCtabEntityIcon,
  entityDisplayCategory,
  TacticalMap,
  type TacticalLayer,
  type TerrainStatus
} from "./map";
import { parseTacticalEnvelope, type SessionSnapshot } from "./protocol";

const SESSION_TOKEN_KEY = "ctab-web-session-token";

const app = requireElement("app");
const shell = document.createElement("main");
shell.className = "app-shell";

const header = document.createElement("header");
header.className = "topbar";
const brand = document.createElement("div");
brand.className = "brand";
const brandMark = document.createElement("img");
brandMark.className = "brand-mark";
brandMark.src = "/grp9-logo.png";
brandMark.alt = "Gruppe 9 logo";
const brandCopy = document.createElement("div");
appendText(brandCopy, "strong", "cTab Web Companion");
appendText(brandCopy, "span", "Local tactical display", "eyebrow");
brand.append(brandMark, brandCopy);

const headerActions = document.createElement("div");
headerActions.className = "header-actions";
headerActions.setAttribute("role", "toolbar");
headerActions.setAttribute("aria-label", "Map display controls");
const panelButton = toggleButton("Unit panel", true);
panelButton.setAttribute("aria-controls", "unit-panel");
const unitLayerButton = toggleButton("Units", true);
const unitLayerCount = appendText(unitLayerButton, "span", "0", "control-count");
const markerLayerButton = toggleButton("Markers", true);
const markerLayerCount = appendText(markerLayerButton, "span", "0", "control-count");
const gridLayerButton = toggleButton("Grid", true);
const bftNamesButton = toggleButton("BFT names", true);
const mutedMapButton = toggleButton("Muted map", true);
const followPlayerButton = toggleButton("Follow player", false);
const focusButton = actionButton("Center player");
focusButton.classList.add("view-action");
const fitButton = actionButton("Fit terrain");
fitButton.classList.add("view-action");
headerActions.append(
  panelButton,
  unitLayerButton,
  markerLayerButton,
  gridLayerButton,
  bftNamesButton,
  mutedMapButton,
  followPlayerButton,
  focusButton,
  fitButton
);

const connection = document.createElement("div");
connection.className = "connection-summary";
appendText(connection, "span", "LOCAL LINK", "section-label");
const status = appendText(connection, "strong", "Connecting", "status status-waiting");
header.append(brand, headerActions, connection);

const body = document.createElement("div");
body.className = "workspace";
const panel = document.createElement("aside");
panel.className = "side-panel";
panel.id = "unit-panel";

const operation = document.createElement("section");
operation.className = "operation-summary";
appendText(operation, "span", "OPERATION", "section-label");
const missionName = appendText(operation, "strong", "Waiting for Arma...", "mission-name");
const terrainName = appendText(operation, "span", "No terrain data", "muted");

const unitSection = document.createElement("section");
unitSection.className = "unit-section";
const unitSectionHeader = document.createElement("div");
unitSectionHeader.className = "panel-section-header";
appendText(unitSectionHeader, "span", "FRIENDLY UNITS", "section-label");
const unitCount = appendText(unitSectionHeader, "strong", "0 / 0", "unit-count");
const feed = document.createElement("div");
feed.className = "unit-list";
appendText(feed, "span", "A protected local session will appear here.", "empty-state");
unitSection.append(unitSectionHeader, feed);

const panelFooter = document.createElement("footer");
panelFooter.className = "panel-footer";
appendText(panelFooter, "span", "SYSTEM", "section-label");
const runtimeGrid = document.createElement("div");
runtimeGrid.className = "runtime-grid";
const editionStatus = runtimeItem("EDITION", "Waiting");
const terrainStatusItem = runtimeItem("TERRAIN", "Waiting");
const cacheStatus = runtimeItem("TILE CACHE", "Bounded local cache");
const updateStatus = runtimeItem("LAST UPDATE", "Waiting");
runtimeGrid.append(editionStatus.root, terrainStatusItem.root, cacheStatus.root, updateStatus.root);
const diagnosticsButton = actionButton("Copy diagnostics");
diagnosticsButton.classList.add("diagnostics-button");
const terrainSource = appendText(panelFooter, "span", "Map source pending", "terrain-source");
const collapseButton = actionButton("Collapse panel");
collapseButton.classList.add("collapse-button");
collapseButton.setAttribute("aria-controls", "unit-panel");
panelFooter.append(runtimeGrid, diagnosticsButton, collapseButton);
panel.append(operation, unitSection, panelFooter);

const mapStage = document.createElement("section");
mapStage.className = "map-stage";
const mapElement = document.createElement("div");
mapElement.id = "map";
mapElement.setAttribute("aria-label", "Tactical map");
const panelReopenButton = actionButton("Units ›");
panelReopenButton.classList.add("panel-reopen-button");
panelReopenButton.setAttribute("aria-label", "Open friendly unit panel");
panelReopenButton.setAttribute("aria-controls", "unit-panel");
mapStage.append(mapElement, panelReopenButton);
body.append(panel, mapStage);
shell.append(header, body);
app.append(shell);

const token = readSessionToken();
let tacticalMap: TacticalMap | null = null;
let liveState: LiveState | null = null;
if (token === null) {
  setStatus("Missing session token", "error");
} else {
  tacticalMap = new TacticalMap(
    mapElement,
    token,
    renderTerrainStatus,
    (following) => followPlayerButton.setAttribute("aria-pressed", String(following))
  );
  connect(token);
  void refreshRuntimeStatus(token);
  window.setInterval(() => void refreshRuntimeStatus(token), 2_000);
}

focusButton.addEventListener("click", () => tacticalMap?.focusPlayer());
followPlayerButton.addEventListener("click", () => {
  const requested = followPlayerButton.getAttribute("aria-pressed") !== "true";
  const following = tacticalMap?.setFollowPlayer(requested) ?? false;
  followPlayerButton.setAttribute("aria-pressed", String(following));
});
fitButton.addEventListener("click", () => tacticalMap?.fitTerrain());
const togglePanel = (): void => {
  const collapsed = body.classList.toggle("panel-collapsed");
  panelButton.setAttribute("aria-pressed", String(!collapsed));
  window.requestAnimationFrame(() => tacticalMap?.invalidateSize());
};
panelButton.addEventListener("click", togglePanel);
collapseButton.addEventListener("click", togglePanel);
panelReopenButton.addEventListener("click", togglePanel);
bindLayerToggle(unitLayerButton, "entities");
bindLayerToggle(markerLayerButton, "markers");
bindLayerToggle(gridLayerButton, "grid");
bindMapViewToggle(bftNamesButton, (visible) => tacticalMap?.setBftNamesVisible(visible));
bindMapViewToggle(mutedMapButton, (muted) => tacticalMap?.setMapMuted(muted));
diagnosticsButton.addEventListener("click", () => void copyDiagnostics());

function connect(sessionToken: string): void {
  const socketProtocol = window.location.protocol === "https:" ? "wss:" : "ws:";
  const socket = new WebSocket(`${socketProtocol}//${window.location.host}/ws`);
  socket.addEventListener("open", () => {
    socket.send(JSON.stringify({ type: "authenticate", token: sessionToken }));
    setStatus("Authenticating", "waiting");
  });
  socket.addEventListener("message", (event: MessageEvent<unknown>) => {
    if (typeof event.data !== "string" || event.data.length > 262_144) {
      setStatus("Rejected invalid data", "error");
      socket.close(1003, "invalid payload");
      return;
    }
    let parsed: unknown;
    try {
      parsed = JSON.parse(event.data) as unknown;
    } catch {
      setStatus("Rejected malformed data", "error");
      return;
    }
    const envelope = parseTacticalEnvelope(parsed);
    if (envelope !== null) {
      const previous = liveState;
      liveState = applyEnvelope(liveState, envelope);
      if (liveState === previous || liveState === null) {
        return;
      }
      if (envelope.type === "session_snapshot") {
        renderSnapshot(liveState.snapshot);
      } else if (envelope.type === "position_delta") {
        tacticalMap?.updatePositions(liveState.snapshot);
      } else if (envelope.type === "entity_delta") {
        tacticalMap?.updateEntities(liveState.snapshot);
        renderPanel(liveState.snapshot);
      } else {
        tacticalMap?.updateMarkers(liveState.snapshot);
        renderPanel(liveState.snapshot);
      }
      setStatus("Live · localhost", "connected");
    }
  });
  socket.addEventListener("close", () => setStatus("Disconnected", "error"));
  socket.addEventListener("error", () => setStatus("Connection error", "error"));
}

function renderSnapshot(snapshot: SessionSnapshot): void {
  tacticalMap?.render(snapshot);
  renderPanel(snapshot);
}

function renderPanel(snapshot: SessionSnapshot): void {
  replaceText(missionName, snapshot.mission_name);
  replaceText(terrainName, `${snapshot.terrain.display_name} · ${Math.round(snapshot.terrain.world_size / 1_000)} km`);
  const friendlyEntities = sortTacticalEntitiesByLabel(snapshot.entities.filter(isPanelEntity));
  replaceText(unitCount, `${friendlyEntities.length} / ${friendlyEntities.length}`);
  replaceText(unitLayerCount, String(friendlyEntities.length));
  replaceText(markerLayerCount, String(snapshot.markers.length));
  feed.replaceChildren();
  for (const entity of friendlyEntities.slice(0, 200)) {
    const row = document.createElement("div");
    row.className = "unit-row";
    const iconCell = document.createElement("span");
    iconCell.className = "unit-list-icon";
    if (token !== null) {
      const markerIcon = createCtabEntityIcon(entity, token);
      const markerRoot = markerIcon.options.html;
      if (markerRoot instanceof HTMLElement) {
        markerRoot.querySelector(".ctab-bft-label")?.remove();
        markerRoot.classList.add("unit-list-marker");
        iconCell.append(markerRoot);
      }
    }
    const copy = document.createElement("span");
    copy.className = "unit-row-copy";
    appendText(copy, "strong", entity.label);
    appendText(copy, "span", entity.kind === "bft_vehicle" ? "VEHICLE" : "FRIENDLY GROUP", "unit-type");
    row.prepend(iconCell, copy);
    feed.append(row);
  }
  if (friendlyEntities.length > 200) {
    appendText(feed, "span", "Additional friendly units remain visible on the map.", "empty-state");
  } else if (friendlyEntities.length === 0) {
    const message = snapshot.capabilities.bft
      ? "No friendly BFT groups are currently available."
      : "Equip a supported cTab device to display friendly units.";
    appendText(feed, "span", message, "empty-state");
  }
}

function isPanelEntity(entity: SessionSnapshot["entities"][number]): boolean {
  return entityDisplayCategory(entity) !== "member";
}

function renderTerrainStatus(terrainStatus: TerrainStatus): void {
  terrainSource.className = `terrain-source terrain-source-${terrainStatus.kind}`;
  if (terrainStatus.kind === "ready") {
    terrainSource.title = terrainStatus.attribution;
    replaceText(terrainSource, `${terrainStatus.message} | ${terrainStatus.attribution}`);
  } else {
    terrainSource.removeAttribute("title");
    replaceText(terrainSource, terrainStatus.message);
  }
}

interface RuntimeStatus {
  connection: "live" | "waiting";
  edition: "none" | "original" | "devastator" | "unsupported";
  terrain: string | null;
  update_age_ms: number | null;
  heartbeat_timeout_ms: number;
  terrain_cache_bytes: number;
  terrain_cache_limit_bytes: number;
  marker_cache_bytes: number;
  marker_cache_limit_bytes: number;
  diagnostics: string[];
}

let lastRuntimeStatus: RuntimeStatus | null = null;

async function refreshRuntimeStatus(sessionToken: string): Promise<void> {
  try {
    const response = await fetch(`/status/${encodeURIComponent(sessionToken)}`, {
      cache: "no-store",
      credentials: "same-origin"
    });
    if (!response.ok) return;
    const value = await response.json() as unknown;
    if (!isRuntimeStatus(value)) return;
    lastRuntimeStatus = value;
    replaceText(editionStatus.value, editionLabel(value.edition));
    replaceText(terrainStatusItem.value, value.terrain ?? "Waiting");
    replaceText(updateStatus.value, value.update_age_ms === null
      ? "Waiting"
      : `${Math.max(0, Math.round(value.update_age_ms / 1_000))} s ago`);
    replaceText(cacheStatus.value, `${formatBytes(value.terrain_cache_bytes)} / ${formatBytes(value.terrain_cache_limit_bytes)}`);
  } catch {
    replaceText(updateStatus.value, "Unavailable");
  }
}

async function copyDiagnostics(): Promise<void> {
  const value = lastRuntimeStatus;
  const summary = [
    "cTab Web Companion diagnostics",
    `Connection: ${value?.connection ?? "unknown"}`,
    `Edition: ${value?.edition ?? "unknown"}`,
    `Terrain: ${value?.terrain ?? "unknown"}`,
    `Tile cache: ${cacheStatus.value.textContent ?? "unknown"}`,
    `Marker cache: ${value === null ? "unknown" : `${formatBytes(value.marker_cache_bytes)} / ${formatBytes(value.marker_cache_limit_bytes)}`}`,
    `Last update: ${value?.update_age_ms ?? "unknown"} ms`,
    `Events: ${value?.diagnostics.join(", ") ?? "none"}`
  ].join("\n");
  try {
    await navigator.clipboard.writeText(summary);
    replaceText(diagnosticsButton, "Diagnostics copied");
  } catch {
    replaceText(diagnosticsButton, "Copy unavailable");
  }
  window.setTimeout(() => replaceText(diagnosticsButton, "Copy diagnostics"), 2_000);
}

function isRuntimeStatus(value: unknown): value is RuntimeStatus {
  if (typeof value !== "object" || value === null) return false;
  const candidate = value as Record<string, unknown>;
  return (candidate.connection === "live" || candidate.connection === "waiting")
    && ["none", "original", "devastator", "unsupported"].includes(String(candidate.edition))
    && (candidate.terrain === null || typeof candidate.terrain === "string")
    && (candidate.update_age_ms === null || (typeof candidate.update_age_ms === "number" && Number.isFinite(candidate.update_age_ms)))
    && typeof candidate.heartbeat_timeout_ms === "number"
    && isNonNegativeNumber(candidate.terrain_cache_bytes)
    && isNonNegativeNumber(candidate.terrain_cache_limit_bytes)
    && isNonNegativeNumber(candidate.marker_cache_bytes)
    && isNonNegativeNumber(candidate.marker_cache_limit_bytes)
    && Array.isArray(candidate.diagnostics)
    && candidate.diagnostics.length <= 32
    && candidate.diagnostics.every((event) => typeof event === "string" && event.length <= 64);
}

function isNonNegativeNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value) && value >= 0;
}

function formatBytes(bytes: number): string {
  if (bytes < 1_048_576) return `${Math.round(bytes / 1_024)} KiB`;
  return `${Math.round(bytes / 1_048_576)} MiB`;
}

function editionLabel(edition: RuntimeStatus["edition"]): string {
  if (edition === "original") return "cTab Original";
  if (edition === "devastator") return "Devastator";
  if (edition === "unsupported") return "Unsupported";
  return "No cTab";
}

function readSessionToken(): string | null {
  const fragment = new URLSearchParams(window.location.hash.slice(1));
  const fromFragment = fragment.get("token");
  if (fromFragment !== null && /^[A-Za-z0-9_-]{32,128}$/.test(fromFragment)) {
    window.sessionStorage.setItem(SESSION_TOKEN_KEY, fromFragment);
    window.history.replaceState(null, "", `${window.location.pathname}${window.location.search}`);
    return fromFragment;
  }
  const stored = window.sessionStorage.getItem(SESSION_TOKEN_KEY);
  return stored !== null && /^[A-Za-z0-9_-]{32,128}$/.test(stored) ? stored : null;
}

function runtimeItem(label: string, value: string): { root: HTMLElement; value: HTMLElement } {
  const root = document.createElement("div");
  root.className = "runtime-item";
  appendText(root, "span", label, "section-label");
  const valueNode = appendText(root, "strong", value, "runtime-value");
  return { root, value: valueNode };
}

function actionButton(label: string): HTMLButtonElement {
  const button = document.createElement("button");
  button.className = "action-button";
  button.type = "button";
  button.textContent = label;
  return button;
}

function toggleButton(label: string, pressed: boolean): HTMLButtonElement {
  const button = actionButton(label);
  button.classList.add("toggle-button");
  button.setAttribute("aria-pressed", String(pressed));
  return button;
}

function bindLayerToggle(button: HTMLButtonElement, layer: TacticalLayer): void {
  button.addEventListener("click", () => {
    const visible = button.getAttribute("aria-pressed") !== "true";
    button.setAttribute("aria-pressed", String(visible));
    tacticalMap?.setLayerVisible(layer, visible);
  });
}

function bindMapViewToggle(
  button: HTMLButtonElement,
  apply: (enabled: boolean) => void
): void {
  button.addEventListener("click", () => {
    const enabled = button.getAttribute("aria-pressed") !== "true";
    button.setAttribute("aria-pressed", String(enabled));
    apply(enabled);
  });
}

function setStatus(message: string, kind: "waiting" | "connected" | "error"): void {
  replaceText(status, message);
  status.className = `status status-${kind}`;
}

function requireElement(id: string): HTMLElement {
  const element = document.getElementById(id);
  if (element === null) {
    throw new Error(`Missing required element: ${id}`);
  }
  return element;
}
