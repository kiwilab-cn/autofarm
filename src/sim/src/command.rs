use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{CropDef, FacilityKind, TilePos, ZoneId};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum CommandActor {
    Human,
    EditorAi,
    Npc(String),
    Script,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandEnvelope {
    pub id: u64,
    pub actor: CommandActor,
    pub expected_world_revision: u64,
    pub command: GameCommand,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GameCommand {
    Farm(FarmCommand),
    Editor(EditorCommand),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FarmCommand {
    CreateFieldZone {
        origin: TilePos,
        size: (u32, u32),
        crop_id: String,
    },
    SetZoneCrop {
        zone_id: ZoneId,
        crop_id: String,
    },
    SetZonePriority {
        zone_id: ZoneId,
        priority: u8,
    },
    AssignNpc {
        npc_id: String,
        zone_id: ZoneId,
    },
    PurchaseRobot {
        robot_def_id: String,
    },
    SetRobotPolicy {
        robot_id: u64,
        reserve_battery_percent: u8,
    },
    BuildFacility {
        kind: FacilityKind,
        position: TilePos,
    },
    ChangeContractPriority {
        priority: u8,
    },
    StartAutonomyTrial {
        duration_minutes: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EditorCommand {
    PlaceBuilding {
        kind: FacilityKind,
        position: TilePos,
    },
    DeleteEntity {
        entity_id: u64,
    },
    CreateFieldZone {
        origin: TilePos,
        size: (u32, u32),
        crop_id: String,
    },
    SpawnRobot {
        robot_def_id: String,
        count: u8,
        position: TilePos,
    },
    PatchCropDefinition {
        definition: CropDef,
    },
    SetEnvironment {
        weather: crate::Weather,
    },
    GiveCredits {
        amount: i32,
    },
    StartContract {
        contract_id: String,
    },
    SetSimulationSpeed {
        speed: u8,
    },
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CommandError {
    #[error("world revision is stale: expected {expected}, current {current}")]
    StaleRevision { expected: u64, current: u64 },
    #[error("actor is not allowed to execute this command")]
    PermissionDenied,
    #[error("invalid command: {0}")]
    Invalid(String),
    #[error("not enough credits: need {required}, available {available}")]
    InsufficientCredits { required: i32, available: i32 },
    #[error("target not found: {0}")]
    NotFound(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPermissions {
    npc_farm_commands: BTreeSet<&'static str>,
}

impl Default for CommandPermissions {
    fn default() -> Self {
        Self {
            npc_farm_commands: BTreeSet::from([
                "set_zone_crop",
                "set_zone_priority",
                "assign_npc",
                "set_robot_policy",
                "change_contract_priority",
            ]),
        }
    }
}

impl CommandPermissions {
    #[must_use]
    pub fn allows(&self, actor: &CommandActor, command: &GameCommand) -> bool {
        match (actor, command) {
            (CommandActor::Human, GameCommand::Farm(_))
            | (CommandActor::Script, GameCommand::Farm(_))
            | (CommandActor::EditorAi, GameCommand::Editor(_)) => true,
            (CommandActor::Npc(_), GameCommand::Farm(command)) => {
                self.npc_farm_commands.contains(farm_command_name(command))
            }
            _ => false,
        }
    }
}

#[must_use]
pub const fn farm_command_name(command: &FarmCommand) -> &'static str {
    match command {
        FarmCommand::CreateFieldZone { .. } => "create_field_zone",
        FarmCommand::SetZoneCrop { .. } => "set_zone_crop",
        FarmCommand::SetZonePriority { .. } => "set_zone_priority",
        FarmCommand::AssignNpc { .. } => "assign_npc",
        FarmCommand::PurchaseRobot { .. } => "purchase_robot",
        FarmCommand::SetRobotPolicy { .. } => "set_robot_policy",
        FarmCommand::BuildFacility { .. } => "build_facility",
        FarmCommand::ChangeContractPriority { .. } => "change_contract_priority",
        FarmCommand::StartAutonomyTrial { .. } => "start_autonomy_trial",
    }
}
