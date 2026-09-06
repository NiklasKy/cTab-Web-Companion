export interface GridScale {
  spacing: number;
  majorEvery: number;
  digits: number;
}

export function gridScaleForZoom(zoom: number): GridScale {
  if (zoom >= 5) {
    return Object.freeze({ spacing: 100, majorEvery: 10, digits: 3 });
  }
  if (zoom >= 3) {
    return Object.freeze({ spacing: 1_000, majorEvery: 5, digits: 2 });
  }
  if (zoom >= 2) {
    return Object.freeze({ spacing: 2_000, majorEvery: 5, digits: 2 });
  }
  return Object.freeze({ spacing: 5_000, majorEvery: 2, digits: 2 });
}

export function buildVisibleGridPositions(
  minimum: number,
  maximum: number,
  worldSize: number,
  spacing: number
): number[] {
  if (!Number.isFinite(minimum) || !Number.isFinite(maximum)
    || !Number.isFinite(worldSize) || worldSize <= 0
    || !Number.isFinite(spacing) || spacing <= 0
    || maximum < minimum) {
    return [];
  }
  const start = Math.max(0, Math.floor(minimum / spacing) * spacing);
  const end = Math.min(worldSize, Math.ceil(maximum / spacing) * spacing);
  const positions: number[] = [];
  for (let position = start; position <= end; position += spacing) {
    positions.push(position);
  }
  return positions;
}

export function formatGridCoordinate(position: number, scale: GridScale): string {
  const divisor = scale.spacing === 100 ? 100 : 1_000;
  return Math.floor(position / divisor).toString().padStart(scale.digits, "0");
}
