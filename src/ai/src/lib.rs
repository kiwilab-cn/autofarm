use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    sync::{Arc, mpsc},
    thread,
    time::Duration,
};

use autofarm_sim::{
    CommandActor, CommandError, FarmCommand, FarmEvent, FarmSnapshot, GameCommand, GameSimulation,
    ZoneId,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmRequestParams {
    pub model: String,
    pub temperature: f32,
    pub max_output_tokens: u32,
    pub timeout_ms: u64,
    pub response_schema: String,
}

impl Default for LlmRequestParams {
    fn default() -> Self {
        Self {
            model: "mock-autofarm-v1".to_owned(),
            temperature: 0.4,
            max_output_tokens: 1_200,
            timeout_ms: 8_000,
            response_schema: "NpcDecisionV1".to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NpcPersonality {
    pub efficiency: f32,
    pub sustainability: f32,
    pub risk_tolerance: f32,
    pub experimentation: f32,
    pub micromanagement: f32,
    pub sociability: f32,
    pub crop_affinity: BTreeMap<String, f32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NpcMandate {
    pub managed_zones: BTreeSet<ZoneId>,
    pub allowed_crops: BTreeSet<String>,
    pub max_budget: i32,
    pub allowed_commands: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NpcProfile {
    pub id: String,
    pub name: String,
    pub role: String,
    pub personality: NpcPersonality,
    pub mandate: NpcMandate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ZoneSnapshot {
    pub id: ZoneId,
    pub crop_id: String,
    pub priority: u8,
    pub critical_tiles: usize,
    pub pending_jobs: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NpcTurnRequest {
    pub schema_version: String,
    pub npc: NpcProfile,
    pub mandate: NpcMandate,
    pub farm: FarmSnapshot,
    pub managed_zones: Vec<ZoneSnapshot>,
    pub recent_events: Vec<FarmEvent>,
    pub allowed_actions: Vec<String>,
    pub params: LlmRequestParams,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NpcDecision {
    pub message: String,
    pub rationale: String,
    pub actions: Vec<FarmCommand>,
    pub confidence: f32,
}

#[derive(Debug, Error)]
pub enum LlmError {
    #[error("remote LLM is not configured")]
    NotConfigured,
    #[error("LLM request failed: {0}")]
    Request(String),
    #[error("LLM returned invalid structured output: {0}")]
    InvalidResponse(String),
    #[error("NPC profile not found: {0}")]
    UnknownNpc(String),
    #[error("AI command rejected: {0}")]
    Command(#[from] CommandError),
}

pub trait LlmProvider: Send + Sync {
    fn request(&self, request: &NpcTurnRequest) -> Result<NpcDecision, LlmError>;
}

pub fn dispatch_request(
    provider: Arc<dyn LlmProvider>,
    request: NpcTurnRequest,
) -> mpsc::Receiver<Result<NpcDecision, LlmError>> {
    let (sender, receiver) = mpsc::channel();
    let _worker = thread::spawn(move || {
        let result = provider.request(&request);
        let _ignored = sender.send(result);
    });
    receiver
}

#[derive(Debug, Default, Clone, Copy)]
pub struct MockLlmProvider;

impl LlmProvider for MockLlmProvider {
    fn request(&self, request: &NpcTurnRequest) -> Result<NpcDecision, LlmError> {
        Ok(rule_based_decision(request))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteLlmConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

impl RemoteLlmConfig {
    #[must_use]
    pub fn from_env() -> Option<Self> {
        let provider = env::var("LLM_PROVIDER").ok()?;
        if provider.eq_ignore_ascii_case("mock") {
            return None;
        }
        Some(Self {
            base_url: env::var("LLM_BASE_URL").ok()?,
            api_key: env::var("LLM_API_KEY").ok()?,
            model: env::var("LLM_MODEL").ok()?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct RemoteLlmProvider {
    config: RemoteLlmConfig,
}

impl RemoteLlmProvider {
    #[must_use]
    pub const fn new(config: RemoteLlmConfig) -> Self {
        Self { config }
    }
}

impl LlmProvider for RemoteLlmProvider {
    fn request(&self, request: &NpcTurnRequest) -> Result<NpcDecision, LlmError> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_millis(request.params.timeout_ms))
            .build()
            .map_err(|error| LlmError::Request(error.to_string()))?;
        let endpoint = format!(
            "{}/v1/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );
        let prompt = serde_json::to_string(request)
            .map_err(|error| LlmError::InvalidResponse(error.to_string()))?;
        let body = serde_json::json!({
            "model": self.config.model,
            "temperature": request.params.temperature,
            "max_completion_tokens": request.params.max_output_tokens,
            "response_format": { "type": "json_object" },
            "messages": [
                {
                    "role": "system",
                    "content": "You are an autonomous farm manager. Return only NpcDecisionV1 JSON. Never invent actions outside allowed_actions."
                },
                { "role": "user", "content": prompt }
            ]
        });
        let response: RemoteChatResponse = client
            .post(endpoint)
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(|error| LlmError::Request(error.to_string()))?
            .json()
            .map_err(|error| LlmError::InvalidResponse(error.to_string()))?;
        let Some(content) = response
            .choices
            .first()
            .map(|choice| choice.message.content.as_str())
        else {
            return Err(LlmError::InvalidResponse(
                "response has no choices".to_owned(),
            ));
        };
        serde_json::from_str(content).map_err(|error| LlmError::InvalidResponse(error.to_string()))
    }
}

#[derive(Debug, Deserialize)]
struct RemoteChatResponse {
    choices: Vec<RemoteChoice>,
}

#[derive(Debug, Deserialize)]
struct RemoteChoice {
    message: RemoteMessage,
}

#[derive(Debug, Deserialize)]
struct RemoteMessage {
    content: String,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct RuleBasedNpcPlanner;

impl LlmProvider for RuleBasedNpcPlanner {
    fn request(&self, request: &NpcTurnRequest) -> Result<NpcDecision, LlmError> {
        Ok(rule_based_decision(request))
    }
}

pub fn default_profiles() -> BTreeMap<String, NpcProfile> {
    [aster_profile(), mira_profile()]
        .into_iter()
        .map(|profile| (profile.id.clone(), profile))
        .collect()
}

pub fn build_turn_request(simulation: &GameSimulation, profile: &NpcProfile) -> NpcTurnRequest {
    let managed_zone_ids = if profile.mandate.managed_zones.is_empty() {
        simulation.zones.iter().map(|zone| zone.id).collect()
    } else {
        profile.mandate.managed_zones.clone()
    };
    let managed_zones = simulation
        .zones
        .iter()
        .filter(|zone| managed_zone_ids.contains(&zone.id))
        .map(|zone| ZoneSnapshot {
            id: zone.id,
            crop_id: zone.crop_id.clone(),
            priority: zone.priority,
            critical_tiles: simulation
                .grid
                .positions_in_rect(zone.origin, zone.size)
                .into_iter()
                .filter(|position| {
                    simulation
                        .grid
                        .tile(*position)
                        .and_then(|tile| tile.crop.as_ref())
                        .is_some_and(|crop| crop.moisture < 20 || crop.health < 40)
                })
                .count(),
            pending_jobs: simulation
                .jobs
                .iter()
                .filter(|job| job.zone_id == zone.id)
                .count(),
        })
        .collect();
    NpcTurnRequest {
        schema_version: "npc_turn_v1".to_owned(),
        npc: profile.clone(),
        mandate: profile.mandate.clone(),
        farm: simulation.snapshot(),
        managed_zones,
        recent_events: simulation.events.iter().rev().take(12).cloned().collect(),
        allowed_actions: profile.mandate.allowed_commands.iter().cloned().collect(),
        params: LlmRequestParams::default(),
    }
}

pub fn run_npc_turn(
    simulation: &mut GameSimulation,
    npc_id: &str,
    provider: &dyn LlmProvider,
) -> Result<NpcDecision, LlmError> {
    let profiles = default_profiles();
    let Some(profile) = profiles.get(npc_id) else {
        return Err(LlmError::UnknownNpc(npc_id.to_owned()));
    };
    let request = build_turn_request(simulation, profile);
    let decision = provider
        .request(&request)
        .or_else(|_| RuleBasedNpcPlanner.request(&request))?;
    for action in decision.actions.clone() {
        let envelope = simulation.next_command(
            CommandActor::Npc(profile.id.clone()),
            GameCommand::Farm(action),
        );
        simulation.apply_command(envelope)?;
    }
    simulation.record_ai_event(&profile.name, &decision.message, &decision.rationale);
    Ok(decision)
}

fn rule_based_decision(request: &NpcTurnRequest) -> NpcDecision {
    let preferred = request.managed_zones.iter().max_by(|left, right| {
        zone_score(request, left)
            .partial_cmp(&zone_score(request, right))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| right.id.cmp(&left.id))
    });
    let Some(zone) = preferred else {
        return NpcDecision {
            message: "No managed zone is available yet.".to_owned(),
            rationale: "Assign a field before requesting a strategy review.".to_owned(),
            actions: Vec::new(),
            confidence: 1.0,
        };
    };

    let target_priority = if request.npc.id == "aster" {
        if request.farm.active_contract.is_some() {
            92
        } else {
            75
        }
    } else if zone.critical_tiles > 0 {
        96
    } else {
        84
    };
    let (message, rationale) = if request.npc.id == "aster" {
        (
            format!(
                "I raised the {} zone to {} to protect contract throughput.",
                zone.crop_id, target_priority
            ),
            "Aster favors predictable delivery and concentrates the fleet on the strongest contract lane."
                .to_owned(),
        )
    } else {
        (
            format!(
                "I raised the {} zone to {} to preserve crop health and pollination flow.",
                zone.crop_id, target_priority
            ),
            "Mira prioritizes vulnerable, high-value crops and keeps resource headroom for recovery."
                .to_owned(),
        )
    };
    NpcDecision {
        message,
        rationale,
        actions: vec![FarmCommand::SetZonePriority {
            zone_id: zone.id,
            priority: target_priority,
        }],
        confidence: 0.91,
    }
}

fn zone_score(request: &NpcTurnRequest, zone: &ZoneSnapshot) -> f32 {
    let affinity = request
        .npc
        .personality
        .crop_affinity
        .get(&zone.crop_id)
        .copied()
        .unwrap_or(0.2);
    let emergency = zone.critical_tiles as f32 * (1.0 + request.npc.personality.sustainability);
    let throughput = zone.pending_jobs as f32 * request.npc.personality.efficiency * 0.05;
    affinity * 10.0 + emergency + throughput
}

fn aster_profile() -> NpcProfile {
    NpcProfile {
        id: "aster".to_owned(),
        name: "Aster".to_owned(),
        role: "Efficiency-focused farm manager".to_owned(),
        personality: NpcPersonality {
            efficiency: 0.95,
            sustainability: 0.45,
            risk_tolerance: 0.15,
            experimentation: 0.20,
            micromanagement: 0.90,
            sociability: 0.45,
            crop_affinity: BTreeMap::from([
                ("rice".to_owned(), 1.0),
                ("wheat".to_owned(), 1.0),
                ("potato".to_owned(), 0.85),
                ("tomato".to_owned(), 0.35),
                ("strawberry".to_owned(), 0.25),
            ]),
        },
        mandate: default_mandate(),
    }
}

fn mira_profile() -> NpcProfile {
    NpcProfile {
        id: "mira".to_owned(),
        name: "Mira".to_owned(),
        role: "Ecological high-value crop manager".to_owned(),
        personality: NpcPersonality {
            efficiency: 0.65,
            sustainability: 0.95,
            risk_tolerance: 0.30,
            experimentation: 0.55,
            micromanagement: 0.70,
            sociability: 0.85,
            crop_affinity: BTreeMap::from([
                ("rice".to_owned(), 0.92),
                ("wheat".to_owned(), 0.30),
                ("potato".to_owned(), 0.40),
                ("tomato".to_owned(), 0.95),
                ("strawberry".to_owned(), 0.90),
            ]),
        },
        mandate: default_mandate(),
    }
}

fn default_mandate() -> NpcMandate {
    NpcMandate {
        managed_zones: BTreeSet::new(),
        allowed_crops: ["rice", "wheat", "potato", "tomato", "strawberry"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        max_budget: 2_000,
        allowed_commands: [
            "set_zone_crop",
            "set_zone_priority",
            "set_robot_policy",
            "change_contract_priority",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn npc_cannot_execute_forbidden_command() -> Result<(), Box<dyn std::error::Error>> {
        let mut simulation = GameSimulation::new(123)?;
        let envelope = simulation.next_command(
            CommandActor::Npc("mira".to_owned()),
            GameCommand::Farm(FarmCommand::PurchaseRobot {
                robot_def_id: "pollination_drone".to_owned(),
            }),
        );
        assert!(matches!(
            simulation.apply_command(envelope),
            Err(CommandError::PermissionDenied)
        ));
        Ok(())
    }

    #[test]
    fn personalities_choose_different_zones() -> Result<(), Box<dyn std::error::Error>> {
        let mut simulation = GameSimulation::new(123)?;
        let tomato = simulation.next_command(
            CommandActor::Human,
            GameCommand::Farm(FarmCommand::CreateFieldZone {
                origin: autofarm_sim::TilePos::new(2, 30),
                size: (3, 3),
                crop_id: "tomato".to_owned(),
            }),
        );
        simulation.apply_command(tomato)?;
        let profiles = default_profiles();
        let Some(aster) = profiles.get("aster") else {
            return Err("Aster profile missing".into());
        };
        let Some(mira) = profiles.get("mira") else {
            return Err("Mira profile missing".into());
        };
        let aster_decision = MockLlmProvider.request(&build_turn_request(&simulation, aster))?;
        let mira_decision = MockLlmProvider.request(&build_turn_request(&simulation, mira))?;
        assert_ne!(aster_decision.actions, mira_decision.actions);
        Ok(())
    }

    #[test]
    fn provider_failure_uses_rule_based_fallback() -> Result<(), Box<dyn std::error::Error>> {
        struct FailingProvider;
        impl LlmProvider for FailingProvider {
            fn request(&self, _request: &NpcTurnRequest) -> Result<NpcDecision, LlmError> {
                Err(LlmError::Request("offline".to_owned()))
            }
        }

        let mut simulation = GameSimulation::new(123)?;
        let decision = run_npc_turn(&mut simulation, "aster", &FailingProvider)?;
        assert!(!decision.actions.is_empty());
        Ok(())
    }

    #[test]
    fn npc_decision_accepts_documented_tagged_command_json()
    -> Result<(), Box<dyn std::error::Error>> {
        let input = r#"{
            "message": "Prioritizing wheat.",
            "rationale": "The delivery deadline is close.",
            "actions": [
                { "type": "set_zone_priority", "zone_id": 1, "priority": 90 }
            ],
            "confidence": 0.91
        }"#;
        let decision: NpcDecision = serde_json::from_str(input)?;
        assert_eq!(
            decision.actions,
            vec![FarmCommand::SetZonePriority {
                zone_id: 1,
                priority: 90,
            }]
        );
        Ok(())
    }
}
