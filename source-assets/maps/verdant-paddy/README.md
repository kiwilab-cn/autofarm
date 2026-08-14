# Verdant Paddy source art

- `concept-v1.png` — rejected whole-map runtime approach, retained only for palette, materials, and composition reference.
- `terrain-atlas-source.png` — built-in ImageGen 4×4 terrain material source.
- `facility-atlas-source.png` — built-in ImageGen 3×3 facility source on `#ff00ff` chroma key.
- `infrastructure-atlas-source.png` — generated bund/path/channel/culvert source.

Final terrain prompt: strict 4×4 equal-cell top-down 16-bit pixel atlas containing meadow, soil states, stone road, dirt path, concrete, timber, channel/river water, bund, and entrance materials; seamless cells; no scene, labels, objects, or cross-cell detail.

Final facility prompt: strict 3×3 equal-cell top-down 16-bit agricultural facility atlas containing garage, warehouse, charger, dock, packer, solar, battery, pump, and irrigation controller; uniform `#ff00ff` background; no perspective, cast shadows, text, robots, or cross-cell objects.

Both source images were generated with the built-in ImageGen tool. Run `tools/build_map_assets.sh` to create runtime atlases and the Tiled package.
