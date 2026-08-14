use std::fs;

use autofarm_ai::{MockLlmProvider, run_npc_turn};
use autofarm_sim::{CommandActor, FacilityKind, FarmCommand, GameCommand, GameSimulation, TilePos};
use bevy::{app::AppExit, input::mouse::AccumulatedMouseScroll, prelude::*, window::PrimaryWindow};

use crate::{
    render::world_tile,
    state::{GameSession, MenuRoot, ScreenMode, UiAction, UiActionQueue, WorldCamera},
};

const SAVE_PATH: &str = "autofarm-save.ron";

pub struct ControlsPlugin;

impl Plugin for ControlsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                handle_session_keys,
                process_ui_actions,
                move_camera,
                zoom_camera,
                handle_world_pointer,
            ),
        );
    }
}

fn handle_session_keys(
    mut commands: Commands,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut session: ResMut<GameSession>,
    menu: Query<Entity, With<MenuRoot>>,
    mut exit: MessageWriter<AppExit>,
) {
    if session.screen == ScreenMode::MainMenu {
        if keyboard.just_pressed(KeyCode::Enter) {
            for entity in &menu {
                commands.entity(entity).despawn();
            }
            session.screen = ScreenMode::Playing;
            session.status =
                "Rice cell online. Watch prepare → flood → transplant → protect → harvest."
                    .to_owned();
        }
        if keyboard.just_pressed(KeyCode::Escape) {
            exit.write(AppExit::Success);
        }
        return;
    }

    if session.screen == ScreenMode::TrialReport {
        if keyboard.just_pressed(KeyCode::Enter) || keyboard.just_pressed(KeyCode::Escape) {
            session.screen = ScreenMode::Playing;
            session.simulation.clock.paused = false;
            session.status = "Autonomy report archived.".to_owned();
        }
        return;
    }

    if session.screen == ScreenMode::Editor {
        let control =
            keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::ControlRight);
        if control && keyboard.just_pressed(KeyCode::Enter) {
            let mut editor = std::mem::take(&mut session.editor);
            let result = editor.apply(&mut session.simulation);
            session.editor = editor;
            match result {
                Ok(plan) => {
                    session.status = format!("Editor applied: {}", plan.summary);
                    session.screen = ScreenMode::Playing;
                }
                Err(error) => session.status = format!("Editor rejected: {error}"),
            }
        }
        if keyboard.just_pressed(KeyCode::KeyU) {
            let mut editor = std::mem::take(&mut session.editor);
            let result = editor.undo(&mut session.simulation);
            session.editor = editor;
            session.status = match result {
                Ok(()) => "Editor change undone.".to_owned(),
                Err(error) => format!("Undo unavailable: {error}"),
            };
        }
        if keyboard.just_pressed(KeyCode::Escape) || keyboard.just_pressed(KeyCode::F1) {
            session.editor.cancel();
            session.screen = ScreenMode::Playing;
            session.status = "Editor preview cancelled.".to_owned();
        }
        return;
    }

    if keyboard.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
    if keyboard.just_pressed(KeyCode::Space) {
        session.simulation.clock.paused = !session.simulation.clock.paused;
        session.status = if session.simulation.clock.paused {
            "Simulation paused.".to_owned()
        } else {
            "Simulation resumed.".to_owned()
        };
    }
    for (key, speed) in [
        (KeyCode::Digit1, 1),
        (KeyCode::Digit2, 8),
        (KeyCode::Digit3, 64),
        (KeyCode::Digit4, 0),
    ] {
        if keyboard.just_pressed(key) {
            session.simulation.clock.paused = speed == 0;
            session.simulation.clock.speed = speed.max(1);
            session.status = format!("Simulation speed: {speed}x");
        }
    }
    for (key, action) in [
        (KeyCode::KeyF, UiAction::CycleCrop),
        (KeyCode::KeyR, UiAction::BuyRobot),
        (KeyCode::KeyB, UiAction::BuildFacility),
        (KeyCode::KeyN, UiAction::NpcReview),
        (KeyCode::KeyT, UiAction::StartTrial),
        (KeyCode::F1, UiAction::ToggleEditor),
        (KeyCode::KeyS, UiAction::Save),
        (KeyCode::KeyL, UiAction::Load),
    ] {
        if keyboard.just_pressed(key) {
            execute_action(&mut session, action);
        }
    }
    if keyboard.just_pressed(KeyCode::KeyU) {
        let mut editor = std::mem::take(&mut session.editor);
        let result = editor.undo(&mut session.simulation);
        session.editor = editor;
        session.status = match result {
            Ok(()) => "Editor change undone.".to_owned(),
            Err(error) => format!("Undo unavailable: {error}"),
        };
    }
}

