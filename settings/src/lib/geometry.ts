import type { Desk } from './api';

const right = (r: Desk) => r.x + r.w;
const bottom = (r: Desk) => r.y + r.h;

/** Screens closer than this (mm) count as touching, like the crossing engine. */
export const TOUCH_MM = 20;

export function bounds(rects: Desk[]): Desk {
  if (rects.length === 0) return { x: 0, y: 0, w: 1, h: 1 };
  const x = Math.min(...rects.map((r) => r.x));
  const y = Math.min(...rects.map((r) => r.y));
  return { x, y, w: Math.max(...rects.map(right)) - x, h: Math.max(...rects.map(bottom)) - y };
}

/**
 * Magnetic snapping while dragging: each axis jumps to the nearest edge,
 * alignment or centre of another screen within `threshold` mm.
 */
export function snap(m: Desk, others: Desk[], threshold: number): { x: number; y: number } {
  const pick = (value: number, candidates: number[]) => {
    let best = value;
    let dist = threshold;
    for (const c of candidates) {
      const d = Math.abs(c - value);
      if (d < dist) {
        dist = d;
        best = c;
      }
    }
    return best;
  };
  const xs = others.flatMap((o) => [o.x - m.w, right(o), o.x, right(o) - m.w, o.x + o.w / 2 - m.w / 2]);
  const ys = others.flatMap((o) => [o.y - m.h, bottom(o), o.y, bottom(o) - m.h, o.y + o.h / 2 - m.h / 2]);
  return { x: pick(m.x, xs), y: pick(m.y, ys) };
}

export type Stitch = { x: number; y: number; length: number; vertical: boolean };

/** Where two screens meet: the edges the cursor will cross. */
export function stitches(rects: Desk[]): Stitch[] {
  const out: Stitch[] = [];
  for (let i = 0; i < rects.length; i++) {
    for (let j = 0; j < rects.length; j++) {
      if (i === j) continue;
      const a = rects[i];
      const b = rects[j];
      const gapX = b.x - right(a);
      if (gapX > -TOUCH_MM && gapX < TOUCH_MM) {
        const top = Math.max(a.y, b.y);
        const len = Math.min(bottom(a), bottom(b)) - top;
        if (len > 1) out.push({ x: right(a) + gapX / 2, y: top, length: len, vertical: true });
      }
      const gapY = b.y - bottom(a);
      if (gapY > -TOUCH_MM && gapY < TOUCH_MM) {
        const left = Math.max(a.x, b.x);
        const len = Math.min(right(a), right(b)) - left;
        if (len > 1) out.push({ x: left, y: bottom(a) + gapY / 2, length: len, vertical: false });
      }
    }
  }
  return out;
}

/** Same rule as the tray app, so the line on screen and in the window agree. */
export function alignmentLineMm(rects: Desk[]): number {
  const top = Math.max(...rects.map((r) => r.y));
  const bot = Math.min(...rects.map(bottom));
  if (top < bot) return (top + bot) / 2;
  const tallest = rects.reduce((a, b) => (b.h > a.h ? b : a), rects[0]);
  return tallest ? tallest.y + tallest.h / 2 : 0;
}

export type Align = 'bottom' | 'centre' | 'top';

/**
 * Line up the selected screen's row: every screen reachable through
 * side-by-side neighbours moves up or down to match the selected one.
 */
export function alignRow(rects: Desk[], sel: number, how: Align): Desk[] {
  const next = rects.map((r) => ({ ...r }));
  const ref = next[sel];
  const seen = new Set([sel]);
  const queue = [sel];
  while (queue.length) {
    const i = queue.shift()!;
    const a = next[i];
    next.forEach((b, j) => {
      if (seen.has(j)) return;
      const beside = Math.abs(b.x - right(a)) < TOUCH_MM || Math.abs(a.x - right(b)) < TOUCH_MM;
      const overlapY = Math.min(bottom(a), bottom(b)) - Math.max(a.y, b.y) > 0;
      if (beside && overlapY) {
        seen.add(j);
        queue.push(j);
      }
    });
  }
  for (const j of seen) {
    const r = next[j];
    if (how === 'bottom') r.y = bottom(ref) - r.h;
    else if (how === 'top') r.y = ref.y;
    else r.y = ref.y + ref.h / 2 - r.h / 2;
  }
  return next;
}

export const diagonalInches = (w: number, h: number) => Math.hypot(w, h) / 25.4;

/** Resize to a new diagonal keeping the aspect ratio and the bottom-left corner. */
export function withDiagonal(r: Desk, inches: number): Desk {
  const k = (inches * 25.4) / Math.hypot(r.w, r.h);
  const w = r.w * k;
  const h = r.h * k;
  return { x: r.x, y: bottom(r) - h, w, h };
}

/** Black or white text, whichever reads better on `hex`. */
export function textOn(hex: string): string {
  const n = parseInt(hex.replace('#', ''), 16);
  const [r, g, b] = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((c) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  });
  const lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
  return lum > 0.4 ? '#000000' : '#ffffff';
}
