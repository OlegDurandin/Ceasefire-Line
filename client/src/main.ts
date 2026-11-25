// Stage 2 step 1: the client shows a scenario map from the rules core (WASM).

import { Application, Assets, Container, type Texture } from "pixi.js";

import { Camera } from "./camera";
import { hexCenter } from "./hex";
import { drawMap } from "./render/map";
import { drawUnits } from "./render/units";
import { pickSprite, terrainTextureUrl, type SpritesFile } from "./sprites";
import type { ScenarioView } from "./view";
import init, { Core } from "./wasm/ceasefire_wasm.js";

/** Hex corner radius in world pixels. */
const HEX_SIZE = 48;
const SCENARIO = new URLSearchParams(location.search).get("scenario") ?? "golan67";

const text = (url: string) =>
  fetch(url).then((r) => {
    if (!r.ok) throw new Error(`${url}: ${r.status}`);
    return r.text();
  });
const json = async <T>(url: string): Promise<T> => JSON.parse(await text(url)) as T;
const optionalText = (url: string) => fetch(url).then((r) => (r.ok ? r.text() : undefined));

async function main() {
  const panel = document.getElementById("panel")!;
  const stage = (msg: string) => {
    console.info(`[client] ${msg}`);
    panel.textContent = msg;
  };
  stage("Загрузка данных…");
  const [rules, terrain, units, tech, sprites, ru] = await Promise.all([
    text("/data/rules.json"),
    text("/data/terrain.json"),
    text("/data/units.json"),
    text("/data/tech.json"),
    json<SpritesFile>("/data/sprites.json"),
    json<Record<string, string>>("/data/locale/ru.json"),
  ]);
  const base = `/data/scenarios/${SCENARIO}`;
  const [scenario, map, events] = await Promise.all([
    text(`${base}/scenario.json`),
    text(`${base}/map.json`),
    optionalText(`${base}/events.json`),
  ]);

  stage("Загрузка ядра…");
  await init();
  const core = new Core(rules, terrain, units, tech);
  const view = JSON.parse(core.scenarioView(scenario, map, events)) as ScenarioView;

  const app = new Application();
  await app.init({ resizeTo: window, background: 0x12110f, antialias: true });
  document.getElementById("app")!.appendChild(app.canvas);

  // Textures: terrain by type, unit sprites by tier, bridge objects (in parallel).
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
  const techOf = new Map(view.sides.map((s) => [s.id, s.tech]));
  const unitTex = new Map<number, Texture | null>();
  await Promise.all(
    view.units.map(async (u) => {
      const ref = pickSprite(sprites, u.class, u.nation, techOf.get(u.side) ?? {});
      unitTex.set(u.index, ref ? await load(ref.url) : null);
    }),
  );
  stage("Отрисовка…");
  const object = (role: string) => {
    const id = sprites.map_objects[role];
    return id ? load(`/assets/objects/${id}.png`).then((t) => t ?? undefined) : Promise.resolve(undefined);
  };

  const world = new Container();
  world.addChild(
    drawMap(view, { terrain: terrainTex, bridge: await object("bridge"), bridgeDestroyed: await object("bridge_destroyed") }, HEX_SIZE),
  );
  world.addChild(drawUnits(view.units, unitTex, HEX_SIZE));
  app.stage.addChild(world);

  // Start centred on the map.
  const xs = view.hexes.map((h) => hexCenter(h, HEX_SIZE));
  const cx = (Math.min(...xs.map((p) => p.x)) + Math.max(...xs.map((p) => p.x))) / 2;
  const cy = (Math.min(...xs.map((p) => p.y)) + Math.max(...xs.map((p) => p.y))) / 2;
  const scale = 0.6;
  new Camera(world, app.canvas, { scale, x: window.innerWidth / 2 - cx * scale, y: window.innerHeight / 2 - cy * scale });

  const issues = view.issues.length
    ? `<p class="bad">${ru["ui.validation.problems"] ?? "Problems"}: ${view.issues.length}</p><ul>${view.issues.map((i) => `<li>${i}</li>`).join("")}</ul>`
    : `<p class="ok">${ru["ui.validation.ok"] ?? "OK"}</p>`;
  panel.innerHTML = `<h1>${ru[`scenario.${SCENARIO}.name`] ?? SCENARIO}</h1>${issues}`;
}

main().catch((e: unknown) => {
  document.getElementById("panel")!.textContent = String(e);
  console.error(e);
});
