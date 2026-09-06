import { afterEach, describe, expect, it, vi } from "vitest";
import {
  catalogDisplayMaxZoom,
  createArmaMarkerIcon,
  createCtabEntityIcon,
  createOwnPositionIcon,
  entityDisplayCategory,
  labelCollisionThresholdForZoom,
  markerLabelPriority,
  resolveTacticalLabelCollisions,
  setOwnPositionDirection,
  TacticalMap,
  type TerrainStatus
} from "./map";
import type { SessionSnapshot, TacticalMarker } from "./protocol";

const snapshot: SessionSnapshot = Object.freeze({
  mission_name: "Map render smoke",
  ctab_edition: "original",
  capabilities: Object.freeze({ map: true, own_position: true, bft: true }),
  terrain: Object.freeze({
    world_name: "Altis",
    display_name: "Altis",
    world_size: 30_720
  }),
  entities: Object.freeze([
    Object.freeze({
      id: "player",
      label: "Player",
      kind: "player" as const,
      position: Object.freeze({ x: 14_693, y: 16_239 }),
      direction: 0,
      side: "west" as const,
      color: "#155a93",
      icon_path: "",
      overlay_icon_path: ""
    })
  ]) as SessionSnapshot["entities"],
  markers: Object.freeze([]) as unknown as SessionSnapshot["markers"]
});

describe("original cTab BFT icons", () => {
  it("uses token-scoped local primary and overlay icon URLs", () => {
    const token = "b".repeat(64);
    const icon = createCtabEntityIcon({
      id: "ctab-og-group:1:7",
      label: "Alpha 1-2",
      kind: "bft_unit",
      position: { x: 100, y: 200 },
      direction: 450,
      side: "west",
      color: "#00ccff",
      icon_path: "\\A3\\ui_f\\data\\map\\markers\\nato\\b_inf.paa",
      overlay_icon_path: "\\A3\\ui_f\\data\\map\\markers\\nato\\group_1.paa"
    }, token);
    const root = icon.options.html as HTMLElement;
    const sources = Array.from(root.querySelectorAll<HTMLImageElement>("img"))
      .map((image) => image.getAttribute("src"));

    expect(sources[0]).toMatch(new RegExp(`^/entity-icon/${token}/primary/ctab-og-group%3A1%3A7\\?v=[a-f0-9]{8}$`));
    expect(sources[1]).toMatch(new RegExp(`^/entity-icon/${token}/overlay/ctab-og-group%3A1%3A7\\?v=[a-f0-9]{8}$`));
    expect(root.querySelector<HTMLElement>(".ctab-bft-primary")?.style.transform)
      .toBe("rotate(90deg)");
    expect(root.querySelector(".ctab-bft-label")?.textContent).toBe("Alpha 1-2");
    expect(root.style.getPropertyValue("--ctab-bft-color")).toBe("#00ccff");
  });

  it("classifies groups and vehicles above individual squad members", () => {
    const member = {
      id: "ctab-original-unit:1:8",
      label: "Rifleman",
      kind: "bft_unit" as const,
      position: { x: 100, y: 200 },
      direction: 0,
      side: "west" as const,
      color: "#55aaff",
      icon_path: "",
      overlay_icon_path: ""
    };
    expect(entityDisplayCategory(member)).toBe("member");
    expect(entityDisplayCategory({ ...member, id: "ctab-original-group:1:7" })).toBe("group");
    expect(entityDisplayCategory({ ...member, kind: "bft_vehicle" })).toBe("vehicle");
  });

  it("changes the cached overlay URL when cTab reports a new group size", () => {
    const token = "b".repeat(64);
    const base = {
      id: "ctab-devastator-group:1:7",
      label: "Alpha 2-4",
      kind: "bft_unit" as const,
      position: { x: 100, y: 200 },
      direction: 0,
      side: "west" as const,
      color: "#155a93",
      icon_path: "\\A3\\ui_f\\data\\map\\markers\\nato\\b_inf.paa"
    };
    const squad = createCtabEntityIcon({
      ...base,
      overlay_icon_path: "\\A3\\ui_f\\data\\map\\markers\\nato\\group_1.paa"
    }, token);
    const team = createCtabEntityIcon({
      ...base,
      overlay_icon_path: "\\A3\\ui_f\\data\\map\\markers\\nato\\group_0.paa"
    }, token);
    const squadUrl = (squad.options.html as HTMLElement)
      .querySelector<HTMLImageElement>(".ctab-bft-overlay img")?.src;
    const teamUrl = (team.options.html as HTMLElement)
      .querySelector<HTMLImageElement>(".ctab-bft-overlay img")?.src;

    expect(squadUrl).toContain(`/entity-icon/${token}/overlay/ctab-devastator-group%3A1%3A7?v=`);
    expect(teamUrl).toContain(`/entity-icon/${token}/overlay/ctab-devastator-group%3A1%3A7?v=`);
    expect(teamUrl).not.toBe(squadUrl);
  });
});

