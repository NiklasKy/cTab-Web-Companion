import L from "leaflet";
import {
  buildVisibleGridPositions,
  formatGridCoordinate,
  gridScaleForZoom
} from "./grid";
import { markerBrushFillOpacity, resolveMarkerColor } from "./marker-style";
import type { SessionSnapshot, TacticalEntity, TacticalMarker } from "./protocol";
import { parseTerrainMetadata, type TerrainMetadata } from "./terrain";

export type TerrainStatus =
  | { kind: "loading"; message: string }
  | { kind: "ready"; message: string; attribution: string }
  | { kind: "fallback"; message: string };

export type TacticalLayer = "entities" | "markers" | "grid";

export class TacticalMap {
  readonly #element: HTMLElement;
  readonly #sessionToken: string;
  readonly #onTerrainStatus: (status: TerrainStatus) => void;
  readonly #onFollowPlayerChanged: (following: boolean) => void;
  #map: L.Map;
  #baseLayer = L.layerGroup();
  #gridLayer = L.layerGroup();
  #entities = L.layerGroup();
  #markers = L.layerGroup();
  #entityLayers = new Map<string, L.CircleMarker | L.Marker>();
  #entityDirectionElements = new Map<string, HTMLElement>();
  #entityVisualSignatures = new Map<string, string>();
  #markerLayers = new Map<string, L.Layer>();
  #markerVisualSignatures = new Map<string, string>();
  #lastSnapshot: SessionSnapshot | null = null;
  #terrainKey: string | null = null;
  #renderGeneration = 0;
  #labelCollisionFrame: number | null = null;
  #followingPlayer = false;
  #layerVisibility: Record<TacticalLayer, boolean> = {
    entities: true,
    markers: true,
    grid: true
  };

  public constructor(
    element: HTMLElement,
    sessionToken: string,
    onTerrainStatus: (status: TerrainStatus) => void,
    onFollowPlayerChanged: (following: boolean) => void = () => undefined
  ) {
    this.#element = element;
    this.#sessionToken = sessionToken;
    this.#onTerrainStatus = onTerrainStatus;
    this.#onFollowPlayerChanged = onFollowPlayerChanged;
    this.#element.classList.add("map-muted");
    this.#map = this.#createMap(L.CRS.Simple, -5, 4);
    this.#element.addEventListener("wheel", () => this.#setFollowingPlayer(false), { passive: true });
    this.#element.addEventListener("keydown", (event) => {
      if (["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "+", "-"].includes(event.key)) {
        this.#setFollowingPlayer(false);
      }
    });
    this.#element.addEventListener("click", (event) => {
      const target = event.target;
      if (target instanceof Element && target.closest(".leaflet-control-zoom") !== null) {
        this.#setFollowingPlayer(false);
      }
    });
  }

  public render(snapshot: SessionSnapshot): void {
    this.#lastSnapshot = snapshot;
    if (this.#followingPlayer && findPlayer(snapshot) === undefined) {
      this.#setFollowingPlayer(false);
    }
    const accessMode = snapshot.capabilities.map
      ? "map"
      : snapshot.capabilities.own_position ? "gps" : "locked";
    const terrainKey = `${snapshot.terrain.world_name}:${snapshot.terrain.world_size}:${accessMode}`;
    if (terrainKey !== this.#terrainKey) {
      this.#terrainKey = terrainKey;
      const generation = ++this.#renderGeneration;
      if (!snapshot.capabilities.map) {
        this.#renderRestricted(snapshot);
        this.#onTerrainStatus({
          kind: "fallback",
          message: snapshot.capabilities.own_position
            ? "GPS available - terrain map requires a map item"
            : "No map or navigation device - tactical display locked"
        });
        return;
      }
      this.#renderFallback(snapshot, true);
      this.#onTerrainStatus({ kind: "loading", message: "Loading terrain map" });
      this.#loadTerrain(snapshot, generation).catch(() => {
        if (generation === this.#renderGeneration) {
          this.#renderFallback(this.#lastSnapshot ?? snapshot, false);
          this.#onTerrainStatus({
            kind: "fallback",
            message: "Terrain map unavailable - grid fallback active"
          });
        }
      });
      return;
    }
    this.updateEntities(snapshot);
    this.updateMarkers(snapshot);
  }

  public updatePositions(snapshot: SessionSnapshot): void {
    this.#lastSnapshot = snapshot;
    const currentIds = new Set(snapshot.entities.map((entity) => entity.id));
    for (const [id, layer] of this.#entityLayers) {
      if (!currentIds.has(id)) {
        this.#entities.removeLayer(layer);
        this.#entityLayers.delete(id);
        this.#entityDirectionElements.delete(id);
      }
    }
    for (const entity of snapshot.entities) {
      const layer = this.#entityLayers.get(entity.id);
      if (layer === undefined) {
        this.#renderEntity(entity);
      } else {
        layer.setLatLng([entity.position.y, entity.position.x]);
        const directionElement = this.#entityDirectionElements.get(entity.id);
        if (directionElement !== undefined) {
          setOwnPositionDirection(directionElement, entity.direction);
        }
      }
    }
    this.#followPlayerInSnapshot(snapshot);
    this.#scheduleLabelCollisionResolution();
  }

  public updateEntities(snapshot: SessionSnapshot): void {
    this.#lastSnapshot = snapshot;
    const currentIds = new Set(snapshot.entities.map((entity) => entity.id));
    for (const [id, layer] of this.#entityLayers) {
      if (!currentIds.has(id)) {
        this.#entities.removeLayer(layer);
        this.#entityLayers.delete(id);
        this.#entityDirectionElements.delete(id);
        this.#entityVisualSignatures.delete(id);
      }
    }
    for (const entity of snapshot.entities) {
      const signature = entityVisualSignature(entity);
      const existing = this.#entityLayers.get(entity.id);
      if (existing === undefined || this.#entityVisualSignatures.get(entity.id) !== signature) {
        if (existing !== undefined) {
          this.#entities.removeLayer(existing);
          this.#entityLayers.delete(entity.id);
          this.#entityDirectionElements.delete(entity.id);
        }
        this.#renderEntity(entity);
      } else {
        existing.setLatLng([entity.position.y, entity.position.x]);
        const directionElement = this.#entityDirectionElements.get(entity.id);
        if (directionElement !== undefined) {
          setOwnPositionDirection(directionElement, entity.direction);
        }
      }
    }
    this.#followPlayerInSnapshot(snapshot);
    this.#scheduleLabelCollisionResolution();
  }

  public updateMarkers(snapshot: SessionSnapshot): void {
    this.#lastSnapshot = snapshot;
    const currentIds = new Set(snapshot.markers.map((marker) => marker.id));
    for (const [id, layer] of this.#markerLayers) {
      if (!currentIds.has(id)) {
        this.#markers.removeLayer(layer);
        this.#markerLayers.delete(id);
        this.#markerVisualSignatures.delete(id);
      }
    }
    for (const marker of snapshot.markers) {
      const signature = markerVisualSignature(marker);
      const existing = this.#markerLayers.get(marker.id);
      if (existing === undefined || this.#markerVisualSignatures.get(marker.id) !== signature) {
        if (existing !== undefined) {
          this.#markers.removeLayer(existing);
          this.#markerLayers.delete(marker.id);
        }
        this.#renderMarker(marker);
      }
    }
    this.#scheduleLabelCollisionResolution();
  }

  public focusPlayer(): void {
    const player = this.#lastSnapshot === null ? undefined : findPlayer(this.#lastSnapshot);
    if (player !== undefined) {
      this.#map.panTo([player.position.y, player.position.x], { animate: true });
    }
  }

  public setFollowPlayer(following: boolean): boolean {
    if (following && (this.#lastSnapshot === null || findPlayer(this.#lastSnapshot) === undefined)) {
      this.#setFollowingPlayer(false);
      return false;
    }
    this.#setFollowingPlayer(following);
    if (this.#followingPlayer && this.#lastSnapshot !== null) {
      this.#followPlayerInSnapshot(this.#lastSnapshot);
    }
    return this.#followingPlayer;
  }

  public isFollowingPlayer(): boolean {
    return this.#followingPlayer;
  }

  public fitTerrain(): void {
    this.#setFollowingPlayer(false);
    const size = this.#lastSnapshot?.terrain.world_size;
    if (size !== undefined) {
      this.#map.fitBounds([[0, 0], [size, size]], { animate: true, padding: [18, 18] });
    }
  }

  public invalidateSize(): void {
    this.#map.invalidateSize({ animate: false });
  }

  public setLayerVisible(layer: TacticalLayer, visible: boolean): void {
    this.#layerVisibility[layer] = visible;
    const target = layer === "entities"
      ? this.#entities
      : layer === "markers" ? this.#markers : this.#gridLayer;
    if (visible) {
      if (!this.#map.hasLayer(target)) {
        target.addTo(this.#map);
      }
    } else if (this.#map.hasLayer(target)) {
      this.#map.removeLayer(target);
    }
    this.#scheduleLabelCollisionResolution();
  }

  public setBftNamesVisible(visible: boolean): void {
    this.#element.classList.toggle("hide-bft-labels", !visible);
    this.#scheduleLabelCollisionResolution();
  }

  public setMapMuted(muted: boolean): void {
    this.#element.classList.toggle("map-muted", muted);
  }

  async #loadTerrain(snapshot: SessionSnapshot, generation: number): Promise<void> {
    const worldName = encodeURIComponent(snapshot.terrain.world_name);
    const token = encodeURIComponent(this.#sessionToken);
    const response = await fetch(`/terrain/${token}/${worldName}/metadata`, {
      credentials: "same-origin",
      redirect: "error",
      cache: "no-store"
    });
    if (!response.ok) {
      throw new Error("terrain metadata request failed");
    }
    const contentType = response.headers.get("content-type")?.split(";", 1)[0]?.trim();
    if (contentType !== "application/json") {
      throw new Error("terrain metadata content type rejected");
    }
    const raw: unknown = await response.json();
    const metadata = parseTerrainMetadata(raw);
    if (metadata === null
      || metadata.world_name.toLowerCase() !== snapshot.terrain.world_name.toLowerCase()
      || Math.abs(metadata.size_in_meters - snapshot.terrain.world_size) > 0.01
      || generation !== this.#renderGeneration) {
      throw new Error("terrain metadata rejected");
    }
    this.#renderCatalogTerrain(this.#lastSnapshot ?? snapshot, metadata);
    this.#onTerrainStatus({
      kind: "ready",
      message: `${metadata.display_name} - map cached locally on demand`,
      attribution: metadata.attribution
    });
  }

  #renderCatalogTerrain(snapshot: SessionSnapshot, metadata: TerrainMetadata): void {
    const displayMaxZoom = catalogDisplayMaxZoom(metadata.max_zoom);
    this.#replaceMap(createTerrainCrs(metadata), metadata.min_zoom, displayMaxZoom);
    const size = metadata.size_in_meters;
    const bounds = L.latLngBounds([0, 0], [size, size]);
    this.#map.setMaxBounds(bounds.pad(0.08));
    const token = encodeURIComponent(this.#sessionToken);
    const worldName = encodeURIComponent(snapshot.terrain.world_name);
    L.tileLayer(`/terrain/${token}/${worldName}/{z}/{x}/{y}`, {
      bounds,
      minZoom: metadata.min_zoom,
      maxZoom: displayMaxZoom,
      minNativeZoom: metadata.min_zoom,
      maxNativeZoom: metadata.max_zoom,
      tileSize: metadata.tile_size,
      noWrap: true,
      keepBuffer: 2,
      updateWhenIdle: true,
      errorTileUrl: emptyTileDataUrl()
    }).addTo(this.#baseLayer);
    this.#renderOverlays(snapshot);
    this.#focusInitialView(snapshot, Math.min(metadata.max_zoom, metadata.default_zoom + 1));
    this.#attachDynamicGrid(size);
  }

  #renderFallback(snapshot: SessionSnapshot, fitWorld: boolean): void {
    this.#replaceMap(L.CRS.Simple, -5, 4);
    const size = snapshot.terrain.world_size;
    const bounds = L.latLngBounds([0, 0], [size, size]);
    this.#map.setMaxBounds(bounds.pad(0.12));
    L.rectangle(bounds, {
      color: "#71877b",
      fillColor: "#17231f",
      fillOpacity: 1,
      opacity: 0.72,
      weight: 2,
      interactive: false
    }).addTo(this.#baseLayer);
    this.#renderOverlays(snapshot);
    if (fitWorld) {
      this.#map.fitBounds(bounds, { animate: false, padding: [12, 12] });
    } else {
      this.#focusInitialView(snapshot, 3);
    }
    this.#attachDynamicGrid(size);
  }

  #renderRestricted(snapshot: SessionSnapshot): void {
    this.#replaceMap(L.CRS.Simple, -5, 4);
    const size = snapshot.terrain.world_size;
    const bounds = L.latLngBounds([0, 0], [size, size]);
    this.#map.setMaxBounds(bounds.pad(0.12));
    L.rectangle(bounds, {
      color: "#25322d",
      fillColor: "#0d1512",
      fillOpacity: 1,
      opacity: 0.5,
      weight: 1,
      interactive: false
    }).addTo(this.#baseLayer);
    this.#renderOverlays(snapshot);
    this.#focusInitialView(snapshot, snapshot.capabilities.own_position ? 3 : -1);
    if (snapshot.capabilities.own_position) {
      this.#attachDynamicGrid(size);
    }
  }

  #replaceMap(crs: L.CRS, minZoom: number, maxZoom: number): void {
    this.#map.remove();
    this.#baseLayer = L.layerGroup();
    this.#gridLayer = L.layerGroup();
    this.#entities = L.layerGroup();
    this.#markers = L.layerGroup();
    this.#entityLayers.clear();
    this.#entityDirectionElements.clear();
    this.#entityVisualSignatures.clear();
    this.#markerLayers.clear();
    this.#markerVisualSignatures.clear();
    this.#map = this.#createMap(crs, minZoom, maxZoom);
  }

  #createMap(crs: L.CRS, minZoom: number, maxZoom: number): L.Map {
    const map = L.map(this.#element, {
      crs,
      attributionControl: false,
      zoomControl: true,
      zoomSnap: 1,
      minZoom,
      maxZoom
    });
    this.#baseLayer.addTo(map);
    if (this.#layerVisibility.grid) this.#gridLayer.addTo(map);
    if (this.#layerVisibility.entities) this.#entities.addTo(map);
    if (this.#layerVisibility.markers) this.#markers.addTo(map);
    map.on("moveend zoomend", () => this.#scheduleLabelCollisionResolution());
    map.on("dragstart boxzoomstart", () => this.#setFollowingPlayer(false));
    return map;
  }

  #setFollowingPlayer(following: boolean): void {
    if (this.#followingPlayer === following) {
      return;
    }
    this.#followingPlayer = following;
    this.#onFollowPlayerChanged(following);
  }

  #followPlayerInSnapshot(snapshot: SessionSnapshot): void {
    if (!this.#followingPlayer) {
      return;
    }
    const player = findPlayer(snapshot);
    if (player === undefined) {
      this.#setFollowingPlayer(false);
      return;
    }
    this.#map.panTo([player.position.y, player.position.x], { animate: false });
  }

  #attachDynamicGrid(size: number): void {
    const redraw = (): void => this.#renderGrid(size);
    this.#map.on("moveend zoomend", redraw);
    redraw();
  }

  #renderGrid(size: number): void {
    this.#gridLayer.clearLayers();
    const bounds = this.#map.getBounds();
    if (!bounds.isValid()) {
      return;
    }
    const scale = gridScaleForZoom(this.#map.getZoom());
    const west = Math.max(0, bounds.getWest());
    const east = Math.min(size, bounds.getEast());
    const south = Math.max(0, bounds.getSouth());
    const north = Math.min(size, bounds.getNorth());
    const eastings = buildVisibleGridPositions(west, east, size, scale.spacing);
    const northings = buildVisibleGridPositions(south, north, size, scale.spacing);
    const majorDistance = scale.spacing * scale.majorEvery;

    for (const position of eastings) {
      this.#addGridLine([[south, position], [north, position]], position % majorDistance === 0);
      if (position > 0 && position < size) {
        this.#addGridLabel([north, position], formatGridCoordinate(position, scale), "grid-label-easting");
      }
    }
    for (const position of northings) {
      this.#addGridLine([[position, west], [position, east]], position % majorDistance === 0);
      if (position > 0 && position < size) {
        this.#addGridLabel([position, west], formatGridCoordinate(position, scale), "grid-label-northing");
      }
    }
  }

  #addGridLine(points: L.LatLngExpression[], major: boolean): void {
    L.polyline(points, {
      color: major ? "#9aabb4" : "#82939c",
      opacity: major ? 0.34 : 0.2,
      weight: major ? 1.2 : 0.75,
      interactive: false
    }).addTo(this.#gridLayer);
  }

  #addGridLabel(position: L.LatLngExpression, value: string, className: string): void {
    const label = document.createElement("span");
    label.textContent = value;
    L.tooltip({
      permanent: true,
      direction: "center",
      className: `grid-label ${className}`,
      interactive: false,
      opacity: 0.88
    }).setLatLng(position).setContent(label).addTo(this.#gridLayer);
  }

  #focusInitialView(snapshot: SessionSnapshot, zoom: number): void {
    const player = snapshot.entities.find((entity) => entity.kind === "player");
    const center: L.LatLngExpression = player === undefined
      ? [snapshot.terrain.world_size / 2, snapshot.terrain.world_size / 2]
      : [player.position.y, player.position.x];
    this.#map.setView(center, zoom, { animate: false });
  }

  #renderOverlays(snapshot: SessionSnapshot): void {
    this.#entities.clearLayers();
    this.#markers.clearLayers();
    this.#entityLayers.clear();
    this.#entityDirectionElements.clear();
    this.#entityVisualSignatures.clear();
    this.#markerLayers.clear();
    this.#markerVisualSignatures.clear();
    for (const entity of snapshot.entities) {
      this.#renderEntity(entity);
    }
    for (const marker of snapshot.markers) {
      this.#renderMarker(marker);
    }
    this.#scheduleLabelCollisionResolution();
  }

  #renderEntity(entity: TacticalEntity): void {
    let layer: L.CircleMarker | L.Marker;
    if (entity.kind === "player") {
      const icon = createOwnPositionIcon(entity.direction);
      layer = L.marker([entity.position.y, entity.position.x], {
        icon,
        interactive: false,
        keyboard: false,
        zIndexOffset: 1_200
      }).addTo(this.#entities);
      const root = icon.options.html;
      if (root instanceof HTMLElement) {
        const directionElement = root.querySelector<HTMLElement>(".own-position-heading");
        if (directionElement !== null) {
          this.#entityDirectionElements.set(entity.id, directionElement);
        }
      }
    } else {
      const icon = createCtabEntityIcon(entity, this.#sessionToken);
      const category = entityDisplayCategory(entity);
      layer = L.marker([entity.position.y, entity.position.x], {
        icon,
        interactive: false,
        keyboard: false,
        zIndexOffset: category === "group" ? 1_000 : category === "vehicle" ? 900 : 500
      }).addTo(this.#entities);
      const root = icon.options.html;
      if (root instanceof HTMLElement) {
        const directionElement = root.querySelector<HTMLElement>(".ctab-bft-primary");
        if (directionElement !== null) {
          this.#entityDirectionElements.set(entity.id, directionElement);
        }
      }
    }
    this.#entityLayers.set(entity.id, layer);
    this.#entityVisualSignatures.set(entity.id, entityVisualSignature(entity));
  }

  #renderMarker(marker: TacticalMarker): void {
    const color = resolveMarkerColor(marker.color);
    const pathOptions: L.PathOptions = {
      color,
      fillColor: color,
      fillOpacity: markerBrushFillOpacity(marker.brush, marker.alpha),
      opacity: marker.alpha,
      weight: 2,
      interactive: false
    };
    let layer: L.Layer;
    if (marker.kind === "rectangle") {
      layer = L.polygon(rectanglePoints(marker), pathOptions);
    } else if (marker.kind === "ellipse") {
      layer = L.polygon(ellipsePoints(marker), pathOptions);
    } else if (marker.kind === "polyline" && marker.polyline.length >= 2) {
      layer = L.polyline(marker.polyline.map((point): L.LatLngTuple => [point.y, point.x]), pathOptions);
    } else if (marker.kind === "icon") {
      const icon = createArmaMarkerIcon(marker, this.#sessionToken, color);
      layer = L.marker([marker.position.y, marker.position.x], {
        icon,
        interactive: false,
        keyboard: false,
        zIndexOffset: marker.marker_type.startsWith("ctab_user_") ? 700 : 600
      });
    } else {
      layer = L.circleMarker([marker.position.y, marker.position.x], {
        ...pathOptions,
        radius: Math.max(5, Math.min(12, 6 + marker.size.x)) ,
        fillOpacity: Math.min(marker.alpha, 0.65)
      });
    }
    layer.addTo(this.#markers);
    this.#markerLayers.set(marker.id, layer);
    this.#markerVisualSignatures.set(marker.id, markerVisualSignature(marker));
    if (marker.label.length > 0) {
      const label = document.createElement("span");
      label.textContent = marker.label;
      const priority = markerLabelPriority(marker);
      label.dataset.tacticalLabelPriority = String(priority);
      label.dataset.tacticalLabelKey = marker.id;
      layer.bindTooltip(label, {
        permanent: true,
        direction: "right",
        offset: [10, 0],
        className: "arma-marker-label"
      });
    }
  }

  #scheduleLabelCollisionResolution(): void {
    if (this.#labelCollisionFrame !== null) {
      return;
    }
    this.#labelCollisionFrame = window.requestAnimationFrame(() => {
      this.#labelCollisionFrame = null;
      resolveTacticalLabelCollisions(
        this.#element,
        labelCollisionThresholdForZoom(this.#map.getZoom())
      );
    });
  }
}

