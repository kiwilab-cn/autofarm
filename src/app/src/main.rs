mod controls;
mod render;
mod simulation;
mod state;
mod theme;
mod ui;

use autofarm_sim::GameSimulation;
use bevy::{prelude::*, window::WindowResolution};

use crate::{
    controls::ControlsPlugin, render::FarmRenderPlugin, simulation::SimulationPlugin,
    state::GameSession, ui::GameUiPlugin,
};

fn main() -> anyhow::Result<()> {
    let simulation = GameSimulation::new(0xA170_F4A2)?;
    let smoke_mode = std::env::var_os("AUTOFARM_SMOKE").is_some();
    let mut session = GameSession::new(simulation);
    if smoke_mode {
        session.screen = state::ScreenMode::Playing;
        session.simulation.clock.speed = 64;
        session.status = "Automated runtime smoke test.".to_owned();
    }
    let mut app = App::new();
    app.insert_resource(ClearColor(theme::BACKGROUND))
        .insert_resource(session)
        .add_plugins(
            DefaultPlugins
                .set(AssetPlugin {
                    file_path: "../../assets".to_owned(),
                    ..default()
                })
                .set(ImagePlugin::default_nearest())
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "AUTOFARM — Autonomous Agriculture".to_owned(),
                        resolution: WindowResolution::new(1440, 900),
                        resizable: true,
                        ..default()
                    }),
                    ..default()
                }),
        )
        .add_plugins((
            SimulationPlugin,
            FarmRenderPlugin,
            GameUiPlugin,
            ControlsPlugin,
        ));
    if smoke_mode {
        app.insert_resource(SmokeFrames::default())
            .add_systems(Update, exit_after_smoke_test);
    }
    app.run();
    Ok(())
}

#[derive(Resource, Default)]
struct SmokeFrames(u16);

fn exit_after_smoke_test(
    mut frames: ResMut<SmokeFrames>,
    mut exit: MessageWriter<bevy::app::AppExit>,
) {
    frames.0 += 1;
    if frames.0 >= 180 {
        exit.write(bevy::app::AppExit::Success);
    }
}
