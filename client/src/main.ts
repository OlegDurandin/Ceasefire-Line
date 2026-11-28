// Stage 2: the client plays (hotseat) or edits (?mode=edit) a scenario on the rules core (WASM).

import { parseSettings, type Settings } from "./settings";
import { Application, Assets, Container, Graphics, Sprite, type Texture } from "pixi.js";

import { Editor } from "./editor/editor";
import type { MapDoc, ScenarioDoc } from "./editor/model";

import { Camera } from "./camera";
import { hexCenter, hexCorners, offsetToAxial } from "./hex";
import { hillshadePlacement, type HillshadeMeta } from "./hillshade";
import type { Rules } from "./explain";
import type { Facing } from "./facing";
import { decodeSave } from "./save";
import { parseFlag, type FlagBook } from "./render/flag";
import { Play } from "./play";
import { installTooltips } from "./ui/tooltip";
import { drawControl } from "./render/control";
import { drawMap, setGridActive } from "./render/map";
import { BakedMap, pickLevel, worldPerBakePx, type TilesMeta } from "./render/tiles";
import { drawUnits, expStars, sideColors, type Token } from "./render/units";
import { miniIndex, pickSprite, styledSprite, terrainTextureUrl, type MiniIndex, type SpritesFile, type UnitStyle } from "./sprites";
import type { ScenarioView } from "./view";
import { briefingWindow, showStory, type BriefingFile, type EventWindow, type ScenarioFacts } from "./history";
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
  installTooltips();
  const panel = document.getElementById("panel")!;
  const stage = (msg: string) => {
    console.info(`[client] ${msg}`);
    panel.textContent = msg;
    panel.style.display = "block";
  };
  stage("Загрузка данных…");
  const [rules, terrain, units, tech, spritesText, ruText, aiText, flagsText] = await Promise.all([
    text("/data/rules.json"),
    text("/data/terrain.json"),
    text("/data/units.json"),
    text("/data/tech.json"),
    text("/data/sprites.json"),
    text("/data/locale/ru.json"),
    text("/data/ai.json"),
    text("/data/flags.json"),
  ]);
  const sprites = JSON.parse(spritesText) as SpritesFile;
  // R-UI-03: flags of fictional nations, referenced by id from scenario.json.
  const flagBook = JSON.parse(flagsText) as FlagBook;
  // R-UI-03: miniatures by default (sprites.json), medallions with ?units=medallion or the U key.
  // The settings (gear button) remember the choice per browser.
  const settingsKey = "cl.settings";
  let stored: Settings = {};
  try {
    stored = parseSettings(localStorage.getItem(settingsKey));
  } catch {
    stored = {};
  }
  const storedStyle = stored.units;
  const unitParam = PARAMS.get("units") ?? storedStyle;
  const unitStyle: UnitStyle = unitParam === "medallion" || unitParam === "miniature" ? unitParam : (sprites.default_style ?? "medallion");
  const miniManifest = sprites.styles?.miniature?.manifest;
  const minis: MiniIndex = miniManifest
    ? miniIndex(JSON.parse((await optionalText(`/assets/${miniManifest}`)) ?? "[]") as { id: string; side: string; anchor: [number, number] }[])
    : new Map();
  const ru = JSON.parse(ruText) as Record<string, string>;
  const base = `/data/scenarios/${SCENARIO}`;
  const [scenario, map, events, briefingText] = await Promise.all([
    text(`${base}/scenario.json`),
    text(`${base}/map.json`),
    optionalText(`${base}/events.json`),
    optionalText(`${base}/briefing.json`),
  ]);
  // R-UI-07: briefing, event windows and reference keys (texts are locale keys).
  const briefing = briefingText ? (JSON.parse(briefingText) as BriefingFile) : null;
  const eventWindows = new Map<string, EventWindow>(
    events
      ? (JSON.parse(events) as { events: { id: string; important?: boolean; window?: EventWindow }[] }).events
          .filter((e) => e.important && e.window)
          .map((e) => [e.id, e.window!] as const)
      : [],
  );
  const scenarioSides = (JSON.parse(scenario) as { sides: { id: number; key: string; color?: string }[] }).sides;
  const sideKeys = scenarioSides.reduce<string[]>((acc, s) => {
    acc[s.id] = s.key;
    return acc;
  }, []);
  const colors = sideColors(scenarioSides);
  // R-UI-03: default facing of each side's models, towards the enemy.
  const sideFacing = scenarioSides.reduce<Facing[]>((acc, s) => {
    acc[s.id] = (s as { facing?: string }).facing === "right" ? "right" : "left";
    return acc;
  }, []);
  const scenarioMeta = JSON.parse(scenario) as { start_date?: string; start_turn?: number };

  stage("Загрузка ядра…");
  await init();
  const core = new Core(rules, terrain, units, tech);
  const terrainData = JSON.parse(terrain) as {
    road_ap: number;
    track_ap: number;
    terrain: { id: string; ap: number; defense_mult: number; max_entrench: number }[];
  };
  const view = JSON.parse(core.scenarioView(scenario, map, events)) as ScenarioView;

  const app = new Application();
  // Transparent: the page's dark cloth with a vignette shows around the map (index.html).
  await app.init({ resizeTo: window, backgroundAlpha: 0, antialias: true });
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
  // R-UI-04 minimap: the map's hexes, and the coarsest baked level (one tile) as its picture.
  const minX = Math.min(...xs.map((p) => p.x)) - HEX_SIZE;
  const minY = Math.min(...xs.map((p) => p.y)) - HEX_SIZE;
  const mapRect = { x: minX, y: minY, w: Math.max(...xs.map((p) => p.x)) + HEX_SIZE - minX, h: Math.max(...xs.map((p) => p.y)) + HEX_SIZE - minY };
  const top = tilesMeta?.levels[tilesMeta.levels.length - 1];
  const overview =
    tilesMeta && top && top.cols === 1 && top.rows === 1
      ? {
          url: `${base}/tiles/${top.z}/0_0.${tilesMeta.format}`,
          rect: {
            x: (tilesMeta.top_left_from_hex00_km[0] * HEX_SIZE) / tilesMeta.hex_radius_km,
            y: (tilesMeta.top_left_from_hex00_km[1] * HEX_SIZE) / tilesMeta.hex_radius_km,
            w: tilesMeta.width_px * worldPerBakePx(tilesMeta, HEX_SIZE),
            h: tilesMeta.height_px * worldPerBakePx(tilesMeta, HEX_SIZE),
          },
        }
      : null;

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
        const base = pickSprite(sprites, u.class, u.nation, v.sides.find((x) => x.id === u.side)?.tech ?? {}, scenarioMeta.start_date ? Number(scenarioMeta.start_date.slice(0, 4)) : undefined, u.model);
        const ref = base ? styledSprite(base, unitStyle, sprites, minis) : null;
        if (ref && !unitTextures.has(ref.url)) unitTextures.set(ref.url, await load(ref.url));
        tex.set(u.index, ref ? (unitTextures.get(ref.url) ?? null) : null);
        tokens.push({ key: u.index, side: u.side, col: u.col, row: u.row, label: u.hq_rating !== null ? `★${u.hq_rating}` : String(u.strength), stars: expStars(u.exp), dim: false, style: ref?.style ?? "medallion", anchor: ref?.anchor ?? [0.5, 0.5], facing: sideFacing[u.side] ?? "left" });
      }
      world.addChild(
        drawUnits(
          tokens,
          tex,
          HEX_SIZE,
          colors,
          scenarioSides.reduce<ReturnType<typeof parseFlag>[]>((acc, x) => {
            acc[x.id] = parseFlag((x as { flag?: unknown }).flag, flagBook);
            return acc;
          }, []),
        ),
      );
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
    // R-UI-05 / stage 3: a saved command log is replayed (deterministic core).
    const saveKey = `cl.save.${SCENARIO}`;
    let saved: string | null = null;
    try {
      saved = localStorage.getItem(saveKey);
    } catch {
      saved = null;
    }
    let game = new Game(core, scenario, map, events);
    let savedFacing = new Map<number, Facing>();
    let aiSide: number | null | undefined;
    let aiSeed: number | null = null;
    const save = saved ? decodeSave(saved) : null;
    if (save && save.commands.length) {
      try {
        const resumed = Game.replay(core, scenario, map, events, JSON.stringify(save.commands));
        const st = JSON.parse(resumed.state()) as { turn: number; active_side: number; outcome: unknown };
        const side = ru[`side.${sideKeys[st.active_side] ?? ""}`] ?? String(st.active_side);
        if (!st.outcome && (await askContinue(ru, st.turn, side))) {
          game = resumed;
          savedFacing = save.facing;
          aiSide = save.ai;
          aiSeed = save.aiSeed;
        }
      } catch (e) {
        console.warn("[client] save does not replay:", e);
        panel.style.display = "block";
        panel.textContent = ru["ui.saved_broken"] ?? "save ignored";
      }
    }
    // R-AI-01: a new game asks who plays (or `?vs=hotseat|<side key>`).
    const newGame = aiSide === undefined;
    if (aiSide === undefined) {
      const index = await optionalText("/data/scenarios/index.json");
      const scenarios = index ? ((JSON.parse(index) as { scenarios?: string[] }).scenarios ?? [SCENARIO]) : [SCENARIO];
      aiSide = await chooseMode(ru, sideKeys, PARAMS.get("vs"), scenarios);
    }
    // A new game draws the AI's seed; a resumed one keeps it (R-AI-01).
    if (aiSeed === null) aiSeed = crypto.getRandomValues(new Uint32Array(1))[0]!;
    game.setAi(aiText, scenario, aiSeed);
    devGame = game;
    const facts = JSON.parse(scenario) as ScenarioFacts;
    const sideLabel = (id: number) => ru[`side.${sideKeys[id] ?? ""}`] ?? String(id);
    const objectiveName = (hex: [number, number]) => ru[`place.${SCENARIO}.${hex[0]}_${hex[1]}`] ?? `[${hex[0]}, ${hex[1]}]`;
    const suddenDeath = (JSON.parse(rules) as { national_morale: { sudden_death_pct: number } }).national_morale.sudden_death_pct;
    const showBriefing = briefing
      ? () =>
          showStory(
            briefingWindow(briefing, facts, ru, {
              sideName: sideLabel,
              objectiveName,
              suddenDeath,
              role: aiSide === null || aiSide === undefined ? undefined : sideKeys[1 - aiSide],
            }),
          )
      : undefined;
    const stories = PARAMS.get("story") !== "0";
    const play = new Play(game, world, camera, sprites, ru, sideKeys, colors, HEX_SIZE, { terrain: terrainTex, bridge, bridgeDestroyed }, {
      rules: JSON.parse(rules) as Rules,
      maxEntrench: new Map(
        (JSON.parse(terrain) as { terrain: { id: string; max_entrench: number }[] }).terrain.map((t) => [t.id, t.max_entrench] as const),
      ),
      startDate: scenarioMeta.start_date,
      startTurn: scenarioMeta.start_turn ?? 1,
      suddenDeathPct: (JSON.parse(rules) as { national_morale: { sudden_death_pct: number } }).national_morale.sudden_death_pct,
      style: unitStyle,
      minis,
      saveKey,
      settingsKey,
      aiSpeed: stored.aiSpeed ?? "x1",
      sideFacing,
      savedFacing,
      aiSide,
      aiSeed,
      hexes: new Map(view.hexes.map((h) => [`${h.col},${h.row}`, h] as const)),
      roadHexes: new Set(view.roads.flatMap((e) => [e.a.join(","), e.b.join(",")])),
      trackHexes: new Set(view.tracks.flatMap((e) => [e.a.join(","), e.b.join(",")])),
      terrain: new Map(terrainData.terrain.map((t) => [t.id, t] as const)),
      roadAp: terrainData.road_ap,
      trackAp: terrainData.track_ap,
      terrainTexture: (t: string) => terrainTextureUrl(sprites, t),
      minimap: { world: mapRect, image: overview },
      flags: scenarioSides.reduce<ReturnType<typeof parseFlag>[]>((acc, s) => {
        acc[s.id] = parseFlag((s as { flag?: unknown }).flag, flagBook);
        return acc;
      }, []),
      classAp: new Map((JSON.parse(units) as { units: { id: string; ap?: number }[] }).units.filter((u) => u.ap !== undefined).map((u) => [u.id, u.ap!] as const)),
      history: {
        briefing: showBriefing,
        events: eventWindows,
        classInfo: new Map(
          (JSON.parse(units) as { units: { id: string; info?: string }[] }).units.filter((u) => u.info).map((u) => [u.id, u.info!] as const),
        ),
        modelInfo: (sprites as { model_info?: Record<string, string> }).model_info ?? {},
        windows: stories,
      },
    });
    const onCamera = camera.onChange;
    camera.onChange = (v) => {
      onCamera?.(v);
      play.onCamera();
    };
    await play.refresh();
    // The camera: the whole map in the area the HUD leaves free, panning limited to the map.
    const insets = () => {
      const r = (sel: string) => document.querySelector(sel)?.getBoundingClientRect();
      const tabs = r(".tb-tabs");
      const tools = r(".tb-tools");
      const row = r("#hud-row");
      return {
        top: Math.ceil(Math.max(tabs?.bottom ?? 0, r(".tb-left")?.bottom ?? 0)) + 4,
        right: tools ? Math.ceil(window.innerWidth - tools.left) + 4 : 0,
        bottom: row ? Math.ceil(window.innerHeight - row.top) + 4 : 0,
        left: 0,
      };
    };
    camera.setBounds(mapRect, insets());
    camera.fit();
    window.addEventListener("resize", () => camera.setBounds(mapRect, insets()));
    if (!view.issues.length) panel.style.display = "none";
    // R-UI-07: a new game opens with the briefing.
    if (newGame && stories && showBriefing) void showBriefing();
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

