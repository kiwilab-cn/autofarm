use autofarm_sim::{FarmEvent, JobStatus, RobotState};
use bevy::{
    input_focus::InputFocus,
    prelude::*,
    text::{EditableText, TextCursorStyle},
};

use crate::{
    state::{
        EditorPanel, EditorPanelText, EditorPromptInput, EventLogText, GameHud, GameSession,
        HintText, InspectorText, MenuRoot, ScreenMode, TopBarText, TrialPanel, TrialPanelText,
        UiAction, UiActionQueue,
    },
    theme,
};

pub struct GameUiPlugin;

impl Plugin for GameUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InputFocus>()
            .init_resource::<UiActionQueue>()
            .add_systems(Startup, (spawn_title_screen, spawn_hud))
            .add_systems(
                Update,
                (
                    handle_buttons,
                    focus_editor_input,
                    submit_editor_prompt,
                    update_hud_text,
                    update_panel_visibility,
                ),
            );
    }
}

fn spawn_title_screen(mut commands: Commands, assets: Res<AssetServer>, session: Res<GameSession>) {
    if session.screen != ScreenMode::MainMenu {
        return;
    }
    commands
        .spawn((
            Node {
                width: percent(100),
                height: percent(100),
                ..default()
            },
            BackgroundColor(Color::BLACK),
            GlobalZIndex(100),
            MenuRoot,
        ))
        .with_children(|root| {
            root.spawn((
                ImageNode::new(assets.load("art/autofarm-key-art.png")),
                Node {
                    position_type: PositionType::Absolute,
                    width: percent(100),
                    height: percent(100),
                    ..default()
                },
            ));
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(54),
                    top: px(54),
                    width: px(530),
                    padding: UiRect::all(px(26)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(12),
                    border: UiRect::all(px(2)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.02, 0.06, 0.055, 0.88)),
                BorderColor::all(theme::PANEL_BORDER),
                children![
                    label("AUTOFARM", 58.0, theme::TEXT),
                    label("DESIGN THE FARM THAT RUNS ITSELF", 19.0, theme::ACCENT),
                    label(
                        "Robots cultivate. Managers decide. You design the system.",
                        17.0,
                        theme::MUTED,
                    ),
                    label("[ ENTER ]  NEW GAME", 24.0, theme::GOLD),
                ],
            ));
            root.spawn((
                Text::new("V0.1  •  MOCK AI READY  •  NO API KEY REQUIRED"),
                TextFont {
                    font_size: FontSize::Px(14.0),
                    ..default()
                },
                TextColor(theme::TEXT),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(58),
                    bottom: px(34),
                    ..default()
                },
            ));
        });
}

