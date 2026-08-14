use std::collections::{BTreeMap, BTreeSet};

use autofarm_editor::PreviewKind;
use autofarm_sim::{
    CropInstance, FacilityKind, JobKind, Robot, RobotBody, RobotState, Season, TerrainKind, TilePos,
};
use bevy::prelude::*;

use crate::{
    state::{GameSession, ScreenMode, WorldCamera},
    theme,
};

const TILE_SIZE: f32 = 32.0;

pub struct FarmRenderPlugin;

impl Plugin for FarmRenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VisualCache>()
            .add_systems(Startup, setup_world)
            .add_systems(
                Update,
                (
                    update_tile_visuals,
                    sync_crop_visuals,
                    sync_robot_visuals,
                    sync_work_effects,
                    sync_facility_visuals,
                    sync_zone_visuals,
                    sync_selection_visual,
                    sync_editor_preview,
                ),
            );
    }
}

#[derive(Resource, Default)]
struct VisualCache {
    tile_minute: u64,
    tile_revision: u64,
    crop_minute: u64,
    crop_revision: u64,
}

#[derive(Resource)]
struct PixelArtAssets {
    paddy_water: Handle<Image>,
    rice_stages: [Handle<Image>; 6],
    paddy_rover: Handle<Image>,
    rice_transplanter: Handle<Image>,
    pest_control_drone: Handle<Image>,
    rice_harvester: Handle<Image>,
    paddy_rover_sheet: Handle<Image>,
    rice_spider_sheet: Handle<Image>,
    pest_drone_sheet: Handle<Image>,
    rice_harvester_sheet: Handle<Image>,
}

impl PixelArtAssets {
    fn load(assets: &AssetServer) -> Self {
        Self {
            paddy_water: assets.load("art/pixel/paddy-water.png"),
            rice_stages: [
                assets.load("art/pixel/rice-stage-0.png"),
                assets.load("art/pixel/rice-stage-1.png"),
                assets.load("art/pixel/rice-stage-2.png"),
                assets.load("art/pixel/rice-stage-3.png"),
                assets.load("art/pixel/rice-stage-4.png"),
                assets.load("art/pixel/rice-stage-5.png"),
            ],
            paddy_rover: assets.load("art/pixel/paddy-rover.png"),
            rice_transplanter: assets.load("art/pixel/rice-transplanter.png"),
            pest_control_drone: assets.load("art/pixel/pest-control-drone.png"),
            rice_harvester: assets.load("art/pixel/rice-harvester.png"),
            paddy_rover_sheet: assets.load("art/pixel/animations/paddy-rover-sheet.png"),
            rice_spider_sheet: assets.load("art/pixel/animations/rice-spider-sheet.png"),
            pest_drone_sheet: assets.load("art/pixel/animations/pest-drone-sheet.png"),
            rice_harvester_sheet: assets.load("art/pixel/animations/rice-harvester-sheet.png"),
        }
    }

    fn robot(&self, def_id: &str) -> Option<Handle<Image>> {
        match def_id {
            "paddy_rover" => Some(self.paddy_rover.clone()),
            "rice_transplanter" => Some(self.rice_transplanter.clone()),
            "pest_control_drone" => Some(self.pest_control_drone.clone()),
            "rice_harvester" => Some(self.rice_harvester.clone()),
            _ => None,
        }
    }

    fn robot_animation(&self, def_id: &str) -> Option<Handle<Image>> {
        match def_id {
            "paddy_rover" => Some(self.paddy_rover_sheet.clone()),
            "rice_transplanter" => Some(self.rice_spider_sheet.clone()),
            "pest_control_drone" => Some(self.pest_drone_sheet.clone()),
            "rice_harvester" => Some(self.rice_harvester_sheet.clone()),
            _ => None,
        }
    }
}

#[derive(Component)]
struct TileVisual(TilePos);

#[derive(Component)]
struct CropVisual(TilePos);

#[derive(Component)]
struct RobotVisual {
    id: u64,
}

#[derive(Component)]
struct BatteryBar(u64);

