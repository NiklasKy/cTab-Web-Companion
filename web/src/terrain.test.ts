import { describe, expect, it } from "vitest";
import L from "leaflet";
import { createTerrainCrs } from "./map";
import { parseTerrainMetadata, type TerrainMetadata } from "./terrain";

const altis: TerrainMetadata = Object.freeze({
  world_name: "Altis",
  catalog_name: "altis",
  display_name: "Altis",
  attribution: "Copyright Bohemia Interactive",
  size_in_meters: 30_720,
  origin_x: 0,
  origin_y: 0,
  map_id: 3,
  layer_id: 3,
  min_zoom: 0,
  max_zoom: 6,
  default_zoom: 2,
  tile_size: 381,
  factor_x: 0.012375,
  factor_y: 0.012375
});

describe("terrain metadata", () => {
  it("accepts the explicit bounded catalog schema", () => {
    expect(parseTerrainMetadata(altis)).toEqual(altis);
  });

  it("rejects missing attribution and invalid zoom ranges", () => {
    const noAttribution: unknown = {
      world_name: "Altis",
      catalog_name: "altis",
      display_name: "Altis",
      attribution: "",
      size_in_meters: 30_720,
      origin_x: 0,
      origin_y: 0,
      map_id: 3,
      layer_id: 3,
      min_zoom: 0,
      max_zoom: 6,
      default_zoom: 2,
      tile_size: 381,
      factor_x: 0.012375,
      factor_y: 0.012375
    };
    expect(parseTerrainMetadata(noAttribution)).toBeNull();
  });

  it("maps Arma metre coordinates against the catalog tile edge", () => {
    const crs = createTerrainCrs(altis);
    const southWest = crs.latLngToPoint(L.latLng(0, 0), 0);
    const northEast = crs.latLngToPoint(L.latLng(30_720, 30_720), 0);
    expect(southWest.x).toBeCloseTo(0, 5);
    expect(southWest.y).toBeCloseTo(381, 5);
    expect(northEast.x).toBeCloseTo(380.16, 5);
    expect(northEast.y).toBeCloseTo(0.84, 5);
  });

  it("preserves the catalog padding when overzooming Altis", () => {
    const crs = createTerrainCrs(altis);
    const northEast = crs.latLngToPoint(L.latLng(30_720, 30_720), 7);
    expect(northEast.y).toBeCloseTo(107.52, 5);
  });

  it.each([
    ["Kamino", -8_192],
    ["Jabiim", 2_560],
    ["G.O.S N'Djenahoud", 10_240]
  ])("normalizes the %s raster origin to Arma-local coordinates", (_name, originY) => {
    const offsetMetadata: TerrainMetadata = {
      ...altis,
      origin_x: 5_000,
      origin_y: originY
    };
    const crs = createTerrainCrs(offsetMetadata);
    const southWest = crs.latLngToPoint(L.latLng(0, 0), 0);
    const northEast = crs.latLngToPoint(L.latLng(30_720, 30_720), 0);

    expect(southWest.x).toBeCloseTo(0, 5);
    expect(southWest.y).toBeCloseTo(381, 5);
    expect(northEast.x).toBeCloseTo(380.16, 5);
    expect(northEast.y).toBeCloseTo(0.84, 5);
  });
});
