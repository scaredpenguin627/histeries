//! Seat-scoped age progression through commands, save/load, and replay.

use histeries_hex::{FlatHex, TileId};
use histeries_rules_core::{CityId, PlayerId, UnitId};
use histeries_sim::{Command, DispatchError, MapState, ReplayLog, Session, UnitKind};

#[test]
fn research_reaches_ages_and_unlocks_policy_only_for_the_researching_seat() {
    let first = PlayerId::new(1);
    let other = PlayerId::new(2);
    let mut session = Session::with_map(
        [first, other],
        MapState::blank(FlatHex::new(5, 5, false, false).unwrap()),
    );
    for (index, player, tile) in [(0, first, 0), (1, other, 24)] {
        let unit = UnitId::new(index);
        session
            .dispatch(Command::UnitSpawn {
                player,
                unit,
                tile: TileId::new(tile),
                kind: UnitKind::Settler,
            })
            .unwrap();
        session
            .dispatch(Command::CityFound {
                player,
                unit,
                city: CityId::new(index),
            })
            .unwrap();
    }
    let policy = |player| Command::PolicyAdopt {
        player,
        slot: 0,
        policy: "policy_orbital_research".to_owned(),
    };
    let mut round_hashes = Vec::new();
    for number in 1..=20 {
        let tech = format!("tech_{number:02}");
        let research = Command::ResearchTechnology {
            player: first,
            technology: tech,
        };
        let mut loaded = Session::from_save_bytes(&session.save_bytes()).unwrap();
        session.dispatch(research.clone()).unwrap();
        loaded.dispatch(research).unwrap();
        while session
            .observe(first)
            .unwrap()
            .progress()
            .current_technology
            .is_some()
        {
            assert!(session.turn() < 1000);
            for player in [first, other] {
                let end = Command::TurnEnd { player };
                session.dispatch(end.clone()).unwrap();
                loaded.dispatch(end).unwrap();
            }
            assert_eq!(loaded.state_hash(), session.state_hash());
            round_hashes.push(session.state_hash());
        }
        let observed = session.observe(first).unwrap();
        assert_eq!(loaded.observe(first).unwrap(), observed);
        let age = observed.progress().age.as_ref().unwrap();
        let expected = match number {
            1..=11 => "founding",
            12..=19 => "industrial",
            _ => "orbital",
        };
        assert_eq!(age.id, expected);
        let available = observed
            .progress()
            .available_policies
            .iter()
            .any(|id| id == "policy_orbital_research");
        assert_eq!(available, number == 20);
        if number < 20 {
            assert_eq!(
                session.dispatch(policy(first)),
                Err(DispatchError::MissingPrerequisite)
            );
        } else {
            assert_eq!(age.name, "Orbital Age");
            assert_eq!(age.unlocks, ["orbital_access"]);
            session.dispatch(policy(first)).unwrap();
        }
    }
    let other_view = session.observe(other).unwrap();
    let other_age = other_view.progress().age.as_ref().unwrap();
    assert_eq!(other_age.id, "founding");
    assert!(other_age.unlocks.is_empty());
    assert!(other_view.progress().available_policies.is_empty());
    assert_eq!(
        session.dispatch(policy(other)),
        Err(DispatchError::MissingPrerequisite)
    );

    let recorded = ReplayLog::decode(&session.replay().encode()).unwrap();
    let mut playback = recorded.playback_session();
    let mut hashes = round_hashes.into_iter();
    for command in recorded.commands() {
        let before = playback.turn();
        playback.dispatch(command.clone()).unwrap();
        if playback.turn() != before {
            assert_eq!(Some(playback.state_hash()), hashes.next());
        }
    }
    assert_eq!(hashes.next(), None);
    assert_eq!(playback.save_bytes(), session.save_bytes());
    assert_eq!(
        playback.observe(first).unwrap(),
        session.observe(first).unwrap()
    );
}
