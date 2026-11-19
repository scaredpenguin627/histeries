//! Operational methods for settlement, research, city development and defense.

use std::collections::BTreeSet;

use crate::{Operation, PlannerVersion, VictoryObjective, base_ruleset};
use histeries_hex::TileId;
use histeries_rules_core::NamedRng;
use histeries_sim::{
    BuildingKind, CityObservation, Command, DistrictKind, Observation, ProductionItem, Unit,
    UnitKind,
};

/// Decompose the strategic objective into city projects and currently executable orders.
pub fn plan(
    observation: &Observation,
    objective: VictoryObjective,
    operations: &mut Vec<Operation>,
) -> Vec<Command> {
    layered(observation, objective, operations, PlannerVersion::V2)
}

pub(crate) fn layered(
    observation: &Observation,
    objective: VictoryObjective,
    operations: &mut Vec<Operation>,
    version: PlannerVersion,
) -> Vec<Command> {
    economy(observation, Policy::Layered(objective, version), operations)
}

/// Select the best immediately available economic return, without planning prerequisites.
pub fn greedy(observation: &Observation, operations: &mut Vec<Operation>) -> Vec<Command> {
    economy(observation, Policy::Greedy, operations)
}

/// Sample legal economic choices from the player's named stream.
pub fn random(
    observation: &Observation,
    rng: &mut NamedRng,
    operations: &mut Vec<Operation>,
) -> Vec<Command> {
    economy(observation, Policy::Random(rng), operations)
}

pub(super) enum Policy<'a> {
    Layered(VictoryObjective, PlannerVersion),
    Greedy,
    Random(&'a mut NamedRng),
}

impl Policy<'_> {
    /// Stable first-maximum tie breaking for the deterministic policies.
    fn choose<T, S: Ord>(&mut self, choices: Vec<(S, T)>) -> Option<T> {
        if let Self::Random(rng) = self {
            let index = rng.next_bounded(choices.len() as u64)? as usize;
            return choices.into_iter().nth(index).map(|(_, value)| value);
        }
        choices
            .into_iter()
            .rev()
            .max_by(|a, b| a.0.cmp(&b.0))
            .map(|(_, value)| value)
    }
}

