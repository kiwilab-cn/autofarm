use serde::{Deserialize, Serialize};

use crate::{FacilityKind, FarmGrid, TerrainKind, Tile, TilePos};

const VERDANT_PADDY: &str = include_str!("../../../assets/maps/verdant-paddy/map.ron");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerrainRegionDef {
    pub terrain: TerrainKind,
    pub origin: TilePos,
    pub size: (u32, u32),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapFacilityDef {
    pub kind: FacilityKind,
    pub position: TilePos,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapZoneDef {
    pub name: String,
    pub origin: TilePos,
    pub size: (u32, u32),
    pub crop_id: String,
    pub priority: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapDefinition {
    pub id: String,
    pub display_name: String,
    pub width: u32,
    pub height: u32,
    pub tile_size: u32,
    pub background_asset: String,
    pub default_terrain: TerrainKind,
    pub terrain_regions: Vec<TerrainRegionDef>,
    pub garage_exit: TilePos,
    pub garage_bays: Vec<TilePos>,
    pub starter_facilities: Vec<MapFacilityDef>,
    pub starter_zones: Vec<MapZoneDef>,
    pub starter_robots: Vec<String>,
}

impl MapDefinition {
    pub fn load_embedded() -> Result<Self, ron::error::SpannedError> {
        ron::from_str(VERDANT_PADDY)
    }
}

impl FarmGrid {
    #[must_use]
    pub fn from_definition(map: &MapDefinition) -> Self {
        let mut grid = Self {
            width: map.width,
            height: map.height,
            tiles: vec![Tile::new(map.default_terrain); (map.width * map.height) as usize],
        };
        for region in &map.terrain_regions {
            for position in grid.positions_in_rect(region.origin, region.size) {
                if let Some(tile) = grid.tile_mut(position) {
                    tile.terrain = region.terrain;
                }
            }
        }
        grid
    }
}
