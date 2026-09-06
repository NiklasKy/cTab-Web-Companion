const ARMA_MARKER_COLORS = new Map<string, string>([
  ["default", "#1d1d1d"],
  ["colorblack", "#1d1d1d"],
  ["colorgrey", "#808080"],
  ["colorred", "#ed1c24"],
  ["colorredalpha", "#ed1c24"],
  ["coloryellow", "#ffe600"],
  ["colorgreen", "#00c914"],
  ["colorblue", "#1747e8"],
  ["colorwhite", "#f5f5f1"],
  ["colorpink", "#cf65ad"],
  ["colororange", "#e58b2b"],
  ["colorbrown", "#7a5038"],
  ["colorwest", "#155a93"],
  ["colorblufor", "#155a93"],
  ["coloreast", "#9b1118"],
  ["coloropfor", "#9b1118"],
  ["colorguer", "#16812b"],
  ["colorindependent", "#16812b"],
  ["colorciv", "#75139a"],
  ["colorcivilian", "#75139a"],
  ["colorunknown", "#b59a00"]
]);

export function resolveMarkerColor(value: string): string {
  const mapped = ARMA_MARKER_COLORS.get(value.trim().toLowerCase());
  if (mapped !== undefined) {
    return mapped;
  }
  if (/^#[0-9a-fA-F]{3}(?:[0-9a-fA-F]{3})?(?:[0-9a-fA-F]{2})?$/.test(value)) {
    return value;
  }
  const direct = parseDirectColor(value);
  return direct ?? "#1d1d1d";
}

export function markerBrushFillOpacity(brush: string, alpha: number): number {
  return brush.trim().toLowerCase() === "border" ? 0 : Math.min(alpha, 0.28);
}

function parseDirectColor(value: string): string | null {
  if (value.length > 80 || !value.startsWith("#(") || !value.endsWith(")")) {
    return null;
  }
  const components = value.slice(2, -1).split(",").map((component) => component.trim());
  if (components.length !== 3 && components.length !== 4) {
    return null;
  }
  const normalized = components.map(parseNormalizedComponent);
  if (normalized.some((component) => component === null)) {
    return null;
  }
  const [red = 0, green = 0, blue = 0, alpha = 1] = normalized as number[];
  return `rgba(${Math.round(red * 255)}, ${Math.round(green * 255)}, ${Math.round(blue * 255)}, ${alpha})`;
}

function parseNormalizedComponent(value: string): number | null {
  if (!/^(?:0(?:\.\d{1,6})?|1(?:\.0{1,6})?)$/.test(value)) {
    return null;
  }
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed >= 0 && parsed <= 1 ? parsed : null;
}