fn economy(
    observation: &Observation,
    mut policy: Policy<'_>,
    operations: &mut Vec<Operation>,
) -> Vec<Command> {
    if observation.victory().is_some() {
        return Vec::new();
    }
    let player = observation.seat();
    let layered = matches!(policy, Policy::Layered(..));
    let (mut commands, mut reserved) =
        crate::settlement::plan_settlers(observation, operations, &mut policy);
    let vacated: BTreeSet<_> = commands
        .iter()
        .filter_map(|command| match command {
            Command::UnitMove { unit, .. } => observation
                .units()
                .iter()
                .find(|owned| owned.id == *unit)
                .map(|owned| owned.tile),
            _ => None,
        })
        .collect();
    let horizon = if layered {
        research_horizon(observation)
    } else {
        0
    };
    let mut military = observation
        .units()
        .iter()
        .filter(|unit| unit.kind.is_military())
        .count();
    military += observation
        .cities()
        .iter()
        .flat_map(|city| city.production.iter().chain(&city.waiting))
        .filter(|item| matches!(item, ProductionItem::Unit(kind) if kind.is_military()))
        .count();
    let mut has_assault_unit = observation.units().iter().any(|u| u.kind.is_military() && u.kind.range() == 1)
        || observation.cities().iter().flat_map(|c| c.production.iter().chain(&c.waiting))
            .any(|item| matches!(item, ProductionItem::Unit(kind) if kind.is_military() && kind.range() == 1));
    let enemy_city = observation
        .visible_cities()
        .iter()
        .any(|city| city.owner != player);
    let hostiles = observation
        .visible_units()
        .iter()
        .filter(|unit| unit.owner != Some(player) && unit.kind.is_military())
        .count();
    for city in observation.cities() {
        if !city.waiting.is_empty()
            || (city.production.is_some()
                && !(layered && matches!(city.production, Some(ProductionItem::Building(_)))))
        {
            continue;
        }
        let sites = city_sites(observation, city.tile, &reserved, &vacated);
        let next = match policy {
            Policy::Layered(objective, version) => {
                let demand = if objective == VictoryObjective::Domination {
                    hostiles
                } else {
                    hostiles.min(2)
                };
                if sites.can_spawn_military
                    && version == PlannerVersion::V2
                    && objective == VictoryObjective::Domination
                    && enemy_city
                    && !has_assault_unit
                {
                    Some((None, ProductionItem::Unit(UnitKind::Warrior)))
                } else if sites.can_spawn_military && military < demand {
                    Some((None, ProductionItem::Unit(UnitKind::Archer)))
                } else {
                    let site = if version == PlannerVersion::V2 {
                        // New districts and their existing neighbors both earn adjacency.
                        sites.tiles.iter().copied().max_by_key(|tile| {
                            let i = observation
                                .visible_tiles()
                                .binary_search_by_key(tile, |t| t.tile)
                                .expect("site was selected from visible tiles");
                            let neighbors = &observation.visible_tiles()[i].neighbors;
                            let edges = usize::from(neighbors.contains(&city.tile))
                                + 2 * city
                                    .districts
                                    .iter()
                                    .filter(|d| neighbors.contains(&d.tile))
                                    .count();
                            (edges, std::cmp::Reverse(*tile))
                        })
                    } else {
                        sites.tiles.first().copied()
                    };
                    development(city, site, sites.can_spawn_civilian, horizon, &mut policy)
                }
            }
            _ => baseline_production(city, &sites, &mut policy, military < hostiles),
        };
        if let Some((goal, item)) = next {
            if Some(item) == city.production {
                continue;
            }
            if let Some(building) = goal {
                operations.push(Operation::DevelopCity(city.id, building));
            }
            match item {
                ProductionItem::District { tile, .. } => {
                    reserved.insert(tile);
                }
                ProductionItem::Unit(kind) if kind.is_military() => {
                    military += 1;
                    has_assault_unit |= kind.range() == 1;
                }
                _ => {}
            }
            operations.push(Operation::Produce(city.id, item));
            commands.push(Command::CitySetProduction {
                player,
                city: city.id,
                item,
            });
            if matches!(policy, Policy::Layered(_, PlannerVersion::V2))
                && matches!(item, ProductionItem::District { .. })
                && let Some(building) = goal
            {
                let item = ProductionItem::Building(building);
                commands.push(Command::CityEnqueueProduction {
                    player,
                    city: city.id,
                    item,
                });
                operations.push(Operation::Produce(city.id, item));
            }
        }
    }
    plan_progress(observation, &mut policy, operations, &mut commands);
    commands
}

/// Free district sites in increasing distance and canonical tile order.
struct CitySites {
    tiles: Vec<TileId>,
    can_spawn_military: bool,
    can_spawn_civilian: bool,
}

// An observed walk bounded by the registry's maximum site radius is sufficient
// proof of range, including when fog hides a shorter path.
fn city_sites(
    observation: &Observation,
    center: TileId,
    reserved: &BTreeSet<TileId>,
    vacated: &BTreeSet<TileId>,
) -> CitySites {
    let mut sites = CitySites {
        tiles: Vec::new(),
        can_spawn_military: false,
        can_spawn_civilian: false,
    };
    let mut reached = BTreeSet::from([center]);
    let mut frontier = vec![center];
    for distance in 1..=3 {
        let mut next = Vec::new();
        for id in frontier {
            let Ok(i) = observation
                .visible_tiles()
                .binary_search_by_key(&id, |tile| tile.tile)
            else {
                continue;
            };
            for &neighbor in &observation.visible_tiles()[i].neighbors {
                if reached.insert(neighbor) {
                    next.push(neighbor);
                }
            }
        }
        next.sort_unstable();
        for id in &next {
            let Ok(i) = observation
                .visible_tiles()
                .binary_search_by_key(id, |tile| tile.tile)
            else {
                continue;
            };
            let tile = &observation.visible_tiles()[i];
            if reserved.contains(id)
                || tile.district_reserved
                || Unit::movement_cost(tile.terrain, false).is_none()
                || (tile.occupied && tile.units.is_empty())
            {
                continue;
            }
            // Operational settler moves precede city orders; its escort stays behind.
            let mut units = tile.units;
            if vacated.contains(id) {
                units.civilian = None;
            }
            if units.is_empty() {
                sites.tiles.push(*id);
            }
            if distance == 1 {
                sites.can_spawn_military |=
                    units.can_enter(Some(observation.seat()), UnitKind::Archer);
                sites.can_spawn_civilian |=
                    units.can_enter(Some(observation.seat()), UnitKind::Settler);
            }
        }
        frontier = next;
    }
    sites
}

