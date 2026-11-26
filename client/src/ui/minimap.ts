// R-UI-04 minimap in the centre of the hex mode (board «Режим гекса»): the
// coarsest level of the baked map (or the hexes in control colours when there
// are no tiles), unit dots in side colours and a frame of the visible area.
// A click moves the camera there.

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface MiniTransform {
  scale: number;
  ox: number;
  oy: number;
}

/** World rect fitted (contained, centred) into a w × h canvas. */
export function fitTransform(world: Rect, w: number, h: number): MiniTransform {
  const scale = Math.min(w / world.w, h / world.h);
  return { scale, ox: (w - world.w * scale) / 2 - world.x * scale, oy: (h - world.h * scale) / 2 - world.y * scale };
}

export const toMini = (t: MiniTransform, p: { x: number; y: number }) => ({ x: p.x * t.scale + t.ox, y: p.y * t.scale + t.oy });
export const fromMini = (t: MiniTransform, p: { x: number; y: number }) => ({ x: (p.x - t.ox) / t.scale, y: (p.y - t.oy) / t.scale });

export interface MiniDot {
  x: number;
  y: number;
  color: string;
}

export interface MiniHex {
  x: number;
  y: number;
  color: string | null;
}

export class Minimap {
  readonly canvas = document.createElement("canvas");
  private image: HTMLImageElement | null = null;
  private t: MiniTransform;

  constructor(
    private world: Rect,
    /** Where the background image lies in the world (the baked map), if any. */
    private imageRect: Rect | null,
    imageUrl: string | null,
    private w: number,
    private h: number,
    label: string,
    onPick: (x: number, y: number) => void,
  ) {
    const dpr = Math.min(2, window.devicePixelRatio || 1);
    this.canvas.width = Math.round(w * dpr);
    this.canvas.height = Math.round(h * dpr);
    this.canvas.style.width = `${w}px`;
    this.canvas.style.height = `${h}px`;
    this.canvas.className = "mm";
    this.canvas.setAttribute("role", "img");
    this.canvas.setAttribute("aria-label", label);
    this.canvas.getContext("2d")?.scale(dpr, dpr);
    this.t = fitTransform(world, w, h);
    if (imageUrl) {
      const img = new Image();
      img.onload = () => {
        this.image = img;
        this.draw(this.last.hexes, this.last.dots, this.last.view);
      };
      img.src = imageUrl;
    }
    this.canvas.addEventListener("click", (e) => {
      const r = this.canvas.getBoundingClientRect();
      const p = fromMini(this.t, { x: ((e.clientX - r.left) * this.w) / r.width, y: ((e.clientY - r.top) * this.h) / r.height });
      onPick(p.x, p.y);
    });
  }

  private last: { hexes: MiniHex[]; dots: MiniDot[]; view: Rect | null } = { hexes: [], dots: [], view: null };

  /** `hexes` are drawn only without a background image; `hexSize` in world px. */
  draw(hexes: MiniHex[], dots: MiniDot[], view: Rect | null, hexSize = 0) {
    this.last = { hexes, dots, view };
    const g = this.canvas.getContext("2d");
    if (!g) return;
    g.clearRect(0, 0, this.w, this.h);
    g.fillStyle = "#1c1e21";
    g.fillRect(0, 0, this.w, this.h);
    if (this.image && this.imageRect) {
      const a = toMini(this.t, this.imageRect);
      g.drawImage(this.image, a.x, a.y, this.imageRect.w * this.t.scale, this.imageRect.h * this.t.scale);
    } else {
      const r = Math.max(1, hexSize * this.t.scale);
      for (const hx of hexes) {
        if (!hx.color) continue;
        const p = toMini(this.t, hx);
        g.fillStyle = hx.color;
        g.fillRect(p.x - r, p.y - r * 0.87, r * 2, r * 1.74);
      }
    }
    for (const d of dots) {
      const p = toMini(this.t, d);
      g.beginPath();
      g.arc(p.x, p.y, 2.5, 0, Math.PI * 2);
      g.fillStyle = d.color;
      g.fill();
      g.lineWidth = 0.8;
      g.strokeStyle = "rgba(0,0,0,0.6)";
      g.stroke();
    }
    if (view) {
      const a = toMini(this.t, view);
      g.strokeStyle = "#ffffff";
      g.lineWidth = 1.5;
      g.strokeRect(a.x, a.y, view.w * this.t.scale, view.h * this.t.scale);
    }
  }
}