type EntityDisplayCategory = "group" | "vehicle" | "member";

function findPlayer(snapshot: SessionSnapshot): TacticalEntity | undefined {
  return snapshot.entities.find((entity) => entity.kind === "player");
}

export function entityDisplayCategory(entity: TacticalEntity): EntityDisplayCategory {
  if (entity.kind === "bft_vehicle") {
    return "vehicle";
  }
  return entity.id.includes("-group:") ? "group" : "member";
}

export function markerLabelPriority(marker: TacticalMarker): number {
  if (marker.marker_type.startsWith("ctab_user_")) {
    return 300;
  }
  return marker.kind === "icon" ? 200 : 150;
}

export function labelCollisionThresholdForZoom(zoom: number): number {
  if (zoom <= 2) {
    return 0.62;
  }
  if (zoom <= 4) {
    return 0.42;
  }
  return 0.18;
}

export function resolveTacticalLabelCollisions(
  root: HTMLElement,
  overlapThreshold = 0.18
): void {
  const safeThreshold = Number.isFinite(overlapThreshold)
    ? Math.min(1, Math.max(0, overlapThreshold))
    : 0.18;
  const sources = Array.from(root.querySelectorAll<HTMLElement>("[data-tactical-label-priority]"));
  const candidates = sources.map((source) => {
    const visual = source.closest<HTMLElement>(".leaflet-tooltip") ?? source;
    visual.classList.remove("tactical-label-collision-hidden");
    return {
      visual,
      priority: Number(source.dataset.tacticalLabelPriority ?? 0),
      key: source.dataset.tacticalLabelKey ?? ""
    };
  });
  candidates.sort((left, right) => right.priority - left.priority || left.key.localeCompare(right.key));

  const occupied: DOMRect[] = [];
  for (const candidate of candidates) {
    const bounds = candidate.visual.getBoundingClientRect();
    if (bounds.width <= 0 || bounds.height <= 0) {
      continue;
    }
    const overlaps = occupied.some((other) => labelsConflict(bounds, other, safeThreshold));
    if (overlaps) {
      candidate.visual.classList.add("tactical-label-collision-hidden");
    } else {
      occupied.push(bounds);
    }
  }
}

