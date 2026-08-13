use autofarm_editor::EditorController;
use autofarm_sim::{GameSimulation, TilePos};
use bevy::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScreenMode {
    #[default]
    MainMenu,
    Playing,
    Editor,
    TrialReport,
}

#[derive(Resource)]
pub struct GameSession {
    pub simulation: GameSimulation,
    pub editor: EditorController,
    pub screen: ScreenMode,
    pub selected_crop: usize,
    pub selected_tile: Option<TilePos>,
    pub drag_start: Option<TilePos>,
    pub robot_purchase_cursor: usize,
    pub facility_build_cursor: usize,
    pub npc_cursor: usize,
    pub status: String,
    pub last_npc_review: u64,
    pub report_seen: bool,
}

impl GameSession {
    #[must_use]
    pub fn new(simulation: GameSimulation) -> Self {
        Self {
            simulation,
            editor: EditorController::default(),
            screen: ScreenMode::MainMenu,
            selected_crop: 0,
            selected_tile: None,
            drag_start: None,
            robot_purchase_cursor: 0,
            facility_build_cursor: 0,
            npc_cursor: 0,
            status: "Press Enter to begin.".to_owned(),
            last_npc_review: 0,
            report_seen: false,
        }
    }

    #[must_use]
    pub fn crop_id(&self) -> &'static str {
        ["wheat", "potato", "tomato", "strawberry"][self.selected_crop % 4]
    }
}

#[derive(Component)]
pub struct WorldCamera;

#[derive(Component)]
pub struct MenuRoot;

#[derive(Component)]
pub struct GameHud;

#[derive(Component)]
pub struct TopBarText;

#[derive(Component)]
pub struct InspectorText;

#[derive(Component)]
pub struct EventLogText;

#[derive(Component)]
pub struct HintText;

#[derive(Component)]
pub struct EditorPanel;

#[derive(Component)]
pub struct EditorPanelText;

#[derive(Component)]
pub struct EditorPromptInput;

#[derive(Component)]
pub struct TrialPanel;

#[derive(Component)]
pub struct TrialPanelText;

#[derive(Resource, Default)]
pub struct UiActionQueue(pub Vec<UiAction>);

#[derive(Component, Debug, Clone, Copy)]
pub enum UiAction {
    CycleCrop,
    BuyRobot,
    BuildFacility,
    NpcReview,
    StartTrial,
    ToggleEditor,
    Save,
    Load,
}
