//! Play a secondary board through the same public commands as a host.

use histeries_hex::{FlatHex, TileId};
use histeries_rules_core::{CityId, PlayerId, UnitId};
use histeries_sim::{Command, DispatchError, MapState, ReplayLog, Session, UnitKind};

const A: PlayerId = PlayerId::new(1);
const B: PlayerId = PlayerId::new(2);

fn round(session: &mut Session) {
    for player in [A, B] {
        session.dispatch(Command::TurnEnd { player }).unwrap();
    }
}

fn launch(player: PlayerId, city: u32) -> Command {
    Command::TheatreLaunch {
        player,
        theatre: "orbit".into(),
        city: CityId::new(city),
    }
}

fn travel(player: PlayerId, tile: TileId) -> Command {
    Command::TheatreMove {
        player,
        theatre: "orbit".into(),
        tile,
    }
}

fn recall(player: PlayerId) -> Command {
    Command::TheatreRecall {
        player,
        theatre: "orbit".into(),
    }
}

fn rejected(session: &mut Session, command: Command, expected: DispatchError) {
    let save = session.save_bytes();
    let hash = session.state_hash();
    let events = session.events().len();
    let orders = session.replay().commands().len();
    assert_eq!(session.dispatch(command), Err(expected));
    assert_eq!(session.state_hash(), hash);
    assert_eq!(session.save_bytes(), save);
    assert_eq!(session.events().len(), events);
    assert_eq!(session.replay().commands().len(), orders);
}

fn research_orbit(session: &mut Session, player: PlayerId) {
    for i in 1..=20 {
        session
            .dispatch(Command::ResearchTechnology {
                player,
                technology: format!("tech_{i:02}"),
            })
            .unwrap();
        for _ in 0..100 {
            if session
                .observe(player)
                .unwrap()
                .progress()
                .current_technology
                .is_none()
            {
                break;
            }
            round(session);
        }
        assert!(
            session
                .observe(player)
                .unwrap()
                .progress()
                .current_technology
                .is_none()
        );
    }
}

