// Stage 2: the client plays (hotseat) or edits (?mode=edit) a scenario on the rules core (WASM).

import { Application, Assets, Container, Graphics, Sprite, type Texture } from "pixi.js";

import { Editor } from "./editor/editor";
import type { MapDoc, ScenarioDoc } from "./editor/model";

import { Camera } from "./camera";
import { hexCenter, hexCorners, offsetToAxial } from "./hex";
import { hillshadePlacement, type HillshadeMeta } from "./hillshade";
import { Play } from "./play";
import { drawControl } from "./render/control";
import { drawMap, setGridActive } from "./render/map";
import { BakedMap, pickLevel, type TilesMeta } from "./render/tiles";
import { drawUnits, expStars, sideColors, type Token } from "./render/units";
import { pickSprite, terrainTextureUrl, type SpritesFile } from "./sprites";
import type { ScenarioView } from "./view";
import init, { Core, Game } from "./wasm/ceasefire_wasm.js";

/** Hex corner radius in world pixels. */
const HEX_SIZE = 48;
/** How strongly the relief shading darkens the terrain textures. */
const HILLSHADE_ALPHA = 0.7;
const PARAMS = new URLSearchParams(location.search);
const SCENARIO = PARAMS.get("scenario") ?? "golan67";
const EDIT = PARAMS.get("mode") === "edit";
/** `?baked=0` draws the live map even when baked tiles exist (comparisons). */
const BAKED = PARAMS.get("baked") !== "0";

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
  const scenarioSides = (JSON.parse(scenario) as { sides: { id: number; key: string; color?: string }[] }).sides;
  const sideKeys = scenarioSides.reduce<string[]>((acc, s) => {
    acc[s.id] = s.key;
    return acc;
  }, []);
  const colors = sideColors(scenarioSides);

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
    Object.keys(sprites.terrain).map(async (t) => {
      const url = terrainTextureUrl(sprites, t);
      const tex = url ? await load(url) : null;
      if (tex) terrainTex.set(t, tex);
    }),
  );
  const object = async (role: string) => {
    const id = sprites.map_objects[role];
    return id ? ((await load(`/assets/objects/${id}.png`)) ?? undefined) : undefined;
  };

  const bridge = await object("bridge");
  const bridgeDestroyed = await object("bridge_destroyed");
  // Relief shading only for generated maps (map.json geo.hillshade).
  const hasShade = !!(JSON.parse(map) as { geo?: { hillshade?: string } }).geo?.hillshade;
  const shadeMeta = hasShade ? await optionalText(`${base}/hillshade.json`) : undefined;
  const meta = shadeMeta ? (JSON.parse(shadeMeta) as HillshadeMeta) : null;
  const shadeTex = meta ? await load(`${base}/${meta.png}`) : null;

  // Painted map from mapgen bake (play only: the editor shows edits live).
  const tilesText = !EDIT && BAKED ? await optionalText(`${base}/tiles.json`) : undefined;
  const tilesMeta = tilesText ? (JSON.parse(tilesText) as TilesMeta) : null;
  const baked = tilesMeta ? new BakedMap(tilesMeta, base, HEX_SIZE) : null;

  /** Map layer from a core view; relief shading over the textures, under the grid. */
  const buildMap = (v: ScenarioView) => {
    // In play, bridges are a live layer of Play (their state changes).
    const layer = drawMap(v, { terrain: terrainTex, bridge, bridgeDestroyed }, HEX_SIZE, app.renderer, !!baked, EDIT);
    if (baked) {
      // Clipped to the hexes: the map is where the rules are.
      const clip = new Graphics();
      for (const h of v.hexes) clip.poly(hexCorners(hexCenter(h, HEX_SIZE), HEX_SIZE).flatMap((p) => [p.x, p.y])).fill(0xffffff);
      layer.addChildAt(baked.container, 0);
      layer.addChild(clip);
      baked.container.mask = clip;
    } else if (meta && shadeTex) {
      const shade = new Sprite(shadeTex);
      const p = hillshadePlacement(meta, HEX_SIZE);
      shade.position.set(p.x, p.y);
      shade.scale.set(p.scale);
      shade.blendMode = "multiply";
      shade.alpha = HILLSHADE_ALPHA;
      layer.addChildAt(shade, 1);
    }
    return layer;
  };

  const world = new Container();
  world.addChild(buildMap(view));
  app.stage.addChild(world);

  const xs = view.hexes.map((h) => hexCenter(h, HEX_SIZE));
  const cx = (Math.min(...xs.map((p) => p.x)) + Math.max(...xs.map((p) => p.x))) / 2;
  const cy = (Math.min(...xs.map((p) => p.y)) + Math.max(...xs.map((p) => p.y))) / 2;
  const scale = 0.6;
  const camera = new Camera(world, app.canvas, { scale, x: window.innerWidth / 2 - cx * scale, y: window.innerHeight / 2 - cy * scale });
  if (baked && tilesMeta) {
    stage("Загрузка карты…");
    const level = (s: number) => pickLevel(tilesMeta, HEX_SIZE, s);
    await baked.show(level(scale));
    camera.onChange = (v) => void baked.show(level(v.scale));
  }

  let devGame: Game | null = null;
  if (EDIT) {
    // Units drawn from the scenario document, sprites by tier as in play.
    const unitTextures = new Map<string, Texture | null>();
    const render = async (v: ScenarioView) => {
      world.removeChildren().forEach((c) => c.destroy({ children: true }));
      world.addChild(buildMap(v));
      // The editor works hex by hex: grid always shown.
      setGridActive(world, true);
      const owners = new Map<string, number>();
      for (const h of v.hexes) if (h.owner !== null) owners.set(`${h.col},${h.row}`, h.owner);
      const control = new Graphics();
      drawControl(control, HEX_SIZE, owners, colors, v.neutral);
      world.addChild(control);
      const tex = new Map<number, Texture | null>();
      const tokens: Token[] = [];
      for (const u of v.units) {
        const ref = pickSprite(sprites, u.class, u.nation, v.sides.find((x) => x.id === u.side)?.tech ?? {});
        if (ref && !unitTextures.has(ref.url)) unitTextures.set(ref.url, await load(ref.url));
        tex.set(u.index, ref ? (unitTextures.get(ref.url) ?? null) : null);
        tokens.push({ key: u.index, side: u.side, col: u.col, row: u.row, label: u.hq_rating !== null ? `★${u.hq_rating}` : String(u.strength), stars: expStars(u.exp), dim: false });
      }
      world.addChild(drawUnits(tokens, tex, HEX_SIZE, colors));
    };
    const terrainIds = (JSON.parse(terrain) as { terrain: { id: string }[] }).terrain.map((t) => t.id);
    const classIds = (JSON.parse(units) as { units: { id: string; target_class: string }[] }).units
      .filter((u) => u.target_class === "soft" || u.target_class === "hard")
      .map((u) => u.id);
    const editor = new Editor(
      { render, world, camera, hexSize: HEX_SIZE, view: (scn, m) => JSON.parse(core.scenarioView(scn, m, events)) as ScenarioView },
      JSON.parse(map) as MapDoc,
      JSON.parse(scenario) as ScenarioDoc,
      SCENARIO,
      ru,
      terrainIds,
      classIds,
    );
    await editor.refresh();
  } else {
    // Scenario validation stays visible only when something is wrong.
    if (view.issues.length) {
      panel.innerHTML = `<p class="bad">${ru["ui.validation.problems"] ?? "Problems"}: ${view.issues.length}</p><ul>${view.issues.map((i) => `<li>${JSON.stringify(i)}</li>`).join("")}</ul>`;
    } else {
      panel.style.display = "none";
    }
    stage("Ход…");
    const game = new Game(core, scenario, map, events);
    devGame = game;
    const play = new Play(game, world, camera, sprites, ru, sideKeys, colors, HEX_SIZE, { terrain: terrainTex, bridge, bridgeDestroyed });
    await play.refresh();
    if (!view.issues.length) panel.style.display = "none";
  }

  // Dev-only hook for end-to-end checks: hex → screen coordinates.
  if (import.meta.env.DEV) {
    (window as unknown as { __cl: unknown }).__cl = {
      game: devGame,
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
