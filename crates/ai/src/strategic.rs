//! Integer utility over the two Phase 1 victory paths.

use crate::{PlannerVersion, VictoryObjective, base_ruleset};
use histeries_sim::Observation;

/// Choose the seat's objective. Switching away from an established objective
/// requires a challenger to lead by more than the switching margin.
pub fn choose_objective(
    observation: &Observation,
    previous: Option<VictoryObjective>,
) -> VictoryObjective {
    evaluate(observation, previous, PlannerVersion::V2)
}

pub(crate) fn evaluate(
    observation: &Observation,
    previous: Option<VictoryObjective>,
    version: PlannerVersion,
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
    if version == PlannerVersion::V1 {
        return select(science, domination, previous);
    }
    // Compare actual research infrastructure with immediately exploitable or
    // endangered city perimeters. Counts alone miss both opportunities.
    let research_bonus: i64 = observation
        .cities()
        .iter()
        .flat_map(|city| &city.buildings)
        .filter_map(|building| base_ruleset().building(building.ruleset_id()))
        .map(|rule| i64::from(rule.science_bonus()))
        .sum();
    let mut conquest = 0;
    for city in observation.visible_cities() {
        let Ok(index) = observation
            .visible_tiles()
            .binary_search_by_key(&city.tile, |t| t.tile)
        else {
            continue;
        };
        let mut guarded = false;
        let mut invader = false;
        for tile in &observation.visible_tiles()[index].neighbors {
            let Ok(i) = observation
                .visible_tiles()
                .binary_search_by_key(tile, |t| t.tile)
            else {
                continue;
            };
            let slots = &observation.visible_tiles()[i].units;
            let Some(id) = slots.military else { continue };
            guarded |= slots.owner == Some(city.owner);
            if slots.owner != Some(city.owner)
                && slots.owner.is_some()
                && let Ok(u) = observation
                    .visible_units()
                    .binary_search_by_key(&id, |u| u.id)
            {
                let unit = observation.visible_units()[u];
                invader |= unit.kind.range() == 1
                    && (city.owner == observation.seat() || unit.owner == Some(observation.seat()));
            }
        }
        if invader && !guarded {
            conquest += 24 + 2 * i64::from(city.population);
        }
    }
    select(
        science + 3 * research_bonus,
        domination + conquest,
        previous,
    )
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
