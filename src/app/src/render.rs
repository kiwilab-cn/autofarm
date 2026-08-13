use std::collections::{BTreeMap, BTreeSet};

use autofarm_editor::PreviewKind;
use autofarm_sim::{FacilityKind, RobotBody, TilePos};
use bevy::prelude::*;

use crate::{
    state::{GameSession, ScreenMode, WorldCamera},
    theme,
};

const TILE_SIZE: f32 = 12.0;

pub struct FarmRenderPlugin;

impl Plugin for FarmRenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VisualCache>()
            .add_systems(Startup, setup_world)
            .add_systems(
                Update,
                (
                    update_tile_visuals,
                    sync_robot_visuals,
                    sync_facility_visuals,
                    sync_zone_visuals,
                    sync_editor_preview,
                ),
            );
    }
}

#[derive(Resource, Default)]
struct VisualCache {
    tile_minute: u64,
    tile_revision: u64,
}

#[derive(Component)]
struct TileVisual(TilePos);

#[derive(Component)]
struct RobotVisual(u64);

#[derive(Component)]
struct BatteryBar(u64);

#[derive(Component)]
struct FacilityVisual(u64);

#[derive(Component)]
struct ZoneVisual(u64);

#[derive(Component)]
struct EditorPreviewVisual;

fn setup_world(mut commands: Commands, session: Res<GameSession>) {
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scale: 1.2,
            ..OrthographicProjection::default_2d()
        }),
        Transform::from_xyz(0.0, 0.0, 1000.0),
        WorldCamera,
    ));

    for y in 0..session.simulation.grid.height {
        for x in 0..session.simulation.grid.width {
            let position = TilePos::new(x, y);
            let color = session
                .simulation
                .grid
                .tile(position)
                .map_or(theme::BACKGROUND, |tile| theme::terrain_color(tile.terrain));
            commands.spawn((
                Sprite::from_color(color, Vec2::splat(TILE_SIZE - 0.8)),
                Transform::from_translation(tile_world(position).extend(0.0)),
                TileVisual(position),
            ));
        }
    }
}

fn update_tile_visuals(
    session: Res<GameSession>,
    mut cache: ResMut<VisualCache>,
    mut tiles: Query<(&TileVisual, &mut Sprite)>,
) {
    if cache.tile_minute == session.simulation.clock.minute
        && cache.tile_revision == session.simulation.world_revision
    {
        return;
    }
    for (visual, mut sprite) in &mut tiles {
        let Some(tile) = session.simulation.grid.tile(visual.0) else {
            continue;
        };
        sprite.color = tile
            .crop
            .as_ref()
            .map_or_else(|| theme::terrain_color(tile.terrain), theme::crop_color);
    }
    cache.tile_minute = session.simulation.clock.minute;
    cache.tile_revision = session.simulation.world_revision;
}

fn sync_robot_visuals(
    mut commands: Commands,
    session: Res<GameSession>,
    mut visuals: Query<(Entity, &RobotVisual, &mut Transform, &mut Sprite)>,
    mut battery_bars: Query<(&BatteryBar, &mut Sprite, &mut Transform), Without<RobotVisual>>,
) {
    let mut existing = BTreeSet::new();
    for (entity, visual, mut transform, mut sprite) in &mut visuals {
        let Some(robot) = session
            .simulation
            .robots
            .iter()
            .find(|robot| robot.id == visual.0)
        else {
            commands.entity(entity).despawn();
            continue;
        };
        existing.insert(robot.id);
        transform.translation = tile_world(robot.position).extend(10.0);
        sprite.color = robot_color(robot.body);
    }
    for robot in &session.simulation.robots {
        if existing.contains(&robot.id) {
            continue;
        }
        let size = robot_size(robot.body);
        commands
            .spawn((
                Sprite::from_color(robot_color(robot.body), size),
                Transform::from_translation(tile_world(robot.position).extend(10.0)),
                RobotVisual(robot.id),
            ))
            .with_children(|parent| {
                parent.spawn((
                    Sprite::from_color(theme::ACCENT, Vec2::new(size.x, 1.5)),
                    Transform::from_xyz(0.0, size.y * 0.75, 0.2),
                    BatteryBar(robot.id),
                ));
            });
    }
    for (bar, mut sprite, mut transform) in &mut battery_bars {
        let Some(robot) = session
            .simulation
            .robots
            .iter()
            .find(|robot| robot.id == bar.0)
        else {
            continue;
        };
        let ratio = (robot.battery / robot.battery_capacity).clamp(0.0, 1.0);
        let width = robot_size(robot.body).x * ratio;
        sprite.custom_size = Some(Vec2::new(width.max(0.5), 1.5));
        sprite.color = if ratio < 0.2 {
            theme::DANGER
        } else {
            theme::ACCENT
        };
        transform.translation.x = -(robot_size(robot.body).x - width) * 0.5;
    }
}