fn research_horizon(observation: &Observation) -> i64 {
    let rules = base_ruleset();
    let progress = observation.progress();
    let remaining: i64 = rules
        .technologies()
        .filter(|node| !progress.technologies.iter().any(|done| done == node.id()))
        .map(|node| i64::from(node.cost()))
        .sum();
    let buildings = observation
        .cities()
        .iter()
        .flat_map(|city| &city.buildings)
        .filter_map(|kind| rules.building(kind.ruleset_id()));
    let policies = progress
        .policies
        .iter()
        .filter_map(|(_, id)| rules.policy(id));
    let rate = observation
        .cities()
        .iter()
        .map(|city| i64::from(city.population))
        .sum::<i64>()
        + buildings.map(|b| i64::from(b.science_bonus())).sum::<i64>()
        + policies.map(|p| i64::from(p.science_bonus())).sum::<i64>();
    (remaining - progress.technology_progress).max(0) / rate.max(1)
}

fn has_district(city: &CityObservation, kind: DistrictKind) -> bool {
    kind == DistrictKind::CityCenter || city.districts.iter().any(|built| built.kind == kind)
}

/// Each method has an ordered prerequisite (establish the required district)
/// and a terminal task (construct the target). The city queue persists across
/// rounds. Only the first unsatisfied task is dispatched on each observation.
fn development(
    city: &CityObservation,
    site: Option<TileId>,
    can_settle: bool,
    horizon: i64,
    policy: &mut Policy<'_>,
) -> Option<(Option<BuildingKind>, ProductionItem)> {
    let mut projects = Vec::new();
    let production_rate = i64::from(city.population)
        + city
            .buildings
            .iter()
            .filter_map(|kind| base_ruleset().building(kind.ruleset_id()))
            .map(|b| i64::from(b.production_bonus()))
            .sum::<i64>()
        + city
            .districts
            .iter()
            .filter_map(|district| base_ruleset().district(district.kind.ruleset_id()))
            .map(|d| i64::from(d.production_bonus()))
            .sum::<i64>();
    let productive_turns =
        |cost: i64| (horizon - (cost + production_rate - 1) / production_rate.max(1)).max(0);
    if can_settle {
        let item = ProductionItem::Unit(UnitKind::Settler);
        let turns = (productive_turns(item.cost()) - 1).max(0);
        // Newly founded towns start at population one and grow to two in five
        // rounds. Both citizens supply science and production on future turns.
        let population_turns = turns + (turns - 5).max(0);
        projects.push((14 * population_turns * 1000 / item.cost(), (None, item)));
    }
    for building in BuildingKind::ALL {
        if city.buildings.contains(&building) {
            continue;
        }
        let Some(definition) = base_ruleset().building(building.ruleset_id()) else {
            continue;
        };
        let Some(required) = building.required_district() else {
            continue;
        };
        let next = if has_district(city, required) {
            ProductionItem::Building(building)
        } else if let Some(tile) = site {
            ProductionItem::District {
                kind: required,
                tile,
            }
        } else {
            continue;
        };
        let cost = building.production_cost()
            + if has_district(city, required) {
                0
            } else {
                required.production_cost()
            };
        let remaining = if city.production == Some(next) {
            (cost - city.production_stored).max(1)
        } else {
            cost
        };
        let turns = productive_turns(remaining);
        let growth_turns = (turns - 5).max(0);
        let gain = (12 * i64::from(definition.science_bonus())
            + 2 * i64::from(definition.production_bonus())
            + i64::from(definition.culture_bonus()))
            * turns
            + 14 * i64::from(definition.housing()) * growth_turns;
        if gain > 0 {
            let score = gain * 1000 / remaining;
            projects.push((score, (Some(building), next)));
        }
    }
    policy.choose(projects)
}

