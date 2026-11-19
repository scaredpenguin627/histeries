//! Deterministic beam search over joint orders on the seat's revealed battlefield.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use histeries_hex::TileId;
use histeries_rules_core::{CityId, NamedRng, PlayerId};
use histeries_sim::{
    CombatDamage, Command, Observation, ProductionItem, Unit, VisibleTileObservation,
    VisibleUnitObservation,
};

use crate::{Operation, PlannerVersion};

const BEAM_WIDTH: usize = 4;

/// Searches four joint plans at each depth, including a move followed by a strike.
pub fn plan(observation: &Observation, operations: &mut Vec<Operation>) -> Vec<Command> {
    layered(observation, operations, PlannerVersion::V2)
}

pub(crate) fn layered(
    observation: &Observation,
    operations: &mut Vec<Operation>,
    version: PlannerVersion,
) -> Vec<Command> {
    search(observation, operations, BEAM_WIDTH, None, version)
}

/// Commits to the best immediate action for each unit in ascending ID order.
pub fn greedy(observation: &Observation, operations: &mut Vec<Operation>) -> Vec<Command> {
    search(observation, operations, 1, None, PlannerVersion::V1)
}

/// Selects one legal order per military unit using the seat's seeded stream.
pub fn random(
    observation: &Observation,
    operations: &mut Vec<Operation>,
    seed: u64,
) -> Vec<Command> {
    let mut master = [0; 32];
    master[..8].copy_from_slice(&seed.to_le_bytes());
    master[8..12].copy_from_slice(&observation.turn().to_le_bytes());
    let rng = NamedRng::new(
        master,
        &format!("tactics-player-{}", observation.seat().index()),
    );
    search(observation, operations, 1, Some(rng), PlannerVersion::V1)
}

struct Battlefield<'a> {
    enemy_cities: BTreeMap<TileId, (CityId, PlayerId)>,
    tiles: BTreeMap<TileId, &'a VisibleTileObservation>,
    blocked: BTreeSet<TileId>,
    // Reachable staging tiles around observed hostile cities.
    home_distance: BTreeMap<TileId, i32>,
    threatened_cities: Vec<TileId>,
    assault_distance: BTreeMap<TileId, i32>,
    // Cities whose complete perimeter is covered by the initial observation.
    scouted: BTreeSet<CityId>,
    // Distances to the closest observed hostile through revealed traversable tiles.
    threat_distance: BTreeMap<TileId, i32>,
}

fn is_melee(observation: &Observation, id: Option<histeries_rules_core::UnitId>) -> bool {
    id.and_then(|id| {
        observation
            .visible_units()
            .binary_search_by_key(&id, |u| u.id)
            .ok()
    })
    .is_some_and(|i| observation.visible_units()[i].kind.range() == 1)
}

