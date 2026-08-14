use autofarm_ai::LlmRequestParams;
use autofarm_sim::{
    CommandActor, CommandError, EditorCommand, FacilityKind, GameCommand, GameSimulation, TilePos,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PreviewKind {
    Field,
    Facility,
    Robot,
    Environment,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewMarker {
    pub kind: PreviewKind,
    pub position: TilePos,
    pub size: (u32, u32),
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorPlan {
    pub summary: String,
    pub rationale: String,
    pub expected_world_revision: u64,
    pub commands: Vec<EditorCommand>,
    pub preview: Vec<PreviewMarker>,
    pub params: LlmRequestParams,
}

#[derive(Debug, Error)]
pub enum EditorError {
    #[error("editor could not understand the requested change")]
    UnsupportedIntent,
    #[error("there is no previewed plan to apply")]
    NoPendingPlan,
    #[error("there is no editor change to undo")]
    NoUndoHistory,
    #[error(transparent)]
    Command(#[from] CommandError),
}

#[derive(Debug, Default, Clone)]
pub struct EditorController {
    pending: Option<EditorPlan>,
    history: Vec<GameSimulation>,
}

impl EditorController {
    pub fn preview(
        &mut self,
        prompt: &str,
        simulation: &GameSimulation,
    ) -> Result<&EditorPlan, EditorError> {
        let plan = plan_for_intent(prompt, simulation.world_revision)?;
        self.pending = Some(plan);
        self.pending.as_ref().ok_or(EditorError::NoPendingPlan)
    }

    #[must_use]
    pub const fn pending(&self) -> Option<&EditorPlan> {
        self.pending.as_ref()
    }

    pub fn cancel(&mut self) {
        self.pending = None;
    }

    pub fn apply(&mut self, simulation: &mut GameSimulation) -> Result<EditorPlan, EditorError> {
        let Some(plan) = self.pending.take() else {
            return Err(EditorError::NoPendingPlan);
        };
        if plan.expected_world_revision != simulation.world_revision {
            return Err(EditorError::Command(CommandError::StaleRevision {
                expected: plan.expected_world_revision,
                current: simulation.world_revision,
            }));
        }

        let before = simulation.clone();
        for command in plan.commands.clone() {
            let envelope =
                simulation.next_command(CommandActor::EditorAi, GameCommand::Editor(command));
            if let Err(error) = simulation.apply_command(envelope) {
                *simulation = before;
                return Err(EditorError::Command(error));
            }
        }
        self.history.push(before);
        Ok(plan)
    }

    pub fn undo(&mut self, simulation: &mut GameSimulation) -> Result<(), EditorError> {
        let Some(mut previous) = self.history.pop() else {
            return Err(EditorError::NoUndoHistory);
        };
        previous.world_revision = simulation.world_revision + 1;
        *simulation = previous;
        self.pending = None;
        Ok(())
    }

    #[must_use]
    pub fn undo_depth(&self) -> usize {
        self.history.len()
    }
}

pub fn plan_for_intent(prompt: &str, revision: u64) -> Result<EditorPlan, EditorError> {
    let normalized = prompt.trim().to_lowercase();
    let params = LlmRequestParams {
        temperature: 0.2,
        ..LlmRequestParams::default()
    };
    if normalized.contains("rice") || normalized.contains("水稻") || normalized.contains("稻田")
    {
        return Ok(EditorPlan {
            summary: "Create a second autonomous rice cell".to_owned(),
            rationale: "The plan pairs a flooded paddy with the four specialist machines required for preparation, transplanting, protection, and harvest.".to_owned(),
            expected_world_revision: revision,
            commands: vec![
                EditorCommand::CreateFieldZone {
                    origin: TilePos::new(10, 2),
                    size: (10, 8),
                    crop_id: "rice".to_owned(),
                },
                EditorCommand::SpawnRobot {
                    robot_def_id: "paddy_rover".to_owned(),
                    count: 1,
                    position: TilePos::new(53, 13),
                },
                EditorCommand::SpawnRobot {
                    robot_def_id: "rice_transplanter".to_owned(),
                    count: 1,
                    position: TilePos::new(53, 13),
                },
                EditorCommand::SpawnRobot {
                    robot_def_id: "pest_control_drone".to_owned(),
                    count: 1,
                    position: TilePos::new(53, 13),
                },
                EditorCommand::SpawnRobot {
                    robot_def_id: "rice_harvester".to_owned(),
                    count: 1,
                    position: TilePos::new(53, 13),
                },
            ],
            preview: vec![
                PreviewMarker {
                    kind: PreviewKind::Field,
                    position: TilePos::new(10, 2),
                    size: (10, 8),
                    label: "Flooded Rice Paddy".to_owned(),
                },
                PreviewMarker {
                    kind: PreviewKind::Robot,
                    position: TilePos::new(53, 13),
                    size: (2, 2),
                    label: "Rice Specialist Fleet".to_owned(),
                },
            ],
            params,
        });
    }
    if normalized.contains("tomato") || normalized.contains("番茄") {
        return Ok(EditorPlan {
            summary: "Create the north tomato automation cell".to_owned(),
            rationale: "The zone includes irrigation coverage and two pollination drones so its crop-specific bottleneck is visible immediately.".to_owned(),
            expected_world_revision: revision,
            commands: vec![
                EditorCommand::CreateFieldZone {
                    origin: TilePos::new(1, 20),
                    size: (4, 12),
                    crop_id: "tomato".to_owned(),
                },
                EditorCommand::PlaceBuilding {
                    kind: FacilityKind::IrrigationNode,
                    position: TilePos::new(5, 25),
                },
                EditorCommand::SpawnRobot {
                    robot_def_id: "pollination_drone".to_owned(),
                    count: 2,
                    position: TilePos::new(5, 27),
                },
            ],
            preview: vec![
                PreviewMarker {
                    kind: PreviewKind::Field,
                    position: TilePos::new(1, 20),
                    size: (4, 12),
                    label: "Tomato Field".to_owned(),
                },
                PreviewMarker {
                    kind: PreviewKind::Facility,
                    position: TilePos::new(5, 25),
                    size: (1, 1),
                    label: "Irrigation".to_owned(),
                },
                PreviewMarker {
                    kind: PreviewKind::Robot,
                    position: TilePos::new(5, 27),
                    size: (1, 1),
                    label: "2 x Pollen Drone".to_owned(),
                },
            ],
            params,
        });
    }
    if normalized.contains("wheat") || normalized.contains("小麦") {
        return Ok(EditorPlan {
            summary: "Create a west wheat production field".to_owned(),
            rationale: "A compact field fits the existing rover fleet and provides stable contract throughput.".to_owned(),
            expected_world_revision: revision,
            commands: vec![EditorCommand::CreateFieldZone {
                origin: TilePos::new(1, 38),
                size: (4, 6),
                crop_id: "wheat".to_owned(),
            }],
            preview: vec![PreviewMarker {
                kind: PreviewKind::Field,
                position: TilePos::new(1, 38),
                size: (4, 6),
                label: "Wheat Field".to_owned(),
            }],
            params,
        });
    }
    if normalized.contains("drone") || normalized.contains("无人机") {
        return Ok(EditorPlan {
            summary: "Add one pollination drone".to_owned(),
            rationale:
                "The extra flight unit reduces pollination backlog without changing field geometry."
                    .to_owned(),
            expected_world_revision: revision,
            commands: vec![EditorCommand::SpawnRobot {
                robot_def_id: "pollination_drone".to_owned(),
                count: 1,
                position: TilePos::new(31, 29),
            }],
            preview: vec![PreviewMarker {
                kind: PreviewKind::Robot,
                position: TilePos::new(31, 29),
                size: (1, 1),
                label: "Pollen Drone".to_owned(),
            }],
            params,
        });
    }
    Err(EditorError::UnsupportedIntent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_apply_and_undo_are_transactional() -> Result<(), Box<dyn std::error::Error>> {
        let mut simulation = GameSimulation::new(123)?;
        let initial_zone_count = simulation.zones.len();
        let initial_robot_count = simulation.robots.len();
        let mut editor = EditorController::default();
        let preview = editor.preview("create tomato field", &simulation)?;
        assert_eq!(preview.commands.len(), 3);
        assert_eq!(simulation.zones.len(), initial_zone_count);

        editor.apply(&mut simulation)?;
        assert_eq!(simulation.zones.len(), initial_zone_count + 1);
        assert_eq!(simulation.robots.len(), initial_robot_count + 2);

        editor.undo(&mut simulation)?;
        assert_eq!(simulation.zones.len(), initial_zone_count);
        assert_eq!(simulation.robots.len(), initial_robot_count);
        Ok(())
    }

    #[test]
    fn stale_preview_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
        let mut simulation = GameSimulation::new(123)?;
        let mut editor = EditorController::default();
        editor.preview("add drone", &simulation)?;
        simulation.world_revision += 1;
        assert!(matches!(
            editor.apply(&mut simulation),
            Err(EditorError::Command(CommandError::StaleRevision { .. }))
        ));
        Ok(())
    }
}
