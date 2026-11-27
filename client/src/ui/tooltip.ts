// Small pop-up explanations (R-UI-04): every HUD element with a `title`
// (icons of layers, actions, top bar, card values) gets a styled bubble
// quickly on hover instead of the slow native one, and on touch by a long
// press (R-UI-04 «долгое нажатие — сведения»). The text stays in `title`:
// it is lifted for the time the bubble shows, then put back.

const SHOW_MS = 180;
const LONG_PRESS_MS = 450;
const TOUCH_HIDE_MS = 2500;
const GAP = 8;

/** Where to put the bubble: below the element, or above / to the side when
 * that would leave the viewport. Pure, for tests. */
export function placeTip(
  el: { left: number; top: number; right: number; bottom: number },
  tip: { width: number; height: number },
  view: { width: number; height: number },
): { left: number; top: number } {
  const cx = (el.left + el.right) / 2;
  let left = cx - tip.width / 2;
  let top = el.bottom + GAP;
  if (top + tip.height > view.height - GAP) top = el.top - GAP - tip.height;
  // A tall column at the right edge (layer buttons): to the left of it.
  if (el.right > view.width - 80 && el.bottom - el.top < 80) {
    left = el.left - GAP - tip.width;
    top = (el.top + el.bottom) / 2 - tip.height / 2;
  }
  left = Math.max(GAP, Math.min(left, view.width - GAP - tip.width));
  top = Math.max(GAP, Math.min(top, view.height - GAP - tip.height));
  return { left, top };
}

export function installTooltips(doc: Document = document): void {
  const bubble = doc.createElement("div");
  bubble.id = "tooltip";
  bubble.setAttribute("role", "tooltip");
  doc.body.appendChild(bubble);
  let owner: HTMLElement | null = null;
  let text = "";
  let timer: number | undefined;
  let watch: number | undefined;
  let pressed = false;

  const hide = () => {
    window.clearTimeout(timer);
    window.clearInterval(watch);
    bubble.classList.remove("on");
    if (owner && text && !owner.hasAttribute("title")) owner.setAttribute("title", text);
    owner = null;
    text = "";
  };
  const show = (el: HTMLElement) => {
    const t = el.getAttribute("title") ?? text;
    if (!t) return;
    text = t;
    el.removeAttribute("title"); // no native bubble on top of ours
    // An icon button: its name (aria-label) heads the explanation when they differ.
    const name = el.getAttribute("aria-label");
    bubble.textContent = "";
    if (name && name !== t && !t.startsWith(name)) {
      const head = doc.createElement("b");
      head.textContent = name;
      bubble.append(head, doc.createElement("br"));
    }
    bubble.append(t);
    bubble.classList.add("on");
    const r = el.getBoundingClientRect();
    const b = bubble.getBoundingClientRect();
    const p = placeTip(r, b, { width: window.innerWidth, height: window.innerHeight });
    bubble.style.left = `${p.left}px`;
    bubble.style.top = `${p.top}px`;
    // The HUD re-renders: a replaced element takes its bubble with it.
    window.clearInterval(watch);
    watch = window.setInterval(() => {
      if (!el.isConnected) hide();
    }, 300);
  };
  const target = (e: Event) => (e.target instanceof Element ? (e.target.closest("[title]") as HTMLElement | null) : null);

  doc.addEventListener("pointerover", (e) => {
    if (e.pointerType === "touch") return;
    const el = target(e);
    if (!el || el === owner) return;
    hide();
    owner = el;
    text = el.getAttribute("title") ?? "";
    timer = window.setTimeout(() => owner === el && show(el), SHOW_MS);
  });
  doc.addEventListener("pointerout", (e) => {
    if (e.pointerType === "touch" || !owner) return;
    const to = e.relatedTarget instanceof Node ? e.relatedTarget : null;
    if (to && owner.contains(to)) return;
    hide();
  });
  doc.addEventListener("pointerdown", (e) => {
    hide();
    if (e.pointerType !== "touch") return;
    const el = target(e);
    if (!el) return;
    owner = el;
    text = el.getAttribute("title") ?? "";
    pressed = false;
    timer = window.setTimeout(() => {
      if (owner !== el) return;
      pressed = true;
      show(el);
      window.setTimeout(() => owner === el && hide(), TOUCH_HIDE_MS);
    }, LONG_PRESS_MS);
  });
  doc.addEventListener("pointerup", (e) => {
    if (e.pointerType === "touch" && !pressed) {
      window.clearTimeout(timer);
      if (owner && text && !owner.hasAttribute("title")) owner.setAttribute("title", text);
    }
  });
  // A long press shows the explanation and does not also press the button.
  doc.addEventListener(
    "click",
    (e) => {
      if (pressed) {
        pressed = false;
        e.preventDefault();
        e.stopPropagation();
      }
    },
    true,
  );
  doc.addEventListener("scroll", hide, true);
  window.addEventListener("blur", hide);
}