function labelsConflict(left: DOMRect, right: DOMRect, overlapThreshold: number): boolean {
  const overlapWidth = Math.max(0, Math.min(left.right, right.right) - Math.max(left.left, right.left));
  const overlapHeight = Math.max(0, Math.min(left.bottom, right.bottom) - Math.max(left.top, right.top));
  if (overlapWidth === 0 || overlapHeight === 0) {
    return false;
  }
  const overlapArea = overlapWidth * overlapHeight;
  const smallerArea = Math.min(left.width * left.height, right.width * right.height);
  return smallerArea > 0 && overlapArea / smallerArea >= overlapThreshold;
}

export function createCtabEntityIcon(entity: TacticalEntity, sessionToken: string): L.DivIcon {
  const root = document.createElement("span");
  const category = entityDisplayCategory(entity);
  root.className = `ctab-bft-marker ctab-bft-marker-${entity.kind} ctab-bft-marker-${category}`;
  root.style.setProperty("--ctab-bft-color", resolveMarkerColor(entity.color));

  const primary = createCtabIconLayer(entity, sessionToken, "primary", entity.icon_path);
  primary.classList.add("ctab-bft-primary");
  setOwnPositionDirection(primary, entity.direction);
  root.append(primary);

  if (entity.overlay_icon_path.length > 0) {
    const overlay = createCtabIconLayer(entity, sessionToken, "overlay", entity.overlay_icon_path);
    overlay.classList.add("ctab-bft-overlay");
    root.append(overlay);
  }

  const label = document.createElement("span");
  label.className = "ctab-bft-label";
  label.textContent = entity.label;
  label.dataset.tacticalLabelPriority = String(
    category === "group" ? 400 : category === "vehicle" ? 350 : 100
  );
  label.dataset.tacticalLabelKey = entity.id;
  root.append(label);

  return L.divIcon({
    html: root,
    className: "ctab-bft-marker-host",
    iconSize: [240, 48],
    iconAnchor: [24, 24]
  });
}