fn two_cities() -> Session {
    let home = MapState::blank(FlatHex::new(5, 5, false, false).unwrap());
    let mut session = Session::with_map([A, B], home);
    for (player, index, site) in [(A, 0, 0), (B, 1, 24)] {
        let unit = UnitId::new(index);
        session
            .dispatch(Command::UnitSpawn {
                player,
                unit,
                tile: TileId::new(site),
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
    session
}

#[test]
fn expeditions_unlock_travel_return_and_remember_surveys_privately() {
    let mut session = two_cities();
    assert!(session.observe(A).unwrap().theatres().is_empty());
    rejected(
        &mut session,
        launch(A, 0),
        DispatchError::TheatreUnavailable,
    );
    research_orbit(&mut session, A);
    let before = session.observe(A).unwrap();
    let chart = &before.theatres()[0];
    assert!(chart.can_launch);
    assert_eq!(chart.tiles.len(), 1);
    assert!(!chart.tiles[0].visited && !chart.tiles[0].visible);
    let entry = chart.entry;
    assert!(session.observe(B).unwrap().theatres().is_empty());
    assert_eq!(session.save_bytes()[4..6], 5_u16.to_le_bytes());
    assert_eq!(session.replay().encode()[4..6], 5_u16.to_le_bytes());
    rejected(
        &mut session,
        launch(A, 1),
        DispatchError::UnauthorizedCity(CityId::new(1)),
    );
    rejected(
        &mut session,
        recall(A),
        DispatchError::InvalidExpeditionOrder,
    );
    session.dispatch(launch(A, 0)).unwrap();
    let launched = session.observe(A).unwrap();
    assert_eq!(launched.board(), before.board());
    let chart = &launched.theatres()[0];
    assert_eq!(chart.expedition, Some(entry));
    assert!(!chart.can_move && !chart.can_recall && !chart.can_launch);
    let neighbor = chart
        .tiles
        .iter()
        .find(|t| t.tile != entry && t.visible)
        .unwrap()
        .tile;
    rejected(
        &mut session,
        travel(A, neighbor),
        DispatchError::InvalidExpeditionOrder,
    );
    rejected(
        &mut session,
        recall(A),
        DispatchError::InvalidExpeditionOrder,
    );
    research_orbit(&mut session, B);
    assert!(
        session.observe(B).unwrap().theatres()[0]
            .tiles
            .iter()
            .all(|t| !t.visited)
    );
    session.dispatch(launch(B, 1)).unwrap();
    assert_eq!(
        session.observe(B).unwrap().theatres()[0].expedition,
        Some(entry)
    );
    round(&mut session);
    rejected(
        &mut session,
        travel(A, TileId::new(u32::MAX)),
        DispatchError::InvalidExpeditionOrder,
    );
    rejected(
        &mut session,
        travel(A, entry),
        DispatchError::InvalidExpeditionOrder,
    );
    rejected(
        &mut session,
        launch(A, 0),
        DispatchError::InvalidExpeditionOrder,
    );
    session.dispatch(travel(A, neighbor)).unwrap();
    session.dispatch(Command::TurnEnd { player: A }).unwrap();
    rejected(
        &mut session,
        travel(A, entry),
        DispatchError::TurnAlreadyEnded,
    );
    let mid = session.save_bytes();
    assert_eq!(mid[4..6], 6_u16.to_le_bytes());
    let mut restored = Session::from_save_bytes(&mid).unwrap();
    assert_eq!(restored.observe(A).unwrap(), session.observe(A).unwrap());
    assert_eq!(restored.save_bytes(), mid);
    for game in [&mut session, &mut restored] {
        game.dispatch(Command::TurnEnd { player: B }).unwrap();
        rejected(game, recall(A), DispatchError::InvalidExpeditionOrder);
        game.dispatch(travel(A, entry)).unwrap();
        rejected(game, recall(A), DispatchError::InvalidExpeditionOrder);
        round(game);
        assert!(game.observe(A).unwrap().theatres()[0].can_recall);
        game.dispatch(recall(A)).unwrap();
        rejected(game, launch(A, 0), DispatchError::InvalidExpeditionOrder);
        let observed = game.observe(A).unwrap();
        let chart = &observed.theatres()[0];
        assert_eq!(chart.expedition, None);
        assert_eq!(chart.tiles.iter().filter(|t| t.visited).count(), 2);
        assert!(chart.tiles.iter().all(|t| !t.visible));
        let other = game.observe(B).unwrap();
        assert_eq!(
            other.theatres()[0]
                .tiles
                .iter()
                .filter(|t| t.visited)
                .count(),
            1
        );
        assert_eq!(other.theatres()[0].expedition, Some(entry));
        round(game);
        assert!(game.observe(A).unwrap().theatres()[0].can_launch);
        game.dispatch(launch(A, 0)).unwrap();
    }
    assert_eq!(restored.state_hash(), session.state_hash());
    for game in [&session, &restored] {
        let replay = game.replay().encode();
        assert_eq!(replay[4..6], 6_u16.to_le_bytes());
        let replayed = ReplayLog::decode(&replay).unwrap().replay().unwrap();
        assert_eq!(replayed.state_hash(), game.state_hash());
        assert_eq!(replayed.encode(), game.save_bytes());
    }
}

#[test]
fn malformed_expedition_documents_are_rejected() {
    let mut session = two_cities();
    research_orbit(&mut session, A);
    session.dispatch(launch(A, 0)).unwrap();
    let save = session.save_bytes();
    // Last three u32 fields are current site, visited count and visited site.
    let n = save.len();
    for (offset, invalid) in [(n - 4, 31), (n - 8, 0), (n - 12, 30), (n - 17, u32::MAX)] {
        let mut corrupted = save.clone();
        corrupted[offset..offset + 4].copy_from_slice(&invalid.to_le_bytes());
        assert!(Session::from_save_bytes(&corrupted).is_err());
    }
    let mut downgraded = save;
    downgraded[4..6].copy_from_slice(&5_u16.to_le_bytes());
    assert!(Session::from_save_bytes(&downgraded).is_err());
    let mut replay = session.replay().encode();
    replay[4..6].copy_from_slice(&5_u16.to_le_bytes());
    assert!(ReplayLog::decode(&replay).is_err());
    for command in [launch(A, 0), travel(A, TileId::new(3)), recall(A)] {
        let mut raw = command.encode();
        assert_eq!(Command::decode(&raw), Ok(command));
        assert!(Command::decode(&raw[..raw.len() - 1]).is_err());
        raw[4..6].copy_from_slice(&3_u16.to_le_bytes());
        assert!(Command::decode(&raw).is_err());
    }
}
