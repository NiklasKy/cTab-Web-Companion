import { describe, expect, it } from "vitest";
import { applyEnvelope } from "./live-state";
import type { SnapshotEnvelope } from "./protocol";

const snapshot: SnapshotEnvelope = {
  protocol_version: 1,
  session_id: "session-a",
  sequence: 1,
  type: "session_snapshot",
  payload: {
    mission_name: "Live test",
    ctab_edition: "original",
    capabilities: { map: true, own_position: true, bft: true },
    terrain: { world_name: "Altis", display_name: "Altis", world_size: 30_720 },
    entities: [{
      id: "player-local", label: "You", kind: "player",
      position: { x: 100, y: 200 }, direction: 0, side: "west",
      color: "#155a93", icon_path: "", overlay_icon_path: ""
    }],
    markers: []
  }
};

describe("live state", () => {
  it("applies ordered position and marker deltas", () => {
    let state = applyEnvelope(null, snapshot);
    state = applyEnvelope(state, {
      protocol_version: 1, session_id: "session-a", sequence: 2, type: "position_delta",
      payload: { updated: [{ id: "player-local", position: { x: 300, y: 400 }, direction: 90 }], removed: [] }
    });
    state = applyEnvelope(state, {
      protocol_version: 1, session_id: "session-a", sequence: 3, type: "marker_delta",
      payload: { updated: [{
        id: "marker-1", label: "Target", kind: "rectangle", position: { x: 500, y: 600 },
        direction: 45, color: "ColorRed", alpha: 0.8, marker_type: "", icon_path: "",
        overlay_icon_path: "", brush: "Solid",
        size: { x: 50, y: 20 }, polyline: [], channel: 1
      }], removed: [] }
    });
    expect(state?.snapshot.entities[0]?.position).toEqual({ x: 300, y: 400 });
    expect(state?.snapshot.markers[0]?.kind).toBe("rectangle");
  });

  it("adds, replaces, and removes normalized cTab entities", () => {
    let state = applyEnvelope(null, snapshot);
    state = applyEnvelope(state, {
      protocol_version: 1, session_id: "session-a", sequence: 2, type: "entity_delta",
      payload: { updated: [{
        id: "ctab-og-unit:1:2", label: "Rifleman", kind: "bft_unit",
        position: { x: 700, y: 800 }, direction: 45, side: "west",
        color: "#00ccff", icon_path: "\\A3\\ui_f\\data\\map\\vehicleicons\\iconMan_ca.paa",
        overlay_icon_path: ""
      }], removed: [] }
    });
    expect(state?.snapshot.entities).toHaveLength(2);
    state = applyEnvelope(state, {
      protocol_version: 1, session_id: "session-a", sequence: 3, type: "entity_delta",
      payload: { updated: [], removed: ["ctab-og-unit:1:2"] }
    });
    expect(state?.snapshot.entities.map((entity) => entity.id)).toEqual(["player-local"]);
  });

  it("ignores stale and foreign-session deltas", () => {
    const current = applyEnvelope(null, snapshot);
    const stale = applyEnvelope(current, {
      protocol_version: 1, session_id: "session-a", sequence: 1, type: "position_delta",
      payload: { updated: [], removed: ["player-local"] }
    });
    const foreign = applyEnvelope(current, {
      protocol_version: 1, session_id: "session-b", sequence: 2, type: "position_delta",
      payload: { updated: [], removed: ["player-local"] }
    });
    expect(stale).toBe(current);
    expect(foreign).toBe(current);
  });

  it("uses a newer snapshot as the authoritative marker and entity inventory", () => {
    let state = applyEnvelope(null, snapshot);
    const staleMarker = {
      id: "ctab-original-user:17", label: "Stale", kind: "icon" as const,
      position: { x: 500, y: 600 }, direction: 0, color: "#16812b", alpha: 1,
      marker_type: "ctab_user_original_unknown_none", icon_path: "", overlay_icon_path: "",
      brush: "Solid", size: { x: 1, y: 1 }, polyline: [], channel: -1
    };
    state = applyEnvelope(state, {
      protocol_version: 1, session_id: "session-a", sequence: 2, type: "marker_delta",
      payload: { updated: [staleMarker, { ...staleMarker, id: "ctab-original-user:18" }], removed: [] }
    });
    state = applyEnvelope(state, {
      ...snapshot,
      sequence: 3,
      payload: { ...snapshot.payload, markers: [] }
    });

    expect(state?.snapshot.markers).toEqual([]);
    expect(state?.snapshot.entities.map((entity) => entity.id)).toEqual(["player-local"]);
  });

  it("uses a newer snapshot to revoke tactical capabilities and cached data", () => {
    let state = applyEnvelope(null, snapshot);
    state = applyEnvelope(state, {
      ...snapshot,
      sequence: 2,
      payload: {
        ...snapshot.payload,
        capabilities: { map: false, own_position: false, bft: false },
        entities: [],
        markers: []
      }
    });

    expect(state?.snapshot.capabilities).toEqual({ map: false, own_position: false, bft: false });
    expect(state?.snapshot.entities).toEqual([]);
    expect(state?.snapshot.markers).toEqual([]);
  });
});