fn sync_facility_visuals(
    mut commands: Commands,
    session: Res<GameSession>,
    mut visuals: Query<(Entity, &FacilityVisual, &mut Transform, &mut Sprite)>,
) {
    let mut existing = BTreeSet::new();
    for (entity, visual, mut transform, mut sprite) in &mut visuals {
        let Some(facility) = session
            .simulation
            .facilities
            .iter()
            .find(|facility| facility.id == visual.0)
        else {
            commands.entity(entity).despawn();
            continue;
        };
        existing.insert(facility.id);
        transform.translation = tile_world(facility.position).extend(5.0);
        sprite.color = facility_color(facility.kind);
    }
    for facility in &session.simulation.facilities {
        if existing.contains(&facility.id) {
            continue;
        }
        commands.spawn((
            Sprite::from_color(facility_color(facility.kind), Vec2::splat(TILE_SIZE - 1.5)),
            Transform::from_translation(tile_world(facility.position).extend(5.0)),
            FacilityVisual(facility.id),
        ));
    }
}

fn sync_zone_visuals(
    mut commands: Commands,
    session: Res<GameSession>,
    mut visuals: Query<(Entity, &ZoneVisual, &mut Transform, &mut Sprite)>,
) {
    let zones: BTreeMap<_, _> = session
        .simulation
        .zones
        .iter()
        .map(|zone| (zone.id, zone))
        .collect();
    let mut existing = BTreeSet::new();
    for (entity, visual, mut transform, mut sprite) in &mut visuals {
        let Some(zone) = zones.get(&visual.0) else {
            commands.entity(entity).despawn();
            continue;
        };
        existing.insert(zone.id);
        let center = zone_center(zone.origin, zone.size);
        transform.translation = center.extend(1.0);
        sprite.custom_size = Some(Vec2::new(
            zone.size.0 as f32 * TILE_SIZE,
            zone.size.1 as f32 * TILE_SIZE,
        ));
        sprite.color = zone_color(&zone.crop_id);
    }
    for zone in &session.simulation.zones {
        if existing.contains(&zone.id) {
            continue;
        }
        commands.spawn((
            Sprite::from_color(
                zone_color(&zone.crop_id),
                Vec2::new(
                    zone.size.0 as f32 * TILE_SIZE,
                    zone.size.1 as f32 * TILE_SIZE,
                ),
            ),
            Transform::from_translation(zone_center(zone.origin, zone.size).extend(1.0)),
            ZoneVisual(zone.id),
        ));
    }
}