afterEach(() => {
  vi.unstubAllGlobals();
  document.body.replaceChildren();
});

describe("tactical map startup", () => {
  it("does not request terrain data without map capability", () => {
    const fetchMock = vi.fn();
    vi.stubGlobal("fetch", fetchMock);
    const element = document.createElement("div");
    document.body.append(element);
    const statuses: TerrainStatus[] = [];
    const map = new TacticalMap(element, "a".repeat(64), (status) => statuses.push(status));

    map.render({
      ...snapshot,
      capabilities: { map: false, own_position: true, bft: false },
      markers: []
    });

    expect(fetchMock).not.toHaveBeenCalled();
    expect(statuses.at(-1)).toEqual({
      kind: "fallback",
      message: "GPS available - terrain map requires a map item"
    });
  });

  it("renders a snapshot before the terrain request completes", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response("", { status: 404 })));
    const element = document.createElement("div");
    document.body.append(element);
    const statuses: TerrainStatus[] = [];
    const map = new TacticalMap(element, "a".repeat(64), (status) => statuses.push(status));

    expect(() => map.render(snapshot)).not.toThrow();
    await vi.waitFor(() => {
      expect(statuses.at(-1)?.kind).toBe("fallback");
    });
  });

  it("moves the own-position marker and updates its live heading", () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response("", { status: 404 })));
    const element = document.createElement("div");
    document.body.append(element);
    const map = new TacticalMap(element, "a".repeat(64), () => undefined);
    map.render(snapshot);
    const originalCore = element.querySelector(".own-position-core");
    const updated: SessionSnapshot = {
      ...snapshot,
      entities: snapshot.entities.map((entity) => ({
        ...entity,
        position: { x: 15_000, y: 17_000 },
        direction: 135
      }))
    };

    map.updatePositions(updated);

    expect(element.querySelector(".own-position-core")).toBe(originalCore);
  });

  it("toggles tactical layers without rebuilding the map", () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response("", { status: 404 })));
    const element = document.createElement("div");
    document.body.append(element);
    const map = new TacticalMap(element, "a".repeat(64), () => undefined);
    map.render(snapshot);
    expect(element.querySelector(".own-position-core")).not.toBeNull();

    map.setLayerVisible("entities", false);
    expect(element.querySelector(".own-position-core")).toBeNull();
    map.setLayerVisible("entities", true);
    expect(element.querySelector(".own-position-core")).not.toBeNull();

    expect(element.classList.contains("map-muted")).toBe(true);
    map.setMapMuted(false);
    expect(element.classList.contains("map-muted")).toBe(false);
    map.setBftNamesVisible(false);
    expect(element.classList.contains("hide-bft-labels")).toBe(true);
  });

  it("keeps an unchanged cTab symbol mounted during topology sync", () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response("", { status: 404 })));
    const element = document.createElement("div");
    document.body.append(element);
    const map = new TacticalMap(element, "a".repeat(64), () => undefined);
    const withBft: SessionSnapshot = {
      ...snapshot,
      entities: [...snapshot.entities, {
        id: "ctab-og-unit:1:2", label: "Alpha 1-2", kind: "bft_unit",
        position: { x: 100, y: 200 }, direction: 0, side: "west",
        color: "#155A93CC", icon_path: "\\A3\\ui_f\\data\\map\\vehicleicons\\iconMan_ca.paa",
        overlay_icon_path: ""
      }]
    };
    map.render(withBft);
    const originalMarker = element.querySelector(".ctab-bft-marker");
    const moved: SessionSnapshot = {
      ...withBft,
      entities: withBft.entities.map((entity) => entity.id === "ctab-og-unit:1:2"
        ? { ...entity, position: { x: 300, y: 400 }, direction: 90 }
        : entity)
    };

    map.updateEntities(moved);

    expect(element.querySelector(".ctab-bft-marker")).toBe(originalMarker);
    expect(originalMarker?.querySelector<HTMLElement>(".ctab-bft-primary")?.style.transform)
      .toBe("rotate(90deg)");
  });

  it("follows an available player and releases on manual map input", () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response("", { status: 404 })));
    const element = document.createElement("div");
    document.body.append(element);
    const changes: boolean[] = [];
    const map = new TacticalMap(element, "a".repeat(64), () => undefined, (following) => {
      changes.push(following);
    });
    map.render(snapshot);

    expect(map.setFollowPlayer(true)).toBe(true);
    expect(map.isFollowingPlayer()).toBe(true);
    element.dispatchEvent(new Event("wheel"));

    expect(map.isFollowingPlayer()).toBe(false);
    expect(changes).toEqual([true, false]);
  });

  it("does not enable follow mode without an own-position entity", () => {
    const element = document.createElement("div");
    document.body.append(element);
    const map = new TacticalMap(element, "a".repeat(64), () => undefined);
    map.render({
      ...snapshot,
      capabilities: { map: false, own_position: false, bft: false },
      entities: []
    });

    expect(map.setFollowPlayer(true)).toBe(false);
    expect(map.isFollowingPlayer()).toBe(false);
  });
});

