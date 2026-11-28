# Что сгенерировать для «Срединных земель» (сценарии 8–10)

## Фон главного меню

### menu_background
Сохранить: `assets/ui/menu_background.png`

```
Painterly gouache realism, muted natural colors, soft upper-left lighting, visible brush texture, mid-century military history book plate; no text, flags, insignia or identifiable people; neutral tone, no gore; period-correct 1967–1973 equipment. Generate 1536×1024. Subject: Wide desert-and-hills panorama at golden hour: an armoured column on a dusty road in the middle distance, low ridges and a ceasefire-line watchtower on the horizon, calm empty sky in the upper third (space for a title), dark foreground rocks at the bottom edge.
```

Финальные иллюстрации: WebP q85; брифинги 1600×900, события 1200×675, фон меню 1920×1080 (`assets/ui/menu_background.webp`). PNG-пути ниже обозначают исходники генерации. Версия промптов: `midlands-v1`.

Стиль взят из `assets/manifest.json` проекта. Одна картинка на запрос, сохранить под указанным именем.
Фигурки новых стран не нужны: используются готовые западные (`isr`) и советские (`egy_syr`) с цветами сторон. Из партии 1967 года понадобятся AMX-13, Super Sherman и Т-34-85 для Тарвии.

## Текстуры местности (1024 × 1024)

### terrain_forest
Сохранить: `assets/terrain/terrain_forest.png`

```
Painterly gouache realism, muted natural colors, upper-left lighting, no text, flags or insignia. Seamless square terrain texture, 1024×1024, strictly orthographic top-down, tileable on all sides, no vignette, no border shadows, no large isolated features at the edges. Subject: Seamless square orthographic top-down terrain: temperate deciduous forest canopy, dense tree crowns of mixed greens with small clearings
```

### terrain_forest_pine
Сохранить: `assets/terrain/terrain_forest_pine.png`

```
Painterly gouache realism, muted natural colors, upper-left lighting, no text, flags or insignia. Seamless square terrain texture, 1024×1024, strictly orthographic top-down, tileable on all sides, no vignette, no border shadows, no large isolated features at the edges. Subject: Seamless square orthographic top-down terrain: dark coniferous pine forest canopy on slopes, scattered rocks between trees
```

### terrain_steppe
Сохранить: `assets/terrain/terrain_steppe.png`

```
Painterly gouache realism, muted natural colors, upper-left lighting, no text, flags or insignia. Seamless square terrain texture, 1024×1024, strictly orthographic top-down, tileable on all sides, no vignette, no border shadows, no large isolated features at the edges. Subject: Seamless square orthographic top-down terrain: dry steppe grassland, short yellow-green grass with bare earth patches and faint tracks
```

### terrain_alpine_meadow
Сохранить: `assets/terrain/terrain_alpine_meadow.png`

```
Painterly gouache realism, muted natural colors, upper-left lighting, no text, flags or insignia. Seamless square terrain texture, 1024×1024, strictly orthographic top-down, tileable on all sides, no vignette, no border shadows, no large isolated features at the edges. Subject: Seamless square orthographic top-down terrain: high mountain meadow, short grass, scattered boulders and scree
```

### terrain_town_temperate
Сохранить: `assets/terrain/terrain_town_temperate.png`

```
Painterly gouache realism, muted natural colors, upper-left lighting, no text, flags or insignia. Seamless square terrain texture, 1024×1024, strictly orthographic top-down, tileable on all sides, no vignette, no border shadows, no large isolated features at the edges. Subject: Seamless square orthographic top-down terrain: dense small town with pitched red-tiled roofs, narrow streets and small gardens
```

## Объекты на карте (512 × 512, прозрачный фон)

### obj_oil_field
Сохранить: `assets/objects/obj_oil_field.png`

```
Painterly gouache realism, muted natural colors, upper-left lighting, no text, flags or insignia. Map object, 512×512, PNG RGBA with transparent background, strictly top-down, all four corners fully transparent, at least 8 px transparent padding. Subject: Orthographic top-down transparent map object: oil field: three oil derricks and pumpjacks with pipes and a small storage tank, compact cluster
```

## Иллюстрации брифингов и событий

Генерировать 1536 × 1024; агент обрежет до 16:9 и сожмёт в WebP.

### briefing_velen
Сохранить: `assets/briefing/briefing_velen.png`

```
Painterly gouache realism, muted natural colors, soft upper-left lighting, visible brush texture, mid-century military history book plate; no text, flags, insignia or identifiable people; neutral tone, no gore; period-correct 1970s equipment of a fictional country, no real-world national markings. Generate 1536×1024. Subject: A river-valley border town under siege at dawn, low hills and woods around it, smoke over the outskirts, armoured columns on distant roads, autumn colours
```