fn spawn_hud(mut commands: Commands) {
    commands
        .spawn((
            Node {
                width: percent(100),
                height: percent(100),
                ..default()
            },
            Visibility::Hidden,
            GlobalZIndex(50),
            GameHud,
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0),
                    right: px(0),
                    top: px(0),
                    height: px(56),
                    padding: UiRect::axes(px(18), px(8)),
                    align_items: AlignItems::Center,
                    border: UiRect::bottom(px(2)),
                    ..default()
                },
                BackgroundColor(theme::PANEL),
                BorderColor::all(theme::PANEL_BORDER),
                children![label_with::<TopBarText>("", 18.0, theme::TEXT)],
            ));

            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0),
                    top: px(56),
                    bottom: px(100),
                    width: px(220),
                    padding: UiRect::all(px(14)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(8),
                    border: UiRect::right(px(2)),
                    ..default()
                },
                BackgroundColor(theme::PANEL),
                BorderColor::all(theme::PANEL_BORDER),
            ))
            .with_children(|panel| {
                panel.spawn(label("BUILD & CONTROL", 17.0, theme::ACCENT));
                for (text, action) in [
                    ("F  CYCLE CROP", UiAction::CycleCrop),
                    ("R  BUY ROBOT", UiAction::BuyRobot),
                    ("B  BUILD FACILITY", UiAction::BuildFacility),
                    ("N  NPC REVIEW", UiAction::NpcReview),
                    ("T  AUTONOMY TRIAL", UiAction::StartTrial),
                    ("F1 AI EDITOR", UiAction::ToggleEditor),
                    ("S  SAVE", UiAction::Save),
                    ("L  LOAD", UiAction::Load),
                ] {
                    panel
                        .spawn((
                            Button,
                            Node {
                                width: percent(100),
                                height: px(38),
                                padding: UiRect::horizontal(px(10)),
                                align_items: AlignItems::Center,
                                border: UiRect::all(px(1)),
                                border_radius: BorderRadius::all(px(4)),
                                ..default()
                            },
                            BackgroundColor(theme::BUTTON),
                            BorderColor::all(theme::PANEL_BORDER),
                            action,
                        ))
                        .with_children(|button| {
                            button.spawn(label(text, 13.0, theme::TEXT));
                        });
                }
                panel.spawn(label(
                    "Drag on the map to place a rectangular field.",
                    13.0,
                    theme::MUTED,
                ));
            });

            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    right: px(0),
                    top: px(56),
                    bottom: px(100),
                    width: px(310),
                    padding: UiRect::all(px(16)),
                    flex_direction: FlexDirection::Column,
                    border: UiRect::left(px(2)),
                    ..default()
                },
                BackgroundColor(theme::PANEL),
                BorderColor::all(theme::PANEL_BORDER),
                children![
                    label("INSPECTOR", 17.0, theme::ACCENT),
                    label_with::<InspectorText>("", 14.0, theme::TEXT),
                ],
            ));

            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0),
                    right: px(0),
                    bottom: px(0),
                    height: px(100),
                    padding: UiRect::axes(px(18), px(8)),
                    flex_direction: FlexDirection::Column,
                    border: UiRect::top(px(2)),
                    ..default()
                },
                BackgroundColor(theme::PANEL),
                BorderColor::all(theme::PANEL_BORDER),
                children![
                    label_with::<EventLogText>("", 13.0, theme::MUTED),
                    label_with::<HintText>("", 14.0, theme::TEXT),
                ],
            ));

            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(245),
                    right: px(335),
                    bottom: px(118),
                    min_height: px(170),
                    padding: UiRect::all(px(18)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(8),
                    border: UiRect::all(px(2)),
                    ..default()
                },
                BackgroundColor(theme::PANEL_ALT),
                BorderColor::all(theme::ACCENT),
                Visibility::Hidden,
                EditorPanel,
                children![
                    label("DEVELOPER AI — PREVIEW", 18.0, theme::ACCENT),
                    (
                        Node {
                            width: percent(100),
                            min_height: px(38),
                            padding: UiRect::all(px(8)),
                            border: UiRect::all(px(1)),
                            ..default()
                        },
                        EditableText::new("create tomato field"),
                        TextCursorStyle::default(),
                        TextFont {
                            font_size: FontSize::Px(15.0),
                            ..default()
                        },
                        TextColor(theme::TEXT),
                        BackgroundColor(theme::BUTTON),
                        BorderColor::all(theme::PANEL_BORDER),
                        EditorPromptInput,
                    ),
                    label_with::<EditorPanelText>("", 14.0, theme::TEXT),
                    label(
                        "[ENTER] PREVIEW   [CTRL+ENTER] APPLY   [U] UNDO   [F1/ESC] CANCEL",
                        14.0,
                        theme::GOLD
                    ),
                ],
            ));

            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: percent(32),
                    top: percent(20),
                    width: percent(36),
                    min_height: px(370),
                    padding: UiRect::all(px(28)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(14),
                    border: UiRect::all(px(3)),
                    ..default()
                },
                BackgroundColor(theme::PANEL),
                BorderColor::all(theme::GOLD),
                Visibility::Hidden,
                TrialPanel,
                children![
                    label("AUTONOMY REPORT", 28.0, theme::GOLD),
                    label_with::<TrialPanelText>("", 18.0, theme::TEXT),
                    label("[ENTER] RETURN TO FARM", 15.0, theme::ACCENT),
                ],
            ));
        });
}

