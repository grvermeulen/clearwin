export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/**
 * Squarified treemap layout: splits `bounds` into one rectangle per value,
 * with areas proportional to the values and shapes kept close to square.
 * `values` must be sorted largest first; zero values get no rectangle.
 */
export function squarify(values: number[], bounds: Rect): (Rect | null)[] {
  const total = values.reduce((a, b) => a + b, 0);
  const out: (Rect | null)[] = values.map(() => null);
  if (total <= 0 || bounds.w <= 0 || bounds.h <= 0) return out;

  const scale = (bounds.w * bounds.h) / total;
  const items = values.map((v, i) => ({ i, area: v * scale })).filter((it) => it.area > 0);
  let free = { ...bounds };
  let start = 0;

  const worst = (row: number[], side: number) => {
    const sum = row.reduce((a, b) => a + b, 0);
    const max = Math.max(...row);
    const min = Math.min(...row);
    return Math.max((side * side * max) / (sum * sum), (sum * sum) / (side * side * min));
  };

  while (start < items.length) {
    const side = Math.min(free.w, free.h);
    let end = start + 1;
    while (end < items.length) {
      const current = items.slice(start, end).map((it) => it.area);
      const next = items.slice(start, end + 1).map((it) => it.area);
      if (worst(next, side) > worst(current, side)) break;
      end++;
    }
    const row = items.slice(start, end);
    const rowArea = row.reduce((a, it) => a + it.area, 0);
    if (free.w >= free.h) {
      // Lay the row out as a column on the left.
      const colW = rowArea / free.h;
      let y = free.y;
      for (const it of row) {
        const h = it.area / colW;
        out[it.i] = { x: free.x, y, w: colW, h };
        y += h;
      }
      free = { x: free.x + colW, y: free.y, w: free.w - colW, h: free.h };
    } else {
      // Lay the row out as a strip along the top.
      const rowH = rowArea / free.w;
      let x = free.x;
      for (const it of row) {
        const w = it.area / rowH;
        out[it.i] = { x, y: free.y, w, h: rowH };
        x += w;
      }
      free = { x: free.x, y: free.y + rowH, w: free.w, h: free.h - rowH };
    }
    start = end;
  }
  return out;
}