function createCtabIconLayer(
  entity: TacticalEntity,
  sessionToken: string,
  slot: "primary" | "overlay",
  iconPath: string
): HTMLElement {
  const layer = document.createElement("span");
  layer.className = `ctab-bft-layer ctab-bft-layer-${slot}`;

  const fallback = document.createElement("span");
  fallback.className = "ctab-bft-fallback";
  layer.append(fallback);

  const iconUrl = ctabEntityIconUrl(sessionToken, entity.id, slot, iconPath);
  if (iconUrl === null) {
    return layer;
  }

  const mask = document.createElement("span");
  mask.className = "ctab-bft-mask";
  mask.style.maskImage = `url("${iconUrl}")`;
  mask.style.webkitMaskImage = `url("${iconUrl}")`;
  layer.append(mask);

  const texture = document.createElement("img");
  texture.className = "ctab-bft-texture";
  texture.alt = "";
  texture.decoding = "async";
  texture.draggable = false;
  texture.addEventListener("load", () => layer.classList.add("ctab-bft-layer-ready"), { once: true });
  texture.addEventListener("error", () => texture.remove(), { once: true });
  texture.src = iconUrl;
  layer.append(texture);
  return layer;
}

function ctabEntityIconUrl(
  sessionToken: string,
  entityId: string,
  slot: "primary" | "overlay",
  iconPath: string
): string | null {
  if (!/^[a-f0-9]{64}$/i.test(sessionToken)
    || entityId.length === 0 || entityId.length > 512
    || iconPath.length === 0 || iconPath.length > 512) {
    return null;
  }
  return `/entity-icon/${encodeURIComponent(sessionToken)}/${slot}/${encodeURIComponent(entityId)}?v=${iconPathVersion(iconPath)}`;
}

