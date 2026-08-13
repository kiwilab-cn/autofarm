# Verdant Paddy map package

- `map.ron` is the authoritative logical map: dimensions, ordered terrain regions, garage lanes and bays, facilities, starter fields, and starter fleet.
- `../../art/pixel/maps/verdant-paddy/base-map.png` is the generated 2048×2048 visual base for the 64×64 grid at 32 pixels per tile.
- Region order is significant. Later rectangles intentionally replace earlier terrain for entrances, farm roads, and culverts.
- Runtime state is layered over the base image in this order: terrain state, field zones, crops, facilities, robots, work effects, interaction markers.
- Collision and pathfinding always use `map.ron`; decorative pixels never change navigation rules.
