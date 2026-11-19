//! Greedy opponents for seat-scoped interactive matches.
use crate::{GreedyPlayer, Player, plan_seats};
use histeries_rules_core::PlayerId;
use histeries_sim::{Command, MatchHost, Observation};

/// Plans each pending opponent from its own observation and applies orders in seat order.
pub fn advance_greedy_opponents(host: &mut MatchHost, bots: &[PlayerId]) -> Result<(), String> {
    if bots.is_empty() || host.session().winner().is_some() {
        return Ok(());
    }
    let round = host.session().turn();
    let mut controllers = vec![GreedyPlayer; bots.len()];
    let mut seats: Vec<(&mut (dyn Player + Send), Observation)> = Vec::new();
    for (&seat, controller) in bots.iter().zip(&mut controllers) {
        if !host.session().has_ended_turn(seat) {
            let view = host.session().observe(seat).map_err(|e| e.to_string())?;
            seats.push((controller, view));
        }
    }
    // plan_seats sorts the seat/observation pairs and returns the plans in that order.
    let plans = plan_seats(&mut seats, bots.len());
    debug_assert_eq!(seats.len(), plans.len());
    for ((_, view), plan) in seats.into_iter().zip(plans) {
        if host.session().turn() != round {
            break;
        }
        if host.session().has_ended_turn(view.seat()) {
            continue;
        }
        for order in plan.commands {
            if host.session().winner().is_some() {
                return Ok(());
            }
            // Earlier orders may have invalidated the remaining plan. A rejected
            // tactical order never prevents the opponent handing the turn back.
            let ending = matches!(order, Command::TurnEnd { .. });
            let applied = host.dispatch(view.seat(), order);
            if ending {
                applied?;
            }
        }
    }
    Ok(())
}
