// Curated hues rather than a raw hash-to-360 spread, so neighboring
// branches don't land on muddy/adjacent colors.
const HUES = [4, 24, 45, 100, 160, 190, 213, 250, 280, 320, 340, 60];

function hashString(value: string): number {
  let hash = 0;
  for (let i = 0; i < value.length; i++) {
    hash = (hash * 31 + value.charCodeAt(i)) >>> 0;
  }
  return hash;
}

/** The path segment directly under `rootPath` that `path` descends from. */
export function topLevelSegment(path: string, rootPath: string): string {
  const normalizedRoot = rootPath.endsWith("/") ? rootPath : rootPath + "/";
  const rel = path.startsWith(normalizedRoot) ? path.slice(normalizedRoot.length) : path;
  const seg = rel.split("/")[0];
  return seg || path;
}

/**
 * Every node's color is derived from its top-level ancestor (relative to
 * the volume window's root), so a branch's hue stays stable as you zoom in
 * and out of it. Depth only adjusts lightness, giving the nested "cushion"
 * look without the color identity drifting per level.
 */
export function backgroundForEntry(path: string, rootPath: string, depth: number): string {
  const branch = topLevelSegment(path, rootPath);
  const hue = HUES[hashString(branch) % HUES.length];
  const lightness = Math.min(30 + depth * 6, 62);
  const highlight = Math.min(lightness + 20, 82);
  return `radial-gradient(circle at 32% 26%, hsl(${hue}, 62%, ${highlight}%) 0%, hsl(${hue}, 55%, ${lightness}%) 75%)`;
}

export function swatchForEntry(path: string, rootPath: string): string {
  const branch = topLevelSegment(path, rootPath);
  const hue = HUES[hashString(branch) % HUES.length];
  return `hsl(${hue}, 55%, 45%)`;
}
