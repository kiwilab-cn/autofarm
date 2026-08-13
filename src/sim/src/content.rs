use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{ContractDef, CropDef, RobotDef};

const CROPS_RON: &str = include_str!("../../../assets/data/crops.ron");
const ROBOTS_RON: &str = include_str!("../../../assets/data/robots.ron");
const CONTRACTS_RON: &str = include_str!("../../../assets/data/contracts.ron");

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ContentError {
    #[error("invalid {kind} content: {message}")]
    Invalid { kind: &'static str, message: String },
    #[error("content catalog is missing required entry: {0}")]
    MissingRequired(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentCatalog {
    pub crops: BTreeMap<String, CropDef>,
    pub robots: BTreeMap<String, RobotDef>,
    pub contracts: Vec<ContractDef>,
}

impl ContentCatalog {
    pub fn load_embedded() -> Result<Self, ContentError> {
        let crops: Vec<CropDef> =
            ron::from_str(CROPS_RON).map_err(|error| ContentError::Invalid {
                kind: "crop",
                message: error.to_string(),
            })?;
        let robots: Vec<RobotDef> =
            ron::from_str(ROBOTS_RON).map_err(|error| ContentError::Invalid {
                kind: "robot",
                message: error.to_string(),
            })?;
        let contracts: Vec<ContractDef> =
            ron::from_str(CONTRACTS_RON).map_err(|error| ContentError::Invalid {
                kind: "contract",
                message: error.to_string(),
            })?;

        let catalog = Self {
            crops: crops
                .into_iter()
                .map(|crop| (crop.id.clone(), crop))
                .collect(),
            robots: robots
                .into_iter()
                .map(|robot| (robot.id.clone(), robot))
                .collect(),
            contracts,
        };
        catalog.validate()?;
        Ok(catalog)
    }

    fn validate(&self) -> Result<(), ContentError> {
        for crop in ["wheat", "potato", "tomato", "strawberry"] {
            let Some(definition) = self.crops.get(crop) else {
                return Err(ContentError::MissingRequired(crop.to_owned()));
            };
            if definition.stages.len() < 2 {
                return Err(ContentError::Invalid {
                    kind: "crop",
                    message: format!("{crop} needs at least two growth stages"),
                });
            }
        }
        for robot in [
            "basic_rover",
            "pollination_drone",
            "field_quadruped",
            "biped_farmhand",
        ] {
            if !self.robots.contains_key(robot) {
                return Err(ContentError::MissingRequired(robot.to_owned()));
            }
        }
        if self.contracts.len() < 4 {
            return Err(ContentError::MissingRequired(
                "four starter contracts".to_owned(),
            ));
        }
        Ok(())
    }
}
