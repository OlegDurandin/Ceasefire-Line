// Stroke icons of the approved HUD mock (components «Верхняя панель» and
// «Панель действий»), 24×24 view box; the colour comes from `currentColor`.

const PATHS: Record<string, string> = {
  supply: '<path d="M3 8l9-4 9 4-9 4-9-4z"></path><path d="M3 8v8l9 4 9-4V8"></path>',
  zoc: '<path d="M7 3h10l5 9-5 9H7l-5-9z"></path>',
  hq: '<path d="M5 21V4h12l-3 4 3 4H5"></path>',
  umbrella: '<path d="M3 13a9 9 0 0118 0z"></path><path d="M12 13v7"></path>',
  trail: '<path d="M4 18c3 0 4-4 7-4s3 3 6 3"></path><path d="M15 6l4 2-2 4"></path><circle cx="4" cy="18" r="1.6"></circle>',
  intent: '<circle cx="12" cy="12" r="7"></circle><circle cx="12" cy="12" r="2.5"></circle><path d="M12 2v3M12 19v3M2 12h3M19 12h3"></path>',
  objectives: '<path d="M12 3l2.6 5.6 6.1.6-4.6 4.1 1.3 6-5.4-3.1-5.4 3.1 1.3-6L3.3 9.2l6.1-.6z"></path>',
  settings: '<circle cx="12" cy="12" r="3.2"></circle><path d="M12 2v3M12 19v3M2 12h3M19 12h3M5 5l2 2M17 17l2 2M5 19l2-2M17 7l2-2"></path>',
  help: '<circle cx="12" cy="12" r="9"></circle><path d="M9.5 9.5a2.5 2.5 0 015 0c0 2-2.5 2-2.5 4M12 17h.01"></path>',
  save: '<path d="M5 3h11l3 3v15H5z"></path><path d="M8 3v6h8V3M8 21v-7h8v7"></path>',
  menu: '<path d="M12 3v9"></path><path d="M6.3 6.3a8 8 0 1011.4 0"></path>',
  end: '<path d="M5 12h11M12 6l6 6-6 6"></path>',
  prev: '<path d="M15 5l-7 7 7 7"></path>',
  next: '<path d="M9 5l7 7-7 7"></path>',
  reinforce: '<path d="M12 5v14M5 12h14"></path>',
  elite: '<path d="M9 7v10M4 12h10"></path><path d="M18 3l1.2 2.5 2.8.3-2 1.9.5 2.8-2.5-1.4-2.5 1.4.5-2.8-2-1.9 2.8-.3z"></path>',
  transfer: '<path d="M4 8h14l-3-3M20 16H6l3 3"></path>',
  pin: '<path d="M9 3h6l-1 6 3 3H7l3-3z"></path><path d="M12 12v9"></path>',
  bombard: '<circle cx="12" cy="12" r="7"></circle><path d="M12 2v5M12 17v5M2 12h5M17 12h5"></path>',
  bridge: '<path d="M2 15c4-6 16-6 20 0"></path><path d="M2 15h20M7 11v4M12 9.5V15M17 11v4"></path>',
  heli: '<path d="M3 6h18M12 6v4"></path><path d="M6 14a6 4 0 0012 0 6 4 0 00-12 0zM18 14h4M8 18l-1 3M16 18l1 3"></path>',
  undo: '<path d="M9 6L4 11l5 5"></path><path d="M4 11h10a5 5 0 010 10h-3"></path>',
  info: '<path d="M12 11v7M12 6.5v.5"></path>',
};

export function icon(name: string, size = 18, stroke = 1.8): string {
  return `<svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${PATHS[name] ?? ""}</svg>`;
}
