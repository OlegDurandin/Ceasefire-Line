// Stage 3 acceptance (GDD section 18): play a scenario to the end through
// the UI only — taps on units, hexes and targets, the HUD's buttons, tabs and
// keys. Every scenario command is given through the interface at least once
// (action bar, tabs, keys); the core's state (dev hook window.__cl) is read
// only to choose where to tap and, at the end, to check the command log.
//
//   npm run dev                      # in another terminal
//   npm run e2e                      # E2E_URL, CHROME, E2E_SHOTS (a directory) to override
//
// Exits non-zero if the game does not reach its outcome, a command the
// scenario allows was never given, a HUD check fails, or the page logs errors.
import puppeteer from "puppeteer-core";

const URL = process.env.E2E_URL ?? "http://localhost:5173/";
const CHROME = process.env.CHROME ?? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const shots = process.env.E2E_SHOTS;
const browser = await puppeteer.launch({ executablePath: CHROME, headless: true, args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader"] });
const page = await browser.newPage();
const errs = []; page.on("pageerror", (e) => errs.push(String(e))); page.on("console", (m) => m.type() === "error" && errs.push(m.text()));
await page.setViewport({ width: 1600, height: 1400 });
await page.goto(URL, { waitUntil: "networkidle0", timeout: 60000 });
await page.evaluate(() => localStorage.clear());
await page.reload({ waitUntil: "networkidle0" });
await page.waitForFunction(() => document.getElementById("top")?.textContent?.includes("Ход"));
await new Promise(r => setTimeout(r, 600));
const wait = (ms) => new Promise(r => setTimeout(r, ms));
const state = () => page.evaluate(() => JSON.parse(window.__cl.game.state()));
const shot = async (name) => { if (shots) await page.screenshot({ path: `${shots}/${name}.png` }); };
const stats = { attacks: 0, moves: 0, phases: 0, offscreen: 0, noReach: 0 };
/** HUD checks: name → passed. */
const ui = {};
const check = (name, ok) => { ui[name] = ui[name] || !!ok; };
// Shown on screen (fixed panels have no offsetParent, so the box decides).
const visible = (sel) => page.evaluate((sel) => { const e = document.querySelector(sel); if (!e) return false; const r = e.getBoundingClientRect(); return r.width > 0 && r.height > 0 && getComputedStyle(e).visibility !== "hidden"; }, sel);
const click = async (sel) => { if (!(await visible(sel))) return false; await page.click(sel); await wait(150); return true; };
const tapHex = async (c, r) => {
  const p = await page.evaluate((c, r) => window.__cl.screenOf(c, r), c, r);
  const hit = await page.evaluate((x, y) => document.elementFromPoint(x, y)?.tagName, p.x, p.y);
  if (hit !== "CANVAS") { stats.covered = (stats.covered ?? 0) + 1; stats.lastCover = hit + "@" + c + "," + r; return false; }
  await page.mouse.click(p.x, p.y); await wait(120); return true;
};
const enabled = (a) => page.evaluate((a) => document.querySelector(`[data-action="${a}"]`)?.getAttribute("aria-disabled") === "false", a);
const selectUnit = async (u) => { await page.keyboard.press("Escape"); return tapHex(u.col, u.row); };
let rnd = Number(process.env.E2E_SEED ?? 12345); const rand = (n) => { rnd ^= rnd << 13; rnd ^= rnd >>> 17; rnd ^= rnd << 5; return (rnd >>> 0) % n; };

/** Once: tabs, layers, settings, help, save, menu, minimap, «i», N. */
async function hudTour() {
  for (const tab of ["purchases", "reinforcements", "tech", "reports"]) {
    await click(`[data-tab="${tab}"]`);
    check(`tab ${tab}`, await visible("#panel"));
  }
  await click('[data-tab="map"]');
  check("tab map hides the panel", !(await visible("#panel")));
  for (const l of ["supply", "zoc", "hq", "umbrella", "objectives"]) {
    const on = () => page.evaluate((l) => document.getElementById(`layer-${l}`)?.getAttribute("aria-pressed"), l);
    const before = await on(); await click(`#layer-${l}`); const after = await on(); await click(`#layer-${l}`);
    check(`layer ${l}`, before !== after);
  }
  await click("#settings");
  await page.click('#modal input[value="medallion"]'); await wait(200);
  await page.click('#modal input[value="miniature"]'); await wait(200);
  check("settings", await page.evaluate(() => JSON.parse(localStorage.getItem("cl.settings") ?? "{}").units === "miniature"));
  await click("#m-close");
  await click("#help"); check("help", (await page.evaluate(() => document.querySelector("#modal .panel")?.innerText ?? "")).includes("Enter")); await click("#m-close");
  await click("#save-game"); check("save", await page.evaluate(() => !!localStorage.getItem("cl.save.golan67")));
  await click("#to-menu"); check("menu", await visible("#m-continue")); await click("#m-continue");
  const cam = () => page.evaluate(() => window.__cl.screenOf(0, 0));
  const before = await cam();
  const mm = await page.evaluate(() => { const r = document.querySelector("canvas.mm")?.getBoundingClientRect(); return r && r.width ? { x: r.x + r.width * 0.85, y: r.y + r.height * 0.85 } : null; });
  if (mm) { await page.mouse.click(mm.x, mm.y); await wait(200); }
  const after = await cam();
  check("minimap moves the camera", mm && (before.x !== after.x || before.y !== after.y));
  // back to the front: N selects and centres the next unit
  await page.keyboard.press("n"); await wait(200);
  check("N selects a unit", await page.evaluate(() => !!document.querySelector("#attack .aw-card.left .aw-title")));
  await page.keyboard.press("i"); await wait(150);
  check("i: hex mode", await page.evaluate(() => !!document.querySelector("#attack .aw.hexmode")));
  await page.keyboard.press("i"); await wait(150);
  check("i: back to the unit", await page.evaluate(() => !document.querySelector("#attack .aw.hexmode")));
  await page.keyboard.press("Escape");
}

/** Commands of the action bar and tabs, each tried until it has been given once. */
const done = new Set();
async function orders() {
  const s = await state();
  const mine = s.units.filter(u => u.side === s.active_side);
  // R-TECH-02: a boost from the «Технологии» tab.
  if (!done.has("buy_tech_boost")) {
    await click('[data-tab="tech"]');
    const b = await page.$('#panel button[data-cmd*="buy_tech_boost"]:not([disabled])');
    if (b) { await b.click(); await wait(200); done.add("buy_tech_boost"); }
    await click('[data-tab="map"]');
  }
  // R-ECO-02: plain and elite reinforcement of a damaged unit.
  for (const kind of ["reinforce", "elite"]) {
    if (done.has(kind)) continue;
    for (const u of mine.filter(u => u.strength < 10 && u.hq_rating === null)) {
      if (!(await selectUnit(u))) continue;
      if (await enabled(kind)) { await click(`[data-action="${kind}"]`); await wait(200); done.add(kind); break; }
    }
  }
  // R-HQ-01: pin to the HQ through the list.
  if (!done.has("pin_to_hq")) {
    const u = mine.find(u => u.hq_rating === null);
    if (u && (await selectUnit(u)) && (await enabled("pin"))) {
      await click('[data-action="pin"]');
      // The first choice is «автоматически»; the next ones are the HQs.
      const b = (await page.$$("#combat .pop-list button[data-cmd]"))[1];
      if (b) { await b.click(); await wait(200); done.add("pin_to_hq"); }
    }
  }
  // R-MOVE-05: transfer to a blue hex.
  if (!done.has("transfer")) {
    for (const u of mine) {
      if (!(await selectUnit(u)) || !(await enabled("transfer"))) continue;
      await click('[data-action="transfer"]');
      const hexes = await page.evaluate((id) => JSON.parse(window.__cl.game.transferTargets(id)).hexes, u.id);
      for (const [c, r] of hexes) {
        if (await tapHex(c, r)) {
          const moved = (await state()).units.find(x => x.id === u.id);
          if (moved && (moved.col !== u.col || moved.row !== u.row)) { done.add("transfer"); }
          break;
        }
      }
      if (done.has("transfer")) break;
    }
  }
  // R-UI-05: undo the last movement.
  if (!done.has("undo")) {
    for (const u of (await state()).units.filter(u => u.side === s.active_side && u.can_move && u.ap_halves > 0)) {
      const reach = await page.evaluate((id) => JSON.parse(window.__cl.game.reachable(id)), u.id);
      if (!reach.length || !(await selectUnit(u))) continue;
      const h = reach[0];
      if (!(await tapHex(h.col, h.row))) continue;
      await page.keyboard.down("Control"); await page.keyboard.press("z"); await page.keyboard.up("Control"); await wait(250);
      const back = (await state()).units.find(x => x.id === u.id);
      check("undo returns the unit", back && back.col === u.col && back.row === u.row);
      done.add("undo");
      break;
    }
  }
  // R-COMBAT-04: bombardment of a unit, and of a bridge, from the action bar.
  for (const u of mine.filter(u => u.class === "artillery")) {
    if (!(await selectUnit(u)) || !(await enabled("bombard"))) continue;
    await click('[data-action="bombard"]');
    const bridge = !done.has("bombard_bridge") && done.has("bombard") ? await page.$('#combat button[data-cmd*="bombard_bridge"]') : null;
    if (bridge) { await bridge.click(); await wait(250); done.add("bombard_bridge"); continue; }
    const aim = await page.$("#combat button[data-aim]");
    if (aim) { await aim.click(); await wait(200); }
    if (await click("#strike")) { done.add("bombard"); await wait(1300); }
    else await click("#pop-close");
  }
  await page.keyboard.press("Escape");
}

await hudTour();
await shot("01-start");
for (let guard = 0; guard < 60; guard++) {
  const s = await state();
  if (s.outcome) break;
  await orders();
  if (stats.phases === 1) await shot("02-syria");
  const mine = s.units.filter(u => u.side === s.active_side).map(u => u.id);
  for (const id of mine) {
    let st = await state(); if (st.outcome) break;
    const u = st.units.find(x => x.id === id); if (!u) continue;
    const targets = (await page.evaluate((id) => JSON.parse(window.__cl.game.targets(id)), id)).filter(t => t.kind === "attack");
    if (!(await tapHex(u.col, u.row))) { stats.offscreen++; continue; }
    if (targets.length && rand(3) > 0) {
      // The target with the largest expected losses (more retreats, so advances happen too).
      const best = await page.evaluate((id, ts) => ts.map((t) => ({ t: t.target, e: JSON.parse(window.__cl.game.forecast(id, t.target)).forecast?.defender_losses.expected ?? 0 })).sort((a, b) => b.e - a.e)[0].t, id, targets);
      const t = st.units.find(x => x.id === best);
      await tapHex(t.col, t.row); await tapHex(t.col, t.row); stats.attacks++;
      if ((await state()).pending_advance) stats.pending = (stats.pending ?? 0) + 1;
      if (stats.attacks === 1) await shot("03-result");
      await wait(150);
      if (await click("#advance")) stats.advances = (stats.advances ?? 0) + 1;
      continue;
    }
    const reach = await page.evaluate((id) => JSON.parse(window.__cl.game.reachable(id)), id);
    if (!reach.length) stats.noReach++;
    if (reach.length && rand(4) > 0) {
      const h = reach[rand(reach.length)];
      const before = (await state()).units.find(x => x.id === id);
      await tapHex(h.col, h.row);
      const after = (await state()).units.find(x => x.id === id);
      if (after && (after.col !== before.col || after.row !== before.row)) stats.moves++; else stats.moveFail = (stats.moveFail ?? 0) + 1;
    }
  }
  if ((await state()).outcome) break;
  await page.keyboard.press("Escape");
  // R-UI-04: the round red button (Enter works the same).
  if (stats.phases % 2) await click("#end-phase"); else await page.keyboard.press("Enter");
  await wait(200);
  await page.evaluate(() => { const b = document.getElementById("end-anyway"); if (b && b.offsetParent) b.click(); }); await wait(200);
  stats.phases++;
}
const s = await state();
await shot("04-outcome");
// Every command the scenario allows was given through the interface.
const used = new Set(await page.evaluate(() => JSON.parse(window.__cl.game.commands()).map((c) => (c.type === "reinforce" && c.elite ? "reinforce elite" : c.type))));
const classes = new Set(s.units.map(u => u.class));
// An advance is offered only when a defender retreats or dies (dice): required only if it was offered.
const wanted = ["move", "attack", "bombard", "bombard_bridge", "reinforce", "reinforce elite", "transfer", "pin_to_hq", "buy_tech_boost", "end_phase", ...(stats.pending ? ["advance"] : [])];
const missing = wanted.filter(c => !used.has(c));
console.log("outcome:", JSON.stringify(s.outcome), "turn", s.turn, JSON.stringify(stats));
console.log("commands given:", [...used].sort().join(", "));
console.log("not in this scenario:", [classes.has("engineer") ? null : "build_bridge (no engineers)", "purchase (no force limits)"].filter(Boolean).join("; "));
console.log("HUD checks:", Object.entries(ui).map(([k, v]) => `${v ? "ok" : "FAIL"} ${k}`).join(" · "));
console.log("modal:", await page.evaluate(() => document.querySelector("#modal")?.style.display + " | " + document.querySelector("#modal .panel")?.innerText.replace(/\n/g, " / ")));
console.log("errors:", JSON.stringify(errs));
await browser.close();
const failedUi = Object.entries(ui).filter(([, v]) => !v).map(([k]) => k);
if (!s.outcome || errs.length || missing.length || failedUi.length) {
  console.error("FAILED:", JSON.stringify({ outcome: !!s.outcome, missing, failedUi, errors: errs.length }));
  process.exit(1);
}
