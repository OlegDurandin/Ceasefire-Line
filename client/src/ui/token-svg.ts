// R-UI-03/R-UI-04: the unit token as HTML/SVG for the attack window's
// portraits — the same geometry as on the map (render/token-geom.ts), the
// approved mock's component with the real miniature in place of a sketch.

import { flagCss, plainFlag } from "../render/flag";
import { GROUND_STOPS, sideGradient, sidePath, tokenGeom, type FlagStripes } from "../render/token-geom";

let uid = 0;
const hex = (c: number) => `#${c.toString(16).padStart(6, "0")}`;
const r1 = (v: number) => Math.round(v * 10) / 10;

export interface TokenSvgInput {
  /** Token width in px. */
  w: number;
  color: number;
  flag: FlagStripes | null;
  label: string;
  model: string | null;
  anchor: [number, number];
  mirrored: boolean;
}

export function tokenHtml(t: TokenSvgInput): string {
  const g = tokenGeom(t.w);
  const id = `tk${++uid}`;
  const [top, bottom] = sideGradient(t.color);
  const specks = g.specks.map((s) => `<circle cx="${r1(s.x)}" cy="${r1(s.y)}" r="${r1(s.r)}" fill="${s.c}"></circle>`).join("");
  const stops = GROUND_STOPS.map(([o, c]) => `<stop offset="${o}" stop-color="${c}"></stop>`).join("");
  const flag = flagCss(t.flag ?? plainFlag(hex(t.color)), hex(t.color));
  const model = t.model
    ? `<img src="${t.model}" alt="" style="position: absolute; left: ${r1(g.cx - g.mW * t.anchor[0])}px; top: ${r1(g.contactY - g.mW * t.anchor[1])}px; width: ${g.mW}px; height: ${g.mW}px;${t.mirrored ? " transform: scaleX(-1);" : ""}">`
    : "";
  return `<div class="tk" style="position: relative; width: ${g.w}px; height: ${g.h}px">
  <div aria-hidden="true" style="position: absolute; left: ${r1(g.shL)}px; top: ${r1(g.shT)}px; width: ${r1(g.shW)}px; height: ${r1(g.shH)}px; border-radius: 50%; background: rgba(40, 26, 8, 0.42); filter: blur(${r1(g.shB)}px)"></div>
  <svg width="${g.w}" height="${g.h}" viewBox="0 0 ${g.w} ${g.h}" aria-hidden="true" style="position: absolute; left: 0; top: 0; overflow: visible">
    <defs>
      <linearGradient id="${id}s" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="${hex(top)}"></stop><stop offset="1" stop-color="${hex(bottom)}"></stop></linearGradient>
      <radialGradient id="${id}g" cx="0.4" cy="0.35" r="0.75">${stops}</radialGradient>
    </defs>
    <path d="${sidePath(g)}" fill="url(#${id}s)" stroke="rgba(20,20,20,0.55)" stroke-width="1"></path>
    <ellipse cx="${r1(g.cx)}" cy="${r1(g.cy)}" rx="${r1(g.rx)}" ry="${r1(g.ry)}" fill="url(#${id}g)" stroke="${hex(t.color)}" stroke-width="${r1(g.rimW)}"></ellipse>
    ${specks}
  </svg>
  ${model}
  <div style="position: absolute; left: 50%; top: ${r1(g.plT)}px; transform: translateX(-50%); display: flex; align-items: center; gap: 4px; padding: 0 6px 0 4px; background: #1c1e21; border: 1px solid #c9a46a; border-radius: 3px">
    <span aria-hidden="true" style="display: block; width: ${g.fW}px; height: ${g.fH}px; background: ${flag}; border: 1px solid rgba(255, 255, 255, 0.7)"></span>
    <span style="color: #ffffff; font-weight: 700; font-size: ${g.fs}px; line-height: 1.2">${t.label}</span>
  </div>
</div>`;
}