function iconPathVersion(iconPath: string): string {
  let hash = 2_166_136_261;
  for (let index = 0; index < iconPath.length; index += 1) {
    hash ^= iconPath.charCodeAt(index);
    hash = Math.imul(hash, 16_777_619);
  }
  return (hash >>> 0).toString(16).padStart(8, "0");
}

export function createOwnPositionIcon(direction: number): L.DivIcon {
  void direction;
  const root = document.createElement("span");
  root.className = "own-position-marker";
  root.setAttribute("aria-hidden", "true");
  const core = document.createElement("span");
  core.className = "own-position-core";
  root.append(core);

  return L.divIcon({
    html: root,
    className: "own-position-marker-host",
    iconSize: [36, 36],
    iconAnchor: [18, 18]
  });
}

export function setOwnPositionDirection(element: HTMLElement, direction: number): void {
  element.style.transform = `rotate(${normalizeDirection(direction)}deg)`;
}

export function createArmaMarkerIcon(
  marker: TacticalMarker,
  sessionToken: string,
  color = resolveMarkerColor(marker.color)
): L.DivIcon {
  const width = markerPixelSize(marker.size.x);
  const height = markerPixelSize(marker.size.y);
  const root = document.createElement("span");
  root.className = "arma-marker-icon";
  root.style.width = `${width}px`;
  root.style.height = `${height}px`;
  root.style.opacity = String(marker.alpha);
  root.style.setProperty("--arma-marker-color", color);

  const symbol = document.createElement("span");
  symbol.className = "arma-marker-symbol";
  const isCtabUserMarker = marker.marker_type.startsWith("ctab_user_");
  symbol.style.transform = `rotate(${isCtabUserMarker ? 0 : normalizeDirection(marker.direction)}deg)`;

  const fallback = document.createElement("span");
  fallback.className = "arma-marker-fallback";
  const fallbackLabel = ctabMarkerFallbackLabel(marker.marker_type);
  if (fallbackLabel !== null) {
    fallback.classList.add("ctab-marker-fallback");
    fallback.textContent = fallbackLabel;
  } else if (marker.marker_type.toLowerCase() === "loc_attack") {
    fallback.classList.add("arma-marker-fallback-crossed-swords");
    fallback.textContent = "⚔";
  }
  symbol.append(fallback);

  const iconUrl = markerIconUrl(sessionToken, marker);
  if (iconUrl !== null) {
    const mask = document.createElement("span");
    mask.className = "arma-marker-mask";
    mask.style.maskImage = `url("${iconUrl}")`;
    mask.style.webkitMaskImage = `url("${iconUrl}")`;
    symbol.append(mask);

    const texture = document.createElement("img");
    texture.className = "arma-marker-texture";
    texture.alt = "";
    texture.decoding = "async";
    texture.draggable = false;
    texture.addEventListener("load", () => root.classList.add("arma-marker-icon-ready"), {
      once: true
    });
    texture.addEventListener("error", () => texture.remove(), { once: true });
    texture.src = iconUrl;
    symbol.append(texture);
  }
  root.append(symbol);

  if (marker.overlay_icon_path.length > 0 && iconUrl !== null) {
    const overlayUrl = markerIconUrl(sessionToken, marker, true);
    if (overlayUrl !== null) {
      const overlay = document.createElement("span");
      overlay.className = "arma-marker-overlay";
      const overlayMask = document.createElement("span");
      overlayMask.className = "arma-marker-overlay-mask";
      overlayMask.style.maskImage = `url("${overlayUrl}")`;
      overlayMask.style.webkitMaskImage = `url("${overlayUrl}")`;
      const overlayTexture = document.createElement("img");
      overlayTexture.className = "arma-marker-overlay-texture";
      overlayTexture.alt = "";
      overlayTexture.decoding = "async";
      overlayTexture.draggable = false;
      overlayTexture.addEventListener("load", () => overlay.classList.add("arma-marker-overlay-ready"), {
        once: true
      });
      overlayTexture.addEventListener("error", () => overlayTexture.remove(), { once: true });
      overlayTexture.src = overlayUrl;
      overlay.append(overlayMask, overlayTexture);
      root.append(overlay);
    }
  }

  if (isCtabUserMarker && marker.polyline.length >= 2) {
    const arrow = document.createElement("span");
    arrow.className = "ctab-user-direction";
    arrow.style.transform = `rotate(${normalizeDirection(marker.direction)}deg)`;
    root.append(arrow);
  }

  return L.divIcon({
    html: root,
    className: "arma-marker-icon-host",
    iconSize: [width, height],
    iconAnchor: [width / 2, height / 2]
  });
}