fn sync_editor_preview(
    mut commands: Commands,
    session: Res<GameSession>,
    previews: Query<Entity, With<EditorPreviewVisual>>,
) {
    let should_show = session.screen == ScreenMode::Editor && session.editor.pending().is_some();
    if !should_show {
        for entity in &previews {
            commands.entity(entity).despawn();
        }
        return;
    }
    if !previews.is_empty() {
        return;
    }
    let Some(plan) = session.editor.pending() else {
        return;
    };
    for marker in &plan.preview {
        let color = match marker.kind {
            PreviewKind::Field => Color::srgba(0.25, 1.0, 0.40, 0.32),
            PreviewKind::Facility => Color::srgba(0.25, 0.65, 1.0, 0.65),
            PreviewKind::Robot => Color::srgba(1.0, 0.80, 0.22, 0.75),
            PreviewKind::Environment => Color::srgba(0.7, 0.5, 1.0, 0.5),
        };
        commands.spawn((
            Sprite::from_color(
                color,
                Vec2::new(
                    marker.size.0 as f32 * TILE_SIZE,
                    marker.size.1 as f32 * TILE_SIZE,
                ),
            ),
            Transform::from_translation(zone_center(marker.position, marker.size).extend(20.0)),
            EditorPreviewVisual,
        ));
    }
}

#[must_use]
pub fn tile_world(position: TilePos) -> Vec2 {
    Vec2::new(
        (position.x as f32 - 31.5) * TILE_SIZE,
        (31.5 - position.y as f32) * TILE_SIZE,
    )
}

#[must_use]
pub fn world_tile(world: Vec2) -> Option<TilePos> {
    let x = (world.x / TILE_SIZE + 31.5).round() as i32;
    let y = (31.5 - world.y / TILE_SIZE).round() as i32;
    (x >= 0 && y >= 0 && x < 64 && y < 64).then_some(TilePos::new(x as u32, y as u32))
}

fn zone_center(origin: TilePos, size: (u32, u32)) -> Vec2 {
    let first = tile_world(origin);
    first
        + Vec2::new(
            (size.0.saturating_sub(1)) as f32 * TILE_SIZE * 0.5,
            -(size.1.saturating_sub(1) as f32) * TILE_SIZE * 0.5,
        )
}

fn zone_color(crop_id: &str) -> Color {
    match crop_id {
        "wheat" => Color::srgba(0.95, 0.72, 0.18, 0.09),
        "potato" => Color::srgba(0.54, 0.76, 0.24, 0.09),
        "tomato" => Color::srgba(0.95, 0.22, 0.12, 0.10),
        "strawberry" => Color::srgba(1.0, 0.30, 0.50, 0.10),
        _ => Color::srgba(0.7, 0.8, 0.5, 0.08),
    }
}

fn robot_color(body: RobotBody) -> Color {
    match body {
        RobotBody::Wheeled => theme::GOLD,
        RobotBody::Flying => Color::srgb(0.25, 0.85, 0.98),
        RobotBody::Quadruped => Color::srgb(0.75, 0.42, 0.92),
        RobotBody::Biped => Color::srgb(0.90, 0.88, 0.68),
        RobotBody::Hexapod => Color::srgb(0.95, 0.48, 0.24),
    }
}

fn robot_size(body: RobotBody) -> Vec2 {
    match body {
        RobotBody::Wheeled => Vec2::new(9.5, 6.0),
        RobotBody::Flying => Vec2::new(8.0, 8.0),
        RobotBody::Quadruped => Vec2::new(9.0, 6.5),
        RobotBody::Biped => Vec2::new(5.5, 9.0),
        RobotBody::Hexapod => Vec2::new(10.0, 7.0),
    }
}

fn facility_color(kind: FacilityKind) -> Color {
    match kind {
        FacilityKind::Warehouse => Color::srgb(0.60, 0.48, 0.32),
        FacilityKind::SeedStorage => Color::srgb(0.72, 0.56, 0.20),
        FacilityKind::ChargingStation => Color::srgb(0.20, 0.78, 0.74),
        FacilityKind::WaterPump => Color::srgb(0.16, 0.55, 0.84),
        FacilityKind::IrrigationNode => Color::srgb(0.20, 0.68, 0.95),
        FacilityKind::Packer => Color::srgb(0.78, 0.42, 0.20),
        FacilityKind::ShippingDock => Color::srgb(0.42, 0.63, 0.72),
        FacilityKind::SolarGenerator => Color::srgb(0.14, 0.25, 0.58),
        FacilityKind::Battery => Color::srgb(0.50, 0.86, 0.32),
    }
}
