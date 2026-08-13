use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

pub type EntityId = u64;
pub type JobId = u64;
pub type ZoneId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TilePos {
    pub x: u32,
    pub y: u32,
}

impl TilePos {
    #[must_use]
    pub const fn new(x: u32, y: u32) -> Self {
        Self { x, y }
    }

    #[must_use]
    pub fn manhattan(self, other: Self) -> u32 {
        self.x.abs_diff(other.x) + self.y.abs_diff(other.y)
    }

    #[must_use]
    pub fn step_toward(self, target: Self) -> Self {
        if self.x < target.x {
            Self::new(self.x + 1, self.y)
        } else if self.x > target.x {
            Self::new(self.x - 1, self.y)
        } else if self.y < target.y {
            Self::new(self.x, self.y + 1)
        } else if self.y > target.y {
            Self::new(self.x, self.y - 1)
        } else {
            self
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TerrainKind {
    Soil,
    RoughSoil,
    Grass,
    Water,
    Rock,
    Concrete,
}

impl TerrainKind {
    #[must_use]
    pub const fn farmable(self) -> bool {
        matches!(self, Self::Soil | Self::RoughSoil | Self::Grass)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrowthStageDef {
    pub name: String,
    pub duration_minutes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CropDef {
    pub id: String,
    pub display_name: String,
    pub stages: Vec<GrowthStageDef>,
    pub water_threshold: u8,
    pub fertility_cost: u8,
    pub needs_pollination: bool,
    pub needs_inspection: bool,
    pub harvest_capability: Capability,
    pub harvest_count: u8,
    pub harvest_yield: u32,
    pub market_value: i32,
}

impl CropDef {
    #[must_use]
    pub fn packed_item(&self) -> String {
        format!("packed_{}", self.id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CropInstance {
    pub crop_id: String,
    pub stage_index: usize,
    pub stage_progress: u32,
    pub moisture: u8,
    pub health: u8,
    pub pollinated: bool,
    pub inspection_due: bool,
    pub remaining_harvests: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tile {
    pub terrain: TerrainKind,
    pub moisture: u8,
    pub fertility: u8,
    pub crop: Option<CropInstance>,
    pub building: Option<EntityId>,
    pub occupied: bool,
    pub tilled: bool,
}

impl Tile {
    #[must_use]
    pub const fn new(terrain: TerrainKind) -> Self {
        Self {
            terrain,
            moisture: 50,
            fertility: 80,
            crop: None,
            building: None,
            occupied: false,
            tilled: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FarmGrid {
    pub width: u32,
    pub height: u32,
    pub tiles: Vec<Tile>,
}

impl FarmGrid {
    #[must_use]
    pub fn fixed_map(width: u32, height: u32, seed: u64) -> Self {
        let mut tiles = Vec::with_capacity((width * height) as usize);
        for y in 0..height {
            for x in 0..width {
                let terrain_noise =
                    x.wrapping_mul(73_856_093) ^ y.wrapping_mul(19_349_663) ^ seed as u32;
                let terrain = if x == 0 || y == 0 || x + 1 == width || y + 1 == height {
                    TerrainKind::Rock
                } else if x > 54 && (8..56).contains(&y) {
                    TerrainKind::Water
                } else if terrain_noise.is_multiple_of(31) {
                    TerrainKind::RoughSoil
                } else if x < 5 || y < 5 {
                    TerrainKind::Grass
                } else {
                    TerrainKind::Soil
                };
                tiles.push(Tile::new(terrain));
            }
        }
        Self {
            width,
            height,
            tiles,
        }
    }

    #[must_use]
    pub const fn contains(&self, position: TilePos) -> bool {
        position.x < self.width && position.y < self.height
    }

    #[must_use]
    pub fn index(&self, position: TilePos) -> Option<usize> {
        self.contains(position)
            .then_some((position.y * self.width + position.x) as usize)
    }

    #[must_use]
    pub fn tile(&self, position: TilePos) -> Option<&Tile> {
        self.index(position).and_then(|index| self.tiles.get(index))
    }

    pub fn tile_mut(&mut self, position: TilePos) -> Option<&mut Tile> {
        self.index(position)
            .and_then(|index| self.tiles.get_mut(index))
    }

    #[must_use]
    pub fn positions_in_rect(&self, origin: TilePos, size: (u32, u32)) -> Vec<TilePos> {
        let mut positions = Vec::new();
        for y in origin.y..origin.y.saturating_add(size.1).min(self.height) {
            for x in origin.x..origin.x.saturating_add(size.0).min(self.width) {
                positions.push(TilePos::new(x, y));
            }
        }
        positions
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Capability {
    Till,
    Seed,
    Plant,
    Water,
    Pollinate,
    Inspect,
    Harvest,
    PrecisionHarvest,
    Dig,
    Haul,
    Repair,
    Pack,
    Spray,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RobotBody {
    Wheeled,
    Flying,
    Biped,
    Quadruped,
    Hexapod,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RobotDef {
    pub id: String,
    pub display_name: String,
    pub body: RobotBody,
    pub capabilities: BTreeSet<Capability>,
    pub battery_capacity: f32,
    pub energy_per_tile: f32,
    pub cargo_capacity: u32,
    pub work_speed: f32,
    pub purchase_price: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RobotState {
    Idle,
    MovingToJob(JobId),
    Working(JobId),
    MovingToCharge,
    Charging,
    MovingToStorage,
    Broken,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Inventory {
    pub items: BTreeMap<String, u32>,
    pub capacity: u32,
}

impl Inventory {
    #[must_use]
    pub fn amount(&self, item: &str) -> u32 {
        self.items.get(item).copied().unwrap_or_default()
    }

    #[must_use]
    pub fn used(&self) -> u32 {
        self.items.values().sum()
    }

    pub fn add(&mut self, item: impl Into<String>, amount: u32) -> u32 {
        let accepted = amount.min(self.capacity.saturating_sub(self.used()));
        if accepted > 0 {
            *self.items.entry(item.into()).or_default() += accepted;
        }
        accepted
    }

    pub fn remove(&mut self, item: &str, amount: u32) -> u32 {
        let Some(current) = self.items.get_mut(item) else {
            return 0;
        };
        let removed = amount.min(*current);
        *current -= removed;
        if *current == 0 {
            self.items.remove(item);
        }
        removed
    }

    pub fn drain_into(&mut self, destination: &mut Self) -> u32 {
        let items = std::mem::take(&mut self.items);
        let mut moved = 0;
        for (item, amount) in items {
            moved += destination.add(item, amount);
        }
        moved
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Robot {
    pub id: EntityId,
    pub def_id: String,
    pub body: RobotBody,
    pub capabilities: BTreeSet<Capability>,
    pub battery: f32,
    pub battery_capacity: f32,
    pub energy_per_tile: f32,
    pub work_speed: f32,
    pub state: RobotState,
    pub current_job: Option<JobId>,
    pub inventory: Inventory,
    pub condition: f32,
    pub position: TilePos,
    pub work_progress: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum JobKind {
    Till,
    Seed,
    Plant,
    Water,
    Pollinate,
    Inspect,
    Harvest,
    PrecisionHarvest,
    Dig,
    Haul,
    Repair,
    Recharge,
    Pack,
}

impl JobKind {
    #[must_use]
    pub const fn required_capability(self) -> Capability {
        match self {
            Self::Till => Capability::Till,
            Self::Seed => Capability::Seed,
            Self::Plant => Capability::Plant,
            Self::Water => Capability::Water,
            Self::Pollinate => Capability::Pollinate,
            Self::Inspect => Capability::Inspect,
            Self::Harvest => Capability::Harvest,
            Self::PrecisionHarvest => Capability::PrecisionHarvest,
            Self::Dig => Capability::Dig,
            Self::Haul => Capability::Haul,
            Self::Repair => Capability::Repair,
            Self::Recharge => Capability::Repair,
            Self::Pack => Capability::Pack,
        }
    }

    #[must_use]
    pub const fn effort(self) -> f32 {
        match self {
            Self::Dig | Self::Repair => 4.0,
            Self::Harvest | Self::PrecisionHarvest | Self::Till => 3.0,
            Self::Seed | Self::Plant | Self::Water | Self::Pollinate | Self::Inspect => 2.0,
            Self::Haul | Self::Recharge | Self::Pack => 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobStatus {
    Pending,
    Assigned,
    Completed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    pub id: JobId,
    pub kind: JobKind,
    pub location: TilePos,
    pub required_capability: Capability,
    pub priority: u8,
    pub zone_id: ZoneId,
    pub created_at: u64,
    pub deadline: Option<u64>,
    pub assigned_robot: Option<EntityId>,
    pub status: JobStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldZone {
    pub id: ZoneId,
    pub name: String,
    pub origin: TilePos,
    pub size: (u32, u32),
    pub crop_id: String,
    pub priority: u8,
    pub manager: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum FacilityKind {
    Warehouse,
    SeedStorage,
    ChargingStation,
    WaterPump,
    IrrigationNode,
    Packer,
    ShippingDock,
    SolarGenerator,
    Battery,
}

impl FacilityKind {
    #[must_use]
    pub const fn price(self) -> i32 {
        match self {
            Self::Warehouse => 800,
            Self::SeedStorage => 300,
            Self::ChargingStation => 650,
            Self::WaterPump => 500,
            Self::IrrigationNode => 450,
            Self::Packer => 900,
            Self::ShippingDock => 1000,
            Self::SolarGenerator => 700,
            Self::Battery => 550,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Facility {
    pub id: EntityId,
    pub kind: FacilityKind,
    pub position: TilePos,
    pub powered: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemRequirement {
    pub item: String,
    pub amount: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractDef {
    pub id: String,
    pub display_name: String,
    pub requirements: Vec<ItemRequirement>,
    pub deadline_minutes: u64,
    pub reward: i32,
    pub reputation: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContractStatus {
    Locked,
    Active,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveContract {
    pub definition: ContractDef,
    pub delivered: BTreeMap<String, u32>,
    pub accepted_at: u64,
    pub deadline: u64,
    pub status: ContractStatus,
    pub priority: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Weather {
    Clear,
    Rain,
    Hot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimClock {
    pub minute: u64,
    pub paused: bool,
    pub speed: u8,
}

impl Default for SimClock {
    fn default() -> Self {
        Self {
            minute: 8 * 60,
            paused: false,
            speed: 1,
        }
    }
}

impl SimClock {
    #[must_use]
    pub fn day(self) -> u64 {
        self.minute / 1440 + 1
    }

    #[must_use]
    pub fn hour(self) -> u64 {
        (self.minute % 1440) / 60
    }

    #[must_use]
    pub fn minute_of_hour(self) -> u64 {
        self.minute % 60
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PowerGrid {
    pub production: f32,
    pub consumption: f32,
    pub stored: f32,
    pub capacity: f32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WaterNetwork {
    pub available_water: f32,
    pub max_flow: f32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameMetrics {
    pub crops_produced: u64,
    pub crops_lost: u64,
    pub robot_work_minutes: u64,
    pub robot_idle_minutes: u64,
    pub robot_charge_minutes: u64,
    pub robot_recoveries: u64,
    pub jobs_created: u64,
    pub jobs_completed: u64,
    pub jobs_failed: u64,
    pub energy_generated: u64,
    pub energy_consumed: u64,
    pub water_consumed: u64,
    pub contracts_fulfilled: u64,
    pub contracts_expected: u64,
    pub manual_commands: u64,
    pub npc_commands: u64,
    pub packed_items: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutonomyTrial {
    pub start_time: u64,
    pub end_time: u64,
    pub baseline: GameMetrics,
    pub manual_interventions: u32,
    pub finished: bool,
    pub score: Option<u8>,
    pub grade: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NpcAssignment {
    pub npc_id: String,
    pub display_name: String,
    pub managed_zones: BTreeSet<ZoneId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FarmEvent {
    CropReady(TilePos),
    CropCritical(TilePos),
    RobotLowBattery(EntityId),
    RobotBroken(EntityId),
    StorageFull,
    WaterLow,
    PowerLow,
    ContractAccepted(String),
    ContractNearDeadline(String),
    ContractCompleted(String),
    ContractFailed(String),
    AutonomyTrialStarted,
    AutonomyTrialCompleted(u8),
    AiAction {
        actor: String,
        message: String,
        reason: String,
    },
    Info(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FarmSnapshot {
    pub world_revision: u64,
    pub time: SimClock,
    pub weather: Weather,
    pub credits: i32,
    pub reputation: u32,
    pub power: PowerGrid,
    pub water: WaterNetwork,
    pub inventory: BTreeMap<String, u32>,
    pub robots_idle: usize,
    pub robots_working: usize,
    pub robots_charging: usize,
    pub robots_broken: usize,
    pub active_contract: Option<ActiveContract>,
    pub pending_jobs: usize,
    pub recent_events: Vec<FarmEvent>,
}