impl<'a> Battlefield<'a> {
    fn new(
        observation: &'a Observation,
        operations: &[Operation],
        version: PlannerVersion,
    ) -> Self {
        let tiles: BTreeMap<_, _> = observation
            .visible_tiles()
            .iter()
            .map(|tile| (tile.tile, tile))
            .collect();
        let mut blocked: BTreeSet<_> = tiles
            .values()
            .filter(|tile| {
                tile.district_reserved
                    || (tile.occupied && tile.units.is_empty())
                    || (tile.units.civilian.is_some()
                        && tile.units.owner != Some(observation.seat()))
            })
            .map(|tile| tile.tile)
            .collect();
        let mut new_sites = BTreeMap::new();
        for operation in operations {
            match operation {
                Operation::SettlementSite(unit, tile) => {
                    new_sites.insert(*unit, *tile);
                    blocked.insert(*tile);
                }
                Operation::Settle(unit) => {
                    if !new_sites.contains_key(unit)
                        && let Some(founder) =
                            observation.units().iter().find(|owned| owned.id == *unit)
                    {
                        blocked.insert(founder.tile);
                    }
                }
                Operation::Produce(_, ProductionItem::District { tile, .. }) => {
                    blocked.insert(*tile);
                }
                _ => {}
            }
        }

        let mut queue: VecDeque<_> = observation
            .visible_units()
            .iter()
            .filter(|unit| unit.owner != Some(observation.seat()) && unit.kind.is_military())
            .map(|unit| unit.tile)
            .collect();
        let mut threat_distance: BTreeMap<_, _> = queue.iter().map(|tile| (*tile, 0)).collect();
        while let Some(tile) = queue.pop_front() {
            let distance = threat_distance[&tile] + 1;
            for neighbor in &tiles[&tile].neighbors {
                if let Some(next) = tiles.get(neighbor)
                    && Unit::movement_cost(next.terrain, next.road).is_some()
                    && let std::collections::btree_map::Entry::Vacant(entry) =
                        threat_distance.entry(*neighbor)
                {
                    entry.insert(distance);
                    queue.push_back(*neighbor);
                }
            }
        }
        let enemy_cities: BTreeMap<_, _> = observation
            .visible_cities()
            .iter()
            .filter(|city| city.owner != observation.seat())
            .map(|city| (city.tile, (city.id, city.owner)))
            .collect();
        let mut assault_distance = BTreeMap::new();
        let mut scouted = BTreeSet::new();
        if version == PlannerVersion::V2 {
            for (&center, &(id, _)) in &enemy_cities {
                let perimeter = &tiles[&center].neighbors;
                if perimeter.len() == 6
                    || perimeter.iter().any(|tile| {
                        tiles[tile].units.owner == Some(observation.seat())
                            || observation.cities().iter().any(|city| city.tile == *tile)
                    })
                {
                    scouted.insert(id);
                }
                for &tile in perimeter {
                    if !blocked.contains(&tile)
                        && Unit::movement_cost(tiles[&tile].terrain, tiles[&tile].road).is_some()
                        && assault_distance.insert(tile, 0).is_none()
                    {
                        queue.push_back(tile);
                    }
                }
            }
            while let Some(at) = queue.pop_front() {
                let distance = assault_distance[&at] + 1;
                for &next in &tiles[&at].neighbors {
                    if !blocked.contains(&next)
                        && Unit::movement_cost(tiles[&next].terrain, tiles[&next].road).is_some()
                        && let std::collections::btree_map::Entry::Vacant(entry) =
                            assault_distance.entry(next)
                    {
                        entry.insert(distance);
                        queue.push_back(next);
                    }
                }
            }
        }
        let mut home_distance = BTreeMap::new();
        let mut threatened_cities = Vec::new();
        if version == PlannerVersion::V2 {
            for city in observation.cities() {
                queue.push_back(city.tile);
                home_distance.insert(city.tile, 0);
                if threat_distance
                    .get(&city.tile)
                    .is_some_and(|&distance| distance <= 2)
                    && tiles[&city.tile].neighbors.iter().any(|tile| {
                        std::iter::once(tile)
                            .chain(&tiles[tile].neighbors)
                            .any(|id| {
                                let slots = &tiles[id].units;
                                slots.owner.is_some()
                                    && slots.owner != Some(observation.seat())
                                    && is_melee(observation, slots.military)
                            })
                    })
                {
                    threatened_cities.push(city.tile);
                }
            }
            while let Some(at) = queue.pop_front() {
                let distance = home_distance[&at] + 1;
                for &next in &tiles[&at].neighbors {
                    if let std::collections::btree_map::Entry::Vacant(entry) =
                        home_distance.entry(next)
                    {
                        entry.insert(distance);
                        queue.push_back(next);
                    }
                }
            }
        }
        Self {
            enemy_cities,
            assault_distance,
            scouted,
            home_distance,
            threatened_cities,
            tiles,
            blocked,
            threat_distance,
        }
    }

    fn attack_range(&self, unit: VisibleUnitObservation) -> BTreeMap<TileId, u32> {
        let mut range = BTreeMap::new();
        for neighbor in &self.tiles[&unit.tile].neighbors {
            range.insert(*neighbor, 1);
            if unit.kind.range() == 2
                && let Some(tile) = self.tiles.get(neighbor)
            {
                for outer in &tile.neighbors {
                    range.entry(*outer).or_insert(2);
                }
            }
        }
        range.remove(&unit.tile);
        range
    }
}

#[derive(Clone)]
struct Position {
    captured: BTreeSet<CityId>,
    units: Vec<VisibleUnitObservation>,
    military: BTreeMap<TileId, usize>,
    commands: Vec<Command>,
    score: i64,
}

#[derive(Clone, Copy)]
enum Order {
    Capture(CityId),
    Attack {
        defender: usize,
        damage: CombatDamage,
    },
    Heal {
        hp: i32,
    },
    Move {
        destination: TileId,
        cost: i32,
    },
    Hold,
}

impl Position {
    fn in_zoc(&self, field: &Battlefield<'_>, unit: VisibleUnitObservation, tile: TileId) -> bool {
        field.tiles[&tile]
            .neighbors
            .iter()
            .filter_map(|neighbor| self.military.get(neighbor))
            .any(|index| {
                let neighbor = self.units[*index];
                neighbor.owner != unit.owner && neighbor.kind.is_military()
            })
    }

