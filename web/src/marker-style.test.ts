import { describe, expect, it } from "vitest";
import { markerBrushFillOpacity, resolveMarkerColor } from "./marker-style";

describe("Arma marker colors", () => {
  it.each([
    ["ColorWEST", "#155a93"],
    ["ColorEAST", "#9b1118"],
    ["ColorGUER", "#16812b"],
    ["ColorCIV", "#75139a"],
    ["ColorUNKNOWN", "#b59a00"],
    ["ColorBLUFOR", "#155a93"],
    ["ColorOPFOR", "#9b1118"]
  ])("maps %s to its side color", (input, expected) => {
    expect(resolveMarkerColor(input)).toBe(expected);
  });

  it("accepts bounded direct colors and rejects arbitrary CSS", () => {
    expect(resolveMarkerColor("ColorGrey")).toBe("#808080");
    expect(resolveMarkerColor("#(1,0.5,0,0.75)")).toBe("rgba(255, 128, 0, 0.75)");
    expect(resolveMarkerColor("#00CCFFCC")).toBe("#00CCFFCC");
    expect(resolveMarkerColor("url(javascript:alert(1))")).toBe("#1d1d1d");
  });

  it("only makes the border-only brush transparent", () => {
    expect(markerBrushFillOpacity("Border", 1)).toBe(0);
    expect(markerBrushFillOpacity("SolidBorder", 1)).toBe(0.28);
    expect(markerBrushFillOpacity("Solid", 0.2)).toBe(0.2);
  });
});