fn process_ui_actions(mut session: ResMut<GameSession>, mut queue: ResMut<UiActionQueue>) {
    for action in std::mem::take(&mut queue.0) {
        execute_action(&mut session, action);
    }
}

fn execute_action(session: &mut GameSession, action: UiAction) {
    match action {
        UiAction::CycleCrop => {
            session.selected_crop = (session.selected_crop + 1) % 5;
            session.status = format!("Field crop selected: {}", session.crop_id());
        }
        UiAction::BuyRobot => buy_robot(session),
        UiAction::BuildFacility => build_facility(session),
        UiAction::NpcReview => run_manager_review(session),
        UiAction::StartTrial => start_trial(session),
        UiAction::ToggleEditor => toggle_editor(session),
        UiAction::Save => save_game(session),
        UiAction::Load => load_game(session),
    }
}

fn buy_robot(session: &mut GameSession) {
    let robot_ids = [
        "paddy_rover",
        "rice_transplanter",
        "pest_control_drone",
        "rice_harvester",
        "basic_rover",
        "pollination_drone",
        "field_quadruped",
        "biped_farmhand",
    ];
    let robot_id = robot_ids[session.robot_purchase_cursor % robot_ids.len()];
    let envelope = session.simulation.next_command(
        CommandActor::Human,
        GameCommand::Farm(FarmCommand::PurchaseRobot {
            robot_def_id: robot_id.to_owned(),
        }),
    );
    match session.simulation.apply_command(envelope) {
        Ok(()) => {
            session.robot_purchase_cursor += 1;
            session.status = format!("Purchased {robot_id}.");
        }
        Err(error) => session.status = format!("Purchase rejected: {error}"),
    }
}

fn build_facility(session: &mut GameSession) {
    let candidates = [
        (FacilityKind::SeedStorage, TilePos::new(25, 20)),
        (FacilityKind::IrrigationNode, TilePos::new(23, 20)),
        (FacilityKind::SolarGenerator, TilePos::new(27, 20)),
        (FacilityKind::Battery, TilePos::new(29, 20)),
        (FacilityKind::Packer, TilePos::new(25, 22)),
    ];
    let missing: Vec<_> = candidates
        .into_iter()
        .filter(|(kind, _)| {
            !session
                .simulation
                .facilities
                .iter()
                .any(|facility| facility.kind == *kind)
        })
        .collect();
    let Some((kind, position)) = missing
        .get(session.facility_build_cursor % missing.len().max(1))
        .copied()
    else {
        session.status = "All starter facilities are already online.".to_owned();
        return;
    };
    let envelope = session.simulation.next_command(
        CommandActor::Human,
        GameCommand::Farm(FarmCommand::BuildFacility { kind, position }),
    );
    match session.simulation.apply_command(envelope) {
        Ok(()) => {
            session.status = format!("Built {kind:?} at {},{}.", position.x, position.y);
            session.facility_build_cursor += 1;
        }
        Err(error) => session.status = format!("Build rejected: {error}"),
    }
}

fn run_manager_review(session: &mut GameSession) {
    let npc_id = ["aster", "mira"][session.npc_cursor % 2];
    match run_npc_turn(&mut session.simulation, npc_id, &MockLlmProvider) {
        Ok(decision) => {
            session.status = format!("[{npc_id}] {}", decision.message);
            session.npc_cursor += 1;
        }
        Err(error) => session.status = format!("NPC decision rejected: {error}"),
    }
}

fn start_trial(session: &mut GameSession) {
    let envelope = session.simulation.next_command(
        CommandActor::Human,
        GameCommand::Farm(FarmCommand::StartAutonomyTrial {
            duration_minutes: 1_440,
        }),
    );
    match session.simulation.apply_command(envelope) {
        Ok(()) => {
            session.simulation.clock.paused = false;
            session.report_seen = false;
            session.status = "One-day Autonomy Trial started. Hands off the controls!".to_owned();
        }
        Err(error) => session.status = format!("Trial unavailable: {error}"),
    }
}