    fn strikes(
        &self,
        unit: VisibleUnitObservation,
        field: &Battlefield<'_>,
        range: &BTreeMap<TileId, u32>,
    ) -> Vec<(Order, i32)> {
        let mut options = Vec::new();
        if unit.movement > 0 && unit.kind.range() == 1 {
            for adjacent in &field.tiles[&unit.tile].neighbors {
                if let Some(&(city, owner)) = field.enemy_cities.get(adjacent)
                    && !self.captured.contains(&city)
                    && !field.tiles[adjacent]
                        .neighbors
                        .iter()
                        .filter_map(|tile| self.military.get(tile))
                        .any(|index| self.units[*index].owner == Some(owner))
                {
                    options.push((Order::Capture(city), 400));
                }
            }
        }
        for (tile, distance) in range {
            if let Some(&index) = self.military.get(tile) {
                let target = self.units[index];
                if let Ok(damage) = unit.forecast_attack(target, *distance) {
                    let killed = damage.to_defender >= target.hp;
                    let lost = damage.to_attacker >= unit.hp;
                    let score = 4 * damage.to_defender.min(target.hp)
                        - 2 * damage.to_attacker.min(unit.hp)
                        + 160 * i32::from(killed)
                        - 180 * i32::from(lost);
                    options.push((
                        Order::Attack {
                            defender: index,
                            damage,
                        },
                        score,
                    ));
                }
            }
        }
        options
    }

    fn options(
        &self,
        actor: usize,
        field: &Battlefield<'_>,
        range: &BTreeMap<TileId, u32>,
    ) -> Vec<(Order, i32)> {
        let unit = self.units[actor];
        let mut options = self.strikes(unit, field, range);
        let in_zoc = self.in_zoc(field, unit, unit.tile);
        if let Ok(hp) = unit.forecast_heal(in_zoc) {
            options.push((Order::Heal { hp }, 2 * (hp - unit.hp)));
        }
        for destination in &field.tiles[&unit.tile].neighbors {
            let Some(tile) = field.tiles.get(destination) else {
                continue;
            };
            let Some(cost) = Unit::movement_cost(tile.terrain, tile.road) else {
                continue;
            };
            if unit.movement < cost
                || self.military.contains_key(destination)
                || field.blocked.contains(destination)
                || (in_zoc && !self.in_zoc(field, unit, *destination))
            {
                continue;
            }
            let approach = field
                .threat_distance
                .get(&unit.tile)
                .zip(field.threat_distance.get(destination))
                .map_or(0, |(before, after)| before - after);
            let mut mission = 0;
            if field.enemy_cities.is_empty() && field.threat_distance.is_empty() {
                mission += field
                    .home_distance
                    .get(&unit.tile)
                    .zip(field.home_distance.get(destination))
                    .map_or(0, |(before, after)| 3 * (after - before));
            }
            // Releasing the sole defender exposes the city to a capture order.
            for center in &field.threatened_cities {
                let perimeter = &field.tiles[center].neighbors;
                let stationed = perimeter.iter().any(|tile| {
                    self.military
                        .get(tile)
                        .is_some_and(|&i| i != actor && self.units[i].owner == unit.owner)
                });
                if !stationed {
                    mission += 100
                        * (i32::from(perimeter.contains(destination))
                            - i32::from(perimeter.contains(&unit.tile)));
                }
            }
            options.push((
                Order::Move {
                    destination: *destination,
                    cost,
                },
                1 + mission
                    + 4 * approach
                    + if unit.kind.range() == 1 {
                        field
                            .assault_distance
                            .get(&unit.tile)
                            .zip(field.assault_distance.get(destination))
                            .map_or(0, |(before, after)| 8 * (before - after))
                    } else {
                        0
                    },
            ));
        }
        options.push((Order::Hold, 0));
        options
    }

    fn apply(&mut self, actor: usize, order: Order, player: histeries_rules_core::PlayerId) {
        let unit = self.units[actor].id;
        let command = match order {
            Order::Attack { defender, damage } => {
                for (index, dealt) in [(actor, damage.to_attacker), (defender, damage.to_defender)]
                {
                    let hit = &mut self.units[index];
                    hit.hp -= dealt;
                    if hit.hp <= 0 {
                        self.military.remove(&hit.tile);
                    }
                }
                self.units[actor].movement = 0;
                Command::UnitAttack {
                    player,
                    attacker: unit,
                    defender: self.units[defender].id,
                }
            }
            Order::Heal { hp } => {
                self.units[actor].hp = hp;
                self.units[actor].movement = 0;
                Command::UnitHeal { player, unit }
            }
            Order::Move { destination, cost } => {
                self.military.remove(&self.units[actor].tile);
                self.military.insert(destination, actor);
                self.units[actor].tile = destination;
                self.units[actor].movement -= cost;
                Command::UnitMove {
                    player,
                    unit,
                    destination,
                }
            }
            Order::Capture(city) => {
                self.captured.insert(city);
                self.units[actor].movement = 0;
                Command::UnitCaptureCity { player, unit, city }
            }
            Order::Hold => {
                self.units[actor].movement = 0;
                Command::UnitHold { player, unit }
            }
        };
        self.commands.push(command);
    }
}

