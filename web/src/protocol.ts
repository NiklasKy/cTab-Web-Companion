export const PROTOCOL_VERSION = 1;

export type EntityKind = "player" | "bft_unit" | "bft_vehicle";
export type Side = "west" | "east" | "independent" | "civilian" | "unknown";
export type MarkerKind = "icon" | "rectangle" | "ellipse" | "polyline";
export type CtabEdition = "none" | "original" | "devastator" | "unsupported";

export interface Point2 { x: number; y: number; }

export interface TacticalCapabilities {
  map: boolean;
  own_position: boolean;
  bft: boolean;
}

export interface TacticalEntity {
  id: string;
  label: string;
  kind: EntityKind;
  position: Point2;
  direction: number;
  side: Side;
  color: string;
  icon_path: string;
  overlay_icon_path: string;
}

export interface TacticalMarker {
  id: string;
  label: string;
  kind: MarkerKind;
  position: Point2;
  direction: number;
  color: string;
  alpha: number;
  marker_type: string;
  icon_path: string;
  overlay_icon_path: string;
  brush: string;
  size: Point2;
  polyline: Point2[];
  channel: number;
}

export interface SessionSnapshot {
  mission_name: string;
  ctab_edition: CtabEdition;
  capabilities: TacticalCapabilities;
  terrain: { world_name: string; display_name: string; world_size: number };
  entities: TacticalEntity[];
  markers: TacticalMarker[];
}

export interface PositionUpdate { id: string; position: Point2; direction: number; }
export interface EntityDelta { updated: TacticalEntity[]; removed: string[]; }
export interface PositionDelta { updated: PositionUpdate[]; removed: string[]; }
export interface MarkerDelta { updated: TacticalMarker[]; removed: string[]; }

interface EnvelopeBase { protocol_version: number; session_id: string; sequence: number; }
export type SnapshotEnvelope = EnvelopeBase & { type: "session_snapshot"; payload: SessionSnapshot };
export type EntityDeltaEnvelope = EnvelopeBase & { type: "entity_delta"; payload: EntityDelta };
export type PositionDeltaEnvelope = EnvelopeBase & { type: "position_delta"; payload: PositionDelta };
export type MarkerDeltaEnvelope = EnvelopeBase & { type: "marker_delta"; payload: MarkerDelta };
export type TacticalEnvelope = SnapshotEnvelope | EntityDeltaEnvelope | PositionDeltaEnvelope | MarkerDeltaEnvelope;

const entityKinds = new Set<EntityKind>(["player", "bft_unit", "bft_vehicle"]);
const sides = new Set<Side>(["west", "east", "independent", "civilian", "unknown"]);
const markerKinds = new Set<MarkerKind>(["icon", "rectangle", "ellipse", "polyline"]);
const ctabEditions = new Set<CtabEdition>(["none", "original", "devastator", "unsupported"]);

export function parseTacticalEnvelope(value: unknown): TacticalEnvelope | null {
  if (!isRecord(value)
    || value.protocol_version !== PROTOCOL_VERSION
    || !isRequiredText(value.session_id, 96)
    || !isSequence(value.sequence)
    || !isRecord(value.payload)) {
    return null;
  }
  const base: EnvelopeBase = {
    protocol_version: PROTOCOL_VERSION,
    session_id: value.session_id,
    sequence: value.sequence
  };
  if (value.type === "session_snapshot") {
    const payload = parseSnapshot(value.payload);
    return payload === null ? null : { ...base, type: "session_snapshot", payload };
  }
  if (value.type === "position_delta") {
    const payload = parsePositionDelta(value.payload);
    return payload === null ? null : { ...base, type: "position_delta", payload };
  }
  if (value.type === "entity_delta") {
    const payload = parseEntityDelta(value.payload);
    return payload === null ? null : { ...base, type: "entity_delta", payload };
  }
  if (value.type === "marker_delta") {
    const payload = parseMarkerDelta(value.payload);
    return payload === null ? null : { ...base, type: "marker_delta", payload };
  }
  return null;
}

function parseEntityDelta(payload: Record<string, unknown>): EntityDelta | null {
  if (!Array.isArray(payload.updated) || !Array.isArray(payload.removed)
    || payload.updated.length > 2_048 || payload.removed.length > 2_048) {
    return null;
  }
  const updated = payload.updated.map(parseEntity);
  if (updated.some((item) => item === null) || payload.removed.some((item) => !isRequiredText(item))) {
    return null;
  }
  return {
    updated: updated.filter((item): item is TacticalEntity => item !== null),
    removed: payload.removed as string[]
  };
}

export function parseSnapshotEnvelope(value: unknown): SnapshotEnvelope | null {
  const envelope = parseTacticalEnvelope(value);
  return envelope?.type === "session_snapshot" ? envelope : null;
}

function parseSnapshot(payload: Record<string, unknown>): SessionSnapshot | null {
  if (!isRequiredText(payload.mission_name)
    || !isRecord(payload.terrain)
    || !isRequiredText(payload.terrain.world_name)
    || !isRequiredText(payload.terrain.display_name)
    || !isFiniteNumber(payload.terrain.world_size)
    || payload.terrain.world_size <= 0
    || !Array.isArray(payload.entities)
    || !Array.isArray(payload.markers)
    || payload.entities.length > 2_048
    || payload.markers.length > 4_096) {
    return null;
  }
  const entities = payload.entities.map(parseEntity);
  const markers = payload.markers.map(parseMarker);
  const capabilities = parseCapabilities(payload.capabilities);
  if (entities.some((item) => item === null)
    || markers.some((item) => item === null)
    || capabilities === null) {
    return null;
  }
  return {
    mission_name: payload.mission_name,
    ctab_edition: isCtabEdition(payload.ctab_edition) ? payload.ctab_edition : "none",
    capabilities,
    terrain: {
      world_name: payload.terrain.world_name,
      display_name: payload.terrain.display_name,
      world_size: payload.terrain.world_size
    },
    entities: deduplicateById(entities.filter((item): item is TacticalEntity => item !== null)),
    markers: deduplicateById(markers.filter((item): item is TacticalMarker => item !== null))
  };
}

