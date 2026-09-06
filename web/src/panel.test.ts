import { describe, expect, it } from "vitest";
import { sortTacticalEntitiesByLabel } from "./panel";
import type { TacticalEntity } from "./protocol";

function entity(id: string, label: string): TacticalEntity {
  return {
    id,
    label,
    kind: "bft_unit",
    position: { x: 0, y: 0 },
    direction: 0,
    side: "west",
    color: "#155a93",
    icon_path: "",
    overlay_icon_path: ""
  };
}

describe("unit panel ordering", () => {
  it("sorts labels alphabetically and uses natural number ordering", () => {
    const source = [
      entity("3", "Sabre-1"),
      entity("2", "Alpha 10"),
      entity("1", "alpha 2")
    ];

    expect(sortTacticalEntitiesByLabel(source).map((item) => item.label))
      .toEqual(["alpha 2", "Alpha 10", "Sabre-1"]);
    expect(source.map((item) => item.label))
      .toEqual(["Sabre-1", "Alpha 10", "alpha 2"]);
  });
});
