//! Selects reachable city sites from a settler's visible neighborhood.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use histeries_hex::TileId;
use histeries_sim::{
    Command, Observation, Unit, UnitKind, UnitObservation, VisibleTileObservation,
};

use crate::{Operation, operational::Policy};

pub(crate) fn plan_settlers(
    observation: &Observation,
    operations: &mut Vec<Operation>,
    policy: &mut Policy<'_>,
) -> (Vec<Command>, BTreeSet<TileId>) {
    let player = observation.seat();
    let mut planned = Vec::new();
    let mut sites = BTreeSet::new();
    let mut settlers = observation
        .units()
        .iter()
        .filter(|unit| unit.kind == UnitKind::Settler)
        .peekable();
    if settlers.peek().is_none() {
        return (planned, sites);
    }
    let tiles: BTreeMap<_, _> = observation
        .visible_tiles()
        .iter()
        .map(|tile| (tile.tile, tile))
        .collect();
    let enemy_zoc = observation
        .visible_units()
        .iter()
        .filter(|unit| unit.owner != Some(player) && unit.kind.is_military())
        .flat_map(|unit| tiles[&unit.tile].neighbors.iter().copied())
        .collect();
    let mut field = SettlementMap {
        tiles,
        enemy_zoc,
        cities: observation.cities().iter().map(|city| city.tile).collect(),
        routes: BTreeSet::new(),
    };
    for settler in settlers {
        let Some(path) = field.choose(settler, policy) else {
            continue;
        };
        let site = path.last().copied().unwrap_or(settler.tile);
        for destination in path {
            planned.push(Command::UnitMove {
                player,
                unit: settler.id,
                destination,
            });
            field.routes.insert(destination);
        }
        if site != settler.tile {
            operations.push(Operation::SettlementSite(settler.id, site));
        }
        operations.push(Operation::Settle(settler.id));
        planned.push(Command::CitySettle {
            player,
            unit: settler.id,
        });
        sites.insert(site);
        field.cities.insert(site);
    }
    (planned, sites)
}

struct SettlementMap<'a> {
    tiles: BTreeMap<TileId, &'a VisibleTileObservation>,
    enemy_zoc: BTreeSet<TileId>,
    cities: BTreeSet<TileId>,
    routes: BTreeSet<TileId>,
}

