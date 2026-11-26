// Pan / zoom with mouse, wheel and two-finger pinch (also for Telegram later).

import type { Container } from "pixi.js";

export interface View {
  x: number;
  y: number;
  scale: number;
}

export const MIN_SCALE = 0.25;
export const MAX_SCALE = 3;
/** Pointer travel (px) below which a press counts as a tap, not a drag. */
export const TAP_SLOP = 6;

/** Zoom by `factor` keeping the screen point (px, py) fixed. */
export function zoomAt(v: View, factor: number, px: number, py: number): View {
  const scale = Math.min(MAX_SCALE, Math.max(MIN_SCALE, v.scale * factor));
  const k = scale / v.scale;
  return { scale, x: px - (px - v.x) * k, y: py - (py - v.y) * k };
}

export interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** Screen area left free by the HUD, px from each edge. */
export interface Insets {
  top: number;
  right: number;
  bottom: number;
  left: number;
}

/** The free screen area: the screen minus the HUD. */
export function freeArea(width: number, height: number, i: Insets): Box {
  return { x: i.left, y: i.top, w: Math.max(1, width - i.left - i.right), h: Math.max(1, height - i.top - i.bottom) };
}

/** The whole map (world `map`) fitted into the free area and centred. */
export function fitView(map: Box, area: Box): View {
  const scale = Math.min(MAX_SCALE, Math.max(MIN_SCALE, Math.min(area.w / map.w, area.h / map.h)));
  return { scale, x: area.x + (area.w - map.w * scale) / 2 - map.x * scale, y: area.y + (area.h - map.h * scale) / 2 - map.y * scale };
}

/**
 * Pan limits: a map bigger than the free area may not leave a gap at its
 * edges; a smaller one stays inside the area. Either way it never goes off
 * screen.
 */
export function clampView(v: View, map: Box, area: Box): View {
  const axis = (pos: number, start: number, size: number, aStart: number, aSize: number) => {
    const a = aStart + aSize - (start + size) * v.scale; // map's far edge on the area's far edge
    const b = aStart - start * v.scale; // map's near edge on the area's near edge
    return Math.min(Math.max(a, b), Math.max(Math.min(a, b), pos));
  };
  return { scale: v.scale, x: axis(v.x, map.x, map.w, area.x, area.w), y: axis(v.y, map.y, map.h, area.y, area.h) };
}

export class Camera {
  /** The map's world box and the HUD insets; panning is limited to them. */
  private bounds: Box | null = null;
  private insets: Insets = { top: 0, right: 0, bottom: 0, left: 0 };
  private view: View;
  private pointers = new Map<number, { x: number; y: number }>();
  private pinch: number | null = null;
  private travel = 0;
  private multi = false;
  /** Called with screen coordinates when a single pointer taps without dragging. */
  onTap: ((x: number, y: number) => void) | null = null;
  /** Called after every pan or zoom. */
  onChange: ((v: View) => void) | null = null;
  /** Called with screen coordinates when the pointer moves with no button down. */
  onHover: ((x: number, y: number) => void) | null = null;

  constructor(
    private world: Container,
    private el: HTMLElement,
    start: View,
  ) {
    this.view = start;
    this.apply();
    el.addEventListener("pointerdown", this.down);
    el.addEventListener("pointermove", this.move);
    el.addEventListener("pointerup", this.up);
    el.addEventListener("pointercancel", this.up);
    el.addEventListener("wheel", this.wheel, { passive: false });
    window.addEventListener("resize", () => this.apply());
  }

  get current(): View {
    return this.view;
  }

  private area(): Box {
    return freeArea(window.innerWidth, window.innerHeight, this.insets);
  }

  /** Limits panning to the map and remembers the HUD's insets. */
  setBounds(map: Box, insets: Insets) {
    this.bounds = map;
    this.insets = insets;
    this.apply();
  }

  /** The whole map in the free area, centred. */
  fit() {
    if (!this.bounds) return;
    this.view = fitView(this.bounds, this.area());
    this.apply();
  }

  private apply() {
    if (this.bounds) this.view = clampView(this.view, this.bounds, this.area());
    this.world.position.set(this.view.x, this.view.y);
    this.world.scale.set(this.view.scale);
    this.onChange?.(this.view);
  }

  private down = (e: PointerEvent) => {
    this.el.setPointerCapture(e.pointerId);
    this.pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
    this.pinch = null;
    if (this.pointers.size === 1) {
      this.travel = 0;
      this.multi = false;
    } else {
      this.multi = true;
    }
  };

  private move = (e: PointerEvent) => {
    const prev = this.pointers.get(e.pointerId);
    if (!prev) {
      if (!this.pointers.size) this.onHover?.(e.clientX, e.clientY);
      return;
    }
    const next = { x: e.clientX, y: e.clientY };
    this.pointers.set(e.pointerId, next);
    this.travel += Math.hypot(next.x - prev.x, next.y - prev.y);
    const all = [...this.pointers.values()];
    if (all.length === 1) {
      this.view = { ...this.view, x: this.view.x + next.x - prev.x, y: this.view.y + next.y - prev.y };
    } else if (all.length === 2) {
      const [a, b] = all as [{ x: number; y: number }, { x: number; y: number }];
      const dist = Math.hypot(a.x - b.x, a.y - b.y);
      if (this.pinch !== null && this.pinch > 0) {
        this.view = zoomAt(this.view, dist / this.pinch, (a.x + b.x) / 2, (a.y + b.y) / 2);
      }
      this.pinch = dist;
    }
    this.apply();
  };

  private up = (e: PointerEvent) => {
    const had = this.pointers.delete(e.pointerId);
    this.pinch = null;
    if (had && e.type === "pointerup" && this.pointers.size === 0 && !this.multi && this.travel < TAP_SLOP) {
      this.onTap?.(e.clientX, e.clientY);
    }
  };

  /** Puts a world point in the middle of the screen (R-UI-05: next unit). */
  centerOn(x: number, y: number) {
    this.view = { ...this.view, x: window.innerWidth / 2 - x * this.view.scale, y: window.innerHeight / 2 - y * this.view.scale };
    this.apply();
  }

  /** Screen → world coordinates. */
  toWorld(x: number, y: number): { x: number; y: number } {
    return { x: (x - this.view.x) / this.view.scale, y: (y - this.view.y) / this.view.scale };
  }

  private wheel = (e: WheelEvent) => {
    e.preventDefault();
    this.view = zoomAt(this.view, Math.exp(-e.deltaY * 0.0015), e.clientX, e.clientY);
    this.apply();
  };
}