#[derive(Component)]
struct WorkEffectVisual(u64);

#[derive(Component)]
struct FacilityVisual(u64);

#[derive(Component)]
struct ZoneVisual(u64);

#[derive(Component)]
struct SelectionVisual;

#[derive(Component)]
struct EditorPreviewVisual;

fn setup_world(mut commands: Commands, session: Res<GameSession>, assets: Res<AssetServer>) {
    commands.insert_resource(PixelArtAssets::load(&assets));
    let focus = tile_world(TilePos::new(19, 18));
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scale: 0.82,
            ..OrthographicProjection::default_2d()
        }),
        Transform::from_xyz(focus.x, focus.y, 1000.0),
        WorldCamera,
    ));

    for y in 0..session.simulation.grid.height {
        for x in 0..session.simulation.grid.width {
            let position = TilePos::new(x, y);
            let color = session
                .simulation
                .grid
                .tile(position)
                .map_or(theme::BACKGROUND, |tile| {
                    seasonal_terrain_color(tile.terrain, session.simulation.clock.season())
                });
            commands.spawn((
                Sprite::from_color(color, Vec2::splat(TILE_SIZE - 1.5)),
                Transform::from_translation(tile_world(position).extend(0.0)),
                TileVisual(position),
            ));
        }
    }
    commands.spawn((
        Sprite::from_color(theme::GOLD.with_alpha(0.28), Vec2::splat(TILE_SIZE + 3.0)),
        Transform::from_translation(
            session
                .selected_tile
                .map_or(Vec3::ZERO, |position| tile_world(position).extend(2.0)),
        ),
        if session.selected_tile.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        },
        SelectionVisual,
    ));
}

fn update_tile_visuals(
    session: Res<GameSession>,
    assets: Res<PixelArtAssets>,
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
        sprite.custom_size = Some(Vec2::splat(TILE_SIZE - 1.5));
        if tile.water_level > 0 {
            sprite.image = assets.paddy_water.clone();
            let brightness = 0.72 + f32::from(tile.water_level) / 360.0;
            sprite.color = Color::srgb(brightness, brightness, brightness);
        } else {
            sprite.image = Handle::default();
            sprite.color = if tile.tilled {
                Color::srgb(0.24, 0.13, 0.07)
            } else {
                seasonal_terrain_color(tile.terrain, session.simulation.clock.season())
            };
        }
    }
    cache.tile_minute = session.simulation.clock.minute;
    cache.tile_revision = session.simulation.world_revision;
}

fn sync_crop_visuals(
    mut commands: Commands,
    session: Res<GameSession>,
    assets: Res<PixelArtAssets>,
    mut cache: ResMut<VisualCache>,
    mut visuals: Query<(Entity, &CropVisual, &mut Transform, &mut Sprite)>,
) {
    if cache.crop_minute == session.simulation.clock.minute
        && cache.crop_revision == session.simulation.world_revision
    {
        return;
    }

    let mut existing = BTreeSet::new();
    for (entity, visual, mut transform, mut sprite) in &mut visuals {
        let Some(crop) = session
            .simulation
            .grid
            .tile(visual.0)
            .and_then(|tile| tile.crop.as_ref())
        else {
            commands.entity(entity).despawn();
            continue;
        };
        existing.insert(visual.0);
        configure_crop_sprite(&mut sprite, crop, &assets);
        transform.translation = crop_world(visual.0, crop);
    }

    for y in 0..session.simulation.grid.height {
        for x in 0..session.simulation.grid.width {
            let position = TilePos::new(x, y);
            if existing.contains(&position) {
                continue;
            }
            let Some(crop) = session
                .simulation
                .grid
                .tile(position)
                .and_then(|tile| tile.crop.as_ref())
            else {
                continue;
            };
            let mut sprite = Sprite::default();
            configure_crop_sprite(&mut sprite, crop, &assets);
            commands.spawn((
                sprite,
                Transform::from_translation(crop_world(position, crop)),
                CropVisual(position),
            ));
        }
    }
    cache.crop_minute = session.simulation.clock.minute;
    cache.crop_revision = session.simulation.world_revision;
}

