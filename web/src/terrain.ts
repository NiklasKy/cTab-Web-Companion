export interface TerrainMetadata {
  world_name: string;
  catalog_name: string;
  display_name: string;
  attribution: string;
  size_in_meters: number;
  origin_x: number;
  origin_y: number;
  map_id: number;
  layer_id: number;
  min_zoom: number;
  max_zoom: number;
  default_zoom: number;
  tile_size: number;
  factor_x: number;
  factor_y: number;
}

export function parseTerrainMetadata(value: unknown): TerrainMetadata | null {
  if (!isRecord(value)
    || !isText(value.world_name)
    || !isSlug(value.catalog_name)
    || !isText(value.display_name)
    || !isText(value.attribution)
    || !isPositiveFinite(value.size_in_meters)
    || !isFiniteNumber(value.origin_x)
    || !isFiniteNumber(value.origin_y)
    || !isPositiveInteger(value.map_id)
    || !isPositiveInteger(value.layer_id)
    || !isBoundedInteger(value.min_zoom, 0, 12)
    || !isBoundedInteger(value.max_zoom, value.min_zoom, 12)
    || !isBoundedInteger(value.default_zoom, value.min_zoom, value.max_zoom)
    || !isBoundedInteger(value.tile_size, 64, 1_024)
    || !isPositiveFinite(value.factor_x)
    || !isPositiveFinite(value.factor_y)) {
    return null;
  }
  return {
    world_name: value.world_name,
    catalog_name: value.catalog_name,
    display_name: value.display_name,
    attribution: value.attribution,
    size_in_meters: value.size_in_meters,
    origin_x: value.origin_x,
    origin_y: value.origin_y,
    map_id: value.map_id,
    layer_id: value.layer_id,
    min_zoom: value.min_zoom,
    max_zoom: value.max_zoom,
    default_zoom: value.default_zoom,
    tile_size: value.tile_size,
    factor_x: value.factor_x,
    factor_y: value.factor_y
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isText(value: unknown): value is string {
  return typeof value === "string" && value.length > 0 && value.length <= 512 && !value.includes("\0");
}

function isSlug(value: unknown): value is string {
  return typeof value === "string" && /^[a-z0-9_]{1,64}$/.test(value);
}

function isFiniteNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

function isPositiveFinite(value: unknown): value is number {
  return isFiniteNumber(value) && value > 0;
}

function isPositiveInteger(value: unknown): value is number {
  return Number.isInteger(value) && isFiniteNumber(value) && value > 0;
}

function isBoundedInteger(value: unknown, minimum: number, maximum: number): value is number {
  return Number.isInteger(value) && isFiniteNumber(value) && value >= minimum && value <= maximum;
}
