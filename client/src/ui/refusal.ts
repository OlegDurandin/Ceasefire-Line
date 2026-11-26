// Why the core refuses a command, in the player's language. The core's
// `Game.check` returns the rule error's text (English, `RuleError` Display);
// the family and variant name map to `refusal.<family>.<Variant>` in the
// locale. Unknown texts are shown as they are, so nothing is ever hidden.

type Locale = Record<string, string>;

const FAMILIES: [RegExp, string][] = [
  [/^illegal economy action: (\w+)/, "economy"],
  [/^illegal strategic transfer of unit \d+: (\w+)/, "transfer"],
  [/^illegal move of unit \d+: (\w+)/, "move"],
  [/^illegal attack by unit \d+: (\w+)/, "attack"],
  [/^illegal bridge action by unit \d+: (\w+)/, "bridge"],
];

const WHOLE: [RegExp, string][] = [
  [/^the game is over/, "refusal.game_over"],
  [/^unit \d+ does not belong to the active side/, "refusal.not_your_unit"],
  [/^unit \d+ cannot be pinned to \d+/, "refusal.pin"],
  [/^unit \d+ is in a combat/, "refusal.in_combat"],
  [/^combats waiting for a result/, "refusal.combats_pending"],
];

/** Locale key for a core refusal, or null if the text is not recognised. */
export function refusalKey(raw: string): string | null {
  for (const [re, family] of FAMILIES) {
    const m = re.exec(raw);
    if (m) return `refusal.${family}.${m[1]}`;
  }
  for (const [re, key] of WHOLE) if (re.test(raw)) return key;
  return null;
}

export function refusalText(raw: string, ru: Locale): string {
  const key = refusalKey(raw);
  return (key && ru[key]) || raw;
}