fn focus_editor_input(
    session: Res<GameSession>,
    input: Query<Entity, With<EditorPromptInput>>,
    mut focus: ResMut<InputFocus>,
) {
    if session.screen != ScreenMode::Editor {
        return;
    }
    let Ok(entity) = input.single() else {
        return;
    };
    if focus.get() != Some(entity) {
        focus.set(entity, bevy::input_focus::FocusCause::Navigated);
    }
}

fn submit_editor_prompt(
    keyboard: Res<ButtonInput<KeyCode>>,
    input: Query<&EditableText, With<EditorPromptInput>>,
    mut session: ResMut<GameSession>,
) {
    let control = keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::ControlRight);
    if session.screen != ScreenMode::Editor || control || !keyboard.just_pressed(KeyCode::Enter) {
        return;
    }
    let Ok(input) = input.single() else {
        return;
    };
    let mut prompt = String::new();
    prompt.reserve(input.value().into_iter().map(str::len).sum());
    for value in input.value() {
        prompt.push_str(value);
    }
    let simulation = session.simulation.clone();
    match session.editor.preview(&prompt, &simulation) {
        Ok(plan) => session.status = format!("Preview ready: {}", plan.summary),
        Err(error) => session.status = format!("Editor could not plan that request: {error}"),
    }
}

#[allow(clippy::type_complexity)]
fn handle_buttons(
    mut focus: ResMut<InputFocus>,
    mut queue: ResMut<UiActionQueue>,
    mut buttons: Query<
        (
            Entity,
            &Interaction,
            &mut BackgroundColor,
            &mut BorderColor,
            &UiAction,
        ),
        (Changed<Interaction>, With<Button>),
    >,
) {
    for (entity, interaction, mut background, mut border, action) in &mut buttons {
        match *interaction {
            Interaction::Pressed => {
                focus.set(entity, bevy::input_focus::FocusCause::Pressed);
                background.0 = theme::BUTTON_PRESS;
                *border = BorderColor::all(theme::GOLD);
                queue.0.push(*action);
            }
            Interaction::Hovered => {
                background.0 = theme::BUTTON_HOVER;
                *border = BorderColor::all(theme::ACCENT);
            }
            Interaction::None => {
                background.0 = theme::BUTTON;
                *border = BorderColor::all(theme::PANEL_BORDER);
            }
        }
    }
}

