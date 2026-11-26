//! Produce a playable late-game scenario by issuing ordinary research and turn orders.
#![forbid(unsafe_code)]

use histeries_ruleset::base_ruleset;
use histeries_sim::{BoardTopology, Command, seeded_frontier_on};
use std::{env, error::Error, fs};

fn main() -> Result<(), Box<dyn Error>> {
    let output = env::args_os()
        .nth(1)
        .ok_or("expected a destination .hst file")?;
    let mut session = seeded_frontier_on(7, 2, BoardTopology::geodesic(3)?)?;
    let seats: Vec<_> = session.seat_ids().collect();
    let player = seats[0];
    let rules = base_ruleset();
    let theatre = rules
        .theatre("orbit")
        .ok_or("missing Orbital Survey board")?;
    loop {
        let view = session.observe(player)?;
        let progress = view.progress();
        if progress
            .age
            .as_ref()
            .is_some_and(|age| age.unlocks.iter().any(|id| id == theatre.required_unlock()))
        {
            fs::write(output, session.save_bytes())?;
            println!(
                "round={} seat={} hash={:016x}",
                session.turn(),
                player.index(),
                session.state_hash().value()
            );
            return Ok(());
        }
        if session.turn() > 1000 || session.winner().is_some() {
            return Err("match ended before Orbital Survey opened".into());
        }
        if progress.current_technology.is_none() {
            let next = rules
                .technologies()
                .find(|node| {
                    !progress.technologies.iter().any(|id| id == node.id())
                        && node
                            .prerequisites()
                            .iter()
                            .all(|id| progress.technologies.contains(id))
                })
                .ok_or("no reachable technology remains")?;
            session.dispatch(Command::ResearchTechnology {
                player,
                technology: next.id().to_owned(),
            })?;
        }
        for &seat in &seats {
            session.dispatch(Command::TurnEnd { player: seat })?;
        }
    }
}
