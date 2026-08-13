use autofarm_ai::{MockLlmProvider, run_npc_turn};
use bevy::{prelude::*, time::Fixed};

use crate::state::{GameSession, ScreenMode};

pub struct SimulationPlugin;

impl Plugin for SimulationPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Time::<Fixed>::from_hz(10.0))
            .add_systems(FixedUpdate, tick_simulation)
            .add_systems(Update, detect_trial_completion);
    }
}

fn tick_simulation(mut session: ResMut<GameSession>) {
    if !matches!(session.screen, ScreenMode::Playing | ScreenMode::Editor) {
        return;
    }
    session.simulation.tick();
    let minute = session.simulation.clock.minute;
    if minute.saturating_sub(session.last_npc_review) >= 60 {
        let npc_id = if (minute / 60).is_multiple_of(2) {
            "aster"
        } else {
            "mira"
        };
        match run_npc_turn(&mut session.simulation, npc_id, &MockLlmProvider) {
            Ok(decision) => session.status = decision.message,
            Err(error) => session.status = format!("NPC planner fallback failed: {error}"),
        }
        session.last_npc_review = minute;
    }
}

fn detect_trial_completion(mut session: ResMut<GameSession>) {
    let finished = session
        .simulation
        .autonomy_trial
        .as_ref()
        .is_some_and(|trial| trial.finished);
    if finished && !session.report_seen {
        session.screen = ScreenMode::TrialReport;
        session.simulation.clock.paused = true;
        session.report_seen = true;
    }
}