fn baseline_production(
    city: &CityObservation,
    sites: &CitySites,
    policy: &mut Policy<'_>,
    threatened: bool,
) -> Option<(Option<BuildingKind>, ProductionItem)> {
    let mut options = Vec::new();
    for building in BuildingKind::ALL {
        if city.buildings.contains(&building)
            || building
                .required_district()
                .is_some_and(|kind| !has_district(city, kind))
        {
            continue;
        }
        let Some(definition) = base_ruleset().building(building.ruleset_id()) else {
            continue;
        };
        // The greedy player values direct per-turn output per production spent.
        let output = i64::from(definition.science_bonus())
            + i64::from(definition.culture_bonus())
            + i64::from(definition.food_bonus())
            + i64::from(definition.production_bonus());
        options.push((
            output * 1000 / building.production_cost(),
            ProductionItem::Building(building),
        ));
    }
    for kind in [
        UnitKind::Warrior,
        UnitKind::Archer,
        UnitKind::Builder,
        UnitKind::Settler,
    ] {
        if (kind.is_military() && !sites.can_spawn_military)
            || (!kind.is_military() && !sites.can_spawn_civilian)
        {
            continue;
        }
        // A founded size-one city immediately contributes one science and one
        // culture. Compare that return to buildings on the same cost basis.
        let output = if threatened && kind.is_military() {
            i64::from(kind.defense())
        } else if kind == UnitKind::Settler {
            2
        } else {
            0
        };
        let score = output * 1000 / kind.production_cost();
        options.push((score, ProductionItem::Unit(kind)));
    }
    for &tile in &sites.tiles {
        for kind in DistrictKind::ALL
            .into_iter()
            .filter(|kind| !has_district(city, *kind))
        {
            let output = base_ruleset()
                .district(kind.ruleset_id())
                .map_or(0, |district| district.production_bonus());
            options.push((
                i64::from(output) * 1000 / kind.production_cost(),
                ProductionItem::District { kind, tile },
            ));
        }
    }
    policy.choose(options).map(|item| (None, item))
}