describe("tactical label hierarchy", () => {
  it("allows more partial label overlap while zoomed out", () => {
    expect(labelCollisionThresholdForZoom(1))
      .toBeGreaterThan(labelCollisionThresholdForZoom(6));

    const root = document.createElement("div");
    const first = document.createElement("span");
    first.dataset.tacticalLabelPriority = "200";
    first.dataset.tacticalLabelKey = "first";
    const second = document.createElement("span");
    second.dataset.tacticalLabelPriority = "200";
    second.dataset.tacticalLabelKey = "second";
    root.append(first, second);
    vi.spyOn(first, "getBoundingClientRect").mockReturnValue({
      left: 0, top: 0, right: 100, bottom: 25, width: 100, height: 25,
      x: 0, y: 0, toJSON: () => ({})
    } as DOMRect);
    vi.spyOn(second, "getBoundingClientRect").mockReturnValue({
      left: 50, top: 0, right: 150, bottom: 25, width: 100, height: 25,
      x: 50, y: 0, toJSON: () => ({})
    } as DOMRect);

    resolveTacticalLabelCollisions(root, labelCollisionThresholdForZoom(1));

    expect(second.classList.contains("tactical-label-collision-hidden")).toBe(false);
  });

  it("prioritizes cTab tactical markers above regular map-marker labels", () => {
    const marker: TacticalMarker = {
      id: "marker-1", label: "Marker", kind: "icon", position: { x: 1, y: 2 },
      direction: 0, color: "ColorWEST", alpha: 1, marker_type: "mil_dot",
      icon_path: "", overlay_icon_path: "", brush: "Solid", size: { x: 1, y: 1 },
      polyline: [], channel: 0
    };
    expect(markerLabelPriority({ ...marker, marker_type: "ctab_user_original_unknown_none" }))
      .toBeGreaterThan(markerLabelPriority(marker));
  });

  it("hides the lower-priority label when two labels overlap", () => {
    const root = document.createElement("div");
    const high = document.createElement("span");
    high.dataset.tacticalLabelPriority = "400";
    high.dataset.tacticalLabelKey = "group";
    const lowTooltip = document.createElement("div");
    lowTooltip.className = "leaflet-tooltip";
    const low = document.createElement("span");
    low.dataset.tacticalLabelPriority = "100";
    low.dataset.tacticalLabelKey = "member";
    lowTooltip.append(low);
    root.append(high, lowTooltip);
    const bounds = {
      left: 10, top: 10, right: 110, bottom: 35, width: 100, height: 25,
      x: 10, y: 10, toJSON: () => ({})
    } as DOMRect;
    vi.spyOn(high, "getBoundingClientRect").mockReturnValue(bounds);
    vi.spyOn(lowTooltip, "getBoundingClientRect").mockReturnValue(bounds);

    resolveTacticalLabelCollisions(root);

    expect(high.classList.contains("tactical-label-collision-hidden")).toBe(false);
    expect(lowTooltip.classList.contains("tactical-label-collision-hidden")).toBe(true);
  });

  it("keeps labels visible when only a narrow edge overlaps", () => {
    const root = document.createElement("div");
    const high = document.createElement("span");
    high.dataset.tacticalLabelPriority = "400";
    high.dataset.tacticalLabelKey = "group";
    const low = document.createElement("span");
    low.dataset.tacticalLabelPriority = "100";
    low.dataset.tacticalLabelKey = "member";
    root.append(high, low);
    vi.spyOn(high, "getBoundingClientRect").mockReturnValue({
      left: 10, top: 10, right: 110, bottom: 35, width: 100, height: 25,
      x: 10, y: 10, toJSON: () => ({})
    } as DOMRect);
    vi.spyOn(low, "getBoundingClientRect").mockReturnValue({
      left: 106, top: 10, right: 206, bottom: 35, width: 100, height: 25,
      x: 106, y: 10, toJSON: () => ({})
    } as DOMRect);

    resolveTacticalLabelCollisions(root);

    expect(high.classList.contains("tactical-label-collision-hidden")).toBe(false);
    expect(low.classList.contains("tactical-label-collision-hidden")).toBe(false);
  });
});

