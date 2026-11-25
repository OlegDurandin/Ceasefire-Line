# Отчёт по ассетам

Сгенерирован `sim assets-check`. Файлов по манифесту: 41, в порядке: 37. PNG всего: 25.4 МБ, из них игровые (terrain + units): 24.0 МБ.

Пороги: шов — средняя разница RGB противоположных краёв > 12; у юнитов прозрачная рамка 1 px. Для справки в деталях — та же метрика между соседними столбцами и строками внутри текстуры: если край близок к ней, шов, скорее всего, на глаз не виден. Модели техники визуально не оценивались.

Привязка `data/sprites.json`: замечаний нет.

| id | файл | статус | детали |
| --- | --- | --- | --- |
| terrain_desert_flat | `terrain/terrain_desert_flat.png` | ок | края: л–п 7.5, в–н 9.7; внутри: соседние столбцы 8.2, строки 8.7 |
| terrain_dunes | `terrain/terrain_dunes.png` | ок | края: л–п 5.6, в–н 7.0; внутри: соседние столбцы 5.7, строки 6.1 |
| terrain_hills_rocky | `terrain/terrain_hills_rocky.png` | шов (лево–право 10.1, верх–низ 15.4) | края: л–п 10.1, в–н 15.4; внутри: соседние столбцы 11.6, строки 13.6 |
| terrain_mountains | `terrain/terrain_mountains.png` | ок | края: л–п 8.2, в–н 8.9; внутри: соседние столбцы 8.6, строки 8.3 |
| terrain_saltmarsh | `terrain/terrain_saltmarsh.png` | шов (лево–право 11.5, верх–низ 15.1) | края: л–п 11.5, в–н 15.1; внутри: соседние столбцы 11.6, строки 15.4 |
| terrain_basalt_plateau | `terrain/terrain_basalt_plateau.png` | ок | края: л–п 7.6, в–н 8.4; внутри: соседние столбцы 8.7, строки 8.5 |
| terrain_farmland | `terrain/terrain_farmland.png` | шов (лево–право 13.1, верх–низ 13.5) | края: л–п 13.1, в–н 13.5; внутри: соседние столбцы 14.3, строки 13.2 |
| terrain_town | `terrain/terrain_town.png` | шов (лево–право 10.1, верх–низ 14.2) | края: л–п 10.1, в–н 14.2; внутри: соседние столбцы 10.6, строки 16.5 |
| terrain_water | `terrain/terrain_water.png` | ок | края: л–п 1.6, в–н 2.2; внутри: соседние столбцы 1.9, строки 2.2 |
| isr_centurion | `units/isr/isr_centurion.png` | ок |  |
| isr_m48 | `units/isr/isr_m48.png` | ок |  |
| isr_m60 | `units/isr/isr_m60.png` | ок |  |
| isr_super_sherman | `units/isr/isr_super_sherman.png` | ок |  |
| isr_m3_halftrack | `units/isr/isr_m3_halftrack.png` | ок |  |
| isr_m113 | `units/isr/isr_m113.png` | ок |  |
| isr_infantry | `units/isr/isr_infantry.png` | ок |  |
| isr_m109 | `units/isr/isr_m109.png` | ок |  |
| isr_bridging | `units/isr/isr_bridging.png` | ок |  |
| isr_hq | `units/isr/isr_hq.png` | ок |  |
| isr_mirage3 | `units/isr/isr_mirage3.png` | ок |  |
| isr_a4 | `units/isr/isr_a4.png` | ок |  |
| isr_f4 | `units/isr/isr_f4.png` | ок |  |
| isr_super_frelon | `units/isr/isr_super_frelon.png` | ок |  |
| isr_saar3 | `units/isr/isr_saar3.png` | ок |  |
| egy_t55 | `units/egy_syr/egy_t55.png` | ок |  |
| egy_t62 | `units/egy_syr/egy_t62.png` | ок |  |
| egy_btr152 | `units/egy_syr/egy_btr152.png` | ок |  |
| egy_btr50 | `units/egy_syr/egy_btr50.png` | ок |  |
| egy_bmp1 | `units/egy_syr/egy_bmp1.png` | ок |  |
| egy_infantry | `units/egy_syr/egy_infantry.png` | ок |  |
| egy_atgm_team | `units/egy_syr/egy_atgm_team.png` | ок |  |
| egy_m46 | `units/egy_syr/egy_m46.png` | ок |  |
| egy_sa2 | `units/egy_syr/egy_sa2.png` | ок |  |
| egy_sa6 | `units/egy_syr/egy_sa6.png` | ок |  |
| egy_zsu23 | `units/egy_syr/egy_zsu23.png` | ок |  |
| egy_pmp | `units/egy_syr/egy_pmp.png` | ок |  |
| egy_hq | `units/egy_syr/egy_hq.png` | ок |  |
| egy_mig21 | `units/egy_syr/egy_mig21.png` | ок |  |
| egy_su7 | `units/egy_syr/egy_su7.png` | ок |  |
| egy_mi8 | `units/egy_syr/egy_mi8.png` | ок |  |
| egy_osa | `units/egy_syr/egy_osa.png` | ок |  |
