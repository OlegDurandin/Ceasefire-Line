# Клиент Ceasefire Line

TypeScript + PixiJS + Vite; правила — ядро на Rust, собранное в WASM (`crates/wasm`).

## Запуск локально

Нужны: Rust (stable) с целью `wasm32-unknown-unknown`, `wasm-pack`, Node.js 22+.

```bash
rustup target add wasm32-unknown-unknown   # один раз
cd client
npm install                                 # один раз
npm run wasm                                # собрать ядро в src/wasm (после каждой правки Rust)
npm run dev                                 # http://localhost:5173/  (?scenario=golan67 по умолчанию)
```

Проверки: `npm run typecheck`, `npm test`. Сборка для выкладки: `npm run build` → `client/dist/` (вместе с `data/` и `assets/`, без сырых данных и архивов).

## Режим игры (хотсит)

Касание своего юнита — выбор: зелёные гексы — куда можно пойти, оранжевые — кого можно атаковать. Касание цели — прогноз боя «−атакующий | −защитник» с диапазоном; второе касание или «Атаковать» — бой. «Занять гекс» после боя, если цель ушла. «Конец фазы» передаёт ход второй стороне за тем же экраном.
