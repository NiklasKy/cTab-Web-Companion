import { describe, expect, it } from "vitest";
import { parseSnapshotEnvelope, parseTacticalEnvelope } from "./protocol";

const fixture = {
  protocol_version: 1,
  session_id: "phase1-session",
  sequence: 1,
  type: "session_snapshot",
  payload: {
    mission_name: "Synthetic",
    ctab_edition: "original",
    capabilities: { map: true, own_position: true, bft: true },
    terrain: { world_name: "Synthetic_Altis", display_name: "Grid", world_size: 30_720 },
    entities: [{
      id: "player",
      label: "You",
      kind: "player",
      position: { x: 100, y: 200 },
      direction: 90,
      side: "west",
      color: "#155a93",
      icon_path: "",
      overlay_icon_path: ""
    }],
    markers: [{
      id: "objective",
      label: "Objective",
      kind: "icon",
      position: { x: 300, y: 400 },
      direction: 0,
      color: "#fff",
      alpha: 1,
      marker_type: "mil_objective",
      icon_path: "\\A3\\ui_f\\data\\map\\markers\\military\\objective_CA.paa",
      overlay_icon_path: "",
      brush: "Solid",
      size: { x: 1, y: 1 },
      polyline: [],
      channel: 0
    }]
  }
};

describe("snapshot parser", () => {
  it("accepts the phase 1 fixture", () => {
    expect(parseSnapshotEnvelope(fixture)?.payload.entities).toHaveLength(1);
  });

  it("rejects an unsupported protocol version", () => {
    expect(parseSnapshotEnvelope({ ...fixture, protocol_version: 99 })).toBeNull();
  });

  it("rejects non-finite coordinates", () => {
    const invalid = structuredClone(fixture);
    invalid.payload.entities[0]!.position.x = Number.NaN;
    expect(parseSnapshotEnvelope(invalid)).toBeNull();
  });

  it("defaults missing capabilities to no access and rejects malformed capability values", () => {
    const legacy = structuredClone(fixture) as unknown as {
      payload: { capabilities?: unknown };
    };
    delete legacy.payload.capabilities;
    expect(parseSnapshotEnvelope(legacy)?.payload.capabilities).toEqual({
      map: false,
      own_position: false,
      bft: false
    });

    const malformed = structuredClone(fixture) as unknown as {
      payload: { capabilities: { map: unknown } };
    };
    malformed.payload.capabilities.map = "yes";
    expect(parseSnapshotEnvelope(malformed)).toBeNull();
  });

  it("canonicalizes duplicate entity and marker identifiers", () => {
    const duplicated = structuredClone(fixture);
    duplicated.payload.entities.push({
      ...duplicated.payload.entities[0]!,
      label: "Updated player",
      position: { x: 900, y: 800 }
    });
    duplicated.payload.markers.push({
      ...duplicated.payload.markers[0]!,
      label: "Updated objective"
    });

    const parsed = parseSnapshotEnvelope(duplicated);
    expect(parsed?.payload.entities).toHaveLength(1);
    expect(parsed?.payload.entities[0]?.label).toBe("Updated player");
    expect(parsed?.payload.markers).toHaveLength(1);
    expect(parsed?.payload.markers[0]?.label).toBe("Updated objective");
  });

  it("accepts empty marker text and parses deltas", () => {
    const markerFixture = structuredClone(fixture);
    markerFixture.payload.markers[0]!.label = "";
    expect(parseSnapshotEnvelope(markerFixture)).not.toBeNull();

    expect(parseTacticalEnvelope({
      protocol_version: 1,
      session_id: "phase1-session",
      sequence: 2,
      type: "position_delta",
      payload: { updated: [{ id: "player", position: { x: 500, y: 600 }, direction: 180 }], removed: [] }
    })?.type).toBe("position_delta");

    expect(parseTacticalEnvelope({
      protocol_version: 1,
      session_id: "phase1-session",
      sequence: 3,
      type: "entity_delta",
      payload: { updated: [fixture.payload.entities[0]], removed: [] }
    })?.type).toBe("entity_delta");
  });
});