/// Project both research tracks, including same-turn unlocks, before dispatch.
fn plan_progress(
    observation: &Observation,
    policy: &mut Policy<'_>,
    operations: &mut Vec<Operation>,
    commands: &mut Vec<Command>,
) {
    let player = observation.seat();
    let progress = observation.progress();
    let rules = base_ruleset();
    let mut priorities = BTreeSet::new();
    let mut pending = Vec::new();
    if matches!(policy, Policy::Layered(..))
        && let Some(goal) = rules
            .policies()
            .filter(|policy| policy.science_bonus() > 0)
            .max_by_key(|policy| policy.science_bonus())
            .and_then(|policy| policy.required_civic())
    {
        pending.push(goal);
    }
    while let Some(id) = pending.pop() {
        if priorities.insert(id)
            && let Some(node) = rules.civic(id)
        {
            pending.extend(node.prerequisites().iter().map(String::as_str));
        }
    }
    let technology_count = rules.technologies().count();
    let can_chain = progress.technologies.len() < technology_count
        && observation
            .visible_cities()
            .iter()
            .any(|city| city.owner != player);
    let civic_path = if progress.current_civic.is_some() {
        Vec::new()
    } else {
        research_path(
            rules
                .civics()
                .map(|node| (node.id(), node.cost(), node.prerequisites())),
            &progress.civics,
            progress.civic_progress,
            &priorities,
            can_chain,
            policy,
        )
    };
    let tech_path = if progress.current_technology.is_some() {
        Vec::new()
    } else {
        research_path(
            rules
                .technologies()
                .map(|node| (node.id(), node.cost(), node.prerequisites())),
            &progress.technologies,
            progress.technology_progress,
            &BTreeSet::new(),
            can_chain,
            policy,
        )
    };
    // When fog leaves domination possible, spend banked points only after
    // publishing every other nonterminal progression order.
    let mut possible_winner = None;
    for (path, is_technology) in [(&tech_path, true), (&civic_path, false)] {
        for (i, &(id, instant)) in path.iter().enumerate() {
            let order = if is_technology {
                (
                    Operation::ResearchTechnology(id.to_owned()),
                    Command::ResearchTechnology {
                        player,
                        technology: id.to_owned(),
                    },
                )
            } else {
                (
                    Operation::ResearchCivic(id.to_owned()),
                    Command::ResearchCivic {
                        player,
                        civic: id.to_owned(),
                    },
                )
            };
            if instant
                && (!can_chain
                    || (is_technology && progress.technologies.len() + i + 1 == technology_count))
            {
                if possible_winner.is_none() {
                    possible_winner = Some(order);
                }
                break;
            }
            operations.push(order.0);
            commands.push(order.1);
        }
    }
    // A civic completed above can make its policy legal immediately.
    let civics = |id: &str| {
        progress.civics.iter().any(|done| done == id)
            || (can_chain
                && civic_path
                    .iter()
                    .any(|&(done, instant)| instant && done == id))
    };
    let unlocked: Vec<_> = rules
        .policies()
        .filter(|option| option.required_civic().is_none_or(civics))
        .collect();
    let mut active: std::collections::BTreeMap<_, _> = progress
        .policies
        .iter()
        .map(|(slot, id)| (*slot, id.as_str()))
        .collect();
    let completed = progress.civics.len()
        + if can_chain {
            civic_path.iter().filter(|(_, instant)| *instant).count()
        } else {
            0
        };
    let slots = histeries_sim::PlayerProgress::slots_for_civics(completed);
    let value = |id: &str| {
        rules.policy(id).map_or((0, 0), |option| {
            (option.science_bonus(), option.culture_bonus())
        })
    };
    // Fill empty slots first, then let newly available policies improve
    // existing assignments in order of increasing current return.
    let mut order: Vec<_> = (0..slots).collect();
    order.sort_by_key(|slot| active.get(slot).map(|id| value(id)));
    for slot in order {
        let old = active.get(&slot).copied();
        let choices = unlocked
            .iter()
            .filter(|option| {
                !active
                    .iter()
                    .any(|(other, id)| *other != slot && *id == option.id())
            })
            .map(|option| {
                let id = option.id();
                ((value(id), old == Some(id)), id)
            })
            .collect();
        if let Some(id) = policy.choose(choices)
            && old != Some(id)
        {
            active.insert(slot, id);
            operations.push(Operation::AdoptPolicy(slot, id.to_owned()));
            commands.push(Command::PolicyAdopt {
                player,
                slot,
                policy: id.to_owned(),
            });
        }
    }
    if let Some((op, command)) = possible_winner {
        operations.push(op);
        commands.push(command);
    }
}

fn research_path<'a>(
    tree: impl Iterator<Item = (&'a str, u16, &'a [String])>,
    completed: &[String],
    mut bank: i64,
    priority: &BTreeSet<&str>,
    can_chain: bool,
    policy: &mut Policy<'_>,
) -> Vec<(&'a str, bool)> {
    let nodes: Vec<_> = tree.collect();
    let mut done: BTreeSet<_> = completed.iter().map(String::as_str).collect();
    let mut path = Vec::new();
    loop {
        let choices = nodes
            .iter()
            .copied()
            .filter(|(id, _, required)| {
                !done.contains(id)
                    && required
                        .iter()
                        .all(|prerequisite| done.contains(prerequisite.as_str()))
            })
            .map(|(id, cost, _)| {
                // The layered player may favor a policy prerequisite or a target
                // that can keep gaining science while a rival city is out of sight.
                let continue_research =
                    matches!(policy, Policy::Layered(..)) && !can_chain && i64::from(cost) > bank;
                (
                    (
                        priority.contains(id),
                        continue_research,
                        std::cmp::Reverse(cost),
                    ),
                    (id, cost),
                )
            })
            .collect();
        let Some((id, cost)) = policy.choose(choices) else {
            break;
        };
        let instant = i64::from(cost) <= bank;
        path.push((id, instant));
        if !instant || !can_chain {
            break;
        }
        bank -= i64::from(cost);
        done.insert(id);
    }
    path
}
