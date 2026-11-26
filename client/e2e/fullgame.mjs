// Stage 3 acceptance (GDD section 18): play a scenario to the end through
// the UI only — taps on units, hexes and targets, buttons and keys. The
// core's state (dev hook window.__cl) is read only to choose where to tap.
//
//   npm run dev                      # in another terminal
//   npm run e2e                      # E2E_URL, CHROME, E2E_SHOT to override
//
// Exits non-zero if the game does not reach its outcome or the page logs errors.
import puppeteer from "puppeteer-core";

const URL = process.env.E2E_URL ?? "http://localhost:5173/";
const CHROME = process.env.CHROME ?? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const out = process.env.E2E_SHOT;
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
const tapHex = async (c, r) => {
  const p = await page.evaluate((c, r) => window.__cl.screenOf(c, r), c, r);
  const hit = await page.evaluate((x, y) => document.elementFromPoint(x, y)?.tagName, p.x, p.y);
  if (hit !== "CANVAS") { stats.covered = (stats.covered ?? 0) + 1; stats.lastCover = hit + "@" + c + "," + r; return false; }
  await page.mouse.click(p.x, p.y); await wait(120); return true;
};
let rnd = 12345; const rand = (n) => { rnd ^= rnd << 13; rnd ^= rnd >>> 17; rnd ^= rnd << 5; return (rnd >>> 0) % n; };
const stats = { attacks: 0, moves: 0, phases: 0, offscreen: 0, noReach: 0 };
for (let guard = 0; guard < 60; guard++) {
  const s = await state();
  if (s.outcome) break;
  const mine = s.units.filter(u => u.side === s.active_side).map(u => u.id);
  for (const id of mine) {
    let st = await state(); if (st.outcome) break;
    const u = st.units.find(x => x.id === id); if (!u) continue;
    const targets = await page.evaluate((id) => JSON.parse(window.__cl.game.targets(id)), id);
    if (!(await tapHex(u.col, u.row))) { stats.offscreen++; continue; }
    if (targets.length && rand(3) > 0) {
      const t = st.units.find(x => x.id === targets[0].target);
      await tapHex(t.col, t.row); await tapHex(t.col, t.row); stats.attacks++;
      if (rand(2)) { await page.evaluate(() => { const b = document.getElementById("advance"); if (b && b.offsetParent) b.click(); }); await wait(150); }
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
  await page.keyboard.press("Enter"); await wait(200);
  await page.evaluate(() => { const b = document.getElementById("end-anyway"); if (b && b.offsetParent) b.click(); }); await wait(200);
  stats.phases++;
}
const s = await state();
console.log("outcome:", JSON.stringify(s.outcome), "turn", s.turn, JSON.stringify(stats));
console.log("modal:", await page.evaluate(() => document.querySelector("#modal")?.style.display + " | " + document.querySelector("#modal .panel")?.innerText.replace(/\n/g, " / ")));
if (out) await page.screenshot({ path: out });
console.log("errors:", JSON.stringify(errs));
await browser.close();
if (!s.outcome || errs.length) {
  console.error("FAILED: the game did not reach its outcome cleanly");
  process.exit(1);
}
