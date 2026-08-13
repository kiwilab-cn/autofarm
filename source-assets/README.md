# Source assets

This directory contains editable or generated art inputs. The Bevy `AssetServer` never loads files from here. Runtime-ready outputs are rebuilt into `assets/art/` and described by `assets/art/manifest.ron`.

Rules:

- keep map concepts separate from grid-aligned atlas sources;
- preserve the original generation output for non-destructive iteration;
- compile chroma keys, dimensions, frame grids, and Tiled data with versioned tools;
- do not hand-edit compiled runtime atlases.