function parseCapabilities(value: unknown): TacticalCapabilities | null {
  if (value === undefined) {
    return { map: false, own_position: false, bft: false };
  }
  if (!isRecord(value)
    || typeof value.map !== "boolean"
    || typeof value.own_position !== "boolean"
    || typeof value.bft !== "boolean") {
    return null;
  }
  return {
    map: value.map,
    own_position: value.own_position,
    bft: value.bft
  };
}

function deduplicateById<T extends { id: string }>(items: T[]): T[] {
  const indices = new Map<string, number>();
  const unique: T[] = [];
  for (const item of items) {
    const index = indices.get(item.id);
    if (index === undefined) {
      indices.set(item.id, unique.length);
      unique.push(item);
    } else {
      unique[index] = item;
    }
  }
  return unique;
}

function parsePositionDelta(payload: Record<string, unknown>): PositionDelta | null {
  if (!Array.isArray(payload.updated) || !Array.isArray(payload.removed)
    || payload.updated.length > 2_048 || payload.removed.length > 2_048) {
    return null;
  }
  const updated = payload.updated.map(parsePositionUpdate);
  if (updated.some((item) => item === null) || payload.removed.some((item) => !isRequiredText(item))) {
    return null;
  }
  return {
    updated: updated.filter((item): item is PositionUpdate => item !== null),
    removed: payload.removed as string[]
  };
}

function parseMarkerDelta(payload: Record<string, unknown>): MarkerDelta | null {
  if (!Array.isArray(payload.updated) || !Array.isArray(payload.removed)
    || payload.updated.length > 2_048 || payload.removed.length > 2_048) {
    return null;
  }
  const updated = payload.updated.map(parseMarker);
  if (updated.some((item) => item === null) || payload.removed.some((item) => !isRequiredText(item))) {
    return null;
  }
  return {
    updated: updated.filter((item): item is TacticalMarker => item !== null),
    removed: payload.removed as string[]
  };
}

function parseEntity(value: unknown): TacticalEntity | null {
  if (!isRecord(value) || !isRequiredText(value.id) || !isRequiredText(value.label)
    || !isEntityKind(value.kind) || !isPoint(value.position)
    || !isFiniteNumber(value.direction) || !isSide(value.side)
    || !isRequiredText(value.color) || !isOptionalText(value.icon_path)
    || !isOptionalText(value.overlay_icon_path)) {
    return null;
  }
  return { id: value.id, label: value.label, kind: value.kind, position: value.position,
    direction: value.direction, side: value.side, color: value.color,
    icon_path: value.icon_path, overlay_icon_path: value.overlay_icon_path };
}

function parsePositionUpdate(value: unknown): PositionUpdate | null {
  if (!isRecord(value) || !isRequiredText(value.id) || !isPoint(value.position)
    || !isFiniteNumber(value.direction)) {
    return null;
  }
  return { id: value.id, position: value.position, direction: value.direction };
}

function parseMarker(value: unknown): TacticalMarker | null {
  if (!isRecord(value) || !isRequiredText(value.id) || !isOptionalText(value.label)
    || !isMarkerKind(value.kind) || !isPoint(value.position) || !isFiniteNumber(value.direction)
    || !isRequiredText(value.color) || !isFiniteNumber(value.alpha)
    || value.alpha < 0 || value.alpha > 1 || !isOptionalText(value.marker_type)
    || !isOptionalText(value.icon_path) || !isOptionalText(value.overlay_icon_path)
    || !isOptionalText(value.brush)
    || !isSize(value.size) || !Array.isArray(value.polyline)
    || value.polyline.length > 4_096 || value.polyline.some((point) => !isPoint(point))
    || !isChannel(value.channel)) {
    return null;
  }
  return {
    id: value.id, label: value.label, kind: value.kind, position: value.position,
    direction: value.direction, color: value.color, alpha: value.alpha,
    marker_type: value.marker_type, brush: value.brush, size: value.size,
    icon_path: value.icon_path, overlay_icon_path: value.overlay_icon_path,
    polyline: value.polyline as Point2[], channel: value.channel
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
function isRequiredText(value: unknown, maxLength = 512): value is string {
  return typeof value === "string" && value.length > 0 && value.length <= maxLength && !value.includes("\0");
}
function isOptionalText(value: unknown): value is string {
  return typeof value === "string" && value.length <= 512 && !value.includes("\0");
}
function isSequence(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}
function isFiniteNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}
function isChannel(value: unknown): value is number {
  return typeof value === "number" && Number.isInteger(value) && value >= -1 && value <= 15;
}
function isPoint(value: unknown): value is Point2 {
  return isRecord(value) && isFiniteNumber(value.x) && isFiniteNumber(value.y);
}
function isSize(value: unknown): value is Point2 {
  return isPoint(value) && value.x >= 0 && value.y >= 0;
}
function isEntityKind(value: unknown): value is EntityKind {
  return typeof value === "string" && entityKinds.has(value as EntityKind);
}
function isSide(value: unknown): value is Side {
  return typeof value === "string" && sides.has(value as Side);
}
function isMarkerKind(value: unknown): value is MarkerKind {
  return typeof value === "string" && markerKinds.has(value as MarkerKind);
}
function isCtabEdition(value: unknown): value is CtabEdition {
  return typeof value === "string" && ctabEditions.has(value as CtabEdition);
}
