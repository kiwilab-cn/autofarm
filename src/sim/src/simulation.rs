use std::collections::{BTreeMap, BTreeSet};

use ron::ser::PrettyConfig;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::pathfinding::{next_ground_step, path_exists};
use crate::{
    ActiveContract, AutonomyReport, AutonomyTrial, Capability, CommandActor, CommandEnvelope,
    CommandError, CommandPermissions, ContentCatalog, ContentError, ContractStatus, CropInstance,
    EditorCommand, Facility, FacilityKind, FarmCommand, FarmEvent, FarmGrid, FarmSnapshot,
    FieldZone, GameCommand, GameMetrics, Inventory, Job, JobKind, JobStatus, NpcAssignment,
    PowerGrid, Robot, RobotBody, RobotState, SimClock, TerrainKind, TilePos, WaterNetwork, Weather,
    calculate_autonomy_report,
};

pub const SAVE_VERSION: u32 = 1;
const MAP_SIZE: u32 = 64;
const MAX_EVENTS: usize = 80;

#[derive(Debug, Error)]
pub enum SimulationError {
    #[error(transparent)]
    Content(#[from] ContentError),
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
        let mut simulation = Self {
            version: SAVE_VERSION,
            seed,
            catalog: catalog.clone(),
            grid: FarmGrid::fixed_map(MAP_SIZE, MAP_SIZE, seed),
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
        for (kind, position) in [
            (FacilityKind::Warehouse, TilePos::new(29, 29)),
            (FacilityKind::ChargingStation, TilePos::new(31, 29)),
            (FacilityKind::ShippingDock, TilePos::new(33, 29)),
            (FacilityKind::Packer, TilePos::new(29, 31)),
            (FacilityKind::SolarGenerator, TilePos::new(31, 31)),
            (FacilityKind::Battery, TilePos::new(33, 31)),
            (FacilityKind::WaterPump, TilePos::new(35, 29)),
            (FacilityKind::IrrigationNode, TilePos::new(18, 16)),
        ] {
            simulation.spawn_facility_unchecked(kind, position);
        }
        simulation.spawn_robot_unchecked("basic_rover", TilePos::new(31, 29));
        simulation.spawn_robot_unchecked("basic_rover", TilePos::new(31, 29));
        simulation.spawn_robot_unchecked("pollination_drone", TilePos::new(31, 29));
        simulation.create_zone_unchecked(TilePos::new(10, 10), (5, 5), "wheat", 70);
        simulation.push_event(FarmEvent::Info(
            "Starter systems online. Robots are claiming wheat jobs automatically.".to_owned(),
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
                if matches!(*speed, 0 | 1 | 4 | 16) {
                    Ok(())
                } else {
                    Err(CommandError::Invalid(
                        "simulation speed must be 0, 1, 4, or 16".to_owned(),
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
                let position = self.facility_position(FacilityKind::ChargingStation);
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
                    self.spawn_robot_unchecked(&robot_def_id, position);
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
        if self.clock.minute.is_multiple_of(1440) {
            let roll = (self.seed.wrapping_add(self.clock.day() * 17)) % 10;
            self.weather = match roll {
                0..=2 => Weather::Rain,
                3..=4 => Weather::Hot,
                _ => Weather::Clear,
            };
            self.push_event(FarmEvent::Info(format!(
                "Day {} weather: {:?}",
                self.clock.day(),
                self.weather
            )));
        }
    }

    fn update_environment(&mut self) {
        if !self.clock.minute.is_multiple_of(5) {
            return;
        }
        for tile in &mut self.grid.tiles {
            let Some(crop) = tile.crop.as_mut() else {
                continue;
            };
            match self.weather {
                Weather::Rain => crop.moisture = crop.moisture.saturating_add(9).min(100),
                Weather::Hot => crop.moisture = crop.moisture.saturating_sub(4),
                Weather::Clear => crop.moisture = crop.moisture.saturating_sub(2),
            }
            if crop.moisture == 0 {
                crop.health = crop.health.saturating_sub(4);
            } else if crop.moisture < 20 {
                crop.health = crop.health.saturating_sub(1);
            }
            if crop.health == 0 {
                tile.crop = None;
                self.metrics.crops_lost += 1;
            }
        }
    }

    fn update_crops(&mut self) {
        let inspection_tick = self.clock.minute.is_multiple_of(45);
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
                if crop.moisture == 0 || crop.health == 0 {
                    critical_positions.push(position);
                    continue;
                }
                if definition.needs_pollination && crop.stage_index >= 1 && !crop.pollinated {
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
                    job.location == position
                        && job.kind == kind
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
                    deadline: Some(self.clock.minute + 180),
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
        if tile.crop.is_none() {
            return if tile.tilled {
                Some(if zone.crop_id == "wheat" {
                    JobKind::Seed
                } else {
                    JobKind::Plant
                })
            } else {
                Some(JobKind::Till)
            };
        }
        let crop = tile.crop.as_ref()?;
        let definition = self.catalog.crops.get(&crop.crop_id)?;
        if crop.moisture < definition.water_threshold {
            return Some(JobKind::Water);
        }
        if definition.needs_pollination && crop.stage_index >= 1 && !crop.pollinated {
            return Some(JobKind::Pollinate);
        }
        if definition.needs_inspection && crop.inspection_due {
            return Some(JobKind::Inspect);
        }
        if crop.stage_index + 1 == definition.stages.len() {
            return Some(match definition.harvest_capability {
                Capability::Dig => JobKind::Dig,
                Capability::PrecisionHarvest => JobKind::PrecisionHarvest,
                _ => JobKind::Harvest,
            });
        }
        None
    }

    fn assign_jobs(&mut self) {
        for robot_index in 0..self.robots.len() {
            let robot = &self.robots[robot_index];
            if robot.state != RobotState::Idle || robot.battery < 18.0 {
                continue;
            }
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
                robot.state = RobotState::MovingToJob(job_id);
            }
        }
    }

    fn robot_can_reach(&self, robot: &Robot, target: TilePos) -> bool {
        if robot.body == RobotBody::Flying {
            return true;
        }
        path_exists(&self.grid, robot.position, target, robot.body)
    }

    fn update_robots(&mut self) {
        let charger = self.facility_position(FacilityKind::ChargingStation);
        let warehouse = self.facility_position(FacilityKind::Warehouse);
        let mut completed_jobs = Vec::new();
        for index in 0..self.robots.len() {
            let state = self.robots[index].state.clone();
            match state {
                RobotState::Idle => {
                    self.metrics.robot_idle_minutes += 1;
                    if self.robots[index].battery <= 20.0 {
                        let id = self.robots[index].id;
                        self.robots[index].state = RobotState::MovingToCharge;
                        self.push_event(FarmEvent::RobotLowBattery(id));
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
                        if self.robots[index].position == target {
                            self.robots[index].state = RobotState::Working(job_id);
                            self.robots[index].work_progress = 0.0;
                        }
                    } else {
                        self.reset_robot(index);
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
                        completed_jobs.push((index, job_id));
                    }
                }
                RobotState::MovingToCharge => {
                    self.move_robot(index, charger);
                    if self.robots[index].position == charger {
                        self.robots[index].state = RobotState::Charging;
                    }
                }
                RobotState::Charging => {
                    self.metrics.robot_charge_minutes += 1;
                    let capacity = self.robots[index].battery_capacity;
                    self.robots[index].battery = (self.robots[index].battery + 8.0).min(capacity);
                    self.power.stored = (self.power.stored - 2.0).max(0.0);
                    self.metrics.energy_consumed += 2;
                    if self.robots[index].battery >= capacity * 0.92 {
                        self.robots[index].state = RobotState::Idle;
                    }
                }
                RobotState::MovingToStorage => {
                    self.move_robot(index, warehouse);
                    if self.robots[index].position == warehouse {
                        self.robots[index].inventory.drain_into(&mut self.inventory);
                        self.robots[index].state = RobotState::Idle;
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
        let steps = if self.robots[index].body == RobotBody::Flying {
            2
        } else {
            1
        };
        for _ in 0..steps {
            if self.robots[index].position == target {
                break;
            }
            let current = self.robots[index].position;
            let next = if self.robots[index].body == RobotBody::Flying {
                Some(current.step_toward(target))
            } else {
                next_ground_step(&self.grid, current, target, self.robots[index].body)
            };
            let Some(next) = next else {
                break;
            };
            self.robots[index].position = next;
            let energy = self.robots[index].energy_per_tile;
            self.robots[index].battery = (self.robots[index].battery - energy).max(0.0);
            self.metrics.energy_consumed += energy.ceil() as u64;
        }
    }

    fn reset_robot(&mut self, index: usize) {
        self.robots[index].state = RobotState::Idle;
        self.robots[index].current_job = None;
        self.robots[index].work_progress = 0.0;
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
            .unwrap_or_else(|| "wheat".to_owned());
        let crop_definition = self.catalog.crops.get(&zone_crop).cloned();
        let mut harvested: Option<(String, u32)> = None;
        if let Some(tile) = self.grid.tile_mut(job.location) {
            match job.kind {
                JobKind::Till => {
                    tile.tilled = true;
                    tile.terrain = TerrainKind::Soil;
                }
                JobKind::Seed | JobKind::Plant => {
                    if let Some(definition) = crop_definition {
                        tile.crop = Some(CropInstance {
                            crop_id: definition.id,
                            stage_index: 0,
                            stage_progress: 0,
                            moisture: tile.moisture.max(55),
                            health: 100,
                            pollinated: false,
                            inspection_due: definition.needs_inspection,
                            remaining_harvests: definition.harvest_count,
                        });
                        tile.fertility = tile.fertility.saturating_sub(definition.fertility_cost);
                    }
                }
                JobKind::Water => {
                    if let Some(crop) = tile.crop.as_mut() {
                        crop.moisture = crop.moisture.saturating_add(60).min(100);
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
                JobKind::Harvest | JobKind::PrecisionHarvest | JobKind::Dig => {
                    if let Some(crop) = tile.crop.as_mut()
                        && let Some(definition) = self.catalog.crops.get(&crop.crop_id)
                    {
                        harvested = Some((definition.id.clone(), definition.harvest_yield));
                        if crop.remaining_harvests > 1 {
                            crop.remaining_harvests -= 1;
                            crop.stage_index = definition.stages.len().saturating_sub(2);
                            crop.stage_progress = 0;
                            crop.pollinated = !definition.needs_pollination;
                            crop.inspection_due = definition.needs_inspection;
                        } else {
                            tile.crop = None;
                        }
                    }
                }
                JobKind::Haul | JobKind::Repair | JobKind::Recharge | JobKind::Pack => {}
            }
        }
        if let Some((item, amount)) = harvested {
            self.robots[robot_index].inventory.add(item, amount);
            self.metrics.crops_produced += u64::from(amount);
        }
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
            .filter(|robot| matches!(robot.state, RobotState::Working(_)))
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
                .filter(|robot| robot.state == RobotState::Idle)
                .count(),
            robots_working: self
                .robots
                .iter()
                .filter(|robot| {
                    matches!(
                        robot.state,
                        RobotState::Working(_) | RobotState::MovingToJob(_)
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
        let simulation: Self = ron::from_str(input)
            .map_err(|error| SimulationError::Serialization(error.to_string()))?;
        if simulation.version != SAVE_VERSION {
            return Err(SimulationError::UnsupportedSaveVersion {
                found: simulation.version,
                expected: SAVE_VERSION,
            });
        }
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
        if tile.building.is_some() || matches!(tile.terrain, TerrainKind::Water | TerrainKind::Rock)
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
            state: RobotState::Idle,
            current_job: None,
            inventory: Inventory {
                items: BTreeMap::new(),
                capacity: definition.cargo_capacity,
            },
            condition: 100.0,
            position,
            work_progress: 0.0,
        });
        self.push_event(FarmEvent::Info(format!(
            "{} deployed",
            definition.display_name
        )));
        Some(id)
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

    fn delete_entity(&mut self, entity_id: u64) {
        if let Some(index) = self
            .facilities
            .iter()
            .position(|facility| facility.id == entity_id)
        {
            let facility = self.facilities.remove(index);
            if let Some(tile) = self.grid.tile_mut(facility.position) {
                tile.building = None;
                tile.occupied = false;
                tile.terrain = TerrainKind::Soil;
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
    fn crop_progresses_through_stages() -> Result<(), SimulationError> {
        let mut simulation = simulation()?;
        let position = TilePos::new(8, 8);
        let tile = simulation
            .grid
            .tile_mut(position)
            .ok_or_else(|| missing("test tile should exist"))?;
        tile.crop = Some(CropInstance {
            crop_id: "wheat".to_owned(),
            stage_index: 0,
            stage_progress: 0,
            moisture: 100,
            health: 100,
            pollinated: false,
            inspection_due: false,
            remaining_harvests: 1,
        });
        simulation.advance_minutes(45);
        let stage = simulation
            .grid
            .tile(position)
            .and_then(|tile| tile.crop.as_ref())
            .map(|crop| crop.stage_index);
        assert!(stage.is_some_and(|stage| stage >= 2));
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
            stage_index: 0,
            stage_progress: 0,
            moisture: 0,
            health: 20,
            pollinated: false,
            inspection_due: false,
            remaining_harvests: 1,
        });
        simulation.update_environment();
        let health = simulation
            .grid
            .tile(position)
            .and_then(|tile| tile.crop.as_ref())
            .map(|crop| crop.health);
        assert_eq!(health, Some(16));
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
            stage_index: 1,
            stage_progress: 0,
            moisture: 100,
            health: 100,
            pollinated: false,
            inspection_due: false,
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
    fn scheduler_assigns_pollination_to_drone_not_rover() -> Result<(), SimulationError> {
        let mut simulation = simulation()?;
        simulation.jobs.clear();
        for robot in &mut simulation.robots {
            robot.state = RobotState::Idle;
            robot.current_job = None;
        }
        simulation.jobs.push(Job {
            id: 999,
            kind: JobKind::Pollinate,
            location: TilePos::new(12, 12),
            required_capability: Capability::Pollinate,
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
    fn robot_returns_to_charger() -> Result<(), SimulationError> {
        let mut simulation = simulation()?;
        let robot = simulation
            .robots
            .first_mut()
            .ok_or_else(|| missing("starter robot should exist"))?;
        robot.battery = 1.0;
        robot.position = TilePos::new(20, 20);
        robot.state = RobotState::Idle;
        simulation.advance_minutes(40);
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
        let position = TilePos::new(10, 10);
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
            stage_index: 3,
            stage_progress: 1,
            moisture: 100,
            health: 100,
            pollinated: false,
            inspection_due: false,
            remaining_harvests: 1,
        });
        simulation.advance_minutes(140);
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
        simulation.inventory.add("packed_wheat", 100);
        simulation.update_economy();
        assert_eq!(
            simulation.contracts.first().map(|contract| contract.status),
            Some(ContractStatus::Completed)
        );
        assert!(simulation.credits > 5_000);
        Ok(())
    }

    #[test]
    fn starter_farm_completes_wheat_contract_automatically() -> Result<(), SimulationError> {
        let mut simulation = simulation()?;
        simulation.advance_minutes(2_800);
        assert_eq!(
            simulation.contracts.first().map(|contract| contract.status),
            Some(ContractStatus::Completed)
        );
        assert!(simulation.metrics.crops_produced >= 100);
        assert!(simulation.metrics.packed_items >= 100);
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