function markerIconUrl(
  sessionToken: string,
  marker: TacticalMarker,
  overlay = false
): string | null {
  const iconPath = overlay ? marker.overlay_icon_path : marker.icon_path;
  if (!/^[a-f0-9]{64}$/i.test(sessionToken)
    || !/^[A-Za-z0-9_]{1,128}$/.test(marker.marker_type)
    || !isSupportedMarkerIconPath(iconPath)) {
    return null;
  }
  const suffix = overlay ? "/overlay" : "";
  return `/marker-icon/${encodeURIComponent(sessionToken)}/${encodeURIComponent(marker.marker_type)}${suffix}?v=${iconPathVersion(iconPath)}`;
}

function isSupportedMarkerIconPath(value: string): boolean {
  if (/^\\?A3\\/i.test(value)) {
    return true;
  }
  if (!value.startsWith("[") || value.length > 512) {
    return false;
  }
  try {
    const descriptor: unknown = JSON.parse(value);
    return Array.isArray(descriptor)
      && descriptor.length === 5
      && descriptor.every((part) => typeof part === "string")
      && descriptor[0] === "ctab_mod_icon_v1";
  } catch {
    return false;
  }
}

function ctabMarkerFallbackLabel(markerType: string): string | null {
  const labels: ReadonlyArray<readonly [string, string]> = [
    ["_opfor_rifle_", "RFL"],
    ["_opfor_machine_gun_", "MG"],
    ["_opfor_anti_tank_", "AT"],
    ["_opfor_medium_machine_gun_", "MMG"],
    ["_opfor_medium_anti_tank_", "MAT"],
    ["_opfor_medium_mortar_", "MTR"],
    ["_opfor_anti_air_", "AA"],
    ["_checkpoint_", "CKP"],
    ["_start_point_", "SP"],
    ["_assembly_area_", "AA"],
    ["_release_point_", "RP"]
  ];
  const match = labels.find(([identifier]) => markerType.includes(identifier));
  return match?.[1] ?? null;
}