describe("Arma marker icons", () => {
  const marker: TacticalMarker = {
    id: "marker-1",
    label: "Objective",
    kind: "icon",
    position: { x: 1_000, y: 2_000 },
    direction: 450,
    color: "ColorEAST",
    alpha: 0.75,
    marker_type: "mil_objective",
    icon_path: "\\A3\\ui_f\\data\\map\\markers\\military\\objective_CA.paa",
    overlay_icon_path: "",
    brush: "Solid",
    size: { x: 1, y: 1 },
    polyline: [],
    channel: 0
  };

  it("uses a token-scoped local icon URL and applies rotation and color", () => {
    const token = "a".repeat(64);
    const icon = createArmaMarkerIcon(marker, token);
    const root = icon.options.html as HTMLElement;
    const texture = root.querySelector<HTMLImageElement>(".arma-marker-texture");

    expect(texture?.getAttribute("src")).toMatch(
      new RegExp(`^/marker-icon/${token}/mil_objective\\?v=[a-f0-9]{8}$`)
    );
    expect(texture?.draggable).toBe(false);
    expect(root.style.getPropertyValue("--arma-marker-color")).toBe("#9b1118");
    expect(root.style.opacity).toBe("0.75");
    expect(root.querySelector<HTMLElement>(".arma-marker-symbol")?.style.transform)
      .toBe("rotate(90deg)");

    texture?.dispatchEvent(new Event("load"));
    expect(root.classList.contains("arma-marker-icon-ready")).toBe(true);
  });

  it("keeps the fallback and does not request invalid marker paths", () => {
    const unsafe = { ...marker, marker_type: "../secret" };
    const icon = createArmaMarkerIcon(unsafe, "a".repeat(64));
    const root = icon.options.html as HTMLElement;

    expect(root.querySelector(".arma-marker-fallback")).not.toBeNull();
    expect(root.querySelector(".arma-marker-texture")).toBeNull();
    expect(root.querySelector(".arma-marker-mask")).toBeNull();
  });

  it("requests an icon reported by a loaded Workshop mod", () => {
    const modMarker: TacticalMarker = {
      ...marker,
      marker_type: "marker_pack_artillery",
      icon_path: JSON.stringify([
        "ctab_mod_icon_v1",
        "@marker_pack",
        "3123456789",
        "7de4bd5c",
        "marker_pack\\data\\artillery_ca.paa"
      ])
    };
    const token = "b".repeat(64);
    const icon = createArmaMarkerIcon(modMarker, token);
    const root = icon.options.html as HTMLElement;

    expect(root.querySelector<HTMLImageElement>(".arma-marker-texture")?.src)
      .toContain(`/marker-icon/${token}/marker_pack_artillery?v=`);
  });

  it("keeps cTab user-marker overlays and movement separate from symbol rotation", () => {
    const ctabMarker: TacticalMarker = {
      ...marker,
      id: "ctab-og-user:17",
      marker_type: "ctab_user_17",
      overlay_icon_path: "\\A3\\ui_f\\data\\map\\markers\\nato\\group_1.paa",
      direction: 135,
      polyline: [{ x: 1_000, y: 2_000 }, { x: 1_177, y: 1_823 }]
    };
    const token = "c".repeat(64);
    const icon = createArmaMarkerIcon(ctabMarker, token);
    const root = icon.options.html as HTMLElement;

    expect(root.querySelector<HTMLElement>(".arma-marker-symbol")?.style.transform)
      .toBe("rotate(0deg)");
    expect(root.querySelector<HTMLImageElement>(".arma-marker-overlay-texture")?.src)
      .toContain(`/marker-icon/${token}/ctab_user_17/overlay?v=`);
    expect(root.querySelector<HTMLElement>(".ctab-user-direction")?.style.transform)
      .toBe("rotate(135deg)");
  });

  it("renders a Devastator-only naval marker through the local base-game icon route", () => {
    const devastatorMarker: TacticalMarker = {
      ...marker,
      id: "ctab-devastator-user:41",
      marker_type: "ctab_user_devastator_opfor_naval_none",
      icon_path: "\\A3\\ui_f\\data\\map\\markers\\nato\\o_naval.paa"
    };
    const token = "d".repeat(64);
    const icon = createArmaMarkerIcon(devastatorMarker, token);
    const root = icon.options.html as HTMLElement;

    expect(root.querySelector<HTMLImageElement>(".arma-marker-texture")?.src)
      .toContain(`/marker-icon/${token}/ctab_user_devastator_opfor_naval_none?v=`);
  });

  it("uses crossed swords instead of a circular fallback for loc_Attack", () => {
    const attack: TacticalMarker = {
      ...marker,
      marker_type: "loc_Attack",
      icon_path: "\\A3\\ui_f\\data\\map\\markers\\military\\attack_CA.paa",
      color: "#ed1b24"
    };
    const icon = createArmaMarkerIcon(attack, "f".repeat(64));
    const root = icon.options.html as HTMLElement;
    const fallback = root.querySelector<HTMLElement>(".arma-marker-fallback");

    expect(fallback?.classList.contains("arma-marker-fallback-crossed-swords")).toBe(true);
    expect(fallback?.textContent).toBe("⚔");
  });

  it("keeps unchanged markers mounted across authoritative snapshots and removes stale markers", () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response("", { status: 404 })));
    const element = document.createElement("div");
    document.body.append(element);
    const map = new TacticalMap(element, "a".repeat(64), () => undefined);
    const withMarker: SessionSnapshot = { ...snapshot, markers: [marker] };
    map.render(withMarker);
    const originalMarker = element.querySelector(".arma-marker-icon");

    map.render({ ...withMarker, entities: withMarker.entities.map((entity) => ({
      ...entity,
      position: { x: entity.position.x + 10, y: entity.position.y + 10 }
    })) });
    expect(element.querySelector(".arma-marker-icon")).toBe(originalMarker);

    map.render({ ...withMarker, markers: [] });
    expect(element.querySelector(".arma-marker-icon")).toBeNull();
  });

  it("uses an explicit semantic fallback for Devastator-only cTab textures", () => {
    const checkpoint: TacticalMarker = {
      ...marker,
      id: "ctab-devastator-user:37",
      marker_type: "ctab_user_devastator_checkpoint_none",
      icon_path: "\\cTab\\img\\ckp_ca.paa"
    };
    const icon = createArmaMarkerIcon(checkpoint, "e".repeat(64));
    const root = icon.options.html as HTMLElement;
    const fallback = root.querySelector<HTMLElement>(".ctab-marker-fallback");

    expect(fallback?.textContent).toBe("CKP");
    expect(root.querySelector(".arma-marker-texture")).toBeNull();
  });
});

describe("own-position marker", () => {
  it("renders only a compact own-position core", () => {
    const icon = createOwnPositionIcon(450);
    const root = icon.options.html as HTMLElement;

    expect(icon.options.iconSize).toEqual([36, 36]);
    expect(root.querySelector(".own-position-core")).not.toBeNull();
    expect(root.querySelector(".own-position-ring")).toBeNull();
    expect(root.querySelector(".own-position-pointer")).toBeNull();
  });

  it("normalizes cTab symbol rotation", () => {
    const symbol = document.createElement("span");
    setOwnPositionDirection(symbol, -45);
    expect(symbol.style.transform).toBe("rotate(315deg)");
  });
});

describe("catalog overzoom", () => {
  it("allows three browser zoom levels above native map tiles", () => {
    expect(catalogDisplayMaxZoom(6)).toBe(9);
    expect(catalogDisplayMaxZoom(23)).toBe(24);
  });
});
