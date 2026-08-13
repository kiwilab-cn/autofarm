use std::{cmp::Reverse, collections::BinaryHeap};

use crate::{FarmGrid, RobotBody, TerrainKind, TilePos};

#[must_use]
pub(crate) fn path_exists(
    grid: &FarmGrid,
    start: TilePos,
    goal: TilePos,
    body: RobotBody,
    allow_planted_fields: bool,
) -> bool {
    next_ground_step(grid, start, goal, body, allow_planted_fields).is_some()
}

#[must_use]
pub(crate) fn next_ground_step(
    grid: &FarmGrid,
    start: TilePos,
    goal: TilePos,
    body: RobotBody,
    allow_planted_fields: bool,
) -> Option<TilePos> {
    let start_index = grid.index(start)?;
    let goal_index = grid.index(goal)?;
    if start == goal {
        return Some(start);
    }
    if !is_passable(grid, goal, body, allow_planted_fields) {
        return None;
    }

    let mut frontier = BinaryHeap::new();
    let mut cost = vec![u32::MAX; grid.tiles.len()];
    let mut came_from = vec![None; grid.tiles.len()];
    cost[start_index] = 0;
    frontier.push((Reverse(start.manhattan(goal)), Reverse(0_u32), start_index));

    while let Some((_, Reverse(current_cost), current_index)) = frontier.pop() {
        if current_index == goal_index {
            break;
        }
        if current_cost > cost[current_index] {
            continue;
        }
        let current = index_position(grid, current_index);
        for neighbor in neighbors(grid, current) {
            if !is_passable(grid, neighbor, body, allow_planted_fields) {
                continue;
            }
            let step_cost = match (grid.tile(neighbor).map(|tile| tile.terrain), body) {
                (Some(TerrainKind::RoughSoil), RobotBody::Wheeled) => 6,
                (Some(TerrainKind::RoughSoil), _) => 3,
                _ => 1,
            };
            let next_cost = current_cost.saturating_add(step_cost);
            let Some(neighbor_index) = grid.index(neighbor) else {
                continue;
            };
            if next_cost >= cost[neighbor_index] {
                continue;
            }
            cost[neighbor_index] = next_cost;
            came_from[neighbor_index] = Some(current_index);
            let priority = next_cost.saturating_add(neighbor.manhattan(goal));
            frontier.push((Reverse(priority), Reverse(next_cost), neighbor_index));
        }
    }

    came_from[goal_index]?;
    let mut cursor = goal_index;
    while let Some(parent) = came_from[cursor] {
        if parent == start_index {
            return Some(index_position(grid, cursor));
        }
        cursor = parent;
    }
    None
}

fn neighbors(grid: &FarmGrid, position: TilePos) -> impl Iterator<Item = TilePos> + use<> {
    let left = position
        .x
        .checked_sub(1)
        .map(|x| TilePos::new(x, position.y));
    let up = position
        .y
        .checked_sub(1)
        .map(|y| TilePos::new(position.x, y));
    let right = (position.x + 1 < grid.width).then_some(TilePos::new(position.x + 1, position.y));
    let down = (position.y + 1 < grid.height).then_some(TilePos::new(position.x, position.y + 1));
    [left, up, right, down].into_iter().flatten()
}

fn is_passable(
    grid: &FarmGrid,
    position: TilePos,
    body: RobotBody,
    allow_planted_fields: bool,
) -> bool {
    grid.tile(position).is_some_and(|tile| {
        !matches!(tile.terrain, TerrainKind::Water | TerrainKind::Rock)
            && (body != RobotBody::Wheeled || tile.crop.is_none() || allow_planted_fields)
    })
}

fn index_position(grid: &FarmGrid, index: usize) -> TilePos {
    TilePos::new(index as u32 % grid.width, index as u32 / grid.width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CropInstance, Tile};

    fn open_grid() -> FarmGrid {
        FarmGrid {
            width: 5,
            height: 5,
            tiles: vec![Tile::new(TerrainKind::Soil); 25],
        }
    }

    #[test]
    fn a_star_routes_around_water() {
        let mut grid = open_grid();
        if let Some(tile) = grid.tile_mut(TilePos::new(2, 2)) {
            tile.terrain = TerrainKind::Water;
        }
        let step = next_ground_step(
            &grid,
            TilePos::new(1, 2),
            TilePos::new(3, 2),
            RobotBody::Quadruped,
            true,
        );
        assert!(step.is_some_and(|step| step != TilePos::new(2, 2)));
    }

    #[test]
    fn wheeled_robot_routes_around_rough_soil() {
        let mut grid = open_grid();
        if let Some(tile) = grid.tile_mut(TilePos::new(2, 2)) {
            tile.terrain = TerrainKind::RoughSoil;
        }
        let step = next_ground_step(
            &grid,
            TilePos::new(1, 2),
            TilePos::new(3, 2),
            RobotBody::Wheeled,
            false,
        );
        assert!(step.is_some_and(|step| step != TilePos::new(2, 2)));
    }

    #[test]
    fn water_target_is_unreachable() {
        let mut grid = open_grid();
        if let Some(tile) = grid.tile_mut(TilePos::new(3, 2)) {
            tile.terrain = TerrainKind::Water;
        }
        assert!(!path_exists(
            &grid,
            TilePos::new(1, 2),
            TilePos::new(3, 2),
            RobotBody::Biped,
            true,
        ));
    }

    #[test]
    fn wheeled_prep_robot_routes_around_planted_field() {
        let mut grid = open_grid();
        if let Some(tile) = grid.tile_mut(TilePos::new(2, 2)) {
            tile.crop = Some(CropInstance {
                crop_id: "rice".to_owned(),
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
                remaining_harvests: 1,
            });
        }
        let step = next_ground_step(
            &grid,
            TilePos::new(1, 2),
            TilePos::new(3, 2),
            RobotBody::Wheeled,
            false,
        );
        assert!(step.is_some_and(|step| step != TilePos::new(2, 2)));
    }
}