#[allow(clippy::type_complexity)]
fn update_hud_text(
    session: Res<GameSession>,
    mut text_queries: ParamSet<(
        Query<&mut Text, With<TopBarText>>,
        Query<&mut Text, With<InspectorText>>,
        Query<&mut Text, With<EventLogText>>,
        Query<&mut Text, With<HintText>>,
        Query<&mut Text, With<EditorPanelText>>,
        Query<&mut Text, With<TrialPanelText>>,
    )>,
) {
    let snapshot = session.simulation.snapshot();
    for mut text in &mut text_queries.p0() {
        **text = format!(
            "◈ ${:>6}    DAY {}  {:02}:{:02}    {:?}    POWER {:.0}/{:.0} +{:.0}    WATER {:.0}    AUTONOMY {}    SPEED {}x",
            snapshot.credits,
            snapshot.time.day(),
            snapshot.time.hour(),
            snapshot.time.minute_of_hour(),
            snapshot.weather,
            snapshot.power.stored,
            snapshot.power.capacity,
            snapshot.power.production,
            snapshot.water.available_water,
            session
                .simulation
                .last_autonomy_report
                .as_ref()
                .map_or("--".to_owned(), |report| report.score.to_string()),
            if snapshot.time.paused {
                0
            } else {
                snapshot.time.speed
            },
        );
    }
    let inspector = inspector_text(&session);
    for mut text in &mut text_queries.p1() {
        **text = inspector.clone();
    }
    let events = session
        .simulation
        .events
        .iter()
        .rev()
        .take(3)
        .rev()
        .map(event_text)
        .collect::<Vec<_>>()
        .join("    •    ");
    for mut text in &mut text_queries.p2() {
        **text = format!("EVENTS  {events}");
    }
    for mut text in &mut text_queries.p3() {
        **text = format!(
            "SELECTED: {} FIELD    |    {}",
            session.crop_id().to_uppercase(),
            session.status
        );
    }
    let editor = session.editor.pending().map_or_else(
        || "No valid preview. Supported intents: create tomato field / create wheat field / add drone".to_owned(),
        |plan| {
            format!(
                "> create tomato field\n\nAI: {}\nReason: {}\n\n{} typed command(s), world revision {}",
                plan.summary,
                plan.rationale,
                plan.commands.len(),
                plan.expected_world_revision,
            )
        },
    );
    for mut text in &mut text_queries.p4() {
        **text = editor.clone();
    }
    let report = session.simulation.last_autonomy_report.as_ref().map_or_else(
        || "Trial still running...".to_owned(),
        |report| {
            format!(
                "DELIVERY                 {:>5.1}%\nAUTOMATION UPTIME         {:>5.1}%\nMANUAL INTERVENTIONS      {:>5}\nCROP WASTE                {:>5.1}%\nENERGY EFFICIENCY         {:>5.1}%\nROBOT RECOVERY            {:>5.1}%\n\nAUTONOMY SCORE            {:>3} / 100\nGRADE                      {}",
                report.delivery_percent,
                report.automation_uptime_percent,
                report.manual_interventions,
                report.crop_waste_percent,
                report.energy_efficiency_percent,
                report.robot_recovery_percent,
                report.score,
                report.grade,
            )
        },
    );
    for mut text in &mut text_queries.p5() {
        **text = report.clone();
    }
}

