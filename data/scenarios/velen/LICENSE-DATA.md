# Данные карты

Земля вымышленная. `map.json`, `elevation.png`, `procedural.json`, `hillshade.png`, `hillshade.json`, `tiles.json` и `tiles/` в этой папке построены утилитой `mapgen procgen` (процедурный рельеф, R-UI-01) из параметров `data/procgen/velen.json` (сид 83) и правлены вручную (`map.json` `_comment`); `mapgen bake --map data/scenarios/velen` запекает тайлы из них. Внешних геоданных (DEM, OpenStreetMap) нет, сторонних лицензий на карту нет; текстуры местности — ассеты игры из `assets/terrain`.
