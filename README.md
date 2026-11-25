# Ceasefire Line

Гекс-варгейм оперативного уровня об арабо-израильских войнах 1967 и 1973 годов. Правила — ядро на Rust (`crates/core`), в браузере оно работает через WebAssembly; клиент — TypeScript + PixiJS + Vite (`client/`). Правила игры — `docs/gdd.md`, решения и толкования — `docs/decisions.md`, правила для разработчиков — `AGENTS.md`.

## Запуск с нуля

Проверено на macOS (Apple Silicon); на Linux команды те же.

### 1. Что установить (один раз)

| Что | Версия | Как |
| --- | --- | --- |
| Rust (stable) | 1.87+ | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Цель WebAssembly | — | `rustup target add wasm32-unknown-unknown` |
| wasm-pack | 0.13+ | `cargo install wasm-pack` |
| Node.js | 22+ | https://nodejs.org или `brew install node` |

### 2. Собрать и запустить

```bash
cd client
npm install        # один раз
npm run wasm       # собрать ядро в WASM (client/src/wasm); повторять после каждой правки Rust
npm run dev        # dev-сервер Vite
```

### 3. Открыть в браузере

| Режим | Адрес |
| --- | --- |
| Игра, «Голаны-67», хотсит | http://localhost:5173/ |
| Редактор карты и сценария | http://localhost:5173/?mode=edit |
| Другой сценарий | добавить `?scenario=<id>` (папка `data/scenarios/<id>/`) |
| Игра без запечённой карты | добавить `?baked=0` |

Если порт 5173 занят, Vite выберет следующий и напечатает адрес в терминале.

## Как играть

- **Выбор юнита** — касание своего юнита. Зелёные гексы — куда можно пойти, оранжевая штриховка — кого можно атаковать.
- **Ход** — касание зелёного гекса.
- **Атака** — касание цели: внизу прогноз «−потери атакующего | −потери защитника» с диапазоном (10–90-й перцентили) и раскладкой A, D, p. Второе касание цели или «Атаковать» — бой. Если защитник ушёл, танк или мех. бригада могут «Занять гекс».
- **Конец фазы** — кнопка вверху; ход переходит ко второй стороне за тем же экраном. Вверху — сторона, ход (день/ночь), MPP, национальная мораль, сколько ходов до перемирия.
- **Карта** — перетаскивание мышью или пальцем, колесо или щипок — масштаб.
- После последнего хода или обрушения национальной морали — окно с итогом.

## Как редактировать

В режиме `?mode=edit` вверху выбирается инструмент: местность (с флажком «перевал» для гор), стороны гексов (касание у стороны: нет → дорога → река/канал → мост → разрушенный мост), владение гексами, юниты (класс, сторона, рейтинг штаба), ластик; ↶ / ↷ — отмена и повтор. Ошибки сценария (проверка ядра) обводятся красным на карте и перечислены справа. У сгенерированных карт (`mapgen`) есть подложка: локальная картинка, растянутая на bbox карты, с ползунком прозрачности.

«Сохранить» работает только с dev-сервером и перезаписывает `data/scenarios/<id>/map.json` и `scenario.json`. Вернуть исходные файлы: `git checkout -- data/scenarios/<id>/`.

## Проверки

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd client && npm run typecheck && npm test
```

Утилиты:

```bash
cargo run -p sim --release -- combat-matrix > matrix.csv   # матрица боёв
cargo run -p sim --release -- balance                      # проверки раздела 17 → docs/balance-report.md
cargo run -p sim --release -- assets-check                 # ассеты → docs/assets-report.md
cargo run -p mapgen --release -- \
  --dem data/raw/golan67 \
  --osm data/raw/golan67/israel-and-palestine-latest.osm.pbf --osm data/raw/golan67/syria-latest.osm.pbf \
  --bbox 35.45,32.60,36.50,33.45 --hex-km 5 --out data/scenarios/<id>   # карта из рельефа и OSM
cargo run -p mapgen --release -- bake --map data/scenarios/golan67 \
  --dem data/raw/golan67 \
  --osm data/raw/golan67/israel-and-palestine-latest.osm.pbf --osm data/raw/golan67/syria-latest.osm.pbf \
  --osm data/raw/golan67/lebanon-latest.osm.pbf --osm data/raw/golan67/jordan-latest.osm.pbf   # живописная карта в тайлы
```

`mapgen bake` запекает `map.json` сценария (после правок в редакторе) в пирамиду тайлов `tiles/<z>/<x>_<y>.webp` + `tiles.json`; результат детерминирован (отпечаток `fnv1a64` в `tiles.json`). После правки местности, городов или дорог в редакторе карту нужно запечь заново. Игра рисует тайлы под живыми слоями (контроль, фронт, сетка, подсветки, юниты); редактор — всегда живой рендер; `?baked=0` показывает живой рендер и в игре.

Сырые данные (`data/raw/`) в Git не хранятся; откуда их брать — `docs/decisions.md`, запись R-UI-01.

## Если что-то не так

- **`npm install` падает с `EACCES` в `~/.npm`** — в кэше npm есть файлы root: `sudo chown -R $(id -u):$(id -g) ~/.npm`.
- **`npm run wasm` долго висит на `cargo metadata`** — Cargo ждёт сеть. Скачайте зависимости и соберите офлайн: `cargo fetch`, затем `CARGO_NET_OFFLINE=true npm run wasm`.
- **Пустой экран** — в консоли браузера будет причина; чаще всего не собран WASM (`npm run wasm`).

## Данные и лицензии

Карты, сгенерированные из открытых данных, сопровождаются файлом `LICENSE-DATA.md` в папке сценария (Copernicus DEM GLO-30, OpenStreetMap — ODbL). Обязательные уведомления для титров — `data/credits.json`.
