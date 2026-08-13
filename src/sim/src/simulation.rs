use std::collections::{BTreeMap, BTreeSet};

use ron::ser::PrettyConfig;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::pathfinding::{next_ground_step, path_exists};
use crate::{
    ActiveContract, AutonomyReport, AutonomyTrial, Capability, CommandActor, CommandEnvelope,
    CommandError, CommandPermissions, ContentCatalog, ContentError, ContractStatus, CropInstance,
    EditorCommand, Facility, FacilityKind, FarmCommand, FarmEvent, FarmGrid, FarmSnapshot,
    FieldZone, GameCommand, GameMetrics, Inventory, Job, JobKind, JobStatus, MapDefinition,
    NpcAssignment, PowerGrid, Robot, RobotBody, RobotState, SimClock, TerrainKind, TilePos,
    WaterNetwork, Weather, calculate_autonomy_report,
};

pub const SAVE_VERSION: u32 = 6;
const MAX_EVENTS: usize = 80;
const IDLE_RETURN_DELAY_MINUTES: f32 = 12.0;

#[derive(Debug, Error)]
pub enum SimulationError {
    #[error(transparent)]
    Content(#[from] ContentError),
    #[error("map definition failed: {0}")]
    Map(String),
    #[error("save serialization failed: {0}")]
    Serialization(String),
    #[error("unsupported save version {found}; expected {expected}")]
    UnsupportedSaveVersion { found: u32, expected: u32 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameSimulation {
    pub version: u32,
    pub seed: u64,
    pub catalog: ContentCatalog,
    pub map_id: String,
    #[serde(skip, default)]
    pub map: MapDefinition,
    pub grid: FarmGrid,
    pub clock: SimClock,
    pub weather: Weather,
    pub credits: i32,
    pub reputation: u32,
    pub power: PowerGrid,
    pub water: WaterNetwork,
    pub inventory: Inventory,
    pub robots: Vec<Robot>,
    pub facilities: Vec<Facility>,
    pub zones: Vec<FieldZone>,
    pub jobs: Vec<Job>,
    pub contracts: Vec<ActiveContract>,
    pub current_contract: Option<usize>,
    pub npc_assignments: BTreeMap<String, NpcAssignment>,
    pub metrics: GameMetrics,
    pub autonomy_trial: Option<AutonomyTrial>,
    pub last_autonomy_report: Option<AutonomyReport>,
    pub events: Vec<FarmEvent>,
    pub world_revision: u64,
    next_entity_id: u64,
    next_job_id: u64,
    next_zone_id: u64,
    next_command_id: u64,
}

impl GameSimulation {
    pub fn new(seed: u64) -> Result<Self, SimulationError> {
        let catalog = ContentCatalog::load_embedded()?;
        let map = MapDefinition::load_embedded()
            .map_err(|error| SimulationError::Map(error.to_string()))?;
        let map_id = map.id.clone();
        let mut simulation = Self {
            version: SAVE_VERSION,
            seed,
            catalog: catalog.clone(),
            map_id,
            grid: FarmGrid::from_definition(&map),
            map,
            clock: SimClock::default(),
            weather: Weather::Clear,
            credits: 5_000,
            reputation: 0,
            power: PowerGrid {
                production: 40.0,
                consumption: 0.0,
                stored: 400.0,
                capacity: 800.0,
            },
            water: WaterNetwork {
                available_water: 1_000.0,
                max_flow: 40.0,
            },
            inventory: Inventory {
                items: BTreeMap::new(),
                capacity: 2_000,
            },
            robots: Vec::new(),
            facilities: Vec::new(),
            zones: Vec::new(),
            jobs: Vec::new(),
            contracts: catalog
                .contracts
                .iter()
                .enumerate()
                .map(|(index, definition)| ActiveContract {
                    definition: definition.clone(),
                    delivered: BTreeMap::new(),
                    accepted_at: 0,
                    deadline: 0,
                    status: if index == 0 {
                        ContractStatus::Active
                    } else {
                        ContractStatus::Locked
                    },
                    priority: 70,
                })
                .collect(),
            current_contract: Some(0),
            npc_assignments: default_npc_assignments(),
            metrics: GameMetrics {
                contracts_expected: 1,
                ..GameMetrics::default()
            },
            autonomy_trial: None,
            last_autonomy_report: None,
            events: Vec::new(),
            world_revision: 0,
            next_entity_id: 1,
            next_job_id: 1,
            next_zone_id: 1,
            next_command_id: 1,
        };

        simulation.activate_contract(0);
        for facility in simulation.map.starter_facilities.clone() {
            simulation.spawn_map_facility_unchecked(&facility);
        }
        for (robot_id, bay) in simulation
            .map
            .starter_robots
            .clone()
            .into_iter()
            .zip(simulation.map.garage_bays.clone())
        {
            simulation.spawn_robot_unchecked(&robot_id, bay);
        }
        for zone in simulation.map.starter_zones.clone() {
            let id = simulation.create_zone_unchecked(
                zone.origin,
                zone.size,
                &zone.crop_id,
                zone.priority,
            );
            if let Some(created) = simulation.zones.iter_mut().find(|created| created.id == id) {
                created.name = zone.name;
            }
        }
        simulation.push_event(FarmEvent::Info(
            "Rice cells online: garage departure, plough, rotary till, flood, transplant, protect, harvest.".to_owned(),
        ));
        Ok(simulation)
    }

    #[must_use]
    pub fn next_command(&mut self, actor: CommandActor, command: GameCommand) -> CommandEnvelope {
        let envelope = CommandEnvelope {
            id: self.next_command_id,
            actor,
            expected_world_revision: self.world_revision,
            command,
        };
        self.next_command_id += 1;
        envelope
    }

    pub fn validate_command(&self, envelope: &CommandEnvelope) -> Result<(), CommandError> {
        if envelope.expected_world_revision != self.world_revision {
            return Err(CommandError::StaleRevision {
                expected: envelope.expected_world_revision,
                current: self.world_revision,
            });
        }
        if !CommandPermissions::default().allows(&envelope.actor, &envelope.command) {
            return Err(CommandError::PermissionDenied);
        }

        match &envelope.command {
            GameCommand::Farm(command) => self.validate_farm_command(command),
            GameCommand::Editor(command) => self.validate_editor_command(command),
        }
    }

    pub fn apply_command(&mut self, envelope: CommandEnvelope) -> Result<(), CommandError> {
        self.validate_command(&envelope)?;
        let actor = envelope.actor.clone();
        let starts_trial = matches!(
            &envelope.command,
            GameCommand::Farm(FarmCommand::StartAutonomyTrial { .. })
        );
        match envelope.command {
            GameCommand::Farm(command) => self.execute_farm_command(command)?,
            GameCommand::Editor(command) => self.execute_editor_command(command)?,
        }

        match actor {
            CommandActor::Human => {
                self.metrics.manual_commands += 1;
                if let Some(trial) = self.autonomy_trial.as_mut()
                    && !trial.finished
                    && !starts_trial
                {
                    trial.manual_interventions += 1;
                }
            }
            CommandActor::Npc(_) => self.metrics.npc_commands += 1,
            CommandActor::EditorAi | CommandActor::Script => {}
        }
        self.world_revision += 1;
        Ok(())
    }

    fn validate_farm_command(&self, command: &FarmCommand) -> Result<(), CommandError> {
        match command {
            FarmCommand::CreateFieldZone {
                origin,
                size,
                crop_id,
            } => self.validate_new_zone(*origin, *size, crop_id),
            FarmCommand::SetZoneCrop { zone_id, crop_id } => {
                self.require_zone(*zone_id)?;
                self.require_crop(crop_id)
            }
            FarmCommand::SetZonePriority { zone_id, priority } => {
                self.require_zone(*zone_id)?;
                if *priority > 100 {
                    return Err(CommandError::Invalid(
                        "zone priority must be between 0 and 100".to_owned(),
                    ));
                }
                Ok(())
            }
            FarmCommand::AssignNpc { npc_id, zone_id } => {
                self.require_zone(*zone_id)?;
                if !self.npc_assignments.contains_key(npc_id) {
                    return Err(CommandError::NotFound(format!("NPC {npc_id}")));
                }
                Ok(())
            }
            FarmCommand::PurchaseRobot { robot_def_id } => {
                let definition = self.require_robot_def(robot_def_id)?;
                if self.available_garage_bay().is_none() {
                    return Err(CommandError::Invalid(
                        "robot garage has no free parking bay".to_owned(),
                    ));
                }
                self.require_credits(definition.purchase_price)
            }
            FarmCommand::SetRobotPolicy {
                robot_id,
                reserve_battery_percent,
            } => {
                if !self.robots.iter().any(|robot| robot.id == *robot_id) {
                    return Err(CommandError::NotFound(format!("robot {robot_id}")));
                }
                if *reserve_battery_percent > 90 {
                    return Err(CommandError::Invalid(
                        "battery reserve cannot exceed 90%".to_owned(),
                    ));
                }
                Ok(())
            }
            FarmCommand::BuildFacility { kind, position } => {
                self.validate_facility_position(*position)?;
                self.require_credits(kind.price())
            }
            FarmCommand::ChangeContractPriority { priority } => {
                if self.current_contract.is_none() || *priority > 100 {
                    return Err(CommandError::Invalid(
                        "active contract and priority 0..100 required".to_owned(),
                    ));
                }
                Ok(())
            }
            FarmCommand::StartAutonomyTrial { duration_minutes } => {
                if *duration_minutes == 0 {
                    return Err(CommandError::Invalid(
                        "trial duration must be positive".to_owned(),
                    ));
                }
                if self
                    .autonomy_trial
                    .as_ref()
                    .is_some_and(|trial| !trial.finished)
                {
                    return Err(CommandError::Invalid(
                        "an autonomy trial is already running".to_owned(),
                    ));
                }
                Ok(())
            }
        }
    }

    fn validate_editor_command(&self, command: &EditorCommand) -> Result<(), CommandError> {
        match command {
            EditorCommand::PlaceBuilding { position, .. } => {
                self.validate_facility_position(*position)
            }
            EditorCommand::DeleteEntity { entity_id } => {
                let exists = self.facilities.iter().any(|item| item.id == *entity_id)
                    || self.robots.iter().any(|item| item.id == *entity_id)
                    || self.zones.iter().any(|item| item.id == *entity_id);
                if exists {
                    Ok(())
                } else {
                    Err(CommandError::NotFound(format!("entity {entity_id}")))
                }
            }
            EditorCommand::CreateFieldZone {
                origin,
                size,
                crop_id,
            } => self.validate_new_zone(*origin, *size, crop_id),
            EditorCommand::SpawnRobot {
                robot_def_id,
                count,
                position,
            } => {
                self.require_robot_def(robot_def_id)?;
                if *count == 0 || !self.grid.contains(*position) {
                    return Err(CommandError::Invalid(
                        "robot count and spawn position are invalid".to_owned(),
                    ));
                }
                Ok(())
            }
            EditorCommand::PatchCropDefinition { definition } => {
                if definition.id.trim().is_empty() || definition.stages.len() < 2 {
                    return Err(CommandError::Invalid(
                        "crop definition needs an id and at least two stages".to_owned(),
                    ));
                }
                Ok(())
            }
            EditorCommand::SetEnvironment { .. } => Ok(()),
            EditorCommand::GiveCredits { amount } => {
                if *amount == 0 {
                    Err(CommandError::Invalid(
                        "credit adjustment cannot be zero".to_owned(),
                    ))
                } else {
                    Ok(())
                }
            }
            EditorCommand::StartContract { contract_id } => {
                if self
                    .contracts
                    .iter()
                    .any(|contract| contract.definition.id == *contract_id)
                {
                    Ok(())
                } else {
                    Err(CommandError::NotFound(format!("contract {contract_id}")))
                }
            }
            EditorCommand::SetSimulationSpeed { speed } => {
                if matches!(*speed, 0 | 1 | 8 | 64) {
                    Ok(())
                } else {
                    Err(CommandError::Invalid(
                        "simulation speed must be 0, 1, 8, or 64".to_owned(),
                    ))
                }
            }
        }
    }

    fn execute_farm_command(&mut self, command: FarmCommand) -> Result<(), CommandError> {
        match command {
            FarmCommand::CreateFieldZone {
                origin,
                size,
                crop_id,
            } => {
                self.create_zone_unchecked(origin, size, &crop_id, 60);
            }
            FarmCommand::SetZoneCrop { zone_id, crop_id } => {
                if let Some(zone) = self.zones.iter_mut().find(|zone| zone.id == zone_id) {
                    zone.crop_id = crop_id;
                }
            }
            FarmCommand::SetZonePriority { zone_id, priority } => {
                let change = self
                    .zones
                    .iter_mut()
                    .find(|zone| zone.id == zone_id)
                    .map(|zone| {
                        let before = zone.priority;
                        zone.priority = priority;
                        (zone.name.clone(), before)
                    });
                if let Some((name, before)) = change {
                    self.push_event(FarmEvent::Info(format!(
                        "Zone {} priority: {before} -> {priority}",
                        name
                    )));
                }
            }
            FarmCommand::AssignNpc { npc_id, zone_id } => {
                if let Some(zone) = self.zones.iter_mut().find(|zone| zone.id == zone_id) {
                    zone.manager = Some(npc_id.clone());
                }
                if let Some(npc) = self.npc_assignments.get_mut(&npc_id) {
                    npc.managed_zones.insert(zone_id);
                }
            }
            FarmCommand::PurchaseRobot { robot_def_id } => {
                let price = self.require_robot_def(&robot_def_id)?.purchase_price;
                self.credits -= price;
                let position = self.available_garage_bay().ok_or_else(|| {
                    CommandError::Invalid("robot garage has no free parking bay".to_owned())
                })?;
                self.spawn_robot_unchecked(&robot_def_id, position);
            }
            FarmCommand::SetRobotPolicy { robot_id, .. } => {
                self.push_event(FarmEvent::Info(format!(
                    "Robot {robot_id} reserve policy updated"
                )));
            }
            FarmCommand::BuildFacility { kind, position } => {
                self.credits -= kind.price();
                self.spawn_facility_unchecked(kind, position);
            }
            FarmCommand::ChangeContractPriority { priority } => {
                if let Some(index) = self.current_contract
                    && let Some(contract) = self.contracts.get_mut(index)
                {
                    contract.priority = priority;
                }
            }
            FarmCommand::StartAutonomyTrial { duration_minutes } => {
                self.start_autonomy_trial(duration_minutes);
            }
        }
        Ok(())
    }

    fn execute_editor_command(&mut self, command: EditorCommand) -> Result<(), CommandError> {
        match command {
            EditorCommand::PlaceBuilding { kind, position } => {
                self.spawn_facility_unchecked(kind, position);
            }
            EditorCommand::DeleteEntity { entity_id } => self.delete_entity(entity_id),
            EditorCommand::CreateFieldZone {
                origin,
                size,
                crop_id,
            } => {
                self.create_zone_unchecked(origin, size, &crop_id, 60);
            }
            EditorCommand::SpawnRobot {
                robot_def_id,
                count,
                position,
            } => {
                for _ in 0..count {
                    self.spawn_robot_unchecked(&robot_def_id, position)
                        .ok_or_else(|| {
                            CommandError::Invalid(format!(
                                "there is no collision-free spawn near ({}, {})",
                                position.x, position.y
                            ))
                        })?;
                }
            }
            EditorCommand::PatchCropDefinition { definition } => {
                self.catalog.crops.insert(definition.id.clone(), definition);
            }
            EditorCommand::SetEnvironment { weather } => self.weather = weather,
            EditorCommand::GiveCredits { amount } => {
                self.credits = self.credits.saturating_add(amount);
            }
            EditorCommand::StartContract { contract_id } => {
                if let Some(index) = self
                    .contracts
                    .iter()
                    .position(|contract| contract.definition.id == contract_id)
                {
                    self.activate_contract(index);
                }
            }
            EditorCommand::SetSimulationSpeed { speed } => {
                self.clock.paused = speed == 0;
                self.clock.speed = speed.max(1);
            }
        }
        Ok(())
    }

    pub fn tick(&mut self) {
        if self.clock.paused {
            return;
        }
        for _ in 0..self.clock.speed {
            self.simulate_minute();
        }
    }

    pub fn advance_minutes(&mut self, minutes: u64) {
        for _ in 0..minutes {
            self.simulate_minute();
        }
    }

    fn simulate_minute(&mut self) {
        self.clock.minute += 1;
        self.update_weather();
        self.update_environment();
        self.update_crops();
        self.generate_jobs();
        self.assign_jobs();
        self.update_robots();
        self.update_logistics();
        self.update_economy();
        self.update_utilities();
        self.finish_autonomy_trial_if_due();
        self.jobs
            .retain(|job| !matches!(job.status, JobStatus::Completed | JobStatus::Cancelled));
    }

    fn update_weather(&mut self) {
        if self.clock.minute.is_multiple_of(SimClock::MINUTES_PER_DAY) {
            let roll = (self.seed.wrapping_add(self.clock.day() * 17)) % 10;
            self.weather = match self.clock.season() {
                crate::Season::Spring => match roll {
                    0..=3 => Weather::Rain,
                    4 => Weather::Hot,
                    _ => Weather::Clear,
                },
                crate::Season::Summer => match roll {
                    0..=1 => Weather::Rain,
                    2..=5 => Weather::Hot,
                    _ => Weather::Clear,
                },
                crate::Season::Autumn => match roll {
                    0..=2 => Weather::Rain,
                    3 => Weather::Hot,
                    _ => Weather::Clear,
                },
                crate::Season::Winter => match roll {
                    0 => Weather::Rain,
                    _ => Weather::Clear,
                },
            };
            if self.clock.day_of_season() == 1 {
                self.push_event(FarmEvent::Info(format!(
                    "Year {} {:?} has begun.",
                    self.clock.year(),
                    self.clock.season()
                )));
            }
            self.push_event(FarmEvent::Info(format!(
                "{:?} day {} weather: {:?}",
                self.clock.season(),
                self.clock.day_of_season(),
                self.weather
            )));
        }
    }

    fn update_environment(&mut self) {
        if !self.clock.minute.is_multiple_of(60) {
            return;
        }
        for tile in &mut self.grid.tiles {
            let Some(crop) = tile.crop.as_mut() else {
                continue;
            };
            if crop.crop_id == "rice" {
                tile.water_level = match self.weather {
                    Weather::Rain => tile.water_level.saturating_add(8).min(100),
                    Weather::Hot => tile.water_level.saturating_sub(2),
                    Weather::Clear => tile.water_level.saturating_sub(1),
                };
                crop.moisture = tile.water_level;
            } else {
                match self.weather {
                    Weather::Rain => crop.moisture = crop.moisture.saturating_add(12).min(100),
                    Weather::Hot => crop.moisture = crop.moisture.saturating_sub(3),
                    Weather::Clear => crop.moisture = crop.moisture.saturating_sub(1),
                }
            }
            if crop.moisture == 0 {
                crop.health = crop.health.saturating_sub(1);
            }
            if crop.health == 0 {
                tile.crop = None;
                self.metrics.crops_lost += 1;
            }
        }
    }

    fn update_crops(&mut self) {
        let day_tick = self.clock.minute.is_multiple_of(SimClock::MINUTES_PER_DAY);
        let inspection_tick = self
            .clock
            .minute
            .is_multiple_of(SimClock::MINUTES_PER_DAY * 2);
        let mut ready_positions = Vec::new();
        let mut critical_positions = Vec::new();
        for y in 0..self.grid.height {
            for x in 0..self.grid.width {
                let position = TilePos::new(x, y);
                let Some(tile) = self.grid.tile_mut(position) else {
                    continue;
                };
                let Some(crop) = tile.crop.as_mut() else {
                    continue;
                };
                let Some(definition) = self.catalog.crops.get(&crop.crop_id) else {
                    continue;
                };
                if inspection_tick && definition.needs_inspection {
                    crop.inspection_due = true;
                }
                if day_tick && crop.crop_id == "rice" && crop.stage_index >= 1 {
                    let variation = ((x * 7 + y * 11 + self.clock.day() as u32) % 5) as u8;
                    crop.weed_pressure = crop.weed_pressure.saturating_add(5 + variation).min(100);
                    crop.soil_compaction = crop
                        .soil_compaction
                        .saturating_add(4 + variation / 2)
                        .min(100);
                    if definition.needs_pest_control && crop.stage_index >= 2 {
                        crop.pest_pressure =
                            crop.pest_pressure.saturating_add(6 + variation).min(100);
                        crop.pest_controlled = false;
                    }
                    if crop.pest_pressure >= 70 || crop.weed_pressure >= 80 {
                        crop.health = crop.health.saturating_sub(3);
                    }
                }
                if crop.moisture == 0 || crop.health == 0 {
                    critical_positions.push(position);
                    continue;
                }
                if definition.needs_pollination && crop.stage_index >= 1 && !crop.pollinated {
                    continue;
                }
                if crop.crop_id == "rice" && self.clock.season() == crate::Season::Winter {
                    if day_tick {
                        crop.health = crop.health.saturating_sub(4);
                    }
                    continue;
                }
                let Some(stage) = definition.stages.get(crop.stage_index) else {
                    continue;
                };
                if crop.stage_index + 1 == definition.stages.len() {
                    crop.stage_progress = stage.duration_minutes;
                    continue;
                }
                crop.stage_progress += 1;
                if crop.stage_progress >= stage.duration_minutes {
                    crop.stage_index += 1;
                    crop.stage_progress = 0;
                    if crop.stage_index + 1 == definition.stages.len() {
                        ready_positions.push(position);
                    }
                }
            }
        }
        for position in ready_positions {
            self.push_event(FarmEvent::CropReady(position));
        }
        for position in critical_positions.into_iter().take(2) {
            self.push_event(FarmEvent::CropCritical(position));
        }
    }

    fn generate_jobs(&mut self) {
        let zones = self.zones.clone();
        for zone in zones {
            for position in self.grid.positions_in_rect(zone.origin, zone.size) {
                let Some(kind) = self.required_job(position, &zone) else {
                    continue;
                };
                let already_queued = self.jobs.iter().any(|job| {
                    jobs_share_work_patch(job.kind, job.location, kind, position)
                        && matches!(job.status, JobStatus::Pending | JobStatus::Assigned)
                });
                if already_queued {
                    continue;
                }
                self.jobs.push(Job {
                    id: self.next_job_id,
                    kind,
                    location: position,
                    required_capability: kind.required_capability(),
                    priority: zone.priority,
                    zone_id: zone.id,
                    created_at: self.clock.minute,
                    deadline: Some(self.clock.minute + SimClock::MINUTES_PER_DAY * 2),
                    assigned_robot: None,
                    status: JobStatus::Pending,
                });
                self.next_job_id += 1;
                self.metrics.jobs_created += 1;
            }
        }
    }

    fn required_job(&self, position: TilePos, zone: &FieldZone) -> Option<JobKind> {
        let tile = self.grid.tile(position)?;
        let zone_definition = self.catalog.crops.get(&zone.crop_id)?;
        if tile.crop.is_none() {
            if let Some(preparation) = self.zone_preparation_phase(zone, zone_definition) {
                return match preparation {
                    JobKind::Plow if !tile.plowed => Some(JobKind::Plow),
                    JobKind::Till if !tile.tilled => Some(JobKind::Till),
                    JobKind::FloodPaddy if tile.water_level < 60 => Some(JobKind::FloodPaddy),
                    _ => None,
                };
            }
            return Some(match zone_definition.plant_capability {
                Capability::Seed => JobKind::Seed,
                Capability::Transplant => JobKind::Transplant,
                _ => JobKind::Plant,
            });
        }
        let crop = tile.crop.as_ref()?;
        let definition = self.catalog.crops.get(&crop.crop_id)?;
        if crop.stage_index + 1 == definition.stages.len() {
            return Some(match definition.harvest_capability {
                Capability::Dig => JobKind::Dig,
                Capability::PrecisionHarvest => JobKind::PrecisionHarvest,
                _ => JobKind::Harvest,
            });
        }
        if crop.moisture < definition.water_threshold
            || (definition.requires_flooded_field && tile.water_level < definition.water_threshold)
        {
            return Some(JobKind::Water);
        }
        if crop.crop_id == "rice" && crop.stage_index >= 1 && crop.weed_pressure >= 28 {
            return Some(JobKind::Weed);
        }
        if crop.crop_id == "rice" && crop.stage_index >= 2 && crop.soil_compaction >= 30 {
            return Some(JobKind::LoosenSoil);
        }
        if definition.needs_pest_control && crop.stage_index >= 2 && crop.pest_pressure >= 24 {
            return Some(
                if (position.x as u64 + position.y as u64 + self.clock.day()).is_multiple_of(2) {
                    JobKind::SprayPests
                } else {
                    JobKind::LaserPests
                },
            );
        }
        if definition.needs_pollination && crop.stage_index >= 1 && !crop.pollinated {
            return Some(JobKind::Pollinate);
        }
        if definition.needs_inspection && crop.inspection_due {
            return Some(JobKind::Inspect);
        }
        None
    }

    fn zone_preparation_phase(
        &self,
        zone: &FieldZone,
        definition: &crate::CropDef,
    ) -> Option<JobKind> {
        let empty_tiles: Vec<_> = self
            .grid
            .positions_in_rect(zone.origin, zone.size)
            .into_iter()
            .filter_map(|position| self.grid.tile(position))
            .filter(|tile| tile.crop.is_none())
            .collect();
        if empty_tiles.iter().any(|tile| !tile.plowed) {
            return Some(JobKind::Plow);
        }
        if empty_tiles.iter().any(|tile| !tile.tilled) {
            return Some(JobKind::Till);
        }
        if definition.requires_flooded_field && empty_tiles.iter().any(|tile| tile.water_level < 60)
        {
            return Some(JobKind::FloodPaddy);
        }
        None
    }

    fn assign_jobs(&mut self) {
        for robot_index in 0..self.robots.len() {
            let robot = &self.robots[robot_index];
            if !matches!(robot.state, RobotState::Idle | RobotState::Parked) || robot.battery < 18.0
            {
                continue;
            }
            let was_parked = robot.state == RobotState::Parked;
            let mut best: Option<(usize, i32, u64)> = None;
            for (job_index, job) in self.jobs.iter().enumerate() {
                if job.status != JobStatus::Pending
                    || !robot.capabilities.contains(&job.required_capability)
                    || !self.robot_can_reach(robot, job.location)
                {
                    continue;
                }
                let score = i32::from(job.priority) * 100
                    - robot.position.manhattan(job.location) as i32 * 4
                    - if robot.battery < 35.0 { 500 } else { 0 }
                    + (robot.work_speed * 100.0) as i32;
                let replace = best.is_none_or(|(_, best_score, best_id)| {
                    score > best_score || (score == best_score && job.id < best_id)
                });
                if replace {
                    best = Some((job_index, score, job.id));
                }
            }
            let Some((job_index, _, job_id)) = best else {
                continue;
            };
            if let Some(job) = self.jobs.get_mut(job_index) {
                job.status = JobStatus::Assigned;
                job.assigned_robot = Some(self.robots[robot_index].id);
            }
            if let Some(robot) = self.robots.get_mut(robot_index) {
                robot.current_job = Some(job_id);
                robot.state = if was_parked {
                    RobotState::Departing(job_id)
                } else {
                    RobotState::MovingToJob(job_id)
                };
                robot.work_progress = 0.0;
            }
        }
    }

    fn robot_can_reach(&self, robot: &Robot, target: TilePos) -> bool {
        if robot.body == RobotBody::Flying {
            return true;
        }
        let occupied = self.robot_occupied_tiles(robot.id, robot.body);
        path_exists(
            &self.grid,
            robot.position,
            target,
            robot.body,
            robot_allows_planted_fields(robot),
            &occupied,
        )
    }

    fn update_robots(&mut self) {
        let charger = self.facility_position(FacilityKind::ChargingStation);
        let warehouse = self.facility_position(FacilityKind::Warehouse);
        let mut completed_jobs = Vec::new();
        for index in 0..self.robots.len() {
            let state = self.robots[index].state.clone();
            match state {
                RobotState::Parked => {
                    self.metrics.robot_idle_minutes += 1;
                    if self.robots[index].battery <= 20.0 {
                        let id = self.robots[index].id;
                        self.robots[index].state = RobotState::MovingToCharge;
                        self.push_event(FarmEvent::RobotLowBattery(id));
                    }
                }
                RobotState::Idle => {
                    self.metrics.robot_idle_minutes += 1;
                    if self.robots[index].battery <= 20.0 {
                        let id = self.robots[index].id;
                        self.robots[index].state = RobotState::MovingToCharge;
                        self.push_event(FarmEvent::RobotLowBattery(id));
                    } else {
                        self.robots[index].work_progress += 1.0;
                        if self.robots[index].work_progress >= IDLE_RETURN_DELAY_MINUTES {
                            self.robots[index].state = RobotState::ReturningToGarage;
                            self.robots[index].work_progress = 0.0;
                        }
                    }
                }
                RobotState::Departing(job_id) => {
                    if self.jobs.iter().all(|job| job.id != job_id) {
                        self.reset_robot(index);
                        continue;
                    }
                    let garage_exit = self.map.garage_exit;
                    self.move_robot(index, garage_exit);
                    if self.robot_arrived(index, garage_exit) {
                        self.robots[index].state = RobotState::MovingToJob(job_id);
                    }
                }
                RobotState::MovingToJob(job_id) => {
                    let target = self
                        .jobs
                        .iter()
                        .find(|job| job.id == job_id)
                        .map(|job| job.location);
                    if let Some(target) = target {
                        self.move_robot(index, target);
                        if self.robot_arrived(index, target) {
                            self.robots[index].state = RobotState::Preparing(job_id);
                            self.robots[index].work_progress = 0.0;
                        }
                    } else {
                        self.reset_robot(index);
                    }
                }
                RobotState::Preparing(job_id) => {
                    let Some(kind) = self
                        .jobs
                        .iter()
                        .find(|job| job.id == job_id)
                        .map(|job| job.kind)
                    else {
                        self.reset_robot(index);
                        continue;
                    };
                    self.metrics.robot_work_minutes += 1;
                    self.robots[index].work_progress += 1.0;
                    if self.robots[index].work_progress >= f32::from(kind.preparation_minutes()) {
                        self.robots[index].state = RobotState::Working(job_id);
                        self.robots[index].work_progress = 0.0;
                    }
                }
                RobotState::Working(job_id) => {
                    self.metrics.robot_work_minutes += 1;
                    self.robots[index].battery = (self.robots[index].battery - 0.35).max(0.0);
                    self.metrics.energy_consumed += 1;
                    self.robots[index].work_progress += self.robots[index].work_speed;
                    let effort = self
                        .jobs
                        .iter()
                        .find(|job| job.id == job_id)
                        .map_or(1.0, |job| job.kind.effort());
                    if self.robots[index].work_progress >= effort {
                        self.robots[index].state = RobotState::Finishing(job_id);
                        self.robots[index].work_progress = 0.0;
                    }
                }
                RobotState::Finishing(job_id) => {
                    let Some(kind) = self
                        .jobs
                        .iter()
                        .find(|job| job.id == job_id)
                        .map(|job| job.kind)
                    else {
                        self.reset_robot(index);
                        continue;
                    };
                    self.metrics.robot_work_minutes += 1;
                    self.robots[index].work_progress += 1.0;
                    if self.robots[index].work_progress >= f32::from(kind.finishing_minutes()) {
                        completed_jobs.push((index, job_id));
                    }
                }
                RobotState::MovingToCharge => {
                    self.move_robot(index, charger);
                    if self.robot_arrived(index, charger) {
                        self.robots[index].state = RobotState::Charging;
                    }
                }
                RobotState::Charging => {
                    self.metrics.robot_charge_minutes += 1;
                    let capacity = self.robots[index].battery_capacity;
                    self.robots[index].battery = (self.robots[index].battery + 2.0).min(capacity);
                    self.power.stored = (self.power.stored - 2.0).max(0.0);
                    self.metrics.energy_consumed += 2;
                    if self.robots[index].battery >= capacity * 0.92 {
                        self.robots[index].state = RobotState::ReturningToGarage;
                    }
                }
                RobotState::MovingToStorage => {
                    self.move_robot(index, warehouse);
                    if self.robot_arrived(index, warehouse) {
                        self.robots[index].inventory.drain_into(&mut self.inventory);
                        self.robots[index].state = if self.robots[index].battery <= 20.0 {
                            RobotState::MovingToCharge
                        } else {
                            RobotState::ReturningToGarage
                        };
                    }
                }
                RobotState::ReturningToGarage => {
                    let home = self.robots[index].home_position;
                    self.move_robot(index, home);
                    if self.robot_arrived(index, home) {
                        self.robots[index].state = RobotState::Parked;
                        self.robots[index].work_progress = 0.0;
                    }
                }
                RobotState::Broken => {
                    self.metrics.jobs_failed += 1;
                }
            }
        }
        for (robot_index, job_id) in completed_jobs {
            self.complete_job(robot_index, job_id);
        }
    }

    fn move_robot(&mut self, index: usize, target: TilePos) {
        if let Some(next) = self.robots[index].movement_target {
            self.robots[index].movement_progress += movement_rate(self.robots[index].body);
            if self.robots[index].movement_progress >= 1.0 {
                self.robots[index].position = next;
                self.robots[index].movement_target = None;
                self.robots[index].movement_progress = 0.0;
                let energy = self.robots[index].energy_per_tile;
                self.robots[index].battery = (self.robots[index].battery - energy).max(0.0);
                self.metrics.energy_consumed += energy.ceil() as u64;
            }
            return;
        }
        if self.robots[index].position == target {
            return;
        }
        let current = self.robots[index].position;
        let occupied = self.robot_occupied_tiles(self.robots[index].id, self.robots[index].body);
        let next = if self.robots[index].body == RobotBody::Flying {
            let candidate = current.step_toward(target);
            (!occupied.contains(&candidate)).then_some(candidate)
        } else {
            next_ground_step(
                &self.grid,
                current,
                target,
                self.robots[index].body,
                robot_allows_planted_fields(&self.robots[index]),
                &occupied,
            )
        };
        let Some(next) = next else {
            return;
        };
        self.robots[index].movement_target = Some(next);
        self.robots[index].movement_progress = movement_rate(self.robots[index].body);
    }

    fn reset_robot(&mut self, index: usize) {
        self.robots[index].state = RobotState::Idle;
        self.robots[index].current_job = None;
        self.robots[index].work_progress = 0.0;
        self.robots[index].movement_target = None;
        self.robots[index].movement_progress = 0.0;
    }

    fn robot_arrived(&self, index: usize, target: TilePos) -> bool {
        self.robots[index].position == target && self.robots[index].movement_target.is_none()
    }

    fn robot_occupied_tiles(&self, robot_id: u64, body: RobotBody) -> BTreeSet<TilePos> {
        self.robots
            .iter()
            .filter(|robot| robot.id != robot_id)
            .filter(|robot| {
                if body == RobotBody::Flying {
                    robot.body == RobotBody::Flying
                } else {
                    robot.body != RobotBody::Flying
                }
            })
            .flat_map(|robot| [Some(robot.position), robot.movement_target])
            .flatten()
            .collect()
    }

    fn complete_job(&mut self, robot_index: usize, job_id: u64) {
        let Some(job_index) = self.jobs.iter().position(|job| job.id == job_id) else {
            self.reset_robot(robot_index);
            return;
        };
        let job = self.jobs[job_index].clone();
        let zone_crop = self
            .zones
            .iter()
            .find(|zone| zone.id == job.zone_id)
            .map(|zone| zone.crop_id.clone())
            .unwrap_or_else(|| "rice".to_owned());
        let crop_definition = self.catalog.crops.get(&zone_crop).cloned();
        let patch_positions = self.job_patch_positions(&job);
        let mut harvested = BTreeMap::<String, u32>::new();
        let mut tending_job = None;
        for position in patch_positions {
            let harvest_definition = self
                .grid
                .tile(position)
                .and_then(|tile| tile.crop.as_ref())
                .and_then(|crop| self.catalog.crops.get(&crop.crop_id))
                .cloned();
            let Some(tile) = self.grid.tile_mut(position) else {
                continue;
            };
            match job.kind {
                JobKind::Plow => {
                    tile.plowed = true;
                    tile.tilled = false;
                    tile.terrain = TerrainKind::Soil;
                    tile.water_level = 0;
                }
                JobKind::Till => {
                    tile.tilled = true;
                    tile.terrain = TerrainKind::Soil;
                    tile.water_level = 0;
                }
                JobKind::FloodPaddy => {
                    tile.water_level = 100;
                    tile.moisture = 100;
                    self.water.available_water = (self.water.available_water - 5.0).max(0.0);
                    self.metrics.water_consumed += 5;
                }
                JobKind::Seed | JobKind::Plant | JobKind::Transplant => {
                    if tile.crop.is_none()
                        && let Some(definition) = crop_definition.as_ref()
                    {
                        tile.crop = Some(CropInstance {
                            crop_id: definition.id.clone(),
                            planted_at: self.clock.minute,
                            stage_index: 0,
                            stage_progress: 0,
                            moisture: tile.moisture.max(55),
                            health: 100,
                            pollinated: false,
                            inspection_due: definition.needs_inspection,
                            pest_pressure: 0,
                            pest_controlled: !definition.needs_pest_control,
                            weed_pressure: 12,
                            soil_compaction: 10,
                            remaining_harvests: definition.harvest_count,
                        });
                        tile.fertility = tile.fertility.saturating_sub(definition.fertility_cost);
                    }
                }
                JobKind::Water => {
                    if let Some(crop) = tile.crop.as_mut() {
                        crop.moisture = crop.moisture.saturating_add(60).min(100);
                        if crop.crop_id == "rice" {
                            tile.water_level = tile.water_level.saturating_add(65).min(100);
                            crop.moisture = tile.water_level;
                        }
                    }
                    self.water.available_water = (self.water.available_water - 3.0).max(0.0);
                    self.metrics.water_consumed += 3;
                }
                JobKind::Pollinate => {
                    if let Some(crop) = tile.crop.as_mut() {
                        crop.pollinated = true;
                    }
                }
                JobKind::Inspect => {
                    if let Some(crop) = tile.crop.as_mut() {
                        crop.inspection_due = false;
                        crop.health = crop.health.saturating_add(5).min(100);
                    }
                }
                JobKind::PestControl => {
                    tending_job = Some(JobKind::PestControl);
                }
                JobKind::Weed => {
                    tending_job = Some(JobKind::Weed);
                }
                JobKind::LoosenSoil => {
                    tending_job = Some(JobKind::LoosenSoil);
                }
                JobKind::SprayPests => {
                    tending_job = Some(JobKind::SprayPests);
                }
                JobKind::LaserPests => {
                    tending_job = Some(JobKind::LaserPests);
                }
                JobKind::Harvest | JobKind::PrecisionHarvest | JobKind::Dig => {
                    if let Some(crop) = tile.crop.as_mut()
                        && let Some(definition) = harvest_definition.as_ref()
                    {
                        *harvested.entry(definition.id.clone()).or_default() +=
                            definition.harvest_yield;
                        if crop.remaining_harvests > 1 {
                            crop.remaining_harvests -= 1;
                            crop.stage_index = definition.stages.len().saturating_sub(2);
                            crop.stage_progress = 0;
                            crop.pollinated = !definition.needs_pollination;
                            crop.inspection_due = definition.needs_inspection;
                            crop.pest_pressure = 0;
                            crop.pest_controlled = !definition.needs_pest_control;
                            crop.weed_pressure = 0;
                            crop.soil_compaction = 0;
                        } else {
                            tile.crop = None;
                            tile.plowed = false;
                            tile.tilled = false;
                            tile.water_level = 0;
                        }
                    }
                }
                JobKind::Haul | JobKind::Repair | JobKind::Recharge | JobKind::Pack => {}
            }
        }
        if let Some(kind) = tending_job {
            self.complete_tending_patch(job.location, kind);
        }
        for (item, amount) in harvested {
            let accepted = self.robots[robot_index].inventory.add(item, amount);
            self.metrics.crops_produced += u64::from(accepted);
        }
        self.cancel_patch_jobs(&job);
        self.jobs[job_index].status = JobStatus::Completed;
        self.metrics.jobs_completed += 1;
        self.robots[robot_index].current_job = None;
        self.robots[robot_index].work_progress = 0.0;
        self.robots[robot_index].state = if self.robots[robot_index].inventory.used() > 0 {
            RobotState::MovingToStorage
        } else if self.robots[robot_index].battery <= 20.0 {
            RobotState::MovingToCharge
        } else {
            RobotState::Idle
        };
    }

    fn job_patch_positions(&self, job: &Job) -> Vec<TilePos> {
        if work_group(job.kind) == 0 {
            return vec![job.location];
        }
        let Some(zone) = self.zones.iter().find(|zone| zone.id == job.zone_id) else {
            return vec![job.location];
        };
        let zone_end_x = zone.origin.x.saturating_add(zone.size.0);
        let zone_end_y = zone.origin.y.saturating_add(zone.size.1);
        let start_x = job.location.x.saturating_sub(1).max(zone.origin.x);
        let start_y = job.location.y.saturating_sub(1).max(zone.origin.y);
        let end_x = (job.location.x + 1).min(zone_end_x.saturating_sub(1));
        let end_y = (job.location.y + 1).min(zone_end_y.saturating_sub(1));
        self.grid.positions_in_rect(
            TilePos::new(start_x, start_y),
            (end_x - start_x + 1, end_y - start_y + 1),
        )
    }

    fn cancel_patch_jobs(&mut self, completed: &Job) {
        for queued in &mut self.jobs {
            if queued.id != completed.id
                && queued.zone_id == completed.zone_id
                && jobs_share_work_patch(
                    queued.kind,
                    queued.location,
                    completed.kind,
                    completed.location,
                )
                && matches!(queued.status, JobStatus::Pending | JobStatus::Assigned)
            {
                queued.status = JobStatus::Cancelled;
            }
        }
    }

    fn complete_tending_patch(&mut self, center: TilePos, kind: JobKind) {
        let start_x = center.x.saturating_sub(1);
        let start_y = center.y.saturating_sub(1);
        let end_x = (center.x + 1).min(self.grid.width.saturating_sub(1));
        let end_y = (center.y + 1).min(self.grid.height.saturating_sub(1));
        for y in start_y..=end_y {
            for x in start_x..=end_x {
                let Some(crop) = self
                    .grid
                    .tile_mut(TilePos::new(x, y))
                    .and_then(|tile| tile.crop.as_mut())
                else {
                    continue;
                };
                match kind {
                    JobKind::Weed => crop.weed_pressure = crop.weed_pressure.saturating_sub(70),
                    JobKind::LoosenSoil => {
                        crop.soil_compaction = crop.soil_compaction.saturating_sub(70);
                        crop.health = crop.health.saturating_add(1).min(100);
                    }
                    JobKind::SprayPests => {
                        crop.pest_pressure = crop.pest_pressure.saturating_sub(80);
                        crop.pest_controlled = true;
                    }
                    JobKind::LaserPests | JobKind::PestControl => {
                        crop.pest_pressure = crop.pest_pressure.saturating_sub(55);
                        crop.pest_controlled = true;
                    }
                    _ => {}
                }
            }
        }
    }

    fn update_logistics(&mut self) {
        if !self.has_facility(FacilityKind::Packer) {
            return;
        }
        let crop_ids: Vec<String> = self.catalog.crops.keys().cloned().collect();
        for crop_id in crop_ids {
            let amount = self.inventory.remove(&crop_id, 4);
            if amount > 0 {
                self.inventory.add(format!("packed_{crop_id}"), amount);
                self.metrics.packed_items += u64::from(amount);
            }
        }
    }

    fn update_economy(&mut self) {
        let Some(index) = self.current_contract else {
            return;
        };
        let Some(contract) = self.contracts.get(index) else {
            return;
        };
        if contract.status != ContractStatus::Active {
            return;
        }
        let requirements = contract.definition.requirements.clone();
        for requirement in requirements {
            let delivered = self.contracts[index]
                .delivered
                .get(&requirement.item)
                .copied()
                .unwrap_or_default();
            let needed = requirement.amount.saturating_sub(delivered);
            let amount = self.inventory.remove(&requirement.item, needed);
            if amount > 0 {
                *self.contracts[index]
                    .delivered
                    .entry(requirement.item)
                    .or_default() += amount;
            }
        }

        let complete = self.contracts[index]
            .definition
            .requirements
            .iter()
            .all(|requirement| {
                self.contracts[index]
                    .delivered
                    .get(&requirement.item)
                    .copied()
                    .unwrap_or_default()
                    >= requirement.amount
            });
        if complete {
            self.complete_contract(index);
        } else if self.clock.minute > self.contracts[index].deadline {
            let contract_id = self.contracts[index].definition.id.clone();
            self.contracts[index].status = ContractStatus::Failed;
            self.metrics.jobs_failed += 1;
            self.push_event(FarmEvent::ContractFailed(contract_id));
            self.activate_next_contract(index + 1);
        } else if self.contracts[index]
            .deadline
            .saturating_sub(self.clock.minute)
            == 180
        {
            self.push_event(FarmEvent::ContractNearDeadline(
                self.contracts[index].definition.id.clone(),
            ));
        }
    }

    fn update_utilities(&mut self) {
        let solar_count = self
            .facilities
            .iter()
            .filter(|facility| facility.kind == FacilityKind::SolarGenerator)
            .count() as f32;
        let working = self
            .robots
            .iter()
            .filter(|robot| {
                matches!(
                    robot.state,
                    RobotState::Preparing(_) | RobotState::Working(_) | RobotState::Finishing(_)
                )
            })
            .count() as f32;
        self.power.production = solar_count * 12.0;
        self.power.consumption = self.facilities.len() as f32 * 0.4 + working * 1.2;
        let delta = self.power.production - self.power.consumption;
        self.power.stored = (self.power.stored + delta).clamp(0.0, self.power.capacity);
        self.metrics.energy_generated += self.power.production.round() as u64;
        if self.power.stored < 40.0 && self.clock.minute.is_multiple_of(30) {
            self.push_event(FarmEvent::PowerLow);
        }
        if self.weather == Weather::Rain {
            self.water.available_water = (self.water.available_water + 4.0).min(1_000.0);
        }
        if self.water.available_water < 80.0 && self.clock.minute.is_multiple_of(30) {
            self.push_event(FarmEvent::WaterLow);
        }
    }

    fn start_autonomy_trial(&mut self, duration_minutes: u64) {
        self.autonomy_trial = Some(AutonomyTrial {
            start_time: self.clock.minute,
            end_time: self.clock.minute + duration_minutes,
            baseline: self.metrics.clone(),
            manual_interventions: 0,
            finished: false,
            score: None,
            grade: None,
        });
        self.last_autonomy_report = None;
        self.push_event(FarmEvent::AutonomyTrialStarted);
    }

    fn finish_autonomy_trial_if_due(&mut self) {
        let Some(trial) = self.autonomy_trial.as_ref() else {
            return;
        };
        if trial.finished || self.clock.minute < trial.end_time {
            return;
        }
        let report = calculate_autonomy_report(trial, &self.metrics);
        if let Some(trial) = self.autonomy_trial.as_mut() {
            trial.finished = true;
            trial.score = Some(report.score);
            trial.grade = Some(report.grade.clone());
        }
        self.reputation += u32::from(report.score / 5);
        self.push_event(FarmEvent::AutonomyTrialCompleted(report.score));
        self.last_autonomy_report = Some(report);
    }

    #[must_use]
    pub fn snapshot(&self) -> FarmSnapshot {
        FarmSnapshot {
            world_revision: self.world_revision,
            time: self.clock,
            weather: self.weather,
            credits: self.credits,
            reputation: self.reputation,
            power: self.power.clone(),
            water: self.water.clone(),
            inventory: self.inventory.items.clone(),
            robots_idle: self
                .robots
                .iter()
                .filter(|robot| matches!(robot.state, RobotState::Idle | RobotState::Parked))
                .count(),
            robots_working: self
                .robots
                .iter()
                .filter(|robot| {
                    matches!(
                        robot.state,
                        RobotState::Departing(_)
                            | RobotState::MovingToJob(_)
                            | RobotState::Preparing(_)
                            | RobotState::Working(_)
                            | RobotState::Finishing(_)
                    )
                })
                .count(),
            robots_charging: self
                .robots
                .iter()
                .filter(|robot| {
                    matches!(
                        robot.state,
                        RobotState::Charging | RobotState::MovingToCharge
                    )
                })
                .count(),
            robots_broken: self
                .robots
                .iter()
                .filter(|robot| robot.state == RobotState::Broken)
                .count(),
            active_contract: self
                .current_contract
                .and_then(|index| self.contracts.get(index))
                .cloned(),
            pending_jobs: self
                .jobs
                .iter()
                .filter(|job| job.status == JobStatus::Pending)
                .count(),
            recent_events: self.events.iter().rev().take(10).cloned().collect(),
        }
    }

    pub fn to_ron(&self) -> Result<String, SimulationError> {
        ron::ser::to_string_pretty(self, PrettyConfig::new())
            .map_err(|error| SimulationError::Serialization(error.to_string()))
    }

    pub fn from_ron(input: &str) -> Result<Self, SimulationError> {
        let mut simulation: Self = ron::from_str(input)
            .map_err(|error| SimulationError::Serialization(error.to_string()))?;
        if simulation.version != SAVE_VERSION {
            return Err(SimulationError::UnsupportedSaveVersion {
                found: simulation.version,
                expected: SAVE_VERSION,
            });
        }
        let map = MapDefinition::load_embedded()
            .map_err(|error| SimulationError::Map(error.to_string()))?;
        if simulation.map_id != map.id {
            return Err(SimulationError::Map(format!(
                "save references map {}; runtime provides {}",
                simulation.map_id, map.id
            )));
        }
        simulation.map = map;
        Ok(simulation)
    }

    pub fn record_ai_event(
        &mut self,
        actor: impl Into<String>,
        message: impl Into<String>,
        reason: impl Into<String>,
    ) {
        self.push_event(FarmEvent::AiAction {
            actor: actor.into(),
            message: message.into(),
            reason: reason.into(),
        });
    }

    fn validate_new_zone(
        &self,
        origin: TilePos,
        size: (u32, u32),
        crop_id: &str,
    ) -> Result<(), CommandError> {
        self.require_crop(crop_id)?;
        if size.0 == 0 || size.1 == 0 || size.0 > 16 || size.1 > 16 {
            return Err(CommandError::Invalid(
                "field size must be between 1x1 and 16x16".to_owned(),
            ));
        }
        let positions = self.grid.positions_in_rect(origin, size);
        if positions.len() != (size.0 * size.1) as usize
            || positions.iter().any(|position| {
                self.grid
                    .tile(*position)
                    .is_none_or(|tile| !tile.terrain.farmable() || tile.building.is_some())
            })
        {
            return Err(CommandError::Invalid(
                "field rectangle includes blocked or out-of-bounds tiles".to_owned(),
            ));
        }
        Ok(())
    }

    fn validate_facility_position(&self, position: TilePos) -> Result<(), CommandError> {
        let Some(tile) = self.grid.tile(position) else {
            return Err(CommandError::Invalid(
                "facility position is outside the map".to_owned(),
            ));
        };
        if tile.occupied
            || tile.building.is_some()
            || matches!(
                tile.terrain,
                TerrainKind::Water | TerrainKind::Rock | TerrainKind::IrrigationChannel
            )
        {
            return Err(CommandError::Invalid(
                "facility position is blocked".to_owned(),
            ));
        }
        Ok(())
    }

    fn require_zone(&self, zone_id: u64) -> Result<&FieldZone, CommandError> {
        self.zones
            .iter()
            .find(|zone| zone.id == zone_id)
            .ok_or_else(|| CommandError::NotFound(format!("zone {zone_id}")))
    }

    fn require_crop(&self, crop_id: &str) -> Result<(), CommandError> {
        if self.catalog.crops.contains_key(crop_id) {
            Ok(())
        } else {
            Err(CommandError::NotFound(format!("crop {crop_id}")))
        }
    }

    fn require_robot_def(&self, robot_def_id: &str) -> Result<&crate::RobotDef, CommandError> {
        self.catalog
            .robots
            .get(robot_def_id)
            .ok_or_else(|| CommandError::NotFound(format!("robot definition {robot_def_id}")))
    }

    fn require_credits(&self, amount: i32) -> Result<(), CommandError> {
        if self.credits >= amount {
            Ok(())
        } else {
            Err(CommandError::InsufficientCredits {
                required: amount,
                available: self.credits,
            })
        }
    }

    fn create_zone_unchecked(
        &mut self,
        origin: TilePos,
        size: (u32, u32),
        crop_id: &str,
        priority: u8,
    ) -> u64 {
        let id = self.next_zone_id;
        self.next_zone_id += 1;
        self.zones.push(FieldZone {
            id,
            name: format!("{} Zone {id}", title_case(crop_id)),
            origin,
            size,
            crop_id: crop_id.to_owned(),
            priority,
            manager: None,
        });
        for position in self.grid.positions_in_rect(origin, size) {
            if let Some(tile) = self.grid.tile_mut(position) {
                tile.occupied = true;
            }
        }
        self.push_event(FarmEvent::Info(format!(
            "Created {crop_id} field at ({}, {})",
            origin.x, origin.y
        )));
        id
    }

    fn spawn_robot_unchecked(&mut self, robot_def_id: &str, position: TilePos) -> Option<u64> {
        let definition = self.catalog.robots.get(robot_def_id)?.clone();
        let position = self.available_robot_spawn(position, definition.body, &definition.id)?;
        let id = self.next_entity_id;
        self.next_entity_id += 1;
        self.robots.push(Robot {
            id,
            def_id: definition.id,
            body: definition.body,
            capabilities: definition.capabilities,
            battery: definition.battery_capacity,
            battery_capacity: definition.battery_capacity,
            energy_per_tile: definition.energy_per_tile,
            work_speed: definition.work_speed,
            state: RobotState::Parked,
            current_job: None,
            inventory: Inventory {
                items: BTreeMap::new(),
                capacity: definition.cargo_capacity,
            },
            condition: 100.0,
            position,
            movement_target: None,
            movement_progress: 0.0,
            home_position: position,
            work_progress: 0.0,
        });
        self.push_event(FarmEvent::Info(format!(
            "{} deployed",
            definition.display_name
        )));
        Some(id)
    }

    fn available_robot_spawn(
        &self,
        preferred: TilePos,
        body: RobotBody,
        robot_def_id: &str,
    ) -> Option<TilePos> {
        (0..self.grid.height)
            .flat_map(|y| (0..self.grid.width).map(move |x| TilePos::new(x, y)))
            .filter(|position| {
                let Some(tile) = self.grid.tile(*position) else {
                    return false;
                };
                if tile.building.is_some()
                    || self.robots.iter().any(|robot| robot.position == *position)
                {
                    return false;
                }
                if body == RobotBody::Flying {
                    return true;
                }
                !matches!(
                    tile.terrain,
                    TerrainKind::Water | TerrainKind::Rock | TerrainKind::IrrigationChannel
                ) && !(body == RobotBody::Wheeled && tile.terrain == TerrainKind::PaddyBund)
                    && (body != RobotBody::Wheeled
                        || tile.crop.is_none()
                        || robot_def_id == "rice_harvester")
            })
            .min_by_key(|position| {
                (
                    position.manhattan(preferred),
                    position.y.abs_diff(preferred.y),
                    position.x.abs_diff(preferred.x),
                    position.y,
                    position.x,
                )
            })
    }

    fn spawn_facility_unchecked(&mut self, kind: FacilityKind, position: TilePos) -> u64 {
        let id = self.next_entity_id;
        self.next_entity_id += 1;
        self.facilities.push(Facility {
            id,
            kind,
            position,
            powered: true,
        });
        if let Some(tile) = self.grid.tile_mut(position) {
            tile.building = Some(id);
            tile.occupied = true;
            tile.terrain = TerrainKind::Concrete;
        }
        id
    }

    fn spawn_map_facility_unchecked(&mut self, definition: &crate::MapFacilityDef) -> u64 {
        let id = self.spawn_facility_unchecked(definition.kind, definition.position);
        let collision_areas: Vec<_> = self
            .map
            .collision_areas
            .iter()
            .filter(|area| area.owner == Some(definition.kind))
            .map(|area| (area.origin, area.size))
            .collect();
        for (origin, size) in collision_areas {
            for position in self.grid.positions_in_rect(origin, size) {
                if let Some(tile) = self.grid.tile_mut(position) {
                    tile.building = Some(id);
                    tile.occupied = true;
                }
            }
        }
        id
    }

    fn delete_entity(&mut self, entity_id: u64) {
        if let Some(index) = self
            .facilities
            .iter()
            .position(|facility| facility.id == entity_id)
        {
            let facility = self.facilities.remove(index);
            for tile in &mut self.grid.tiles {
                if tile.building == Some(facility.id) {
                    tile.building = None;
                    tile.occupied = false;
                }
            }
            return;
        }
        self.robots.retain(|robot| robot.id != entity_id);
        if let Some(index) = self.zones.iter().position(|zone| zone.id == entity_id) {
            let zone = self.zones.remove(index);
            for position in self.grid.positions_in_rect(zone.origin, zone.size) {
                if let Some(tile) = self.grid.tile_mut(position) {
                    tile.occupied = false;
                }
            }
        }
    }

    fn facility_position(&self, kind: FacilityKind) -> TilePos {
        self.facilities
            .iter()
            .find(|facility| facility.kind == kind)
            .map_or(TilePos::new(31, 31), |facility| facility.position)
    }

    fn available_garage_bay(&self) -> Option<TilePos> {
        self.map
            .garage_bays
            .iter()
            .copied()
            .find(|bay| self.robots.iter().all(|robot| robot.home_position != *bay))
    }

    fn has_facility(&self, kind: FacilityKind) -> bool {
        self.facilities
            .iter()
            .any(|facility| facility.kind == kind && facility.powered)
    }

    fn complete_contract(&mut self, index: usize) {
        let id = self.contracts[index].definition.id.clone();
        self.credits += self.contracts[index].definition.reward;
        self.reputation += self.contracts[index].definition.reputation;
        self.contracts[index].status = ContractStatus::Completed;
        self.metrics.contracts_fulfilled += 1;
        self.push_event(FarmEvent::ContractCompleted(id));
        self.activate_next_contract(index + 1);
    }

    fn activate_next_contract(&mut self, index: usize) {
        if index < self.contracts.len() {
            self.activate_contract(index);
            self.metrics.contracts_expected += 1;
        } else {
            self.current_contract = None;
        }
    }

    fn activate_contract(&mut self, index: usize) {
        let Some(contract) = self.contracts.get_mut(index) else {
            return;
        };
        contract.status = ContractStatus::Active;
        contract.accepted_at = self.clock.minute;
        contract.deadline = self.clock.minute + contract.definition.deadline_minutes;
        contract.delivered.clear();
        let id = contract.definition.id.clone();
        self.current_contract = Some(index);
        self.push_event(FarmEvent::ContractAccepted(id));
    }

    fn push_event(&mut self, event: FarmEvent) {
        self.events.push(event);
        if self.events.len() > MAX_EVENTS {
            let overflow = self.events.len() - MAX_EVENTS;
            self.events.drain(0..overflow);
        }
    }
}

fn default_npc_assignments() -> BTreeMap<String, NpcAssignment> {
    [("aster", "Aster"), ("mira", "Mira")]
        .into_iter()
        .map(|(npc_id, display_name)| {
            (
                npc_id.to_owned(),
                NpcAssignment {
                    npc_id: npc_id.to_owned(),
                    display_name: display_name.to_owned(),
                    managed_zones: BTreeSet::new(),
                },
            )
        })
        .collect()
}

fn title_case(input: &str) -> String {
    let mut characters = input.chars();
    let Some(first) = characters.next() else {
        return String::new();
    };
    first.to_uppercase().collect::<String>() + characters.as_str()
}

fn robot_allows_planted_fields(robot: &Robot) -> bool {
    robot.body != RobotBody::Wheeled || robot.def_id == "rice_harvester"
}

fn movement_rate(body: RobotBody) -> f32 {
    match body {
        RobotBody::Flying => 0.55,
        RobotBody::Quadruped | RobotBody::Hexapod => 0.38,
        RobotBody::Biped => 0.32,
        RobotBody::Wheeled => 0.28,
    }
}

fn jobs_share_work_patch(
    first_kind: JobKind,
    first_position: TilePos,
    second_kind: JobKind,
    second_position: TilePos,
) -> bool {
    if first_kind == second_kind && first_position == second_position {
        return true;
    }
    let first_group = work_group(first_kind);
    first_group != 0
        && first_group == work_group(second_kind)
        && first_position.manhattan(second_position) <= 2
}

fn work_group(kind: JobKind) -> u8 {
    match kind {
        JobKind::Plow => 1,
        JobKind::Till => 2,
        JobKind::FloodPaddy => 3,
        JobKind::Seed | JobKind::Plant | JobKind::Transplant => 4,
        JobKind::Water => 5,
        JobKind::Pollinate => 6,
        JobKind::Inspect => 7,
        JobKind::Weed => 8,
        JobKind::LoosenSoil => 9,
        JobKind::PestControl | JobKind::SprayPests | JobKind::LaserPests => 10,
        JobKind::Harvest | JobKind::PrecisionHarvest | JobKind::Dig => 11,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn simulation() -> Result<GameSimulation, SimulationError> {
        GameSimulation::new(123)
    }

    fn missing(message: &str) -> SimulationError {
        SimulationError::Serialization(message.to_owned())
    }

    #[test]
    fn calendar_rolls_through_28_day_seasons() {
        let mut clock = SimClock {
            minute: 0,
            paused: false,
            speed: 1,
        };
        assert_eq!(clock.season(), crate::Season::Spring);
        assert_eq!(clock.day_of_season(), 1);
        clock.minute = SimClock::DAYS_PER_SEASON * SimClock::MINUTES_PER_DAY;
        assert_eq!(clock.season(), crate::Season::Summer);
        assert_eq!(clock.day_of_season(), 1);
        clock.minute =
            SimClock::DAYS_PER_SEASON * SimClock::SEASONS_PER_YEAR * SimClock::MINUTES_PER_DAY;
        assert_eq!(clock.season(), crate::Season::Spring);
        assert_eq!(clock.year(), 2);
    }

    #[test]
    fn rice_does_not_change_stage_after_one_day() -> Result<(), SimulationError> {
        let mut simulation = simulation()?;
        let position = TilePos::new(8, 8);
        let planted_at = simulation.clock.minute;
        let tile = simulation
            .grid
            .tile_mut(position)
            .ok_or_else(|| missing("test tile should exist"))?;
        tile.water_level = 100;
        tile.crop = Some(CropInstance {
            crop_id: "rice".to_owned(),
            planted_at,
            stage_index: 0,
            stage_progress: 0,
            moisture: 100,
            health: 100,
            pollinated: false,
            inspection_due: false,
            pest_pressure: 0,
            pest_controlled: true,
            weed_pressure: 0,
            soil_compaction: 0,
            remaining_harvests: 1,
        });
        for _ in 0..SimClock::MINUTES_PER_DAY {
            simulation.clock.minute += 1;
            simulation.update_crops();
        }
        let crop = simulation
            .grid
            .tile(position)
            .and_then(|tile| tile.crop.as_ref())
            .ok_or_else(|| missing("rice should still be growing"))?;
        assert_eq!(crop.stage_index, 0);
        assert_eq!(crop.stage_progress, SimClock::MINUTES_PER_DAY as u32);
        Ok(())
    }

    #[test]
    fn planted_rice_work_is_reserved_for_legged_and_flying_robots() -> Result<(), SimulationError> {
        let simulation = simulation()?;
        let rover = simulation
            .catalog
            .robots
            .get("paddy_rover")
            .ok_or_else(|| missing("paddy rover definition should exist"))?;
        let spider = simulation
            .catalog
            .robots
            .get("rice_transplanter")
            .ok_or_else(|| missing("rice spider definition should exist"))?;
        let drone = simulation
            .catalog
            .robots
            .get("pest_control_drone")
            .ok_or_else(|| missing("pest drone definition should exist"))?;
        assert!(!rover.capabilities.contains(&Capability::Water));
        assert!(!rover.capabilities.contains(&Capability::Weed));
        assert!(spider.capabilities.contains(&Capability::Water));
        assert!(spider.capabilities.contains(&Capability::Weed));
        assert!(spider.capabilities.contains(&Capability::LoosenSoil));
        assert!(drone.capabilities.contains(&Capability::Spray));
        assert!(drone.capabilities.contains(&Capability::LaserPestControl));
        Ok(())
    }

    #[test]
    fn starter_fleet_is_parked_without_overlap() -> Result<(), SimulationError> {
        let simulation = simulation()?;
        assert!(
            simulation
                .facilities
                .iter()
                .any(|facility| facility.kind == FacilityKind::RobotGarage)
        );
        let positions: BTreeSet<_> = simulation
            .robots
            .iter()
            .map(|robot| robot.position)
            .collect();
        assert_eq!(positions.len(), simulation.robots.len());
        assert!(simulation.robots.iter().all(
            |robot| robot.state == RobotState::Parked && robot.position == robot.home_position
        ));
        assert!(
            simulation
                .grid
                .tile(TilePos::new(40, 5))
                .is_some_and(|tile| tile.building.is_some())
        );
        assert!(simulation.map.garage_bays.iter().all(|bay| {
            simulation
                .grid
                .tile(*bay)
                .is_some_and(|tile| tile.building.is_none())
        }));
        Ok(())
    }

    #[test]
    fn generated_map_definition_drives_navigation_and_scenario_layout()
    -> Result<(), SimulationError> {
        let simulation = simulation()?;
        assert_eq!((simulation.map.width, simulation.map.height), (64, 64));
        assert_eq!(simulation.map.tile_size, 32);
        assert_eq!(
            simulation.map.terrain_tileset_asset,
            "art/pixel/tilesets/verdant-paddy-terrain.png"
        );
        assert_eq!(simulation.zones[0].size, (11, 28));
        assert_eq!(simulation.zones[1].size, (13, 28));
        assert_eq!(
            simulation
                .grid
                .tile(TilePos::new(6, 16))
                .map(|tile| tile.terrain),
            Some(TerrainKind::Culvert)
        );
        assert_eq!(
            simulation
                .grid
                .tile(TilePos::new(10, 16))
                .map(|tile| tile.terrain),
            Some(TerrainKind::FarmPath)
        );
        assert_eq!(
            simulation
                .grid
                .tile(TilePos::new(6, 30))
                .map(|tile| tile.terrain),
            Some(TerrainKind::IrrigationChannel)
        );
        Ok(())
    }

    #[test]
    fn wheeled_motion_advances_every_simulation_minute() -> Result<(), SimulationError> {
        let mut simulation = simulation()?;
        let rover_index = simulation
            .robots
            .iter()
            .position(|robot| robot.def_id == "paddy_rover")
            .ok_or_else(|| missing("paddy rover should exist"))?;
        let start = TilePos::new(40, 12);
        let target = TilePos::new(40, 13);
        simulation.robots[rover_index].position = start;

        for expected_progress in [0.28, 0.56, 0.84] {
            simulation.move_robot(rover_index, target);
            let rover = &simulation.robots[rover_index];
            assert_eq!(rover.position, start);
            assert_eq!(rover.movement_target, Some(target));
            assert!((rover.movement_progress - expected_progress).abs() < 0.001);
        }
        simulation.move_robot(rover_index, target);
        assert_eq!(simulation.robots[rover_index].position, target);
        assert_eq!(simulation.robots[rover_index].movement_target, None);
        Ok(())
    }

    #[test]
    fn robots_reserve_their_incoming_tile() -> Result<(), SimulationError> {
        let mut simulation = simulation()?;
        let first = 0;
        let second = 1;
        let reserved = TilePos::new(41, 16);
        simulation.robots[first].position = TilePos::new(40, 16);
        simulation.robots[second].position = TilePos::new(42, 16);
        simulation.move_robot(first, reserved);
        simulation.move_robot(second, reserved);
        assert_eq!(simulation.robots[first].movement_target, Some(reserved));
        assert_eq!(simulation.robots[second].movement_target, None);
        Ok(())
    }

    #[test]
    fn ploughing_deploys_works_and_stows_before_completion() -> Result<(), SimulationError> {
        let mut simulation = simulation()?;
        simulation.jobs.clear();
        let location = simulation
            .zones
            .first()
            .map(|zone| zone.origin)
            .ok_or_else(|| missing("starter rice zone should exist"))?;
        let rover_index = simulation
            .robots
            .iter()
            .position(|robot| robot.def_id == "paddy_rover")
            .ok_or_else(|| missing("paddy rover should exist"))?;
        let rover_id = simulation.robots[rover_index].id;
        simulation.jobs.push(Job {
            id: 9_001,
            kind: JobKind::Plow,
            location,
            required_capability: Capability::Plow,
            priority: 100,
            zone_id: 1,
            created_at: 0,
            deadline: None,
            assigned_robot: Some(rover_id),
            status: JobStatus::Assigned,
        });
        simulation.robots[rover_index].position = location;
        simulation.robots[rover_index].state = RobotState::MovingToJob(9_001);
        simulation.robots[rover_index].current_job = Some(9_001);

        simulation.update_robots();
        assert_eq!(
            simulation.robots[rover_index].state,
            RobotState::Preparing(9_001)
        );
        for _ in 0..JobKind::Plow.preparation_minutes() {
            simulation.update_robots();
        }
        assert_eq!(
            simulation.robots[rover_index].state,
            RobotState::Working(9_001)
        );
        simulation.robots[rover_index].work_progress = JobKind::Plow.effort();
        simulation.update_robots();
        assert_eq!(
            simulation.robots[rover_index].state,
            RobotState::Finishing(9_001)
        );
        for _ in 0..JobKind::Plow.finishing_minutes() {
            simulation.update_robots();
        }
        assert!(
            simulation
                .grid
                .tile(location)
                .is_some_and(|tile| tile.plowed && !tile.tilled)
        );
        assert!(
            simulation
                .grid
                .tile(TilePos::new(location.x + 1, location.y + 1))
                .is_some_and(|tile| tile.plowed && !tile.tilled)
        );
        assert_eq!(simulation.robots[rover_index].state, RobotState::Idle);
        Ok(())
    }

    #[test]
    fn crop_progresses_through_stages() -> Result<(), SimulationError> {
        let mut simulation = simulation()?;
        let position = TilePos::new(8, 8);
        let tile = simulation
            .grid
            .tile_mut(position)
            .ok_or_else(|| missing("test tile should exist"))?;
        tile.crop = Some(CropInstance {
            crop_id: "wheat".to_owned(),
            planted_at: 0,
            stage_index: 0,
            stage_progress: 0,
            moisture: 100,
            health: 100,
            pollinated: false,
            inspection_due: false,
            pest_pressure: 0,
            pest_controlled: true,
            weed_pressure: 0,
            soil_compaction: 0,
            remaining_harvests: 1,
        });
        for _ in 0..(SimClock::MINUTES_PER_DAY * 3 + 1) {
            simulation.clock.minute += 1;
            simulation.update_crops();
        }
        let stage = simulation
            .grid
            .tile(position)
            .and_then(|tile| tile.crop.as_ref())
            .map(|crop| crop.stage_index);
        assert!(stage.is_some_and(|stage| stage >= 1));
        Ok(())
    }

    #[test]
    fn crop_loses_health_without_water() -> Result<(), SimulationError> {
        let mut simulation = simulation()?;
        let position = TilePos::new(8, 8);
        let tile = simulation
            .grid
            .tile_mut(position)
            .ok_or_else(|| missing("test tile should exist"))?;
        tile.crop = Some(CropInstance {
            crop_id: "wheat".to_owned(),
            planted_at: 0,
            stage_index: 0,
            stage_progress: 0,
            moisture: 0,
            health: 20,
            pollinated: false,
            inspection_due: false,
            pest_pressure: 0,
            pest_controlled: true,
            weed_pressure: 0,
            soil_compaction: 0,
            remaining_harvests: 1,
        });
        simulation.update_environment();
        let health = simulation
            .grid
            .tile(position)
            .and_then(|tile| tile.crop.as_ref())
            .map(|crop| crop.health);
        assert_eq!(health, Some(19));
        Ok(())
    }

    #[test]
    fn tomato_waits_for_pollination() -> Result<(), SimulationError> {
        let mut simulation = simulation()?;
        let position = TilePos::new(8, 8);
        let tile = simulation
            .grid
            .tile_mut(position)
            .ok_or_else(|| missing("test tile should exist"))?;
        tile.crop = Some(CropInstance {
            crop_id: "tomato".to_owned(),
            planted_at: 0,
            stage_index: 1,
            stage_progress: 0,
            moisture: 100,
            health: 100,
            pollinated: false,
            inspection_due: false,
            pest_pressure: 0,
            pest_controlled: true,
            weed_pressure: 0,
            soil_compaction: 0,
            remaining_harvests: 3,
        });
        simulation.advance_minutes(30);
        let progress = simulation
            .grid
            .tile(position)
            .and_then(|tile| tile.crop.as_ref())
            .map(|crop| crop.stage_progress);
        assert_eq!(progress, Some(0));
        Ok(())
    }

    #[test]
    fn scheduler_assigns_pest_control_to_drone_not_rover() -> Result<(), SimulationError> {
        let mut simulation = simulation()?;
        simulation.jobs.clear();
        for robot in &mut simulation.robots {
            robot.state = RobotState::Idle;
            robot.current_job = None;
        }
        simulation.jobs.push(Job {
            id: 999,
            kind: JobKind::LaserPests,
            location: TilePos::new(12, 12),
            required_capability: Capability::LaserPestControl,
            priority: 90,
            zone_id: 1,
            created_at: 0,
            deadline: None,
            assigned_robot: None,
            status: JobStatus::Pending,
        });
        simulation.assign_jobs();
        let assigned = simulation.jobs.first().and_then(|job| job.assigned_robot);
        let body = assigned.and_then(|id| {
            simulation
                .robots
                .iter()
                .find(|robot| robot.id == id)
                .map(|robot| robot.body)
        });
        assert_eq!(body, Some(RobotBody::Flying));
        Ok(())
    }

    #[test]
    fn rice_field_requests_jobs_in_cultivation_order() -> Result<(), SimulationError> {
        let mut simulation = simulation()?;
        let zone = simulation
            .zones
            .first()
            .cloned()
            .ok_or_else(|| missing("starter rice zone should exist"))?;
        let position = zone.origin;
        assert_eq!(
            simulation.required_job(position, &zone),
            Some(JobKind::Plow)
        );

        for tile_position in simulation.grid.positions_in_rect(zone.origin, zone.size) {
            let tile = simulation
                .grid
                .tile_mut(tile_position)
                .ok_or_else(|| missing("starter rice tile should exist"))?;
            tile.plowed = true;
        }
        assert_eq!(
            simulation.required_job(position, &zone),
            Some(JobKind::Till)
        );

        for tile_position in simulation.grid.positions_in_rect(zone.origin, zone.size) {
            let tile = simulation
                .grid
                .tile_mut(tile_position)
                .ok_or_else(|| missing("starter rice tile should exist"))?;
            tile.tilled = true;
        }
        assert_eq!(
            simulation.required_job(position, &zone),
            Some(JobKind::FloodPaddy)
        );

        for tile_position in simulation.grid.positions_in_rect(zone.origin, zone.size) {
            let tile = simulation
                .grid
                .tile_mut(tile_position)
                .ok_or_else(|| missing("starter rice tile should exist"))?;
            tile.water_level = 100;
        }
        assert_eq!(
            simulation.required_job(position, &zone),
            Some(JobKind::Transplant)
        );

        let tile = simulation
            .grid
            .tile_mut(position)
            .ok_or_else(|| missing("starter rice tile should exist"))?;
        tile.crop = Some(CropInstance {
            crop_id: "rice".to_owned(),
            planted_at: 0,
            stage_index: 2,
            stage_progress: 0,
            moisture: 100,
            health: 100,
            pollinated: false,
            inspection_due: false,
            pest_pressure: 36,
            pest_controlled: false,
            weed_pressure: 0,
            soil_compaction: 0,
            remaining_harvests: 1,
        });
        assert!(matches!(
            simulation.required_job(position, &zone),
            Some(JobKind::SprayPests | JobKind::LaserPests)
        ));
        Ok(())
    }

    #[test]
    fn robot_returns_to_charger() -> Result<(), SimulationError> {
        let mut simulation = simulation()?;
        let robot = simulation
            .robots
            .first_mut()
            .ok_or_else(|| missing("starter robot should exist"))?;
        robot.battery = 1.0;
        robot.position = TilePos::new(20, 20);
        robot.state = RobotState::Idle;
        simulation.advance_minutes(360);
        let charged = simulation
            .robots
            .first()
            .is_some_and(|robot| robot.battery > 1.0)
            && simulation.metrics.robot_charge_minutes > 0;
        assert!(charged);
        Ok(())
    }

    #[test]
    fn harvest_reaches_inventory_and_packer() -> Result<(), SimulationError> {
        let mut simulation = simulation()?;
        simulation.current_contract = None;
        let position = TilePos::new(10, 20);
        simulation.zones = vec![FieldZone {
            id: 99,
            name: "Harvest Test".to_owned(),
            origin: position,
            size: (1, 1),
            crop_id: "wheat".to_owned(),
            priority: 100,
            manager: None,
        }];
        simulation.jobs.clear();
        let tile = simulation
            .grid
            .tile_mut(position)
            .ok_or_else(|| missing("starter tile should exist"))?;
        tile.tilled = true;
        tile.crop = Some(CropInstance {
            crop_id: "wheat".to_owned(),
            planted_at: 0,
            stage_index: 3,
            stage_progress: 1,
            moisture: 100,
            health: 100,
            pollinated: false,
            inspection_due: false,
            pest_pressure: 0,
            pest_controlled: true,
            weed_pressure: 0,
            soil_compaction: 0,
            remaining_harvests: 1,
        });
        simulation.advance_minutes(720);
        let total = simulation.inventory.amount("wheat")
            + simulation.inventory.amount("packed_wheat")
            + simulation
                .robots
                .iter()
                .map(|robot| robot.inventory.amount("wheat"))
                .sum::<u32>();
        assert!(
            total > 0 || simulation.metrics.contracts_fulfilled > 0,
            "metrics={:?}, robots={:?}, jobs={:?}",
            simulation.metrics,
            simulation.robots,
            simulation.jobs,
        );
        assert!(simulation.metrics.packed_items > 0 || simulation.metrics.contracts_fulfilled > 0);
        Ok(())
    }

    #[test]
    fn contract_completes_after_delivery() -> Result<(), SimulationError> {
        let mut simulation = simulation()?;
        simulation.inventory.add("packed_rice", 120);
        simulation.update_economy();
        assert_eq!(
            simulation.contracts.first().map(|contract| contract.status),
            Some(ContractStatus::Completed)
        );
        assert!(simulation.credits > 5_000);
        Ok(())
    }

    #[test]
    fn starter_farm_completes_rice_contract_automatically() -> Result<(), SimulationError> {
        let mut simulation = simulation()?;
        simulation.advance_minutes(60_000);
        assert_eq!(
            simulation.contracts.first().map(|contract| contract.status),
            Some(ContractStatus::Completed),
            "metrics={:?}, robots={:?}, jobs={}, crops={}, inventory={:?}",
            simulation.metrics,
            simulation.robots,
            simulation.jobs.len(),
            simulation
                .grid
                .tiles
                .iter()
                .filter(|tile| tile.crop.is_some())
                .count(),
            simulation.inventory.items,
        );
        assert!(simulation.metrics.crops_produced >= 120);
        assert!(simulation.metrics.packed_items >= 120);
        Ok(())
    }

    #[test]
    fn autonomy_trial_finishes_with_report() -> Result<(), SimulationError> {
        let mut simulation = simulation()?;
        let envelope = simulation.next_command(
            CommandActor::Human,
            GameCommand::Farm(FarmCommand::StartAutonomyTrial {
                duration_minutes: 120,
            }),
        );
        let applied = simulation.apply_command(envelope);
        assert!(applied.is_ok());
        assert_eq!(
            simulation
                .autonomy_trial
                .as_ref()
                .map(|trial| trial.manual_interventions),
            Some(0)
        );
        simulation.advance_minutes(120);
        assert!(simulation.last_autonomy_report.is_some());
        Ok(())
    }

    #[test]
    fn stale_world_revision_is_rejected() -> Result<(), SimulationError> {
        let mut simulation = simulation()?;
        let envelope = CommandEnvelope {
            id: 1,
            actor: CommandActor::Human,
            expected_world_revision: 99,
            command: GameCommand::Farm(FarmCommand::SetZonePriority {
                zone_id: 1,
                priority: 80,
            }),
        };
        assert!(matches!(
            simulation.apply_command(envelope),
            Err(CommandError::StaleRevision { .. })
        ));
        Ok(())
    }

    #[test]
    fn save_load_preserves_simulation_state() -> Result<(), SimulationError> {
        let mut simulation = simulation()?;
        simulation.advance_minutes(25);
        simulation.credits = 7_777;
        let serialized = simulation.to_ron()?;
        let restored = GameSimulation::from_ron(&serialized)?;
        assert_eq!(restored.credits, 7_777);
        assert_eq!(restored.clock, simulation.clock);
        assert_eq!(restored.grid, simulation.grid);
        Ok(())
    }
}