fn search(
    observation: &Observation,
    operations: &mut Vec<Operation>,
    width: usize,
    mut rng: Option<NamedRng>,
    version: PlannerVersion,
) -> Vec<Command> {
    let units = observation.visible_units();
    let mut actors: Vec<_> = units
        .iter()
        .enumerate()
        .filter(|(_, unit)| unit.owner == Some(observation.seat()) && unit.kind.is_military())
        .collect();
    if version == PlannerVersion::V2 {
        // Fire support resolves before melee assault; IDs break ties within roles.
        actors.sort_by_key(|(_, unit)| std::cmp::Reverse(unit.kind.range()));
    }
    if actors.is_empty() {
        return Vec::new();
    }
    let field = Battlefield::new(observation, operations, version);
    let military = units
        .iter()
        .enumerate()
        .filter(|(_, unit)| unit.kind.is_military())
        .map(|(index, unit)| (unit.tile, index))
        .collect();
    let mut beam = vec![Position {
        captured: BTreeSet::new(),
        units: units.to_vec(),
        military,
        commands: Vec::new(),
        score: 0,
    }];
    for (actor, unit) in actors {
        let range = field.attack_range(*unit);
        let mut ranges_after_move = BTreeMap::new();
        let mut candidates = Vec::new();
        for (index, position) in beam.iter().enumerate() {
            let options = position.options(actor, &field, &range);
            if let Some(stream) = &mut rng {
                let choice = stream
                    .next_bounded(options.len() as u64)
                    .expect("hold is always available");
                candidates.push((0, index, options[choice as usize].0, None));
            } else {
                for (order, utility) in options {
                    let score = position.score + i64::from(utility);
                    candidates.push((score, index, order, None));
                    if version == PlannerVersion::V2
                        && let Order::Move { destination, cost } = order
                        && cost < unit.movement
                        && (!field.threat_distance.is_empty()
                            || field.assault_distance.get(&destination) == Some(&0))
                    {
                        // A move can leave enough movement for one terminal strike.
                        // Evaluate on the same position that dispatch will see.
                        let moved = VisibleUnitObservation {
                            tile: destination,
                            movement: unit.movement - cost,
                            ..*unit
                        };
                        // Firing range can cross impassable terrain. Geometry is
                        // shared across beam positions, target occupancy is not.
                        let range = ranges_after_move
                            .entry(destination)
                            .or_insert_with(|| field.attack_range(moved));
                        for (follow, value) in position.strikes(moved, &field, range) {
                            match follow {
                                Order::Attack { .. } => {}
                                Order::Capture(city) if field.scouted.contains(&city) => {}
                                _ => continue,
                            }
                            // Prefer a direct strike when moving adds no combat value.
                            candidates.push((
                                position.score + i64::from(value + utility.min(0)) - 1,
                                index,
                                order,
                                Some(follow),
                            ));
                        }
                    }
                }
            }
        }
        // Stable ties retain parent rank, then revealed tile order.
        candidates.sort_by_key(|(score, ..)| std::cmp::Reverse(*score));
        beam = candidates
            .into_iter()
            .take(width)
            .map(|(score, parent, order, follow)| {
                let mut next = beam[parent].clone();
                next.apply(actor, order, observation.seat());
                if let Some(order) = follow {
                    next.apply(actor, order, observation.seat());
                }
                next.score = score;
                next
            })
            .collect();
    }
    let commands = beam.remove(0).commands;
    operations.extend(commands.iter().map(|command| match command {
        Command::UnitAttack {
            attacker, defender, ..
        } => Operation::Attack(*attacker, *defender),
        Command::UnitMove { unit, .. }
        | Command::UnitCaptureCity { unit, .. }
        | Command::UnitHeal { unit, .. }
        | Command::UnitHold { unit, .. } => Operation::UnitAction(*unit),
        _ => unreachable!("only tactical orders enter the beam"),
    }));
    commands
}
