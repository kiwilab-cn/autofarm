# AUTOFARM

AUTOFARM is a playable 2D automation-farming vertical slice built with Rust and Bevy 0.19. You design crop zones, expand a mixed robot fleet, let AI farm managers tune priorities, and prove that the farm can run without manual intervention.

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

The left build panel and bottom hint bar mirror these controls. The starter map already contains a small wheat field so the first autonomous production loop is visible immediately.

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
- `assets/art`: generated project artwork used by the title screen.

The LLM boundary never receives raw ECS state and never mutates the world. AI output becomes a typed `GameCommand`, passes permission and world-revision validation, and only then enters the simulation.

## Verify

```bash
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```