fn toggle_editor(session: &mut GameSession) {
    if session.screen == ScreenMode::Editor {
        session.editor.cancel();
        session.screen = ScreenMode::Playing;
        return;
    }
    session.editor.cancel();
    session.screen = ScreenMode::Editor;
    session.status = "Type an editor request, then press Enter to preview.".to_owned();
}

fn save_game(session: &mut GameSession) {
    match session.simulation.to_ron().and_then(|data| {
        fs::write(SAVE_PATH, data)
            .map_err(|error| autofarm_sim::SimulationError::Serialization(error.to_string()))
    }) {
        Ok(()) => session.status = format!("Saved to {SAVE_PATH}."),
        Err(error) => session.status = format!("Save failed: {error}"),
    }
}

fn load_game(session: &mut GameSession) {
    let result = fs::read_to_string(SAVE_PATH)
        .map_err(|error| error.to_string())
        .and_then(|data| GameSimulation::from_ron(&data).map_err(|error| error.to_string()));
    match result {
        Ok(simulation) => {
            session.simulation = simulation;
            session.editor = Default::default();
            session.report_seen = session
                .simulation
                .autonomy_trial
                .as_ref()
                .is_some_and(|trial| trial.finished);
            session.status = format!("Loaded {SAVE_PATH}.");
        }
        Err(error) => session.status = format!("Load failed: {error}"),
    }
}

fn move_camera(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    session: Res<GameSession>,
    mut camera: Single<&mut Transform, With<WorldCamera>>,
) {
    if !matches!(session.screen, ScreenMode::Playing | ScreenMode::Editor) {
        return;
    }
    let mut direction = Vec2::ZERO;
    if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
        direction.y += 1.0;
    }
    if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
        direction.y -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
        direction.x -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
        direction.x += 1.0;
    }
    if direction != Vec2::ZERO {
        let delta = direction.normalize() * 520.0 * time.delta_secs();
        camera.translation.x = (camera.translation.x + delta.x).clamp(-920.0, 920.0);
        camera.translation.y = (camera.translation.y + delta.y).clamp(-920.0, 920.0);
    }
}

fn zoom_camera(
    scroll: Res<AccumulatedMouseScroll>,
    session: Res<GameSession>,
    mut projection: Single<&mut Projection, With<WorldCamera>>,
) {
    if !matches!(session.screen, ScreenMode::Playing | ScreenMode::Editor) || scroll.delta.y == 0.0
    {
        return;
    }
    if let Projection::Orthographic(orthographic) = &mut **projection {
        orthographic.scale = (orthographic.scale * (1.0 - scroll.delta.y * 0.08)).clamp(0.48, 1.8);
    }
}

fn handle_world_pointer(
    mouse: Res<ButtonInput<MouseButton>>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<WorldCamera>>,
    mut session: ResMut<GameSession>,
) {
    if session.screen != ScreenMode::Playing {
        return;
    }
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    if cursor.x < 220.0
        || cursor.x > window.width() - 310.0
        || cursor.y < 56.0
        || cursor.y > window.height() - 100.0
    {
        return;
    }
    let (camera, transform) = *camera;
    let Ok(world) = camera.viewport_to_world_2d(transform, cursor) else {
        return;
    };
    let Some(tile) = world_tile(world) else {
        return;
    };
    if mouse.just_pressed(MouseButton::Left) {
        session.drag_start = Some(tile);
        session.selected_tile = Some(tile);
    }
    if mouse.just_released(MouseButton::Left) {
        let Some(start) = session.drag_start.take() else {
            return;
        };
        let origin = TilePos::new(start.x.min(tile.x), start.y.min(tile.y));
        let size = (
            start.x.abs_diff(tile.x).saturating_add(1).min(16),
            start.y.abs_diff(tile.y).saturating_add(1).min(16),
        );
        let crop_id = session.crop_id().to_owned();
        let envelope = session.simulation.next_command(
            CommandActor::Human,
            GameCommand::Farm(FarmCommand::CreateFieldZone {
                origin,
                size,
                crop_id: crop_id.clone(),
            }),
        );
        match session.simulation.apply_command(envelope) {
            Ok(()) => {
                session.status = format!(
                    "Created {} field: {}x{} at {},{}.",
                    crop_id, size.0, size.1, origin.x, origin.y
                );
            }
            Err(error) => session.status = format!("Field rejected: {error}"),
        }
    }
}
