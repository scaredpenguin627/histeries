//! Integer utility over the two Phase 1 victory paths.

use crate::VictoryObjective;
use histeries_sim::Observation;

/// Choose the seat's objective. Switching away from an established objective
/// requires a challenger to lead by more than the switching margin.
pub fn choose_objective(
    observation: &Observation,
    previous: Option<VictoryObjective>,
) -> VictoryObjective {
    if observation.victory().is_some() {
        return VictoryObjective::Adaptive;
    }
    let population: i64 = observation
        .cities()
        .iter()
        .map(|city| i64::from(city.population))
        .sum();
    let military = observation
        .units()
        .iter()
        .filter(|unit| unit.kind.is_military())
        .count() as i64;
    let enemies = observation
        .visible_units()
        .iter()
        .filter(|unit| unit.owner != Some(observation.seat()) && unit.kind.is_military())
        .count() as i64;
    let enemy_cities = observation
        .visible_cities()
        .iter()
        .filter(|city| city.owner != observation.seat())
        .count() as i64;
    // Population supplies research every turn. A visible opponent and a force
    // already in the field increase the value of military production.
    let science = observation.cities().len() as i64 * 10
        + population * 3
        + observation.progress().technologies.len() as i64 * 5;
    let domination = military * 4 + enemy_cities * 5 + enemies.min(military + 1) * 4;
    select(science, domination, previous)
}

fn select(science: i64, domination: i64, previous: Option<VictoryObjective>) -> VictoryObjective {
    const SWITCH_MARGIN: i64 = 6;
    match previous {
        Some(VictoryObjective::Science) if domination <= science + SWITCH_MARGIN => {
            VictoryObjective::Science
        }
        Some(VictoryObjective::Domination) if science <= domination + SWITCH_MARGIN => {
            VictoryObjective::Domination
        }
        _ if science > domination => VictoryObjective::Science,
        _ if domination > science => VictoryObjective::Domination,
        _ => VictoryObjective::Adaptive,
    }
}