/**
 * R-AI-01: hotseat, or against the AI for one side. `vs` from the URL skips
 * the question: `hotseat`, or the key of the side the player takes.
 */
function chooseMode(ru: Record<string, string>, sideKeys: string[], vs: string | null, scenarios: string[] = [SCENARIO]): Promise<number | null> {
  const human = (key: string) => sideKeys.indexOf(key);
  if (vs === "hotseat") return Promise.resolve(null);
  if (vs && human(vs) >= 0 && sideKeys.length === 2) return Promise.resolve(1 - human(vs));
  const modal = document.getElementById("modal")!;
  const box = modal.querySelector(".panel")!;
  const sideName = (k: string) => ru[`side.${k}`] ?? k;
  // The scenario: another one reloads the page with `?scenario=<id>`.
  const pick =
    scenarios.length > 1
      ? `<div class="scenario-pick" role="tablist" aria-label="${ru["ui.scenario"] ?? "Scenario"}">${scenarios
          .map(
            (id) =>
              `<button data-scenario="${id}" role="tab" aria-selected="${id === SCENARIO}"${id === SCENARIO ? ' class="on"' : ""}>${ru[`scenario.${id}.name`] ?? id}</button>`,
          )
          .join("")}</div>`
      : "";
  box.innerHTML = `<h2>${ru["ui.mode.title"] ?? "New game"}</h2>${pick}<div class="menu">
    <button id="mode-hotseat">${ru["ui.mode.hotseat"] ?? "Hotseat"}</button>
    ${sideKeys
      .map((k, i) => `<button id="mode-play-${k}" data-ai="${sideKeys.length === 2 ? 1 - i : ""}">${(ru["ui.mode.vs_ai"] ?? "{side}").replace("{side}", sideName(k))}</button>`)
      .join("")}</div>`;
  modal.style.display = "flex";
  return new Promise((resolve) => {
    const done = (v: number | null) => {
      modal.style.display = "none";
      resolve(v);
    };
    document.getElementById("mode-hotseat")!.onclick = () => done(null);
    for (const b of box.querySelectorAll<HTMLButtonElement>("[data-scenario]")) {
      b.onclick = () => {
        if (b.dataset.scenario === SCENARIO) return;
        const url = new URL(location.href);
        url.searchParams.set("scenario", b.dataset.scenario!);
        url.searchParams.delete("vs");
        location.assign(url.toString());
      };
    }
    for (const b of box.querySelectorAll<HTMLButtonElement>("[data-ai]")) {
      b.onclick = () => done(b.dataset.ai === "" ? null : Number(b.dataset.ai));
    }
  });
}

/** «Continue the saved game?» in the modal; resolves with the choice. */
function askContinue(ru: Record<string, string>, turn: number, side: string): Promise<boolean> {
  const modal = document.getElementById("modal")!;
  const box = modal.querySelector(".panel")!;
  box.innerHTML = `<p>${(ru["ui.saved_game"] ?? "").replace("{turn}", String(turn)).replace("{side}", side)}</p>
    <button id="resume">${ru["ui.continue"] ?? "Continue"}</button> <button id="fresh">${ru["ui.new_game"] ?? "New game"}</button>`;
  modal.style.display = "flex";
  return new Promise((resolve) => {
    const done = (v: boolean) => {
      modal.style.display = "none";
      resolve(v);
    };
    document.getElementById("resume")!.onclick = () => done(true);
    document.getElementById("fresh")!.onclick = () => done(false);
  });
}

main().catch((e: unknown) => {
  const panel = document.getElementById("panel")!;
  panel.style.display = "block";
  panel.textContent = String(e);
  console.error(e);
});
