# Map authoring standard

AUTOFARM maps use Tiled's JSON formats as their editable source of truth:

- `autofarm.tiled-project` defines shared object classes and property schemas.
- `tilesets/*.tsj` are external, reusable tileset definitions.
- `<map-id>/<map-id>.tmj` owns ordered visual layers and gameplay object layers.
- Runtime atlases live under `assets/art/pixel/tilesets/`; editable generation sources live under `source-assets/`.

Required visual layers are `Ground`, `Terrain`, `Infrastructure`, and `Decoration`. Required gameplay layers are grouped under `Gameplay`: `Structures`, `Field Zones`, `Spawns`, `Navigation`, and hidden `Collision`.

Tile layers are rendered in authored order and compile into simulation terrain. Object layers provide semantic anchors, extents, movement costs, and collision regions; decoration never changes navigation. Run `tools/build_map_assets.sh` to rebuild the deterministic package and processed atlases.
