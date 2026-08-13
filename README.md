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
| `1`, `2`, `3`, `4` | Set speed to 1x / 4x / 16x / pause |
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

The left build panel and bottom hint bar mirror these controls. The camera opens on a 10×8 flooded paddy with large 32-pixel tiles, six visible rice growth stages, and a role-specific fleet:

- the wheeled paddy rover tills, floods, and maintains water levels;
- the six-legged transplanter places seedlings;
- the quadcopter sprays or uses its laser to clear pests;
- the tracked harvester collects mature golden rice.

All four machines claim typed jobs automatically. Select a tile to inspect paddy water, crop health, pest pressure, current growth stage, and queued work.

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
- `assets/art`: generated project artwork for the title screen, rice stages, paddy water, and the specialist robot fleet.

The LLM boundary never receives raw ECS state and never mutates the world. AI output becomes a typed `GameCommand`, passes permission and world-revision validation, and only then enters the simulation.

## Verify

```bash
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```