impl SettlementMap<'_> {
    fn choose(&self, settler: &UnitObservation, policy: &mut Policy<'_>) -> Option<Vec<TileId>> {
        let choices = self.reachable_sites(settler);
        let (_, path) = match policy {
            Policy::Layered(_) => choices
                .into_iter()
                .rev()
                .max_by_key(|(site, _)| self.site_value(*site, settler.tile))?,
            // All new cities give identical immediate yields on the current ruleset.
            Policy::Greedy => choices
                .into_iter()
                .min_by_key(|(site, path)| (path.len(), *site))?,
            Policy::Random(rng) => {
                let index = rng.next_bounded(choices.len() as u64)? as usize;
                choices.into_iter().nth(index)?
            }
        };
        Some(path)
    }

    // Breadth-first order preserves the shortest route and canonical neighbor tie
    // break for each site. Other routes can still reach further sites at lower cost.
    fn reachable_sites(&self, settler: &UnitObservation) -> Vec<(TileId, Vec<TileId>)> {
        let mut frontier = VecDeque::from([(settler.tile, settler.movement, Vec::new())]);
        let mut seen = BTreeSet::new();
        let mut choices = Vec::new();
        while let Some((at, movement, path)) = frontier.pop_front() {
            let Some(tile) = self.tiles.get(&at) else {
                continue;
            };
            if !tile.district_reserved
                && Unit::movement_cost(tile.terrain, tile.road).is_some()
                && seen.insert(at)
            {
                choices.push((at, path.clone()));
            }
            if movement == 0 || path.len() == 2 {
                continue;
            }
            for next in &tile.neighbors {
                let Some(dest) = self.tiles.get(next) else {
                    continue;
                };
                let Some(cost) = Unit::movement_cost(dest.terrain, dest.road) else {
                    continue;
                };
                if cost > movement
                    || dest.occupied
                    || self.routes.contains(next)
                    || path.contains(next)
                    || (self.enemy_zoc.contains(&at) && !self.enemy_zoc.contains(next))
                {
                    continue;
                }
                let mut route = path.clone();
                route.push(*next);
                frontier.push_back((*next, movement - cost, route));
            }
        }
        choices
    }

    fn open_for_expansion(&self, tile: TileId, origin: TileId) -> bool {
        self.tiles.get(&tile).is_some_and(|entry| {
            (tile == origin || !entry.occupied)
                && !self.cities.contains(&tile)
                && !entry.district_reserved
                && Unit::movement_cost(entry.terrain, entry.road).is_some()
        })
    }

    fn site_value(&self, site: TileId, origin: TileId) -> i32 {
        let mut value = 0;
        for neighbor in &self.tiles[&site].neighbors {
            if self.open_for_expansion(*neighbor, origin) {
                value += 2;
                // A new city can colonize land that existing cities cannot reach.
                if self.tiles[neighbor]
                    .neighbors
                    .iter()
                    .all(|tile| !self.cities.contains(tile))
                {
                    value += 6;
                }
            }
            if self.cities.contains(neighbor)
                && self.tiles[neighbor]
                    .neighbors
                    .iter()
                    .all(|tile| *tile == site || !self.open_for_expansion(*tile, origin))
            {
                // Founding here prevents another city from producing its next settler.
                value -= 12;
            }
        }
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tactical;
    use histeries_hex::FlatHex;
    use histeries_rules_core::{CityId, PlayerId, UnitId};
    use histeries_sim::{MapState, Session, Terrain};

    const PLAYER: PlayerId = PlayerId::new(1);
    const OTHER: PlayerId = PlayerId::new(2);

    fn map(width: u32) -> Session {
        Session::with_map(
            [PLAYER, OTHER],
            MapState::blank(FlatHex::new(width, 1, false, false).unwrap()),
        )
    }

    fn spawn(session: &mut Session, player: PlayerId, id: u32, kind: UnitKind, tile: u32) {
        session
            .dispatch(Command::UnitSpawn {
                player,
                unit: UnitId::new(id),
                kind,
                tile: TileId::new(tile),
            })
            .unwrap();
    }

    fn capital(session: &mut Session) {
        spawn(session, PLAYER, 0, UnitKind::Settler, 0);
        session
            .dispatch(Command::CityFound {
                player: PLAYER,
                city: CityId::new(0),
                unit: UnitId::new(0),
            })
            .unwrap();
    }

    #[test]
    fn opens_a_second_frontier_and_leaves_the_capitals_exit_clear() {
        for (terrain, expected) in [(Terrain::Plains, 3), (Terrain::Hill, 2)] {
            let mut session = map(7);
            capital(&mut session);
            spawn(&mut session, PLAYER, 1, UnitKind::Settler, 1);
            spawn(&mut session, PLAYER, 2, UnitKind::Warrior, 5);
            session
                .dispatch(Command::EditorSetTerrain {
                    player: PLAYER,
                    tile: TileId::new(2),
                    terrain,
                })
                .unwrap();
            let observation = session.observe(PLAYER).unwrap();
            let mut operations = Vec::new();
            let (commands, sites) = plan_settlers(
                &observation,
                &mut operations,
                &mut Policy::Layered(crate::VictoryObjective::Science),
            );
            assert_eq!(sites, BTreeSet::from([TileId::new(expected)]));
            assert!(operations.contains(&Operation::SettlementSite(
                UnitId::new(1),
                TileId::new(expected)
            )));
            for command in commands {
                session.dispatch(command).unwrap();
            }
            let after = session.observe(PLAYER).unwrap();
            assert_eq!(after.cities()[1].tile, TileId::new(expected));
            assert!(
                !after
                    .visible_tiles()
                    .iter()
                    .find(|tile| tile.tile == TileId::new(1))
                    .unwrap()
                    .occupied
            );
        }
    }

    #[test]
    fn multiple_settlers_plan_disjoint_routes_and_respect_enemy_control() {
        let mut session = map(9);
        spawn(&mut session, PLAYER, 1, UnitKind::Settler, 1);
        spawn(&mut session, OTHER, 2, UnitKind::Warrior, 0);
        spawn(&mut session, PLAYER, 3, UnitKind::Settler, 5);
        spawn(&mut session, PLAYER, 4, UnitKind::Settler, 7);
        let observation = session.observe(PLAYER).unwrap();
        let mut operations = Vec::new();
        let (commands, sites) = plan_settlers(
            &observation,
            &mut operations,
            &mut Policy::Layered(crate::VictoryObjective::Science),
        );
        assert_eq!(sites.len(), 3);
        assert!(sites.contains(&TileId::new(1))); // Enemy control forbids leaving via tile two.
        let mut routes = BTreeSet::new();
        for command in commands {
            if let Command::UnitMove { destination, .. } = &command {
                assert!(routes.insert(*destination));
                assert!(
                    !observation
                        .visible_tiles()
                        .iter()
                        .find(|tile| tile.tile == *destination)
                        .unwrap()
                        .occupied
                );
            }
            session.dispatch(command).unwrap();
        }
        assert_eq!(session.observe(PLAYER).unwrap().cities().len(), 3);
    }

    #[test]
    fn greedy_moves_off_a_reserved_district_and_founds_immediately() {
        let mut session = map(6);
        capital(&mut session);
        session
            .dispatch(Command::CityBuildDistrict {
                player: PLAYER,
                city: CityId::new(0),
                kind: histeries_sim::DistrictKind::Campus,
                tile: TileId::new(2),
            })
            .unwrap();
        spawn(&mut session, PLAYER, 1, UnitKind::Settler, 3);
        session
            .dispatch(Command::UnitMove {
                player: PLAYER,
                unit: UnitId::new(1),
                destination: TileId::new(2),
            })
            .unwrap();
        let observation = session.observe(PLAYER).unwrap();
        let mut operations = Vec::new();
        let commands = crate::operational::greedy(&observation, &mut operations);
        assert!(operations.contains(&Operation::SettlementSite(UnitId::new(1), TileId::new(1))));
        for command in commands {
            session.dispatch(command).unwrap();
        }
        assert_eq!(
            session.observe(PLAYER).unwrap().cities()[1].tile,
            TileId::new(1)
        );
    }

    #[test]
    fn random_samples_sites_once_even_when_several_paths_lead_to_a_site() {
        use histeries_hex::TileTopology;
        use histeries_rules_core::NamedRng;

        let topology = FlatHex::new(3, 3, false, false).unwrap();
        let mut session = Session::with_map([PLAYER, OTHER], MapState::blank(topology));
        let start = TileId::new(4);
        spawn(&mut session, PLAYER, 1, UnitKind::Settler, start.index());
        let observation = session.observe(PLAYER).unwrap();
        let neighbors = topology.neighbors(start);
        let mut expected = vec![start];
        expected.extend(&neighbors);
        for first in neighbors {
            for site in topology.neighbors(first) {
                if !expected.contains(&site) {
                    expected.push(site);
                }
            }
        }
        assert_eq!(expected.len(), 9);
        for seed in 0..64 {
            let mut rng = NamedRng::new([seed; 32], "settlement");
            let mut reference = rng.clone();
            let slot = reference.next_bounded(expected.len() as u64).unwrap() as usize;
            let (_, sites) =
                plan_settlers(&observation, &mut Vec::new(), &mut Policy::Random(&mut rng));
            assert_eq!(sites, BTreeSet::from([expected[slot]]));
        }
    }

    #[test]
    fn tactical_moves_leave_the_new_city_site_clear() {
        let mut session = map(3);
        spawn(&mut session, PLAYER, 1, UnitKind::Warrior, 0);
        spawn(&mut session, PLAYER, 2, UnitKind::Settler, 2);
        let observation = session.observe(PLAYER).unwrap();
        let mut operations = vec![Operation::SettlementSite(UnitId::new(2), TileId::new(1))];
        let commands = tactical::plan(&observation, &mut operations);
        assert_eq!(
            commands,
            [Command::UnitHold {
                player: PLAYER,
                unit: UnitId::new(1)
            }]
        );
    }
}
