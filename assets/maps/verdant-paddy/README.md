# Verdant Paddy map package

`verdant-paddy.tmj` is the single authoritative 64×64 orthogonal map. It opens directly in Tiled with `../autofarm.tiled-project` and external tilesets from `../tilesets/`.

Layer contract:

1. `Ground` — complete meadow base.
2. `Terrain` — paddies, roads, concrete, river, and irrigation water.
3. `Infrastructure` — oriented bunds, field entrances, and culverts.
4. `Decoration` — non-logical visual variation.
5. `Gameplay/Structures` — facility tile objects and simulation anchors.
6. `Gameplay/Field Zones` — crop and priority rectangles.
7. `Gameplay/Spawns` — garage exit, ordered bays, and robot spawns.
8. `Gameplay/Navigation` — authored movement-cost areas.
9. `Gameplay/Collision` — hidden editor/debug collision annotations.

The runtime compiler reads tile and object layers from the same `.tmj`. Static map data is not duplicated in saves; saves store only the map id and mutable simulation state. Run `tools/build_map_assets.sh` after editing the deterministic source generator or source atlases.