fn sync_robot_visuals(
    mut commands: Commands,
    session: Res<GameSession>,
    assets: Res<PixelArtAssets>,
    time: Res<Time>,
    mut visuals: Query<(Entity, &RobotVisual, &mut Transform, &mut Sprite)>,
    mut battery_bars: Query<(&BatteryBar, &mut Sprite, &mut Transform), Without<RobotVisual>>,
) {
    let mut existing = BTreeSet::new();
    for (entity, visual, mut transform, mut sprite) in &mut visuals {
        let Some(robot) = session
            .simulation
            .robots
            .iter()
            .find(|robot| robot.id == visual.id)
        else {
            commands.entity(entity).despawn();
            continue;
        };
        existing.insert(robot.id);
        let target = robot_world(robot);
        let distance = target.truncate() - transform.translation.truncate();
        let max_step = robot_visual_speed(robot) * time.delta_secs();
        if distance.length() <= max_step {
            transform.translation = target;
        } else if let Some(direction) = distance.try_normalize() {
            transform.translation += (direction * max_step).extend(0.0);
            transform.translation.z = target.z;
        }
        let close_to_target = distance.length() < 4.0;
        let job_kind = robot_job_kind(&session, robot);
        let frame = robot_animation_frame(robot, job_kind, close_to_target, time.elapsed_secs());
        configure_robot_sprite(&mut sprite, robot, &assets, frame);
    }
    for robot in &session.simulation.robots {
        if existing.contains(&robot.id) {
            continue;
        }
        let size = robot_size(&robot.def_id, robot.body);
        let mut sprite = Sprite::default();
        configure_robot_sprite(&mut sprite, robot, &assets, 0);
        commands
            .spawn((
                sprite,
                Transform::from_translation(robot_world(robot)),
                RobotVisual { id: robot.id },
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
        let robot_size = robot_size(&robot.def_id, robot.body);
        let width = robot_size.x * ratio;
        sprite.custom_size = Some(Vec2::new(width.max(0.5), 1.5));
        sprite.color = if ratio < 0.2 {
            theme::DANGER
        } else {
            theme::ACCENT
        };
        transform.translation.x = -(robot_size.x - width) * 0.5;
    }
}

fn sync_work_effects(
    mut commands: Commands,
    session: Res<GameSession>,
    mut visuals: Query<(Entity, &WorkEffectVisual, &mut Transform, &mut Sprite)>,
    robots: Query<(&RobotVisual, &Transform), Without<WorkEffectVisual>>,
) {
    let mut wanted = BTreeMap::new();
    for robot in &session.simulation.robots {
        let RobotState::Working(job_id) = &robot.state else {
            continue;
        };
        let Some(kind) = session
            .simulation
            .jobs
            .iter()
            .find(|job| job.id == *job_id)
            .map(|job| job.kind)
        else {
            continue;
        };
        let visual_is_present = robots.iter().any(|(visual, transform)| {
            visual.id == robot.id
                && transform
                    .translation
                    .truncate()
                    .distance(robot_world(robot).truncate())
                    < 6.0
        });
        if !visual_is_present {
            continue;
        }
        wanted.insert(robot.id, work_effect(robot.position, kind));
    }

    for (entity, visual, mut transform, mut sprite) in &mut visuals {
        let Some((position, size, color)) = wanted.remove(&visual.0) else {
            commands.entity(entity).despawn();
            continue;
        };
        transform.translation = position;
        sprite.custom_size = Some(size);
        sprite.color = color;
    }
    for (robot_id, (position, size, color)) in wanted {
        commands.spawn((
            Sprite::from_color(color, size),
            Transform::from_translation(position),
            WorkEffectVisual(robot_id),
        ));
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
            Sprite::from_color(facility_color(facility.kind), Vec2::splat(TILE_SIZE - 5.0)),
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

fn sync_selection_visual(
    session: Res<GameSession>,
    mut selection: Single<(&mut Transform, &mut Visibility), With<SelectionVisual>>,
) {
    let (transform, visibility) = &mut *selection;
    if let Some(position) = session.selected_tile {
        transform.translation = tile_world(position).extend(2.0);
        **visibility = Visibility::Inherited;
    } else {
        **visibility = Visibility::Hidden;
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
        "rice" => Color::srgba(0.12, 0.68, 0.80, 0.11),
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

fn robot_size(def_id: &str, body: RobotBody) -> Vec2 {
    match def_id {
        "paddy_rover" => Vec2::splat(62.0),
        "rice_transplanter" => Vec2::splat(66.0),
        "pest_control_drone" => Vec2::splat(58.0),
        "rice_harvester" => Vec2::splat(68.0),
        _ => match body {
            RobotBody::Wheeled => Vec2::new(27.0, 19.0),
            RobotBody::Flying => Vec2::splat(25.0),
            RobotBody::Quadruped => Vec2::new(28.0, 21.0),
            RobotBody::Biped => Vec2::new(18.0, 30.0),
            RobotBody::Hexapod => Vec2::new(30.0, 23.0),
        },
    }
}

fn configure_crop_sprite(sprite: &mut Sprite, crop: &CropInstance, assets: &PixelArtAssets) {
    if crop.crop_id == "rice" {
        let stage = crop.stage_index.min(assets.rice_stages.len() - 1);
        sprite.image = assets.rice_stages[stage].clone();
        let size = 42.0 + stage as f32 * 3.0;
        sprite.custom_size = Some(Vec2::splat(size));
        let health = 0.55 + f32::from(crop.health) / 220.0;
        sprite.color = Color::srgb(health, health, health);
    } else {
        sprite.image = Handle::default();
        sprite.custom_size = Some(Vec2::splat(21.0 + crop.stage_index as f32 * 2.0));
        sprite.color = theme::crop_color(crop);
    }
}

fn crop_world(position: TilePos, crop: &CropInstance) -> Vec3 {
    let lift = if crop.crop_id == "rice" {
        4.0 + crop.stage_index as f32
    } else {
        0.0
    };
    (tile_world(position) + Vec2::Y * lift).extend(4.0)
}

fn configure_robot_sprite(
    sprite: &mut Sprite,
    robot: &Robot,
    assets: &PixelArtAssets,
    frame: usize,
) {
    sprite.custom_size = Some(robot_size(&robot.def_id, robot.body));
    if let Some(image) = assets.robot_animation(&robot.def_id) {
        sprite.image = image;
        let column = (frame % 4) as f32;
        let row = (frame / 4).min(1) as f32;
        sprite.rect = Some(Rect::new(
            column * 128.0,
            row * 128.0,
            (column + 1.0) * 128.0,
            (row + 1.0) * 128.0,
        ));
        sprite.color = Color::WHITE;
    } else if let Some(image) = assets.robot(&robot.def_id) {
        sprite.image = image;
        sprite.rect = None;
        sprite.color = Color::WHITE;
    } else {
        sprite.image = Handle::default();
        sprite.rect = None;
        sprite.color = robot_color(robot.body);
    }
}

fn robot_job_kind(session: &GameSession, robot: &Robot) -> Option<JobKind> {
    robot.current_job.and_then(|job_id| {
        session
            .simulation
            .jobs
            .iter()
            .find(|job| job.id == job_id)
            .map(|job| job.kind)
    })
}

fn robot_animation_frame(
    robot: &Robot,
    job_kind: Option<JobKind>,
    close_to_target: bool,
    elapsed_seconds: f32,
) -> usize {
    let moving = matches!(
        robot.state,
        RobotState::MovingToJob(_) | RobotState::MovingToCharge | RobotState::MovingToStorage
    ) || !close_to_target;
    let working = matches!(robot.state, RobotState::Working(_)) && close_to_target;
    if working {
        let phase = (elapsed_seconds / 0.72) as usize % 4;
        return match (robot.def_id.as_str(), job_kind) {
            ("pest_control_drone", Some(JobKind::SprayPests)) => 4 + phase % 2,
            ("pest_control_drone", Some(JobKind::LaserPests)) => 6 + phase % 2,
            _ => 4 + phase,
        };
    }
    if moving {
        return (elapsed_seconds / 0.56) as usize % 4;
    }
    0
}

fn robot_visual_speed(robot: &Robot) -> f32 {
    match robot.body {
        RobotBody::Flying => 42.0,
        RobotBody::Quadruped | RobotBody::Hexapod => 24.0,
        RobotBody::Biped => 22.0,
        RobotBody::Wheeled => 27.0,
    }
}

fn robot_world(robot: &Robot) -> Vec3 {
    let lift = if robot.body == RobotBody::Flying {
        14.0
    } else {
        7.0
    };
    (tile_world(robot.position) + Vec2::Y * lift).extend(10.0)
}

fn work_effect(position: TilePos, kind: JobKind) -> (Vec3, Vec2, Color) {
    let center = tile_world(position);
    match kind {
        JobKind::Till => (
            (center - Vec2::Y * 9.0).extend(8.0),
            Vec2::new(44.0, 9.0),
            Color::srgba(0.92, 0.48, 0.16, 0.72),
        ),
        JobKind::FloodPaddy | JobKind::Water => (
            center.extend(8.0),
            Vec2::new(48.0, 6.0),
            Color::srgba(0.18, 0.82, 1.0, 0.78),
        ),
        JobKind::Transplant | JobKind::Plant | JobKind::Seed => (
            (center - Vec2::Y * 10.0).extend(8.0),
            Vec2::new(24.0, 7.0),
            Color::srgba(0.42, 1.0, 0.38, 0.84),
        ),
        JobKind::Weed => (
            (center - Vec2::Y * 10.0).extend(8.0),
            Vec2::new(28.0, 5.0),
            Color::srgba(0.42, 0.92, 0.24, 0.72),
        ),
        JobKind::LoosenSoil => (
            (center - Vec2::Y * 10.0).extend(8.0),
            Vec2::new(36.0, 6.0),
            Color::srgba(0.64, 0.34, 0.14, 0.72),
        ),
        JobKind::SprayPests => (
            (center - Vec2::Y * 17.0).extend(11.0),
            Vec2::new(18.0, 30.0),
            Color::srgba(0.34, 0.82, 1.0, 0.34),
        ),
        JobKind::LaserPests => (
            (center - Vec2::Y * 15.0).extend(11.0),
            Vec2::new(3.0, 42.0),
            Color::srgba(0.18, 0.96, 1.0, 0.86),
        ),
        JobKind::PestControl | JobKind::Pollinate | JobKind::Inspect => (
            (center - Vec2::Y * 15.0).extend(11.0),
            Vec2::new(5.0, 34.0),
            Color::srgba(0.18, 0.96, 1.0, 0.68),
        ),
        JobKind::Harvest | JobKind::PrecisionHarvest | JobKind::Dig => (
            (center - Vec2::Y * 10.0).extend(8.0),
            Vec2::new(50.0, 8.0),
            Color::srgba(1.0, 0.76, 0.16, 0.86),
        ),
        JobKind::Haul | JobKind::Repair | JobKind::Recharge | JobKind::Pack => {
            (center.extend(8.0), Vec2::splat(10.0), theme::ACCENT)
        }
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

fn seasonal_terrain_color(terrain: TerrainKind, season: Season) -> Color {
    match (terrain, season) {
        (TerrainKind::Grass, Season::Summer) => Color::srgb(0.25, 0.44, 0.16),
        (TerrainKind::Grass, Season::Autumn) => Color::srgb(0.36, 0.32, 0.13),
        (TerrainKind::Grass, Season::Winter) => Color::srgb(0.25, 0.30, 0.28),
        (TerrainKind::Soil | TerrainKind::RoughSoil, Season::Winter) => {
            Color::srgb(0.28, 0.24, 0.20)
        }
        _ => theme::terrain_color(terrain),
    }
}
