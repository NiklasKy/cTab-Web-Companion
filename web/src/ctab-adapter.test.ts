import { describe, expect, it } from "vitest";
import { parseTacticalEnvelope } from "./protocol";

const editions = [
  {
    name: "Original cTab 2.2.2.1",
    entityId: "ctab-original-unit:1:2",
    markerId: "ctab-original-user:17",
    markerType: "ctab_user_original_opfor_infantry_squad",
    iconPath: "\\A3\\ui_f\\data\\map\\markers\\nato\\o_inf.paa"
  },
  {
    name: "cTab Devastator Edition 2.3.0.0",
    entityId: "ctab-devastator-unit:1:2",
    markerId: "ctab-devastator-user:17",
    markerType: "ctab_user_devastator_opfor_naval_squad",
    iconPath: "\\A3\\ui_f\\data\\map\\markers\\nato\\o_naval.paa"
  }
] as const;

describe.each(editions)("shared adapter contract: $name", (edition) => {
  it("accepts the normalized BFT and user-marker records", () => {
    const envelope = parseTacticalEnvelope({
      protocol_version: 1,
      session_id: "ctab-adapter-contract",
      sequence: 1,
      type: "session_snapshot",
      payload: {
        mission_name: "Adapter contract",
        ctab_edition: "original",
        capabilities: { map: true, own_position: true, bft: true },
        terrain: { world_name: "Altis", display_name: "Altis", world_size: 30_720 },
        entities: [{
          id: edition.entityId,
          label: "Alpha 1-2",
          kind: "bft_unit",
          position: { x: 14_000, y: 16_000 },
          direction: 90,
          side: "west",
          color: "#155A93CC",
          icon_path: "\\A3\\ui_f\\data\\map\\markers\\nato\\b_inf.paa",
          overlay_icon_path: "\\A3\\ui_f\\data\\map\\markers\\nato\\group_1.paa"
        }],
        markers: [{
          id: edition.markerId,
          label: "12:06",
          kind: "icon",
          position: { x: 14_250, y: 16_150 },
          direction: 45,
          color: "#9B1118CC",
          alpha: 1,
          marker_type: edition.markerType,
          icon_path: edition.iconPath,
          overlay_icon_path: "\\A3\\ui_f\\data\\map\\markers\\nato\\group_1.paa",
          brush: "Solid",
          size: { x: 1, y: 1 },
          polyline: [{ x: 14_250, y: 16_150 }, { x: 14_427, y: 16_327 }],
          channel: -1
        }]
      }
    });

    expect(envelope?.type).toBe("session_snapshot");
    if (envelope?.type !== "session_snapshot") throw new Error("fixture was rejected");
    expect(envelope.payload.entities[0]?.id).toBe(edition.entityId);
    expect(envelope.payload.markers[0]?.marker_type).toBe(edition.markerType);
  });
});
