// Stage 2: the client plays a scenario on the rules core (WASM), hotseat.

import { Application, Assets, Container, Sprite, type Texture } from "pixi.js";

import { Camera } from "./camera";
import { hexCenter, offsetToAxial } from "./hex";
import { hillshadePlacement, type HillshadeMeta } from "./hillshade";
import { Play } from "./play";
import { drawMap } from "./render/map";
import { terrainTextureUrl, type SpritesFile } from "./sprites";
import type { ScenarioView } from "./view";
import init, { Core, Game } from "./wasm/ceasefire_wasm.js";

/** Hex corner radius in world pixels. */
const HEX_SIZE = 48;
/** How strongly the relief shading darkens the terrain textures. */
const HILLSHADE_ALPHA = 0.7;
const SCENARIO = new URLSearchParams(location.search).get("scenario") ?? "golan67";

const text = (url: string) =>
  fetch(url).then((r) => {
    if (!r.ok) throw new Error(`${url}: ${r.status}`);
    return r.text();
  });
const optionalText = (url: string) => fetch(url).then((r) => (r.ok ? r.text() : undefined));

async function main() {
  const panel = document.getElementById("panel")!;
  const stage = (msg: string) => {
    console.info(`[client] ${msg}`);
    panel.textContent = msg;
  };
  stage("Загрузка данных…");
  const [rules, terrain, units, tech, spritesText, ruText] = await Promise.all([
    text("/data/rules.json"),
    text("/data/terrain.json"),
    text("/data/units.json"),
    text("/data/tech.json"),
    text("/data/sprites.json"),
    text("/data/locale/ru.json"),
  ]);
  const sprites = JSON.parse(spritesText) as SpritesFile;
  const ru = JSON.parse(ruText) as Record<string, string>;
  const base = `/data/scenarios/${SCENARIO}`;
  const [scenario, map, events] = await Promise.all([
    text(`${base}/scenario.json`),
    text(`${base}/map.json`),
    optionalText(`${base}/events.json`),
  ]);
  const sideKeys = (JSON.parse(scenario) as { sides: { id: number; key: string }[] }).sides.reduce<string[]>((acc, s) => {
    acc[s.id] = s.key;
    return acc;
  }, []);

  stage("Загрузка ядра…");
  await init();
  const core = new Core(rules, terrain, units, tech);
  const view = JSON.parse(core.scenarioView(scenario, map, events)) as ScenarioView;

  const app = new Application();
  await app.init({ resizeTo: window, background: 0x12110f, antialias: true });
  document.getElementById("app")!.appendChild(app.canvas);

  stage("Загрузка текстур…");
  // Decode on the main thread: worker decoding stalls in some WebViews (and headless Chrome).
  Assets.setPreferences({ preferWorkers: false });
  const load = (url: string) =>
    Assets.load<Texture>(url).catch((e: unknown) => {
      console.error(`[client] texture ${url}:`, e);
      return null;
    });
  const terrainTex = new Map<string, Texture>();
  await Promise.all(
    [...new Set(view.hexes.map((h) => h.terrain))].map(async (t) => {
      const url = terrainTextureUrl(sprites, t);
      const tex = url ? await load(url) : null;
      if (tex) terrainTex.set(t, tex);
    }),
  );
  const object = async (role: string) => {
    const id = sprites.map_objects[role];
    return id ? ((await load(`/assets/objects/${id}.png`)) ?? undefined) : undefined;
  };

  const world = new Container();
  const mapLayer = drawMap(view, { terrain: terrainTex, bridge: await object("bridge"), bridgeDestroyed: await object("bridge_destroyed") }, HEX_SIZE);
  world.addChild(mapLayer);

  // Relief underlay from mapgen, if the scenario has one: over the textures, under the grid.
  const shadeMeta = await optionalText(`${base}/hillshade.json`);
  if (shadeMeta) {
    const meta = JSON.parse(shadeMeta) as HillshadeMeta;
    const tex = await load(`${base}/${meta.png}`);
    if (tex) {
      const shade = new Sprite(tex);
      const p = hillshadePlacement(meta, HEX_SIZE);
      shade.position.set(p.x, p.y);
      shade.scale.set(p.scale);
      shade.blendMode = "multiply";
      shade.alpha = HILLSHADE_ALPHA;
      mapLayer.addChildAt(shade, 1);
    }
  }
  app.stage.addChild(world);

  const xs = view.hexes.map((h) => hexCenter(h, HEX_SIZE));
  const cx = (Math.min(...xs.map((p) => p.x)) + Math.max(...xs.map((p) => p.x))) / 2;
  const cy = (Math.min(...xs.map((p) => p.y)) + Math.max(...xs.map((p) => p.y))) / 2;
  const scale = 0.6;
  const camera = new Camera(world, app.canvas, { scale, x: window.innerWidth / 2 - cx * scale, y: window.innerHeight / 2 - cy * scale });

  // Scenario validation stays visible only when something is wrong.
  if (view.issues.length) {
    panel.innerHTML = `<p class="bad">${ru["ui.validation.problems"] ?? "Problems"}: ${view.issues.length}</p><ul>${view.issues.map((i) => `<li>${i}</li>`).join("")}</ul>`;
  } else {
    panel.style.display = "none";
  }

  stage("Ход…");
  const game = new Game(core, scenario, map, events);
  const play = new Play(game, world, camera, sprites, ru, sideKeys, HEX_SIZE);
  await play.refresh();
  if (!view.issues.length) panel.style.display = "none";

  // Dev-only hook for end-to-end checks: hex → screen coordinates.
  if (import.meta.env.DEV) {
    (window as unknown as { __cl: unknown }).__cl = {
      screenOf: (col: number, row: number) => {
        const p = hexCenter(offsetToAxial({ col, row }), HEX_SIZE);
        const v = camera.current;
        return { x: v.x + p.x * v.scale, y: v.y + p.y * v.scale };
      },
    };
  }
}

main().catch((e: unknown) => {
  const panel = document.getElementById("panel")!;
  panel.style.display = "block";
  panel.textContent = String(e);
  console.error(e);
});