function markerPixelSize(scale: number): number {
  return Math.round(32 * Math.max(0.5, Math.min(3, scale)));
}

function normalizeDirection(direction: number): number {
  return ((direction % 360) + 360) % 360;
}

export function createTerrainCrs(metadata: TerrainMetadata): L.CRS {
  const factorX = metadata.factor_x;
  const factorY = metadata.factor_y;
  const crs = Object.create(L.CRS.Simple) as L.CRS & { transformation: L.Transformation };
  // PlanOps raster origins describe the source image placement. Arma getPosWorld
  // always uses terrain-local metre coordinates, so both layers must start at 0,0.
  crs.transformation = new L.Transformation(
    factorX,
    0,
    -factorY,
    metadata.tile_size
  );
  return crs;
}

export function catalogDisplayMaxZoom(nativeMaxZoom: number): number {
  return Math.min(24, nativeMaxZoom + 3);
}

function emptyTileDataUrl(): string {
  return "data:image/gif;base64,R0lGODlhAQABAAD/ACwAAAAAAQABAAACADs=";
}

function entityVisualSignature(entity: TacticalEntity): string {
  return [
    entity.kind,
    entity.label,
    entity.side,
    entity.color,
    entity.icon_path,
    entity.overlay_icon_path
  ].join("\u0000");
}

function markerVisualSignature(marker: TacticalMarker): string {
  return JSON.stringify(marker);
}

function rectanglePoints(marker: TacticalMarker): L.LatLngExpression[] {
  const corners = [
    [-marker.size.x, -marker.size.y], [marker.size.x, -marker.size.y],
    [marker.size.x, marker.size.y], [-marker.size.x, marker.size.y]
  ];
  return corners.map(([x, y]) => rotatedPoint(marker, x ?? 0, y ?? 0));
}

function ellipsePoints(marker: TacticalMarker): L.LatLngExpression[] {
  const points: L.LatLngExpression[] = [];
  for (let step = 0; step < 48; step += 1) {
    const angle = step / 48 * Math.PI * 2;
    points.push(rotatedPoint(marker, marker.size.x * Math.cos(angle), marker.size.y * Math.sin(angle)));
  }
  return points;
}

function rotatedPoint(marker: TacticalMarker, east: number, north: number): L.LatLngExpression {
  const radians = marker.direction * Math.PI / 180;
  const rotatedEast = east * Math.cos(radians) + north * Math.sin(radians);
  const rotatedNorth = -east * Math.sin(radians) + north * Math.cos(radians);
  return [marker.position.y + rotatedNorth, marker.position.x + rotatedEast];
}
