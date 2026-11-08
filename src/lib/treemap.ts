// Squarified treemap (Bruls, Huizing, van Wijk). Lays out values inside a
// rectangle so each block's area is proportional to its value and blocks
// stay as close to square as possible.

export interface Rect { x: number; y: number; w: number; h: number }

export function squarify(values: number[], bounds: Rect): Rect[] {
  const total = values.reduce((s, v) => s + v, 0);
  if (total <= 0 || values.length === 0) return values.map(() => ({ x: 0, y: 0, w: 0, h: 0 }));
  const scale = (bounds.w * bounds.h) / total;
  const order = values.map((v, i) => ({ area: v * scale, i })).sort((a, b) => b.area - a.area);
  const out: Rect[] = new Array(values.length);
  let { x, y, w, h } = bounds;
  let row: typeof order = [];

  const worst = (r: typeof order, side: number) => {
    const sum = r.reduce((s, e) => s + e.area, 0);
    const max = Math.max(...r.map((e) => e.area));
    const min = Math.min(...r.map((e) => e.area));
    return Math.max((side * side * max) / (sum * sum), (sum * sum) / (side * side * min));
  };

  const layoutRow = (r: typeof order) => {
    const sum = r.reduce((s, e) => s + e.area, 0);
    if (w >= h) {
      const colW = sum / h;
      let cy = y;
      for (const e of r) { const eh = e.area / colW; out[e.i] = { x, y: cy, w: colW, h: eh }; cy += eh; }
      x += colW; w -= colW;
    } else {
      const rowH = sum / w;
      let cx = x;
      for (const e of r) { const ew = e.area / rowH; out[e.i] = { x: cx, y, w: ew, h: rowH }; cx += ew; }
      y += rowH; h -= rowH;
    }
  };

  for (const item of order) {
    const side = Math.min(w, h);
    if (row.length === 0 || worst([...row, item], side) <= worst(row, side)) {
      row.push(item);
    } else {
      layoutRow(row);
      row = [item];
    }
  }
  if (row.length) layoutRow(row);
  return out;
}
