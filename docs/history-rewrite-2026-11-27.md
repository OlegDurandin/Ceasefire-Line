# Переписанная история git (2026-11-27)

По решению дизайнера из истории `main` и `tactical-t1` удалены PDF руководства Strategic Command (`docs/Strategic_Command_War_In_Europe.pdf`, 51 МБ) и две его недокачанные копии (`docs/Не подтвержден 134193.crdownload`, 47 и 35 МБ), случайно закоммиченные через `git add -A`: из-за них клонирование обрывалось («fatal: early EOF»). Содержимое всех коммитов, кроме этих файлов, не изменилось; изменились хэши коммитов начиная с `9daf821`. Ссылки на коммиты в `docs/decisions.md`, отчётах и планах — старые хэши; ниже таблица соответствия. Резервная копия истории до переписывания — git bundle вне репозитория (`Ceasefire-Line-before-history-rewrite-2026-11-27.bundle` у дизайнера).

После этого старые клоны несовместимы: заново клонировать (`git clone https://github.com/OlegDurandin/Ceasefire-Line.git`) или `git fetch && git reset --hard origin/main` (локальные незапушенные коммиты — перенести вручную).

| Старый | Новый | Коммит |
| --- | --- | --- |
| `5b78837` | `419e3a0` | docs: GDD snapshot — victory conditions (R-NM-02), open_exits (R-EVT-01), Sinai-67 conditions, «Отход» and idle_to_objectives (R-AI-01), AI acceptance row (section 17) |
| `33524f6` | `eb3e02a` | docs: agent memory moved into CLAUDE.md — no attribution lines, push after each item, lessons of the session |
| `f948101` | `ce08065` | docs: handoff — machine setup, step 7 state, what is open |
| `a6e377e` | `3d881a4` | feat(ai): tournament weights adopted (Golan-67 vs greedy 0.79 vs 0.69); tournament scores by the share of both sides' VP (R-AI-01, step 7) |
| `e986e0f` | `fff6db7` | feat(sim): tournament scores games by the acceptance measure — VP by date, collapse turn and NM by sudden death (R-AI-01, step 7) |
| `a1c53c5` | `cbadbd9` | feat(ai): scenario hint idle_to_objectives — a defending side's free units go to threatened own objectives; on for both sides of Golan-67 (R-AI-01) |
| `0fbbfaa` | `374e836` | feat(ai): «Отход» — exit value against holding in the common currency, rearguards near the ways out; sim exited steps (R-AI-01, R-NM-02) |
| `759af76` | `5a39a4f` | fix(client): units face the enemy near them (nearer ones weigh more), the side's default otherwise; Sinai-67 side facings corrected (R-UI-03) |
| `9a6af0f` | `36db9f1` | feat(client): scenario choice in the new-game menu from data/scenarios/index.json; check-data keeps the list in line with the folders |
| `14e22d6` | `774c137` | feat(core): per-side sudden death, exit hexes for VP, VP for enemy steps lost, open_exits effect; Sinai-67 conditions and withdrawal order (R-NM-02, R-EVT-01) |
| `530ce6f` | `a6d9032` | docs: Sinai-67 — v1 vs greedy analysed by time to sudden death; open question on the acceptance measure (R-AI-01, R-NM-02) |
| `7ada6ac` | `93660c1` | feat(sim): ai-vs-ai reports mean VP, NM at the end, NM change by reason and the sudden-death share (R-NM-01, R-NM-02, step 7 acceptance) |
| `42f2850` | `86f5e9b` | feat(client): path and AP left for the move under the pointer; WASM pathTo (SC UI item 2, R-UI-04) |
| `9eda3cf` | `6145c3c` | docs: confirmed history — 80th paratroop brigade on the Golan, Egyptian commandos alt, Harel late and partial (R-ECO-03, R-EVT-01) |
| `a61b97e` | `25b6329` | feat(client): «может ещё действовать» marker on own tokens — fresh, partial, spent muted (SC UI item 1, R-UI-03) |
| `af2473c` | `057e552` | docs: AI weight tournament report — no significant gain, weights unchanged; v1 not better than greedy on Sinai-67 by VP (R-AI-01, step 7) |
| `89fdda0` | `6b7f4eb` | docs: force limits — duplicates with the maps and events removed (Egypt Sinai-67 tank 0, infantry 1), designer decisions, Golan-67 turn-3 sources (R-ECO-03) |
| `a57e1fa` | `89b3f42` | feat(sim): tournament — AI weights tuned against greedy, v1 and perturbed v1 on both scenarios, maximin of outcome and VP margin (R-AI-01, step 7) |
| `4c3f72f` | `bbfec07` | data(golan67): turn-3 reinforcements named — 10th Harel brigade (sourced) and the 36th Division tank group (assumption) (R-EVT-01) |
| `e819409` | `c6826ad` | feat(ai): position terms of the hex score — w_cut, w_pocket, w_risk, w_choke with seeded jitter (R-AI-01, step 7 item 6) |
| `39441ad` | `a8c76db` | docs: force limit decisions (Golan-67 without purchases, Sinai-67 limits at the rebalance, alt units); release backlog (R-ECO-03) |
| `26ed051` | `67b5458` | docs: force limits proposal for Golan-67 and Sinai-67 by historical sources, not in the data until the rebalance (R-ECO-03) |
| `7f6048a` | `f27c011` | feat(ai): reserves transferred to the threatened sector by road; tests A/B/C (R-AI-01 sectors, R-MOVE-05, step 7) |
| `ef8a6a7` | `6346649` | data(golan67): no tech boosts in the 4-turn training scenario; e2e asks for a boost only where offered; economy decisions recorded (R-TECH-02, R-AI-01) |
| `9fa7528` | `8fe95ed` | feat(ai): economy in expected VP per MPP — reinforcement by role, purchases with arrival choice, boosts by the forecaster, transfer reserve; sim MPP use (R-AI-01 item 3, R-ECO-02, R-TECH-02) |
| `bf48069` | `660cb54` | feat: supply forecast after entry in the hex card and pointer line, key S for the supply layer; trial recounts capture enemy sources at half value (R-SUP-04, R-SUP-01, SC UI item 11) |
| `887a44d` | `539dd42` | docs: armor_city_penalty covers mobile SAM too (designer decision, R-MOVE-01) |
| `8498995` | `15aa59d` | feat: variant arrival_choice — a purchase arrives at the chosen own source (value 8+, no enemy next to it); choice on the purchase screen (R-ECO-02, SC item 26) |
| `ee91359` | `4dcd0d5` | feat(core): variant armor_city_penalty — hard defenders take hard_defense_mult 1.0 in cities and fortified areas (R-MOVE-01, R-COMBAT-02, SC item 8) |
| `50e7f4f` | `9746e61` | feat(core): variant hq_contact_priority — units in contact attach first (R-HQ-01, SC item 17) |
| `c297b81` | `4f03901` | docs: GDD snapshot — SC verdicts: armor_city_penalty, strips_entrenchment, hq_contact_priority, arrival_choice, air_strength_threshold, transfer_supply_min; П8 wording; stage 5 backlog |
| `b2b2b97` | `92b6f7c` | feat(core): variant transfer_supply_min — strategic transfer at unit supply 4 or more (R-MOVE-05, SC item 34) |
| `417df76` | `9f6e25f` | feat(core): variant air_strength_threshold — fighters intercept and escort at strength 5 or more (R-AIR-02, SC item 30) |
| `aaef2c7` | `fd5e824` | feat(core): entrenchment stripped by the attacker class's strips_entrenchment, HQ 0 (R-COMBAT-02, SC item 11) |
| `778854c` | `f46fa3c` | feat(client): quick styled tooltips for HUD icons, long press on touch; experience stars explained on the air card (R-UI-04) |
| `02a6b7d` | `f659540` | feat(ai): defend own chokepoints — reserves to threatened necks, withdrawal from closing pockets; sim defence metrics, intentions layer (R-AI-01, step 7 item 5) |
| `1bbfb5e` | `bd643e2` | docs(tactical): designer-confirmed T1 interpretations in the spec and snapshot (T-UNIT-01/02, T-SUP-01/02, T-LOS-01, T-COMBAT-02) |
| `1d77ff1` | `77ffeee` | docs: refresh tactical.md snapshot from the living doc |
| `caafe56` | `13e60b5` | fix(ai): HQ withdrawal keeps the HQ supply bonus coverage, objectives and lines; off by default until decided (R-AI-01, R-HQ-01) |
| `3891add` | `8a622e0` | feat(ai): own HQs keep 2 hexes from the enemy within their attachment radius; sim HQ attack metrics (R-AI-01, R-HQ-01, step 7) |
| `78227f8` | `c6cf0df` | feat(core): tactical support in the battle snapshot; tactical reads it from there (R-TAC-03, T-SUP-01, T-SUP-02) |
| `624742b` | `62b6bbd` | Merge branch 'tactical-t1': tactical battles stage T1 (rules, AI, sim, T-RES-05) |
| `436fbdf` | `3a69c8c` | feat(tactical): alternating activations by default; T-RES-05 percentile against 40 reference runs, sim --res05 acceptance, WASM reference margins (T-TURN-01, T-RES-05) |
| `1989c46` | `b0570eb` | docs: tactical T1 night report |
| `5391207` | `9f71bab` | feat(tactical): tactical AI, sim --dump-battles and sim tactical, rear units on the map, WASM size probe (T-TECH-01, T-TECH-02, T-UNIT-01) |
| `27451c0` | `3a0256a` | feat(tactical): T1 rules crate — map, LOS, activations, fire, suppression, assault, support, results, replay (T-SCALE, T-UNIT, T-TERR, T-LOS, T-TURN, T-COMBAT, T-SUP, T-RES) |
| `db726ea` | `20aa343` | feat(ai): operation inertia, breakthrough sectors by P(breach), breakthrough-first phase order; sim persistence, calibration and watch-hex metrics (R-AI-01, step 7) |
| `4761508` | `a2803e1` | docs: K = 10 confirmed; Suez-Giddi-Thamada road checked in the 1967 layer |
| `2261a1e` | `efcd250` | feat(ai): pocket operations valued in expected victory points, objectives in the same currency; AI intentions layer (R-AI-01, step 7 items 2, 7) |
| `d94f713` | `21126a7` | feat: canal ferries carry supply, El Ferdan bridge and its demolition event in Sinai-67; narrowest chokepoint per trunk (R-MOVE-02, R-SUP-02, R-EVT-01, R-AI-01) |
| `3df8fe4` | `c72d38d` | chore: clippy in the pre-push hook, as in CI |
| `c2fc843` | `3b3e6ad` | fix(ai): clippy type complexity in the pocket plan |
| `f2ad98a` | `d5fdb07` | feat(ai): pocket operations (envelopment, chokepoint hold, pinning, finishing) behind a switch; no stranding without supply (R-AI-01, step 7 items 3-4) |
| `939d6e1` | `4050691` | feat(ai): pocket value by trial supply recount, pincers of two chokepoints (R-AI-01, step 7 item 2) |
| `c5cb893` | `0fc48b9` | feat(ai): enemy supply dependency map and chokepoints (R-AI-01, step 7 item 1) |
| `975f89f` | `faef40a` | fix(core): source isolation follows supply path rules; supply_tree with predecessors (R-SUP-01, R-SUP-02) |
| `e0fd340` | `3d37185` | feat(core): isolated sources give at most 3, besieged sources dwindle, as a variant (R-SUP-01) |
| `96225e3` | `c6f841f` | feat(core): desertion in a pocket under pressure as a variant, «под угрозой распада» in the unit card (R-SUP-03) |
| `375c8d5` | `8b4f615` | feat(core): cities and fortified areas hold, entering one gives half entrenchment, as a variant (R-COMBAT-06, R-MOVE-01) |
| `05eea39` | `b5dcfe1` | feat(core): HQ bonuses scale with HQ strength as a variant (R-HQ-02) |
| `7d7418a` | `9e381cc` | feat(client): enemy phase summary window, key L (R-UI-04) |
| `122fba5` | `6c99c95` | feat(client): follow the AI turn: camera, combat results, move trails layer, AI speed setting (R-UI-05) |
| `111e520` | `e304cbe` | docs: sync gdd.md snapshot (SC manual decisions: variants, AI turn, phase summary) |
| `31de106` | `808f292` | docs: designer decisions on the Strategic Command manual comparison |
| `46d6d12` | `2459fa2` | docs: sync gdd.md snapshot with the living GDD (designer edits) |
| `835e4b0` | `e5c0161` | docs: Strategic Command manual vs our rules and UI, gap reports |
| `fe422ee` | `38cf24a` | docs: sync gdd.md snapshot (section 19 links docs/tactical.md) |
| `d7936ae` | `cd6d685` | docs: tactical battles snapshot (docs/tactical.md), referenced from AGENTS.md |
| `eba3501` | `ab51cc9` | docs: sync gdd.md snapshot with the living GDD (transfer sectors, R-COMBAT-07, R-NM-01, sections 17, 19) |
| `5b6779a` | `80149ed` | chore: stop tracking the Strategic Command manual (third-party PDF), ignore docs/*.pdf |
| `ec90374` | `e48051e` | feat(core): reinforced unit is not transferred under reinforce_locks (R-ECO-02, R-MOVE-05); prepared attack per strike test (R-COMBAT-07) |
| `3bc193d` | `99ec6d8` | chore: pre-commit and pre-push hooks, CI; step 7 transfer acceptance on a two-front test map |
| `94e053a` | `216e041` | docs: sync gdd.md snapshot with the living GDD (SC variants on by default, R-SUP-06 formula) |
| `b06aa61` | `0a0f98c` | docs: stage 4 step 7 scope after SC alignment: encirclement, AI economy order, transfers, acceptance |
| `54b101c` | `41d2e31` | chore: every commit builds on its own; sim check-data before commits |
| `9daf821` | `629249e` | fix(core): finish the HQ node formula change, restore the build (R-SUP-06) |
| `1aa5a7f` | `2ee441a` | fix(core): HQ supply node value scales with strength at any strength (R-SUP-06) |
| `645b4c9` | `4ccea39` | feat(data): SC-alignment variants on by default: P1, P2, P3, P7, P9, ZOC from strength 5 (R-COMBAT-02, R-COMBAT-07, R-COMBAT-05, R-MOVE-03, R-UNIT-02, R-NM-01) |
| `d3cb852` | `3d83b53` | docs: sync gdd.md snapshot with the living GDD (SC-alignment variants, R-COMBAT-07, R-SUP-06) |
| `a1fc94e` | `6ab6ec2` | docs: SC-alignment A/B report and flag recommendation |
| `d4e9a0d` | `ce17795` | feat(client): forecast lines and tooltips for prepared attacks, unsupplied targets and stripped entrenchment (R-COMBAT-07, R-COMBAT-05, R-COMBAT-02) |
| `63cfe34` | `988ebeb` | test(core): SC variance test without a pi-like literal |
| `97dde16` | `a9c842d` | feat(sim): A/B options, SC variance experiment and SC-alignment metrics in ai-vs-ai |
| `e86bed4` | `634a0dc` | feat(core): NM for a destroyed unit by its price as a variant (R-NM-01) |
| `04ad2dc` | `c3b9e6b` | feat(core): reinforcement locks the unit, no elite steps next to the enemy, as a variant (R-ECO-02) |
| `7807312` | `1b96fa9` | feat(core): supply 0 costs 2 AP as a variant (R-UNIT-02) |
| `ec6a093` | `07a86a4` | feat(core): strategic transfer costs morale as a variant (R-MOVE-05) |
| `514a932` | `aa84bbf` | feat(core): HQ as a supply node variant (R-SUP-06), sim supply-reach |
| `f70076d` | `1382c49` | feat(core): prepared attack (R-COMBAT-07) and attacks on units without supply (R-COMBAT-05) as variants |
| `b3b6a84` | `9a05887` | feat(core): rule variants section, attacks strip entrenchment (R-COMBAT-02) |
| `fb0a5ae` | `323f4f7` | feat(data): Sinai-67 scenario — forces from sources, first_strike variants, objectives, withdrawal order (section 14, R-EVT-01, R-AIR-01) |
| `351a200` | `c75abed` | feat(ai): seeded per-game spread of the AI's aggressiveness by default (R-AI-01) |
| `994d6ca` | `e861a53` | docs: sync gdd.md snapshot with the living GDD (section 14 Golan-67, R-EVT-01, R-AI-01) |
| `fbc1105` | `d1200c8` | feat: historical victory thresholds for Golan-67, withdrawal on Mas'ade, next-objective operation (R-NM-02, R-EVT-01, R-AI-01) |
| `247af66` | `a1201f2` | data(sinai67): control by the 1949 armistice line (R-MOVE-06) |
| `520da7f` | `24962bf` | docs: sync gdd.md snapshot with the living GDD (section 14, Sinai-67 map) |
| `3e813f2` | `13b0aa6` | feat: Sinai-67 map with 1967 roads and airfields (R-UI-01, R-AIR-01) |
| `8d7fe8d` | `fc4a2c5` | feat(sim): victory point margin table and aggressiveness jitter in ai-vs-ai (section 17) |
| `e4adcef` | `191c3c8` | feat(core): at most two interceptors per strike; lost base costs 30% readiness (R-AIR-02, R-TURN-03) |
| `0f123c3` | `1b6cd8f` | feat(ai): AI v1 flies air strikes before ground attacks (R-AI-01, R-AIR-02) |
| `554cd16` | `49bf2fb` | feat(client): aircraft on the map, air strike forecast, rebase (R-AIR-01, R-AIR-02) |
| `4223ab6` | `3ee90ea` | feat(wasm): aircraft, air targets, air forecast and rebase targets (R-AIR-01, R-AIR-02) |
| `c4340ba` | `3f613eb` | feat(core): aircraft on bases, strikes, interception and escort (R-AIR-01, R-AIR-02) |
| `2355d85` | `280a3aa` | feat(core): air strike and air battle fire (R-AIR-02), mandatory examples A-D |
| `03344d2` | `17b822b` | feat(client): play against the AI (R-AI-01) |
| `93f8db0` | `0fe6e99` | feat(ai): AI v1 with influence maps, operations and exchange scoring (R-AI-01) |
| `9b62cef` | `e025b5b` | feat(ai): AI crate, baseline greedy player and sim ai-vs-ai (R-AI-01, R-AI-02) |
| `adb6275` | `5e7fa67` | docs: sync gdd.md snapshot with the living GDD (R-AIR-01, R-AIR-02, R-AI-01, R-EVT-01, sections 14, 17) |
| `e0823fa` | `8b70569` | feat: withdrawal order also costs 15 readiness points until the next own phase (R-EVT-01) |
| `a51a14c` | `6f32c73` | docs: designer answers for stage 4 (R-AIR-01, R-AIR-02, sections 14, 17) |
| `a005847` | `9a39d3e` | docs: stage 4 plan (AI v1, air without umbrellas, Sinai-67) |
| `5950723` | `d61c4ab` | docs: sync gdd.md snapshot with the living GDD (R-UI-02, R-UI-04, R-EVT-01) |
| `0e683ad` | `ede4d49` | docs: stage 3 closed, stage 4 (AI v1 and Sinai-67) is current |
| `1ac9c1d` | `3566502` | feat: withdrawal order event in Golan-67; any_of/all_of triggers, unit morale and entrenchment effects (R-EVT-01) |
| `87dca6f` | `a114a3f` | feat(client): touch — tap picks a hex, long press is the hover (R-UI-04) |
| `554df0a` | `b2fa767` | data(golan67): drop unsourced Lebanese roads from the 1967 layer (R-UI-01) |
| `8429645` | `9fec6ed` | feat: formation names from the scenario in the attack window (R-UI-04) |
| `2e6c67c` | `9f6856d` | fix(client): notices fade after 2.5 s (R-UI-04) |
| `6a72133` | `2b8fd34` | fix(client): zero losses without a minus (R-UI-04) |
| `136874f` | `3114696` | feat(client): fit the map on start, keep it on screen, cloth backdrop (R-UI-02) |
| `75b9191` | `971d6b6` | docs: stage 3 acceptance report wording |
| `03bb7da` | `baf20a5` | test(client): stage 3 acceptance through the HUD (section 18) |
| `5501f11` | `768afe7` | docs: sync gdd.md snapshot with the living GDD (R-UI-04, R-UI-05 per the approved HUD mock) |
| `058bf3a` | `ef11389` | docs: HUD report with screenshots (R-UI-04) |
| `9513892` | `3f22b5e` | feat(client): HUD as on the approved mock (R-UI-04, R-UI-05) |
| `71c4fba` | `2c8398f` | feat(client): core refusals in Russian (R-UI-04) |
| `a2d6a21` | `abe7805` | docs: sync gdd.md snapshot with the living GDD (sections 13, 17, R-MOVE-05, R-UI-03) |
| `656a474` | `f733e90` | docs: track interpretations accepted, R-MOVE-05 wording (R-MOVE-01, R-MOVE-05) |
| `7f1f5f8` | `7451383` | feat(client): unit facing is saved with the game (R-UI-03, R-UI-05) |
| `c195bc7` | `f08b462` | feat(sim): price order in the cost check; mech 80, artillery 50 (section 17, R-ECO-02) |
| `a5059a9` | `60132f1` | fix(sim): umbrella check at a reference infantry price of 60 MPP (section 17) |
| `ef441f0` | `0b39de9` | fix(client): attack window fits small screens (R-UI-04) |
| `b3bad49` | `af4b99a` | docs: sync gdd.md snapshot with the living GDD (R-UI-03, R-UI-04 per the approved mock) |
| `0f8596f` | `118bb68` | feat(client): attack window and icon strip as on the approved mock (R-UI-04) |
| `5eb8887` | `f5c66a2` | feat(client): unit tokens as on the approved mock (R-UI-03) |
| `e844738` | `7529f0e` | docs: sync gdd.md snapshot with the living GDD (R-UI-03 units, R-UI-04 attack window) |
| `71cc11f` | `2c606c3` | feat(client): attack window after Strategic Command (R-UI-04) |
| `566f906` | `10067e8` | feat(client): units on side-coloured disks, rings for selection and target (R-UI-03) |
| `f88fa6f` | `e67d321` | docs: sync gdd.md snapshot with the living GDD (tracks, historical roads, prices, section 17, R-UI-03 facing, R-UI-05) |
| `ba23d31` | `21643f5` | feat(client): unit facing, «Отменить» and «В меню» (R-UI-03, R-UI-05) |
| `6108913` | `205a4cf` | feat: dirt roads and years of use for historical roads (R-MOVE-01) |
| `7917125` | `92c958d` | feat(data,sim): decision A closed — MPP exchange ratio, +-35%, new prices (section 17) |
| `f5a9a4a` | `0683e30` | feat(data): 1967 road network for Golan-67 as a sourced layer (R-UI-01) |
| `86798bf` | `845f8c2` | test(client): end-to-end game through the UI (stage 3 acceptance) |
| `de2f213` | `d35ccca` | docs: sync gdd.md snapshot with the living GDD (R-UI-03 miniatures, R-UI-05 saves, R-UI-06) |
| `2a48196` | `8732c60` | feat(client): next unit, end-phase warning, undo and saves (R-UI-05) |
| `012a67b` | `3dfabad` | feat(client): map layers and unit status icons (R-UI-02, R-UI-03) |
| `af50db0` | `884a5bf` | feat(mapgen): historical road layer with sources, switchable in one action (R-UI-01) |
| `eedec02` | `45aa50e` | feat(client): units as painted miniatures on bases, medallions as fallback (R-UI-03) |
| `d5e9a0d` | `2087d91` | feat(client): date, combat panel with the formula, tooltips with reasons (R-UI-04) |
| `daee51b` | `3bd20ec` | docs: sync gdd.md snapshot with the living GDD (R-MOVE-06 neutral territory, section 17 combat value) |
| `1208aef` | `4abbe96` | feat(sim): combat value per MPP check and price search (decision A, section 17) |
| `3f6715b` | `d03469c` | feat(core): neutral closed territory as a scenario property (R-MOVE-06) |
| `11eef1e` | `1a00bee` | feat(assets): miniature unit pack v1 and the "miniature" style (R-UI-06) |
| `765f5eb` | `44584cb` | feat(client): every Golan-67 command in the UI, panels and reports (R-UI-04) |
| `cbf1800` | `450f16c` | style(mapgen): rustfmt the bake test |
| `f61aa86` | `00f7320` | docs(agents): stage 2 closed, stage 3 (vertical slice) is current |
| `8ad07aa` | `8222fe7` | docs: sync gdd.md snapshot with the living GDD (section 14 map size, R-UI-01 bake) |
| `24050e8` | `b2f0e66` | docs: render report with screenshots; bake decisions (R-UI-01, R-UI-02) |
| `1802f2c` | `88c3a8f` | feat(client): painted map tiles under the live layers (R-UI-02) |
| `da1a2bf` | `2a25208` | feat(data): baked painted map for Golan-67 (R-UI-01) |
| `3487b59` | `98f8cf2` | feat(mapgen): bake the painted map into a tile pyramid (R-UI-01, R-UI-02) |
| `0fe9cf8` | `cc4c760` | fix(mapgen): no seams between DEM tiles, no chords across OSM areas (R-UI-01) |
| `8cda464` | `469c5db` | feat(data): Golan-67 on the real map from Copernicus DEM and OSM (R-UI-01) |
| `0113b74` | `7d69560` | docs: sync gdd.md snapshot with the living GDD (R-UI-02, R-UI-03 render) |
| `0e5fc57` | `d5b4805` | feat(client): grid nearly invisible until a unit is selected (R-UI-02) |
| `8733b55` | `ae776dd` | feat(client): control wash and front line (R-MOVE-06, R-UI-02) |
| `9ab8150` | `946612e` | feat(client): smooth outlined roads through hex centres (R-UI-02) |
| `7d1ec4c` | `12781be` | feat(client): world-anchored terrain sheets with soft borders (R-UI-02) |
| `038885d` | `c02fc33` | feat(client): readable unit medallions in side colours (R-UI-03) |
| `fd88e66` | `e0aba2a` | docs: README with setup, build and run commands for the game and the editor |
| `1a40529` | `c410fc7` | feat(client): map editor (stage 2, step 4) |
| `ddb4263` | `3982c06` | docs: sync gdd.md snapshot with the living GDD (rev 89, Copernicus DEM in R-UI-01) |
| `3395c40` | `ebce249` | feat(mapgen): Copernicus DEM GLO-30 as the relief source; licence notices |
| `1cc45fc` | `1c30da6` | docs: sync gdd.md snapshot with the living GDD (rev 88, R-TAC-02 97.5/2.5) |
| `bcc9764` | `76fd072` | feat(core): tactical extremes at the 97.5th / 2.5th percentiles (R-TAC-02) |
| `a6c1f90` | `dab9036` | feat(mapgen): OSM areas, roads, rivers, bridges, passes and places (stage 2, step 3) |
| `7ff99fc` | `4261fb0` | docs: sync gdd.md snapshot with the living GDD (rev 84, review fixes) |
| `30a98d7` | `19b842a` | feat(core): tactical extremes are the distribution's max and min (R-TAC-02) |
| `9b0f325` | `47b26fe` | docs: sync gdd.md snapshot with the living GDD (rev 72, section 19 tactical battles) |
| `6599e55` | `6c8cfdc` | feat(core): pluggable combat resolution and battle snapshots (tactical battles) |
| `12af830` | `99d5866` | docs: sync gdd.md snapshot with the living GDD (rev 71, water terrain) |
| `11f67d9` | `efbb23c` | feat(mapgen): DEM relief classification and hillshade (stage 2, step 2) |
| `bf2fe58` | `1795937` | feat(core): water terrain (R-MOVE-01); fix missing event and boost validation |
| `c80521f` | `40c0d5d` | feat(client): hotseat play on a scenario map (stage 2, step 1b) |
| `5f918a6` | `55b8bc7` | docs: sync gdd.md snapshot with the living GDD (rev 70, SAM decision B) |
| `91d15a9` | `a504fd8` | docs: stage 2 decisions (hex size, water, roads, ODbL, client step 1) |
| `91c6e52` | `5d29cd2` | feat(client): browser client skeleton on the core via WASM (stage 2, step 1) |
| `1153bf2` | `83094b5` | docs: price sweep for decision A does not fit the ranges (stopped, numbers logged) |
| `2412f23` | `b32b50b` | feat(balance): SAM shot fraction 0.3, next SAM x0.8, EW 0.08/0.12 (R-AIR-03, R-TECH-01) |
| `0579eaa` | `8b933e4` | docs: sync gdd.md snapshot with the living GDD (rev 63, bridge state, seam check) |
| `d563c16` | `33e54ac` | feat(sim): net exchange per MPP with fatigue on later strikes; sam.shot_fraction |
| `1b86a6f` | `c7f8bc6` | feat(core): bridge state per hex side: none, built, destroyed (R-MOVE-02) |
| `899b36a` | `9a81eb0` | fix(sim): relative seam metric for terrain textures (R-UI-06) |
| `cd32212` | `605d8f4` | chore: stop tracking .DS_Store |
