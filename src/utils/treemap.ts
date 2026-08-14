export interface TreemapItem {
  key: string;
  value: number;
}

export interface TreemapRect {
  key: string;
  x: number;
  y: number;
  width: number;
  height: number;
}

/**
 * Squarified treemap layout (Bruls, Huizing, van Wijk). Coordinates are in
 * whatever unit `width`/`height` are given in — callers pass 0-100 to lay
 * out a percentage-based box, which nests for free under CSS percentage
 * positioning without tracking absolute pixel coordinates through recursion.
 */
export function squarify(
  items: TreemapItem[],
  x: number,
  y: number,
  width: number,
  height: number
): TreemapRect[] {
  const positive = items.filter((item) => item.value > 0);
  const total = positive.reduce((sum, item) => sum + item.value, 0);
  if (total <= 0 || width <= 0 || height <= 0) return [];

  const sorted = [...positive].sort((a, b) => b.value - a.value);
  const area = width * height;
  const scaled = sorted.map((item) => ({
    key: item.key,
    area: (item.value / total) * area,
  }));

  const out: TreemapRect[] = [];
  layoutRow(scaled, x, y, width, height, out);
  return out;
}

function worstRatio(row: { area: number }[], sum: number, side: number): number {
  const areas = row.map((r) => r.area);
  const maxA = Math.max(...areas);
  const minA = Math.min(...areas);
  const sideSq = side * side;
  return Math.max((sideSq * maxA) / (sum * sum), (sum * sum) / (sideSq * minA));
}

function layoutRow(
  items: { key: string; area: number }[],
  x: number,
  y: number,
  width: number,
  height: number,
  out: TreemapRect[]
): void {
  if (items.length === 0) return;
  if (items.length === 1) {
    out.push({ key: items[0].key, x, y, width, height });
    return;
  }

  const wide = width >= height;
  const shortSide = wide ? height : width;

  let row = [items[0]];
  let rowSum = items[0].area;
  let rest = items.slice(1);

  while (rest.length > 0) {
    const next = rest[0];
    const candidateRow = [...row, next];
    const candidateSum = rowSum + next.area;
    if (worstRatio(candidateRow, candidateSum, shortSide) <= worstRatio(row, rowSum, shortSide)) {
      row = candidateRow;
      rowSum = candidateSum;
      rest = rest.slice(1);
    } else {
      break;
    }
  }

  const thickness = rowSum / shortSide;
  let offset = 0;

  if (wide) {
    // Row is a vertical strip pinned to the left, spanning the full height.
    for (const item of row) {
      const itemHeight = item.area / thickness;
      out.push({ key: item.key, x, y: y + offset, width: thickness, height: itemHeight });
      offset += itemHeight;
    }
    layoutRow(rest, x + thickness, y, width - thickness, height, out);
  } else {
    // Row is a horizontal strip pinned to the top, spanning the full width.
    for (const item of row) {
      const itemWidth = item.area / thickness;
      out.push({ key: item.key, x: x + offset, y, width: itemWidth, height: thickness });
      offset += itemWidth;
    }
    layoutRow(rest, x, y + thickness, width, height - thickness, out);
  }
}
