# AUTOFARM

AUTOFARM is a playable 2D automation-farming vertical slice built with Rust and Bevy 0.19. The starter scenario follows one autonomous rice cell from paddy preparation through transplanting, pest control, harvest, packing, and delivery.

## Run

macOS and a current stable Rust toolchain are required.

```bash
cargo run
```

The default `MockLlmProvider` needs no API key. Remote-provider settings are read from `LLM_PROVIDER`, `LLM_BASE_URL`, `LLM_API_KEY`, and `LLM_MODEL`; failed or missing remote configuration always falls back to the deterministic planner.

## Controls

| Input | Action |
| --- | --- |
| `Enter` | Start a new game from the title screen |
| `WASD` / arrows | Pan the farm camera |
| Mouse wheel | Zoom |
| `Space` | Pause / resume |
| `1`, `2`, `3`, `4` | Set speed to 1x / 8x / 64x / pause |
| `F` | Cycle the crop selected for a new field |
| Left drag | Create the selected crop field |
| `R` | Purchase the next robot body |
| `B` | Build the next missing facility |
| `N` | Ask the active Mock NPC manager for a decision |
| `T` | Start a one-day Autonomy Trial |
| `F1` | Toggle the Developer AI Editor |
| `Enter` in editor | Preview the typed natural-language request |
| `Ctrl+Enter` in editor | Apply the previewed plan |
| `U` | Undo the last editor plan |
| `S` / `L` | Save / load `autofarm-save.ron` |
| `Esc` | Close overlays or quit |

The left build panel and bottom hint bar mirror these controls. The camera opens on a generated 2048×2048 production map with two large 11×28 and 13×28 paddies on a 64×64 logical grid. It includes raised bunds, field entrances, stone service roads, feeder and drainage channels, culvert crossings, a four-bay robot garage, a utility yard, six visible rice growth stages, and a role-specific fleet:

- the wheeled paddy rover leaves its garage to deep-plough with a moldboard, changes to a rotary tiller and leveling board, then floods empty paddies before planting;
- the six-legged spider transplanter places seedlings, irrigates planted rows, removes weeds, and loosens compacted mud;
- the quadcopter alternates targeted spray and laser pest control;
- the tracked harvester collects mature golden rice.

Wheeled preparation equipment cannot cross irrigation channels, drive over paddy bunds, or path through planted tiles. Robot movement uses continuous sub-tile progress; ground and air fleets reserve both occupied and incoming tiles so they queue at the garage exit and culverts instead of overlapping. Work follows whole-field `Plow → Till → Flood → Transplant` gates in 3×3 operating patches; each job stops for tool deployment, performs its work cycle, pauses to stow equipment, and only returns to its assigned garage bay after nearby work runs out. Once seedlings are present, only legged machines enter until a mature crop creates a harvester job.

The calendar uses four 28-day seasons. Rice takes roughly 18 in-game days to move from transplanting to a harvest-ready golden stage and stops growing in winter. Select a tile to inspect crop age, stage days, paddy water, weeds, soil compaction, pest pressure, and queued work.

Runtime art follows the conventions in `assets/art/README.md`; the RON manifest records every pixel asset's path, dimensions, frame layout, purpose, and provenance.

## Architecture

The workspace follows an acyclic dependency graph:

```text
autofarm-sim <- autofarm-ai <- autofarm-editor <- autofarm-app
```

- `src/sim`: deterministic 10 Hz-compatible simulation, grid crops, jobs, robots, logistics, economy, commands, trials, and versioned saves.
- `src/ai`: provider boundary, snapshots, personality-aware Mock NPC planners, remote configuration, and rule-based fallback.
- `src/editor`: natural-language demo intents, permission-aware preview/apply, world-revision checks, and undo.
- `src/app`: Bevy plugins, 2D rendering, UI, camera/input, and visual feedback.
- `assets/data`: RON content definitions for crops, robots, and contracts.
- `assets/maps`: data-driven logical maps, garage/facility placement, navigation terrain, and starter scenarios.
- `assets/art`: generated project artwork for the title screen, rice stages, paddy water, and the specialist robot fleet.

The LLM boundary never receives raw ECS state and never mutates the world. AI output becomes a typed `GameCommand`, passes permission and world-revision validation, and only then enters the simulation.

## Verify

```bash
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```