### velen_pocket_closed
Сохранить: `assets/events/velen_pocket_closed.png`

```
Painterly gouache realism, muted natural colors, soft upper-left lighting, visible brush texture, mid-century military history book plate; no text, flags, insignia or identifiable people; neutral tone, no gore; period-correct 1970s equipment of a fictional country, no real-world national markings. Generate 1536×1024. Subject: Armoured vehicles meet on a country road behind a town, closing a ring, a church spire and smoke in the background
```

### velen_relief_breakthrough
Сохранить: `assets/events/velen_relief_breakthrough.png`

```
Painterly gouache realism, muted natural colors, soft upper-left lighting, visible brush texture, mid-century military history book plate; no text, flags, insignia or identifiable people; neutral tone, no gore; period-correct 1970s equipment of a fictional country, no real-world national markings. Generate 1536×1024. Subject: Tanks and armoured personnel carriers push through a wooded valley road toward a besieged town at noon
```

### velen_ceasefire_pressure
Сохранить: `assets/events/velen_ceasefire_pressure.png`

```
Painterly gouache realism, muted natural colors, soft upper-left lighting, visible brush texture, mid-century military history book plate; no text, flags, insignia or identifiable people; neutral tone, no gore; period-correct 1970s equipment of a fictional country, no real-world national markings. Generate 1536×1024. Subject: A government building at night, lit windows, cars at the entrance, a radio mast on the roof, rain
```

### briefing_oil_junction
Сохранить: `assets/briefing/briefing_oil_junction.png`

```
Painterly gouache realism, muted natural colors, soft upper-left lighting, visible brush texture, mid-century military history book plate; no text, flags, insignia or identifiable people; neutral tone, no gore; period-correct 1970s equipment of a fictional country, no real-world national markings. Generate 1536×1024. Subject: Steppe at sunrise: two armoured columns raising dust on converging roads toward a cluster of oil derricks on the horizon
```

### oil_junction_contact
Сохранить: `assets/events/oil_junction_contact.png`

```
Painterly gouache realism, muted natural colors, soft upper-left lighting, visible brush texture, mid-century military history book plate; no text, flags, insignia or identifiable people; neutral tone, no gore; period-correct 1970s equipment of a fictional country, no real-world national markings. Generate 1536×1024. Subject: Tanks deploying from column into line across open steppe, first shell bursts ahead, oil derricks in the distance
```

### oil_junction_fields_burning
Сохранить: `assets/events/oil_junction_fields_burning.png`

```
Painterly gouache realism, muted natural colors, soft upper-left lighting, visible brush texture, mid-century military history book plate; no text, flags, insignia or identifiable people; neutral tone, no gore; period-correct 1970s equipment of a fictional country, no real-world national markings. Generate 1536×1024. Subject: Oil derricks and storage tanks burning on the steppe, black smoke columns, abandoned trucks
```

### briefing_sarna
Сохранить: `assets/briefing/briefing_sarna.png`

```
Painterly gouache realism, muted natural colors, soft upper-left lighting, visible brush texture, mid-century military history book plate; no text, flags, insignia or identifiable people; neutral tone, no gore; period-correct 1970s equipment of a fictional country, no real-world national markings. Generate 1536×1024. Subject: A wide mountain river in a forested foothill valley, three bridges in the distance, snow-capped peaks behind, missile launchers on a ridge
```

### sarna_bridge_blown
Сохранить: `assets/events/sarna_bridge_blown.png`

```
Painterly gouache realism, muted natural colors, soft upper-left lighting, visible brush texture, mid-century military history book plate; no text, flags, insignia or identifiable people; neutral tone, no gore; period-correct 1970s equipment of a fictional country, no real-world national markings. Generate 1536×1024. Subject: A steel road bridge over a mountain river collapsing in a demolition blast, dust and spray
```

### sarna_crossing_under_fire
Сохранить: `assets/events/sarna_crossing_under_fire.png`

```
Painterly gouache realism, muted natural colors, soft upper-left lighting, visible brush texture, mid-century military history book plate; no text, flags, insignia or identifiable people; neutral tone, no gore; period-correct 1970s equipment of a fictional country, no real-world national markings. Generate 1536×1024. Subject: Engineers launch pontoon rafts across a fast river under artillery fire, jets trailing smoke overhead
```

### sarna_pass_taken
Сохранить: `assets/events/sarna_pass_taken.png`

```
Painterly gouache realism, muted natural colors, soft upper-left lighting, visible brush texture, mid-century military history book plate; no text, flags, insignia or identifiable people; neutral tone, no gore; period-correct 1970s equipment of a fictional country, no real-world national markings. Generate 1536×1024. Subject: A light tank column crests a narrow mountain pass road between pine-covered slopes, low clouds
```
