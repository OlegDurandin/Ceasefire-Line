// Stage 4, step 3: a whole game against the AI through the interface. The
// player takes Israel (`?vs=isr`), moves and attacks by taps; the AI plays
// Syria's phases by itself. Checks: the game reaches its outcome, the AI
// moved or attacked, input is blocked during its phase, no page errors.
//
//   npm run dev                      # in another terminal
//   npm run e2e:ai                   # E2E_URL, CHROME to override
import puppeteer from "puppeteer-core";

const BASE = process.env.E2E_URL ?? "http://localhost:5173/";
const CHROME = process.env.CHROME ?? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const browser = await puppeteer.launch({ executablePath: CHROME, headless: true, args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader"] });
const page = await browser.newPage();
const errs = []; page.on("pageerror", (e) => errs.push(String(e))); page.on("console", (m) => m.type() === "error" && errs.push(m.text()));
await page.setViewport({ width: 1600, height: 1400 });
await page.goto(BASE, { waitUntil: "networkidle0", timeout: 60000 });
await page.evaluate(() => localStorage.clear());
await page.goto(`${BASE}?vs=isr`, { waitUntil: "networkidle0", timeout: 60000 });
await page.waitForFunction(() => document.getElementById("top")?.textContent?.includes("Ход"));
const wait = (ms) => new Promise((r) => setTimeout(r, ms));
const state = () => page.evaluate(() => JSON.parse(window.__cl.game.state()));
const HUMAN = 0;
const startSides = new Map((await state()).units.map((u) => [u.id, u.side]));
const tapHex = async (c, r) => {
  const p = await page.evaluate((c, r) => window.__cl.screenOf(c, r), c, r);
  const hit = await page.evaluate((x, y) => document.elementFromPoint(x, y)?.tagName, p.x, p.y);
  if (hit !== "CANVAS") return false;
  await page.mouse.click(p.x, p.y); await wait(120); return true;
};
const stats = { phases: 0, aiPhases: 0, blocked: true };
for (let guard = 0; guard < 40; guard++) {
  let s = await state();
  if (s.outcome) break;
  if (s.active_side !== HUMAN) {
    // The AI's phase: a tap must not select anything, then wait for it to finish.
    const u = s.units.find((x) => x.side !== HUMAN);
    if (u) { await tapHex(u.col, u.row); stats.blocked &&= await page.evaluate(() => !document.querySelector("#attack .aw-card.left .aw-title") || !!document.querySelector("#attack .aw.hexmode")); }
    await page.waitForFunction((h) => { const st = JSON.parse(window.__cl.game.state()); return st.outcome || st.active_side === h; }, { timeout: 60000, polling: 200 }, HUMAN);
    stats.aiPhases++;
    await wait(300);
    // R-UI-04: back to the player, the «Итоги фазы противника» window opens; close it.
    const summary = await page.evaluate(() => {
      const close = document.getElementById("m-close");
      const open = document.getElementById("modal")?.style.display === "flex" && !!close;
      if (open) close.click();
      return open;
    });
    stats.summaries = (stats.summaries ?? 0) + (summary ? 1 : 0);
    await wait(150);
    continue;
  }
  for (const id of s.units.filter((u) => u.side === HUMAN).map((u) => u.id)) {
    s = await state(); if (s.outcome || s.active_side !== HUMAN) break;
    const u = s.units.find((x) => x.id === id); if (!u) continue;
    if (!(await tapHex(u.col, u.row))) continue;
    const targets = (await page.evaluate((id) => JSON.parse(window.__cl.game.targets(id)), id)).filter((t) => t.kind === "attack");
    if (targets.length) {
      const t = s.units.find((x) => x.id === targets[0].target);
      await tapHex(t.col, t.row); await tapHex(t.col, t.row); await wait(150);
      continue;
    }
    const reach = await page.evaluate((id) => JSON.parse(window.__cl.game.reachable(id)), id);
    if (reach.length) await tapHex(reach[Math.floor(reach.length / 2)].col, reach[Math.floor(reach.length / 2)].row);
  }
  await page.keyboard.press("Escape");
  await page.keyboard.press("Enter"); await wait(200);
  await page.evaluate(() => { const b = document.getElementById("end-anyway"); if (b && b.offsetParent) b.click(); }); await wait(300);
  stats.phases++;
}
const s = await state();
const log = await page.evaluate(() => JSON.parse(window.__cl.game.commands()));
const aiActs = log.filter((c) => (c.type === "move" || c.type === "attack" || c.type === "bombard") && startSides.get(c.unit) === 1 - HUMAN).length;
const saved = await page.evaluate(() => JSON.parse(localStorage.getItem("cl.save.golan67") ?? "null")?.ai ?? null);
console.log("outcome:", JSON.stringify(s.outcome), "turn", s.turn, JSON.stringify(stats), "AI commands:", aiActs);
console.log("errors:", JSON.stringify(errs));
await browser.close();
if (!s.outcome || errs.length || aiActs === 0 || !stats.blocked || stats.aiPhases === 0 || !stats.summaries) {
  console.error("FAILED:", JSON.stringify({ outcome: !!s.outcome, aiActs, blocked: stats.blocked, errors: errs.length, saved }));
  process.exit(1);
}
