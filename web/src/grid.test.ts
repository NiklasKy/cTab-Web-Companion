import { describe, expect, it } from "vitest";
import {
  buildVisibleGridPositions,
  formatGridCoordinate,
  gridScaleForZoom
} from "./grid";

describe("Arma-style metric grid", () => {
  it("builds only the visible one-kilometre lines", () => {
    expect(buildVisibleGridPositions(5_250, 9_440, 30_720, 1_000)).toEqual([
      5_000, 6_000, 7_000, 8_000, 9_000, 10_000
    ]);
  });

  it("uses Arma-like two and three digit coordinate labels", () => {
    expect(formatGridCoordinate(6_000, gridScaleForZoom(3))).toBe("06");
    expect(formatGridCoordinate(14_600, gridScaleForZoom(5))).toBe("146");
  });

  it("changes density with zoom and rejects invalid bounds", () => {
    expect(gridScaleForZoom(1).spacing).toBe(5_000);
    expect(gridScaleForZoom(3).spacing).toBe(1_000);
    expect(gridScaleForZoom(5).spacing).toBe(100);
    expect(buildVisibleGridPositions(10, 0, 30_720, 1_000)).toEqual([]);
    expect(buildVisibleGridPositions(0, 10, 0, 1_000)).toEqual([]);
  });
});
