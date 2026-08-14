# Art asset conventions

- Runtime paths are relative to `assets/`; source code never references generated-image or temporary paths.
- Use lowercase kebab-case names and group runtime pixel art by `tilesets`, `environment`, `crops`, and `robots/<model>`.
- Runtime sprites are PNG. Pixel sprites use RGBA where transparency is needed and nearest-neighbor sampling.
- Every atlas uses fixed-size cells with zero spacing and zero margin. Cell size, column count, frame count, and provenance are recorded in `manifest.ron`.
- Maps reference atlases through external Tiled `.tsj` files. Full-map illustrations are concept references only and never ship as collision-bearing runtime maps.
- Editable generations and concept references live under `source-assets/`; only processed runtime images live under `assets/`.
- Rebuild generated map atlases and Tiled data with `tools/build_map_assets.sh`.
- Record dimensions, frame layout, purpose, and provenance in `manifest.ron` whenever an art asset is added or replaced.
