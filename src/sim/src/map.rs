use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::{FacilityKind, FarmGrid, TerrainKind, Tile, TilePos};

const VERDANT_PADDY: &str = include_str!("../../../assets/maps/verdant-paddy/verdant-paddy.tmj");
const TERRAIN_TILESET_ASSET: &str = "art/pixel/tilesets/verdant-paddy-terrain.png";
const INFRASTRUCTURE_TILESET_ASSET: &str = "art/pixel/tilesets/verdant-paddy-infrastructure.png";
const FACILITY_TILESET_ASSET: &str = "art/pixel/tilesets/verdant-paddy-facilities.png";
const TERRAIN_TILE_COUNT: u32 = 16;
const INFRASTRUCTURE_FIRST_GID: u32 = 17;
const INFRASTRUCTURE_TILE_COUNT: u32 = 8;
const TILED_FLIP_HORIZONTAL: u32 = 0x8000_0000;
const TILED_FLIP_VERTICAL: u32 = 0x4000_0000;
const TILED_FLIP_DIAGONAL: u32 = 0x2000_0000;
const TILED_FLIP_HEXAGONAL_120: u32 = 0x1000_0000;
const TILED_FLIP_MASK: u32 = 0xf000_0000;

#[derive(Debug, Error)]
pub enum MapError {
    #[error("Tiled JSON is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Tiled map is invalid: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MapTilesetKind {
    Terrain,
    Infrastructure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapTileCellDef {
    pub tileset: MapTilesetKind,
    pub atlas_index: u16,
    pub flip_x: bool,
    pub flip_y: bool,
    pub rotation_quarters: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapTileLayerDef {
    pub name: String,
    pub render_z: i32,
    pub tiles: Vec<Option<MapTileCellDef>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapFacilityDef {
    pub kind: FacilityKind,
    pub position: TilePos,
    pub visual_origin: TilePos,
    pub visual_size: (u32, u32),
    pub atlas_index: u16,
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
pub struct MapNavigationAreaDef {
    pub name: String,
    pub origin: TilePos,
    pub size: (u32, u32),
    pub ground_cost: u8,
    pub legged_cost: u8,
    pub wheeled_empty_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapCollisionAreaDef {
    pub name: String,
    pub origin: TilePos,
    pub size: (u32, u32),
    pub blocks: String,
    pub owner: Option<FacilityKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapDefinition {
    pub id: String,
    pub display_name: String,
    pub width: u32,
    pub height: u32,
    pub tile_size: u32,
    pub terrain_tileset_asset: String,
    pub infrastructure_tileset_asset: String,
    pub facility_tileset_asset: String,
    pub tile_layers: Vec<MapTileLayerDef>,
    pub terrain_tiles: Vec<TerrainKind>,
    pub garage_exit: TilePos,
    pub garage_bays: Vec<TilePos>,
    pub starter_facilities: Vec<MapFacilityDef>,
    pub starter_zones: Vec<MapZoneDef>,
    pub starter_robots: Vec<String>,
    pub navigation_areas: Vec<MapNavigationAreaDef>,
    pub collision_areas: Vec<MapCollisionAreaDef>,
    #[serde(skip)]
    ordered_garage_bays: Vec<(u32, TilePos)>,
    #[serde(skip)]
    ordered_robot_spawns: Vec<(u32, String)>,
}

impl Default for MapDefinition {
    fn default() -> Self {
        Self {
            id: String::new(),
            display_name: String::new(),
            width: 0,
            height: 0,
            tile_size: 0,
            terrain_tileset_asset: String::new(),
            infrastructure_tileset_asset: String::new(),
            facility_tileset_asset: String::new(),
            tile_layers: Vec::new(),
            terrain_tiles: Vec::new(),
            garage_exit: TilePos::new(0, 0),
            garage_bays: Vec::new(),
            starter_facilities: Vec::new(),
            starter_zones: Vec::new(),
            starter_robots: Vec::new(),
            navigation_areas: Vec::new(),
            collision_areas: Vec::new(),
            ordered_garage_bays: Vec::new(),
            ordered_robot_spawns: Vec::new(),
        }
    }
}

impl MapDefinition {
    pub fn load_embedded() -> Result<Self, MapError> {
        let tiled: TiledMap = serde_json::from_str(VERDANT_PADDY)?;
        Self::from_tiled(tiled)
    }

    fn from_tiled(tiled: TiledMap) -> Result<Self, MapError> {
        if tiled.orientation != "orthogonal" {
            return Err(MapError::Invalid(format!(
                "expected orthogonal orientation, found {}",
                tiled.orientation
            )));
        }
        if tiled.width == 0
            || tiled.height == 0
            || tiled.tilewidth == 0
            || tiled.tilewidth != tiled.tileheight
        {
            return Err(MapError::Invalid(
                "map and square tile dimensions must be non-zero".to_owned(),
            ));
        }
        let tile_count = (tiled.width * tiled.height) as usize;
        let mut definition = Self {
            id: string_property(&tiled.properties, "map_id")?,
            display_name: string_property(&tiled.properties, "display_name")?,
            width: tiled.width,
            height: tiled.height,
            tile_size: tiled.tilewidth,
            terrain_tileset_asset: TERRAIN_TILESET_ASSET.to_owned(),
            infrastructure_tileset_asset: INFRASTRUCTURE_TILESET_ASSET.to_owned(),
            facility_tileset_asset: FACILITY_TILESET_ASSET.to_owned(),
            terrain_tiles: vec![TerrainKind::Grass; tile_count],
            ..Self::default()
        };

        for layer in &tiled.layers {
            definition.read_tiled_layer(layer, tile_count)?;
        }
        definition.ordered_garage_bays.sort_by_key(|entry| entry.0);
        definition.ordered_robot_spawns.sort_by_key(|entry| entry.0);
        definition.garage_bays = std::mem::take(&mut definition.ordered_garage_bays)
            .into_iter()
            .map(|entry| entry.1)
            .collect();
        definition.starter_robots = std::mem::take(&mut definition.ordered_robot_spawns)
            .into_iter()
            .map(|entry| entry.1)
            .collect();
        definition.validate()?;
        Ok(definition)
    }

    fn read_tiled_layer(&mut self, layer: &TiledLayer, tile_count: usize) -> Result<(), MapError> {
        match layer.layer_type.as_str() {
            "tilelayer" => self.read_tile_layer(layer, tile_count),
            "objectgroup" => self.read_object_layer(layer),
            "group" => {
                for child in &layer.layers {
                    self.read_tiled_layer(child, tile_count)?;
                }
                Ok(())
            }
            other => Err(MapError::Invalid(format!(
                "unsupported layer type {other} on {}",
                layer.name
            ))),
        }
    }

    fn read_tile_layer(&mut self, layer: &TiledLayer, tile_count: usize) -> Result<(), MapError> {
        if layer.data.len() != tile_count {
            return Err(MapError::Invalid(format!(
                "layer {} has {} tiles; expected {tile_count}",
                layer.name,
                layer.data.len()
            )));
        }
        let logical = bool_property_or(&layer.properties, "simulation_terrain", false)?;
        let mut tiles = Vec::with_capacity(tile_count);
        for (index, tiled_gid) in layer.data.iter().copied().enumerate() {
            let cell = decode_tiled_gid(tiled_gid)?;
            if logical && let Some(cell) = cell {
                self.terrain_tiles[index] = terrain_for_cell(cell)?;
            }
            tiles.push(cell);
        }
        self.tile_layers.push(MapTileLayerDef {
            name: layer.name.clone(),
            render_z: int_property_or(&layer.properties, "render_z", 0)?,
            tiles,
        });
        Ok(())
    }

    fn read_object_layer(&mut self, layer: &TiledLayer) -> Result<(), MapError> {
        for object in &layer.objects {
            match object.class_name.as_str() {
                "Facility" => self.starter_facilities.push(MapFacilityDef {
                    kind: parse_facility_kind(&string_property(&object.properties, "kind")?)?,
                    position: TilePos::new(
                        u32_property(&object.properties, "anchor_x")?,
                        u32_property(&object.properties, "anchor_y")?,
                    ),
                    visual_origin: pixel_position(object.x, object.y, self.tile_size)?,
                    visual_size: pixel_size(object.width, object.height, self.tile_size)?,
                    atlas_index: u16_property(&object.properties, "atlas_index")?,
                }),
                "FieldZone" => self.starter_zones.push(MapZoneDef {
                    name: object.name.clone(),
                    origin: pixel_position(object.x, object.y, self.tile_size)?,
                    size: pixel_size(object.width, object.height, self.tile_size)?,
                    crop_id: string_property(&object.properties, "crop_id")?,
                    priority: u8_property(&object.properties, "priority")?,
                }),
                "GarageExit" => {
                    self.garage_exit = pixel_position(object.x, object.y, self.tile_size)?;
                }
                "GarageBay" => self.ordered_garage_bays.push((
                    u32_property(&object.properties, "order")?,
                    pixel_position(object.x, object.y, self.tile_size)?,
                )),
                "RobotSpawn" => self.ordered_robot_spawns.push((
                    u32_property(&object.properties, "order")?,
                    string_property(&object.properties, "robot_id")?,
                )),
                "NavigationArea" => self.navigation_areas.push(MapNavigationAreaDef {
                    name: object.name.clone(),
                    origin: pixel_position(object.x, object.y, self.tile_size)?,
                    size: pixel_size(object.width, object.height, self.tile_size)?,
                    ground_cost: u8_property_or(&object.properties, "ground_cost", 1)?,
                    legged_cost: u8_property_or(&object.properties, "legged_cost", 1)?,
                    wheeled_empty_only: bool_property_or(
                        &object.properties,
                        "wheeled_empty_only",
                        false,
                    )?,
                }),
                "CollisionArea" => self.collision_areas.push(MapCollisionAreaDef {
                    name: object.name.clone(),
                    origin: pixel_position(object.x, object.y, self.tile_size)?,
                    size: pixel_size(object.width, object.height, self.tile_size)?,
                    blocks: string_property(&object.properties, "blocks")?,
                    owner: optional_string_property(&object.properties, "owner")
                        .map(|owner| parse_facility_kind(&owner))
                        .transpose()?,
                }),
                "" => {
                    return Err(MapError::Invalid(format!(
                        "object {} on {} is missing a class",
                        object.name, layer.name
                    )));
                }
                other => {
                    return Err(MapError::Invalid(format!(
                        "unsupported object class {other} on {}",
                        layer.name
                    )));
                }
            }
        }
        Ok(())
    }

    fn validate(&self) -> Result<(), MapError> {
        if self.tile_layers.is_empty()
            || self.terrain_tiles.len() != (self.width * self.height) as usize
        {
            return Err(MapError::Invalid(
                "render and simulation tile layers are incomplete".to_owned(),
            ));
        }
        if self.garage_bays.len() < self.starter_robots.len() {
            return Err(MapError::Invalid(
                "garage has fewer bays than starter robots".to_owned(),
            ));
        }
        if !self.contains(self.garage_exit)
            || self.garage_bays.iter().any(|bay| !self.contains(*bay))
        {
            return Err(MapError::Invalid(
                "garage exit or bay lies outside the map".to_owned(),
            ));
        }
        for facility in &self.starter_facilities {
            self.validate_area(
                &format!("{:?} visual", facility.kind),
                facility.visual_origin,
                facility.visual_size,
            )?;
            if !self.contains(facility.position) {
                return Err(MapError::Invalid(format!(
                    "{:?} interaction point lies outside the map",
                    facility.kind
                )));
            }
            if self.collision_areas.iter().any(|area| {
                area.owner == Some(facility.kind)
                    && area_contains(area.origin, area.size, facility.position)
            }) {
                return Err(MapError::Invalid(format!(
                    "{:?} interaction point lies inside its collision area",
                    facility.kind
                )));
            }
        }
        for zone in &self.starter_zones {
            let end_x = zone.origin.x.saturating_add(zone.size.0);
            let end_y = zone.origin.y.saturating_add(zone.size.1);
            if zone.size.0 == 0 || zone.size.1 == 0 || end_x > self.width || end_y > self.height {
                return Err(MapError::Invalid(format!(
                    "field zone {} lies outside the map",
                    zone.name
                )));
            }
        }
        for area in &self.navigation_areas {
            self.validate_area(&area.name, area.origin, area.size)?;
        }
        for area in &self.collision_areas {
            self.validate_area(&area.name, area.origin, area.size)?;
        }
        Ok(())
    }

    fn validate_area(&self, name: &str, origin: TilePos, size: (u32, u32)) -> Result<(), MapError> {
        if size.0 == 0
            || size.1 == 0
            || origin.x.saturating_add(size.0) > self.width
            || origin.y.saturating_add(size.1) > self.height
        {
            return Err(MapError::Invalid(format!(
                "map area {name} lies outside the map"
            )));
        }
        Ok(())
    }

    #[must_use]
    pub const fn contains(&self, position: TilePos) -> bool {
        position.x < self.width && position.y < self.height
    }
}

fn area_contains(origin: TilePos, size: (u32, u32), position: TilePos) -> bool {
    position.x >= origin.x
        && position.y >= origin.y
        && position.x < origin.x.saturating_add(size.0)
        && position.y < origin.y.saturating_add(size.1)
}

impl FarmGrid {
    #[must_use]
    pub fn from_definition(map: &MapDefinition) -> Self {
        Self {
            width: map.width,
            height: map.height,
            tiles: map.terrain_tiles.iter().copied().map(Tile::new).collect(),
        }
    }
}

fn decode_tiled_gid(tiled_gid: u32) -> Result<Option<MapTileCellDef>, MapError> {
    if tiled_gid == 0 {
        return Ok(None);
    }
    if tiled_gid & TILED_FLIP_HEXAGONAL_120 != 0 {
        return Err(MapError::Invalid(
            "hexagonal rotation flags are invalid on an orthogonal map".to_owned(),
        ));
    }
    let clean_gid = tiled_gid & !TILED_FLIP_MASK;
    let (tileset, atlas_index) = if (1..=TERRAIN_TILE_COUNT).contains(&clean_gid) {
        (MapTilesetKind::Terrain, clean_gid - 1)
    } else if (INFRASTRUCTURE_FIRST_GID..INFRASTRUCTURE_FIRST_GID + INFRASTRUCTURE_TILE_COUNT)
        .contains(&clean_gid)
    {
        (
            MapTilesetKind::Infrastructure,
            clean_gid - INFRASTRUCTURE_FIRST_GID,
        )
    } else {
        return Err(MapError::Invalid(format!(
            "tile layer references non-terrain global tile id {clean_gid}"
        )));
    };
    let horizontal = tiled_gid & TILED_FLIP_HORIZONTAL != 0;
    let vertical = tiled_gid & TILED_FLIP_VERTICAL != 0;
    let diagonal = tiled_gid & TILED_FLIP_DIAGONAL != 0;
    let (flip_x, flip_y, rotation_quarters) = match (horizontal, vertical, diagonal) {
        (false, false, false) => (false, false, 0),
        (true, false, false) => (true, false, 0),
        (false, true, false) => (false, true, 0),
        (true, true, false) => (false, false, 2),
        (true, false, true) => (false, false, 1),
        (false, true, true) => (false, false, 3),
        (false, false, true) => (true, false, 3),
        (true, true, true) => (true, false, 1),
    };
    Ok(Some(MapTileCellDef {
        tileset,
        atlas_index: atlas_index as u16,
        flip_x,
        flip_y,
        rotation_quarters,
    }))
}

fn terrain_for_cell(cell: MapTileCellDef) -> Result<TerrainKind, MapError> {
    match (cell.tileset, cell.atlas_index) {
        (MapTilesetKind::Terrain, 0..=2) => Ok(TerrainKind::Grass),
        (MapTilesetKind::Terrain, 3) => Ok(TerrainKind::Rock),
        (MapTilesetKind::Terrain, 4..=7) => Ok(TerrainKind::Soil),
        (MapTilesetKind::Terrain, 8 | 9 | 15) => Ok(TerrainKind::FarmPath),
        (MapTilesetKind::Terrain, 10) => Ok(TerrainKind::Concrete),
        (MapTilesetKind::Terrain, 11) => Ok(TerrainKind::Culvert),
        (MapTilesetKind::Terrain, 12) => Ok(TerrainKind::IrrigationChannel),
        (MapTilesetKind::Terrain, 13) => Ok(TerrainKind::Water),
        (MapTilesetKind::Terrain, 14) => Ok(TerrainKind::PaddyBund),
        (MapTilesetKind::Infrastructure, 0..=2) => Ok(TerrainKind::PaddyBund),
        (MapTilesetKind::Infrastructure, 3) => Ok(TerrainKind::FarmPath),
        (MapTilesetKind::Infrastructure, 4 | 5) => Ok(TerrainKind::IrrigationChannel),
        (MapTilesetKind::Infrastructure, 6) => Ok(TerrainKind::Culvert),
        (MapTilesetKind::Infrastructure, 7) => Ok(TerrainKind::GarageApron),
        _ => Err(MapError::Invalid(format!(
            "atlas cell {:?}:{} has no simulation terrain mapping",
            cell.tileset, cell.atlas_index
        ))),
    }
}

fn parse_facility_kind(input: &str) -> Result<FacilityKind, MapError> {
    match input {
        "RobotGarage" => Ok(FacilityKind::RobotGarage),
        "Warehouse" => Ok(FacilityKind::Warehouse),
        "SeedStorage" => Ok(FacilityKind::SeedStorage),
        "ChargingStation" => Ok(FacilityKind::ChargingStation),
        "WaterPump" => Ok(FacilityKind::WaterPump),
        "IrrigationNode" => Ok(FacilityKind::IrrigationNode),
        "Packer" => Ok(FacilityKind::Packer),
        "ShippingDock" => Ok(FacilityKind::ShippingDock),
        "SolarGenerator" => Ok(FacilityKind::SolarGenerator),
        "Battery" => Ok(FacilityKind::Battery),
        _ => Err(MapError::Invalid(format!("unknown facility kind {input}"))),
    }
}

fn pixel_position(x: f64, y: f64, tile_size: u32) -> Result<TilePos, MapError> {
    if x < 0.0 || y < 0.0 || !x.is_finite() || !y.is_finite() {
        return Err(MapError::Invalid("object position is invalid".to_owned()));
    }
    Ok(TilePos::new(
        (x / f64::from(tile_size)).floor() as u32,
        (y / f64::from(tile_size)).floor() as u32,
    ))
}

fn pixel_size(width: f64, height: f64, tile_size: u32) -> Result<(u32, u32), MapError> {
    if width <= 0.0 || height <= 0.0 || !width.is_finite() || !height.is_finite() {
        return Err(MapError::Invalid("object size is invalid".to_owned()));
    }
    Ok((
        (width / f64::from(tile_size)).round() as u32,
        (height / f64::from(tile_size)).round() as u32,
    ))
}

fn property<'a>(properties: &'a [TiledProperty], name: &str) -> Result<&'a Value, MapError> {
    properties
        .iter()
        .find(|property| property.name == name)
        .map(|property| &property.value)
        .ok_or_else(|| MapError::Invalid(format!("missing property {name}")))
}

fn string_property(properties: &[TiledProperty], name: &str) -> Result<String, MapError> {
    property(properties, name)?
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| MapError::Invalid(format!("property {name} must be a string")))
}

fn optional_string_property(properties: &[TiledProperty], name: &str) -> Option<String> {
    properties
        .iter()
        .find(|property| property.name == name)
        .and_then(|property| property.value.as_str())
        .map(ToOwned::to_owned)
}

fn bool_property_or(
    properties: &[TiledProperty],
    name: &str,
    default: bool,
) -> Result<bool, MapError> {
    let Some(value) = properties
        .iter()
        .find(|property| property.name == name)
        .map(|property| &property.value)
    else {
        return Ok(default);
    };
    value
        .as_bool()
        .ok_or_else(|| MapError::Invalid(format!("property {name} must be a boolean")))
}

fn int_property_or(
    properties: &[TiledProperty],
    name: &str,
    default: i32,
) -> Result<i32, MapError> {
    let Some(value) = properties
        .iter()
        .find(|property| property.name == name)
        .map(|property| &property.value)
    else {
        return Ok(default);
    };
    let number = value
        .as_i64()
        .ok_or_else(|| MapError::Invalid(format!("property {name} must be an integer")))?;
    i32::try_from(number)
        .map_err(|_| MapError::Invalid(format!("property {name} is outside i32 range")))
}

fn u32_property(properties: &[TiledProperty], name: &str) -> Result<u32, MapError> {
    let number = property(properties, name)?
        .as_u64()
        .ok_or_else(|| MapError::Invalid(format!("property {name} must be unsigned")))?;
    u32::try_from(number)
        .map_err(|_| MapError::Invalid(format!("property {name} is outside u32 range")))
}

fn u16_property(properties: &[TiledProperty], name: &str) -> Result<u16, MapError> {
    let number = u32_property(properties, name)?;
    u16::try_from(number)
        .map_err(|_| MapError::Invalid(format!("property {name} is outside u16 range")))
}

fn u8_property(properties: &[TiledProperty], name: &str) -> Result<u8, MapError> {
    let number = u32_property(properties, name)?;
    u8::try_from(number)
        .map_err(|_| MapError::Invalid(format!("property {name} is outside u8 range")))
}

fn u8_property_or(properties: &[TiledProperty], name: &str, default: u8) -> Result<u8, MapError> {
    let Some(value) = properties
        .iter()
        .find(|property| property.name == name)
        .map(|property| &property.value)
    else {
        return Ok(default);
    };
    let number = value
        .as_u64()
        .ok_or_else(|| MapError::Invalid(format!("property {name} must be unsigned")))?;
    u8::try_from(number)
        .map_err(|_| MapError::Invalid(format!("property {name} is outside u8 range")))
}

#[derive(Debug, Deserialize)]
struct TiledMap {
    orientation: String,
    width: u32,
    height: u32,
    tilewidth: u32,
    tileheight: u32,
    #[serde(default)]
    properties: Vec<TiledProperty>,
    layers: Vec<TiledLayer>,
}

#[derive(Debug, Deserialize)]
struct TiledLayer {
    name: String,
    #[serde(rename = "type")]
    layer_type: String,
    #[serde(default)]
    data: Vec<u32>,
    #[serde(default)]
    properties: Vec<TiledProperty>,
    #[serde(default)]
    objects: Vec<TiledObject>,
    #[serde(default)]
    layers: Vec<TiledLayer>,
}

#[derive(Debug, Deserialize)]
struct TiledObject {
    #[serde(default)]
    name: String,
    #[serde(default, rename = "class")]
    class_name: String,
    x: f64,
    y: f64,
    #[serde(default)]
    width: f64,
    #[serde(default)]
    height: f64,
    #[serde(default)]
    properties: Vec<TiledProperty>,
}

#[derive(Debug, Deserialize)]
struct TiledProperty {
    name: String,
    value: Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiled_map_loads_with_required_production_layers() -> Result<(), MapError> {
        let map = MapDefinition::load_embedded()?;
        let names: Vec<_> = map
            .tile_layers
            .iter()
            .map(|layer| layer.name.as_str())
            .collect();
        assert_eq!(names, ["Ground", "Terrain", "Infrastructure", "Decoration"]);
        assert_eq!(map.tile_layers[0].tiles.len(), 64 * 64);
        assert_eq!(map.starter_facilities.len(), 9);
        assert_eq!(map.starter_zones.len(), 2);
        assert_eq!(map.garage_bays.len(), 4);
        assert_eq!(map.starter_robots.len(), 4);
        assert_eq!(map.navigation_areas.len(), 5);
        assert_eq!(map.collision_areas.len(), 15);
        Ok(())
    }

    #[test]
    fn tiled_layers_compile_into_simulation_terrain() -> Result<(), MapError> {
        let map = MapDefinition::load_embedded()?;
        let grid = FarmGrid::from_definition(&map);
        assert_eq!(
            grid.tile(TilePos::new(10, 20)).map(|tile| tile.terrain),
            Some(TerrainKind::Soil)
        );
        assert_eq!(
            grid.tile(TilePos::new(9, 20)).map(|tile| tile.terrain),
            Some(TerrainKind::PaddyBund)
        );
        assert_eq!(
            grid.tile(TilePos::new(6, 15)).map(|tile| tile.terrain),
            Some(TerrainKind::Culvert)
        );
        assert_eq!(
            grid.tile(TilePos::new(58, 30)).map(|tile| tile.terrain),
            Some(TerrainKind::Water)
        );
        Ok(())
    }
}
