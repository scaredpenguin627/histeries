//! Deterministic players that plan from a single seat's observation.

#![forbid(unsafe_code)]

use histeries_rules_core::{CityId, NamedRng, PlayerId, UnitId};
use histeries_ruleset::base_ruleset;
use histeries_sim::{BuildingKind, Command, Observation, ProductionItem};

pub mod operational;
mod settlement;
pub mod strategic;
pub mod tactical;

/// The victory path selected for a seat.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VictoryObjective {
    /// Complete the technology tree.
    Science,
    /// Secure sole control of the world's cities.
    Domination,
    /// No current victory objective.
    Adaptive,
}

/// A planned task or an immediate order explained by the planner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Operation {
    /// Found a new city with an owned settler.
    Settle(UnitId),
    /// Reserve a visible destination for a settler moving before it founds a city.
    SettlementSite(UnitId, histeries_hex::TileId),
    /// Develop prerequisites until the city can complete the requested building.
    DevelopCity(CityId, BuildingKind),
    /// Accumulate science toward the selected technology.
    ResearchTechnology(String),
    /// Accumulate culture toward the selected civic.
    ResearchCivic(String),
    /// Adopt a policy in an unlocked slot.
    AdoptPolicy(u8, String),
    /// Produce one item in a city.
    Produce(CityId, ProductionItem),
    /// Attack a visible military unit.
    Attack(UnitId, UnitId),
    /// Give an explicit non-attack order to a unit.
    UnitAction(UnitId),
}

/// An ordered stream of commands and the reasons the seat selected them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlayerPlan {
    /// The seat's current objective.
    pub objective: VictoryObjective,
    /// Operational goals and unit orders.
    pub operations: Vec<Operation>,
    /// Commands in the order evaluated by the planner, ending with `turn.end`.
    pub commands: Vec<Command>,
}

/// A controller belongs to one seat for the duration of a game.
pub trait Player: Send {
    /// Plan using only information available to the seat.
    fn plan(&mut self, observation: &Observation) -> PlayerPlan;
}

/// A utility planner with an incumbent objective, an operational HTN and tactical beam search.
#[derive(Clone, Debug, Default)]
pub struct ThreeLayerPlayer {
    incumbent: Option<(PlayerId, VictoryObjective)>,
}

impl ThreeLayerPlayer {
    /// Returns the last seat and objective for a self-play checkpoint.
    pub fn incumbent(&self) -> Option<(PlayerId, VictoryObjective)> {
        self.incumbent
    }

    /// Restores the only controller memory needed between turns.
    pub fn resume(seat: PlayerId, objective: VictoryObjective) -> Self {
        Self {
            incumbent: Some((seat, objective)),
        }
    }
}

impl Player for ThreeLayerPlayer {
    fn plan(&mut self, observation: &Observation) -> PlayerPlan {
        let previous = self
            .incumbent
            .filter(|(seat, _)| *seat == observation.seat())
            .map(|(_, objective)| objective);
        let objective = strategic::choose_objective(observation, previous);
        self.incumbent = Some((observation.seat(), objective));
        let mut operations = Vec::new();
        let mut commands = Vec::new();
        if observation.victory().is_none() {
            commands = operational::plan(observation, objective, &mut operations);
            commands.extend(tactical::plan(observation, &mut operations));
        }
        finish(observation, objective, operations, commands)
    }
}

/// Myopic baseline using immediate legal benefits and a single tactical choice per unit.
#[derive(Clone, Copy, Debug, Default)]
pub struct GreedyPlayer;

impl Player for GreedyPlayer {
    fn plan(&mut self, observation: &Observation) -> PlayerPlan {
        let mut operations = Vec::new();
        let mut commands = Vec::new();
        if observation.victory().is_none() {
            commands = operational::greedy(observation, &mut operations);
            commands.extend(tactical::greedy(observation, &mut operations));
        }
        finish(
            observation,
            VictoryObjective::Adaptive,
            operations,
            commands,
        )
    }
}

/// Seeded baseline sampling legal actions using the seat's named random stream.
#[derive(Clone, Copy, Debug, Default)]
pub struct RandomPlayer {
    seed: u64,
}

impl RandomPlayer {
    /// Creates a baseline for the specified game seed.
    pub const fn new(seed: u64) -> Self {
        Self { seed }
    }
}

impl Player for RandomPlayer {
    fn plan(&mut self, observation: &Observation) -> PlayerPlan {
        let mut operations = Vec::new();
        let mut commands = Vec::new();
        if observation.victory().is_none() {
            let mut seed = [0; 32];
            seed[..8].copy_from_slice(&self.seed.to_le_bytes());
            seed[8..12].copy_from_slice(&observation.turn().to_le_bytes());
            let mut rng = NamedRng::new(
                seed,
                &format!("player/{}/random", observation.seat().index()),
            );
            commands = operational::random(observation, &mut rng, &mut operations);
            commands.extend(tactical::random(
                observation,
                &mut operations,
                rng.next_u64(),
            ));
        }
        finish(
            observation,
            VictoryObjective::Adaptive,
            operations,
            commands,
        )
    }
}

fn finish(
    observation: &Observation,
    objective: VictoryObjective,
    operations: Vec<Operation>,
    mut commands: Vec<Command>,
) -> PlayerPlan {
    commands.push(Command::TurnEnd {
        player: observation.seat(),
    });
    PlayerPlan {
        objective,
        operations,
        commands,
    }
}

/// Plans one immutable round in ascending seat order, with at most `workers` threads.
/// Callers supply one controller and one observation per distinct seat.
pub fn plan_seats(
    seats: &mut [(&mut (dyn Player + Send), Observation)],
    workers: usize,
) -> Vec<PlayerPlan> {
    seats.sort_by_key(|(_, observation)| observation.seat());
    #[cfg(not(target_family = "wasm"))]
    if workers > 1 && seats.len() > 1 {
        let chunk_size = seats.len().div_ceil(workers);
        return std::thread::scope(|scope| {
            let jobs: Vec<_> = seats
                .chunks_mut(chunk_size)
                .map(|chunk| scope.spawn(move || plan_chunk(chunk)))
                .collect();
            jobs.into_iter()
                .flat_map(|job| job.join().expect("seat planner panicked"))
                .collect()
        });
    }
    #[cfg(target_family = "wasm")]
    let _ = workers;
    plan_chunk(seats)
}

fn plan_chunk(seats: &mut [(&mut (dyn Player + Send), Observation)]) -> Vec<PlayerPlan> {
    seats
        .iter_mut()
        .map(|(player, observation)| player.plan(observation))
        .collect()
}
