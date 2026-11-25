// Pan / zoom with mouse, wheel and two-finger pinch (also for Telegram later).

import type { Container } from "pixi.js";

export interface View {
  x: number;
  y: number;
  scale: number;
}

export const MIN_SCALE = 0.25;
export const MAX_SCALE = 3;

/** Zoom by `factor` keeping the screen point (px, py) fixed. */
export function zoomAt(v: View, factor: number, px: number, py: number): View {
  const scale = Math.min(MAX_SCALE, Math.max(MIN_SCALE, v.scale * factor));
  const k = scale / v.scale;
  return { scale, x: px - (px - v.x) * k, y: py - (py - v.y) * k };
}

export class Camera {
  private view: View;
  private pointers = new Map<number, { x: number; y: number }>();
  private pinch: number | null = null;

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
  }

  get current(): View {
    return this.view;
  }

  private apply() {
    this.world.position.set(this.view.x, this.view.y);
    this.world.scale.set(this.view.scale);
  }

  private down = (e: PointerEvent) => {
    this.el.setPointerCapture(e.pointerId);
    this.pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
    this.pinch = null;
  };

  private move = (e: PointerEvent) => {
    const prev = this.pointers.get(e.pointerId);
    if (!prev) return;
    const next = { x: e.clientX, y: e.clientY };
    this.pointers.set(e.pointerId, next);
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
    this.pointers.delete(e.pointerId);
    this.pinch = null;
  };

  private wheel = (e: WheelEvent) => {
    e.preventDefault();
    this.view = zoomAt(this.view, Math.exp(-e.deltaY * 0.0015), e.clientX, e.clientY);
    this.apply();
  };
}
