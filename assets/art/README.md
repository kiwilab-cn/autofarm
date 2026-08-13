# Art asset conventions

- Runtime paths are relative to `assets/`; source code never references generated-image or temporary paths.
- Use lowercase kebab-case names and group pixel art by `maps/<map-id>`, `environment`, `crops`, and `robots/<model>`.
- Runtime sprites are PNG. Pixel sprites use RGBA where transparency is needed and nearest-neighbor sampling.
- Animation and environment atlases use a 4×2 grid of 128×128 cells (`512×256` total).
- Full-map backgrounds use one image at the map's exact runtime pixel size (`grid width × tile size`, `grid height × tile size`). Dynamic water, worked soil, crops, robots, effects, selection, and editor previews remain separate render layers.
- Keep only final runtime assets in the repository. Raw generations, chroma-key intermediates, and processing scripts stay outside `assets/`.
- Record dimensions, frame layout, purpose, and provenance in `manifest.ron` whenever an art asset is added or replaced.
