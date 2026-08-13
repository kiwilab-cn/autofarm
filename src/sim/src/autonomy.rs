use serde::{Deserialize, Serialize};

use crate::{AutonomyTrial, GameMetrics};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutonomyReport {
    pub delivery_percent: f32,
    pub automation_uptime_percent: f32,
    pub manual_interventions: u32,
    pub crop_waste_percent: f32,
    pub energy_efficiency_percent: f32,
    pub robot_recovery_percent: f32,
    pub score: u8,
    pub grade: String,
}

#[must_use]
pub fn calculate_autonomy_report(trial: &AutonomyTrial, current: &GameMetrics) -> AutonomyReport {
    let delta = |current_value: u64, baseline: u64| current_value.saturating_sub(baseline);
    let contracts_expected = delta(
        current.contracts_expected,
        trial.baseline.contracts_expected,
    )
    .max(1);
    let contracts_fulfilled = delta(
        current.contracts_fulfilled,
        trial.baseline.contracts_fulfilled,
    );
    let delivery = ratio(contracts_fulfilled, contracts_expected);

    let jobs_completed = delta(current.jobs_completed, trial.baseline.jobs_completed);
    let jobs_failed = delta(current.jobs_failed, trial.baseline.jobs_failed);
    let uptime = ratio(jobs_completed, jobs_completed + jobs_failed);

    let produced = delta(current.crops_produced, trial.baseline.crops_produced);
    let lost = delta(current.crops_lost, trial.baseline.crops_lost);
    let waste = ratio(lost, produced + lost);

    let generated = delta(current.energy_generated, trial.baseline.energy_generated);
    let consumed = delta(current.energy_consumed, trial.baseline.energy_consumed);
    let energy_efficiency = if consumed == 0 {
        1.0
    } else {
        (generated as f32 / consumed as f32).clamp(0.0, 1.0)
    };

    let recoveries = delta(current.robot_recoveries, trial.baseline.robot_recoveries);
    let recovery = if jobs_failed == 0 {
        1.0
    } else {
        ratio(recoveries, jobs_failed)
    };
    let intervention = (1.0 - trial.manual_interventions as f32 * 0.12).clamp(0.0, 1.0);

    let weighted = delivery * 40.0
        + intervention * 25.0
        + uptime * 15.0
        + (1.0 - waste) * 10.0
        + energy_efficiency * 5.0
        + recovery * 5.0;
    let score = weighted.round().clamp(0.0, 100.0) as u8;
    let grade = match score {
        90..=100 => "S",
        80..=89 => "A",
        70..=79 => "B",
        60..=69 => "C",
        _ => "D",
    }
    .to_owned();

    AutonomyReport {
        delivery_percent: delivery * 100.0,
        automation_uptime_percent: uptime * 100.0,
        manual_interventions: trial.manual_interventions,
        crop_waste_percent: waste * 100.0,
        energy_efficiency_percent: energy_efficiency * 100.0,
        robot_recovery_percent: recovery * 100.0,
        score,
        grade,
    }
}

fn ratio(numerator: u64, denominator: u64) -> f32 {
    if denominator == 0 {
        1.0
    } else {
        (numerator as f32 / denominator as f32).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_intervention_reduces_score() {
        let baseline = GameMetrics::default();
        let current = GameMetrics {
            jobs_completed: 20,
            contracts_expected: 1,
            contracts_fulfilled: 1,
            crops_produced: 100,
            energy_generated: 100,
            energy_consumed: 80,
            ..GameMetrics::default()
        };
        let clean = AutonomyTrial {
            start_time: 0,
            end_time: 100,
            baseline: baseline.clone(),
            manual_interventions: 0,
            finished: false,
            score: None,
            grade: None,
        };
        let mut manual = clean.clone();
        manual.manual_interventions = 3;

        assert!(
            calculate_autonomy_report(&clean, &current).score
                > calculate_autonomy_report(&manual, &current).score
        );
    }
}