#[allow(clippy::type_complexity)]
fn update_panel_visibility(
    session: Res<GameSession>,
    mut hud: Query<&mut Visibility, (With<GameHud>, Without<EditorPanel>, Without<TrialPanel>)>,
    mut editor: Query<&mut Visibility, (With<EditorPanel>, Without<TrialPanel>)>,
    mut trial: Query<&mut Visibility, (With<TrialPanel>, Without<EditorPanel>)>,
) {
    for mut visibility in &mut hud {
        *visibility = if session.screen == ScreenMode::MainMenu {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    }
    for mut visibility in &mut editor {
        *visibility = if session.screen == ScreenMode::Editor {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    for mut visibility in &mut trial {
        *visibility = if session.screen == ScreenMode::TrialReport {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

fn inspector_text(session: &GameSession) -> String {
    if let Some(position) = session.selected_tile
        && let Some(tile) = session.simulation.grid.tile(position)
    {
        let crop = tile.crop.as_ref().map_or_else(
            || "None".to_owned(),
            |crop| {
                let stage = session
                    .simulation
                    .catalog
                    .crops
                    .get(&crop.crop_id)
                    .and_then(|definition| definition.stages.get(crop.stage_index))
                    .map_or("Unknown", |stage| stage.name.as_str());
                format!(
                    "{} / {}\nMoisture {}%  Health {}%\nPollinated {}  Inspection {}",
                    crop.crop_id,
                    stage,
                    crop.moisture,
                    crop.health,
                    yes_no(crop.pollinated),
                    if crop.inspection_due { "DUE" } else { "OK" },
                )
            },
        );
        let jobs = session
            .simulation
            .jobs
            .iter()
            .filter(|job| job.location == position && job.status != JobStatus::Completed)
            .map(|job| format!("{:?}", job.kind))
            .collect::<Vec<_>>()
            .join(", ");
        return format!(
            "\nTILE {}, {}\nTerrain: {:?}\nFertility: {}%\nTilled: {}\n\nCROP\n{}\n\nJOBS\n{}",
            position.x,
            position.y,
            tile.terrain,
            tile.fertility,
            yes_no(tile.tilled),
            crop,
            if jobs.is_empty() { "None" } else { &jobs },
        );
    }

    let contract = session
        .simulation
        .current_contract
        .and_then(|index| session.simulation.contracts.get(index))
        .map_or_else(
            || "No active contract".to_owned(),
            |contract| {
                let progress = contract
                    .definition
                    .requirements
                    .iter()
                    .map(|requirement| {
                        format!(
                            "{} {}/{}",
                            requirement.item,
                            contract
                                .delivered
                                .get(&requirement.item)
                                .copied()
                                .unwrap_or_default(),
                            requirement.amount
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                format!(
                    "{} [{:?}]\n{}\nReward ${}",
                    contract.definition.display_name,
                    contract.status,
                    progress,
                    contract.definition.reward,
                )
            },
        );
    let robot_summary = session
        .simulation
        .robots
        .iter()
        .map(|robot| {
            format!(
                "#{:02} {:?}  {:>3.0}%  {:?}",
                robot.id,
                robot.body,
                robot.battery / robot.battery_capacity * 100.0,
                short_robot_state(&robot.state),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "\nCONTRACT\n{}\n\nFLEET  {} units\n{}\n\nINVENTORY\n{:?}\n\nFIELDS {}  JOBS {}",
        contract,
        session.simulation.robots.len(),
        robot_summary,
        session.simulation.inventory.items,
        session.simulation.zones.len(),
        session.simulation.jobs.len(),
    )
}

fn event_text(event: &FarmEvent) -> String {
    match event {
        FarmEvent::CropReady(position) => format!("Crop ready @ {},{}", position.x, position.y),
        FarmEvent::CropCritical(position) => {
            format!("Crop critical @ {},{}", position.x, position.y)
        }
        FarmEvent::RobotLowBattery(id) => format!("Robot {id} returning to charge"),
        FarmEvent::RobotBroken(id) => format!("Robot {id} broken"),
        FarmEvent::StorageFull => "Storage full".to_owned(),
        FarmEvent::WaterLow => "Water reserve low".to_owned(),
        FarmEvent::PowerLow => "Power reserve low".to_owned(),
        FarmEvent::ContractAccepted(id) => format!("Contract accepted: {id}"),
        FarmEvent::ContractNearDeadline(id) => format!("Deadline near: {id}"),
        FarmEvent::ContractCompleted(id) => format!("Contract complete: {id}"),
        FarmEvent::ContractFailed(id) => format!("Contract failed: {id}"),
        FarmEvent::AutonomyTrialStarted => "Autonomy Trial started".to_owned(),
        FarmEvent::AutonomyTrialCompleted(score) => format!("Autonomy score: {score}"),
        FarmEvent::AiAction { actor, message, .. } => format!("[{actor}] {message}"),
        FarmEvent::Info(message) => message.clone(),
    }
}

fn short_robot_state(state: &RobotState) -> &'static str {
    match state {
        RobotState::Idle => "IDLE",
        RobotState::MovingToJob(_) => "MOVE",
        RobotState::Working(_) => "WORK",
        RobotState::MovingToCharge => "CHARGE→",
        RobotState::Charging => "CHARGING",
        RobotState::MovingToStorage => "HAUL",
        RobotState::Broken => "BROKEN",
    }
}

fn yes_no(value: bool) -> &'static str {
    if value { "YES" } else { "NO" }
}

fn label(text: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(text),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
    )
}

fn label_with<T: DefaultComponent>(
    text: impl Into<String>,
    size: f32,
    color: Color,
) -> impl Bundle {
    (
        Text::new(text),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
        T::default_component(),
    )
}

trait DefaultComponent: Component {
    fn default_component() -> Self;
}

macro_rules! impl_default_component {
    ($($component:ty),+ $(,)?) => {
        $(
            impl DefaultComponent for $component {
                fn default_component() -> Self {
                    Self
                }
            }
        )+
    };
}

impl_default_component!(
    TopBarText,
    InspectorText,
    EventLogText,
    HintText,
    EditorPanelText,
    TrialPanelText,
);
