//! A local player's view of a match hosted by the simulation.
use histeries_ai::advance_greedy_opponents;
use histeries_hex::{Embedding, FlatHex, TileId};
use histeries_net::LockstepHost;
use histeries_rules_core::PlayerId;
use histeries_sim::{
    Command, MatchHost, MatchView, Observation, Session, VictoryKind, Viewpoint, frontier_duel,
    generate_map, seeded_frontier,
};

enum Players {
    Solo {
        host: MatchHost,
        opponents: Vec<PlayerId>,
    },
    Hotseat {
        host: LockstepHost,
        handoff: Option<PlayerId>,
    },
}

/// A locally hosted match with a cached observation for the human player in control.
pub struct ClientSession {
    players: Players,
    observation: Observation,
}

impl ClientSession {
    /// Opens a generated frontier with two equally equipped seats.
    pub fn new(seed: u64) -> Result<Self, String> {
        let topology = FlatHex::new(12, 9, false, false).map_err(|e| e.to_string())?;
        let mut seed_bytes = [0; 32];
        for lane in seed_bytes.as_chunks_mut::<8>().0 {
            lane.copy_from_slice(&seed.to_le_bytes());
        }
        Self::from_session(frontier_duel(generate_map(topology, seed_bytes))?)
    }

    /// Creates a frontier shared by two to eight local human players.
    pub fn hotseat(seed: u64, players: u32) -> Result<Self, String> {
        Self::from_hotseat(seeded_frontier(seed, players)?)
    }

    /// Resumes human seat rotation from the turn completions in an ordinary save.
    pub fn hotseat_from_save(bytes: &[u8]) -> Result<Self, String> {
        Self::from_hotseat(Session::from_save_bytes(bytes).map_err(|e| e.to_string())?)
    }

    fn from_hotseat(session: Session) -> Result<Self, String> {
        let host = LockstepHost::new(session)?;
        let handoff = host.active_seat();
        let player = handoff
            .or_else(|| host.session().winner().map(|(id, _)| id))
            .ok_or("no active or winning player")?;
        let observation = host.session().observe(player).map_err(|e| e.to_string())?;
        Ok(Self {
            players: Players::Hotseat { host, handoff },
            observation,
        })
    }

    /// Loads an ordinary simulation save. The lowest seat ID belongs to the human.
    pub fn from_save_bytes(bytes: &[u8]) -> Result<Self, String> {
        Self::from_session(Session::from_save_bytes(bytes).map_err(|e| e.to_string())?)
    }

    fn from_session(session: Session) -> Result<Self, String> {
        let mut seats = session.seat_ids();
        let player = seats.next().ok_or("the save contains no players")?;
        let opponents: Vec<_> = seats.collect();
        let mut host = MatchHost::new(session);
        if host.session().has_ended_turn(player) {
            advance_greedy_opponents(&mut host, &opponents)?;
        }
        let observation = host.session().observe(player).map_err(|e| e.to_string())?;
        Ok(Self {
            players: Players::Solo { host, opponents },
            observation,
        })
    }

    fn host(&self) -> &MatchHost {
        match &self.players {
            Players::Solo { host, .. } => host,
            Players::Hotseat { host, .. } => host.host(),
        }
    }

    /// Whether this match has shared human controls.
    pub fn is_hotseat(&self) -> bool {
        matches!(self.players, Players::Hotseat { .. })
    }

    /// Seat awaiting explicit confirmation before the board may be presented.
    pub fn handoff_seat(&self) -> Option<PlayerId> {
        match &self.players {
            Players::Hotseat { handoff, .. } => *handoff,
            Players::Solo { .. } => None,
        }
    }

    /// Confirms that the next player has taken the controls.
    pub fn confirm_handoff(&mut self) {
        if let Players::Hotseat { handoff, .. } = &mut self.players {
            *handoff = None;
        }
    }

    /// Encodes the current position as a versioned simulation save.
    pub fn save_bytes(&self) -> Vec<u8> {
        self.host().session().save_bytes()
    }

    /// Returns the current seat's board and private economy.
    pub fn observation(&self) -> &Observation {
        &self.observation
    }

    /// Looks up a visible tile's topology-provided rendering center.
    pub fn embedding(&self, tile: TileId) -> Option<Embedding> {
        let tiles = self.observation.visible_tiles();
        let index = tiles.binary_search_by_key(&tile, |entry| entry.tile).ok()?;
        Some(tiles[index].embedding)
    }

    /// Returns the publicly declared outcome, including victories by other seats.
    pub fn winner(&self) -> Option<(PlayerId, VictoryKind)> {
        self.host().session().winner()
    }

    /// Reconstructs this seat's view at a retained turn boundary.
    pub fn replay(&self, turn: u32) -> Result<Observation, String> {
        let view = self
            .host()
            .replay(Viewpoint::Seat(self.observation.seat()), turn)?;
        match view.view {
            MatchView::Seat(seat) => Ok(*seat),
            MatchView::Spectator { .. } => Err("unexpected spectator projection".into()),
        }
    }

    /// Submits a typed seat order, then refreshes the view from the host.
    pub fn dispatch(&mut self, command: Command) -> Result<(), String> {
        let seat = self.observation.seat();
        let ending = matches!(command, Command::TurnEnd { .. });
        let (next, result) = match &mut self.players {
            Players::Solo { host, opponents } => {
                host.dispatch(seat, command)?;
                let result = if ending {
                    advance_greedy_opponents(host, opponents)
                } else {
                    Ok(())
                };
                (seat, result)
            }
            Players::Hotseat { host, handoff } => {
                if handoff.is_some() {
                    return Err("The next player must confirm the handoff.".into());
                }
                host.submit(seat, host.position(), command)?;
                let next = host.active_seat();
                if ending {
                    *handoff = next;
                }
                (next.unwrap_or(seat), Ok(()))
            }
        };
        self.observation = self
            .host()
            .session()
            .observe(next)
            .map_err(|e| e.to_string())?;
        result
    }

    /// Completes this player's orders for the round.
    pub fn end_turn(&mut self) -> Result<(), String> {
        self.dispatch(Command::TurnEnd {
            player: self.observation.seat(),
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use histeries_rules_core::UnitId;
    use histeries_sim::{MapState, ProductionItem, UnitKind};

    #[test]
    fn generated_game_round_trips_mid_turn_and_opponents_advance_deterministically() {
        let mut game = ClientSession::new(7).unwrap();
        let opening = game.observation().clone();
        let seat = opening.seat();
        let unit = opening.units()[0];
        assert_eq!(opening.cities().len(), 1);
        assert!(opening.visible_tiles().len() < 12 * 9);
        assert!(game.embedding(unit.tile).is_some());
        let hidden_capital = game
            .host()
            .session()
            .observe(PlayerId::new(2))
            .unwrap()
            .cities()[0]
            .tile;
        assert_eq!(game.embedding(hidden_capital), None);
        assert!(
            game.dispatch(Command::TurnEnd {
                player: PlayerId::new(2)
            })
            .is_err()
        );
        assert_eq!(
            game.dispatch(Command::UnitMove {
                player: seat,
                unit: unit.id,
                destination: hidden_capital
            })
            .unwrap_err(),
            "target is unavailable to this seat"
        );
        assert!(
            game.dispatch(Command::EditorSetMarker {
                player: seat,
                value: 100
            })
            .is_err()
        );
        assert_eq!(game.observation(), &opening);
        game.dispatch(Command::CitySetProduction {
            player: seat,
            city: opening.cities()[0].id,
            item: ProductionItem::Unit(UnitKind::Settler),
        })
        .unwrap();
        game.dispatch(Command::ResearchTechnology {
            player: seat,
            technology: "tech_01".into(),
        })
        .unwrap();
        let mut loaded = ClientSession::from_save_bytes(&game.save_bytes()).unwrap();
        assert_eq!(loaded.observation(), game.observation());
        for _ in 0..3 {
            game.end_turn().unwrap();
            loaded.end_turn().unwrap();
            assert_eq!(loaded.save_bytes(), game.save_bytes());
        }
        assert_eq!(game.replay(0).unwrap(), opening);
        assert_eq!(game.replay(3).unwrap(), *game.observation());
        let rival = game.host().session().observe(PlayerId::new(2)).unwrap();
        assert!(
            rival.progress().current_technology.is_some()
                || !rival.progress().technologies.is_empty()
        );
    }

    #[test]
    fn scenario_settler_moves_and_founds_a_city_through_the_player_interface() {
        let [seat, rival] = [3, 9].map(PlayerId::new);
        let mut session = Session::with_map(
            [seat, rival],
            MapState::blank(FlatHex::new(8, 2, false, false).unwrap()),
        );
        let unit = UnitId::new(10);
        session
            .dispatch(Command::UnitSpawn {
                player: seat,
                unit,
                kind: UnitKind::Settler,
                tile: TileId::new(0),
            })
            .unwrap();
        let mut game = ClientSession::from_save_bytes(&session.save_bytes()).unwrap();
        let destination = TileId::new(1);
        game.dispatch(Command::UnitMove {
            player: seat,
            unit,
            destination,
        })
        .unwrap();
        game.dispatch(Command::CitySettle { player: seat, unit })
            .unwrap();
        assert_eq!(game.observation().cities()[0].tile, destination);
        assert!(game.observation().units().is_empty());
    }
    #[test]
    fn duel_openings_provide_each_seat_a_warrior_and_a_capital() {
        for seed in 0..32 {
            let game = ClientSession::new(seed).unwrap();
            for seat in game.host().session().seat_ids() {
                let view = game.host().session().observe(seat).unwrap();
                assert_eq!(view.units().len(), 1);
                assert_eq!(view.units()[0].kind, UnitKind::Warrior);
                assert_eq!(view.cities().len(), 1);
            }
        }
    }

    #[test]
    fn hotseat_rounds_match_manual_orders_for_every_supported_roster() {
        for count in 2..=8 {
            let mut game = ClientSession::hotseat(7, count).unwrap();
            let mut direct = MatchHost::new(Session::from_save_bytes(&game.save_bytes()).unwrap());
            for number in 1..=count {
                let seat = PlayerId::new(number);
                assert_eq!(game.handoff_seat(), Some(seat));
                assert!(game.end_turn().is_err());
                game.confirm_handoff();
                assert_eq!(game.observation(), &direct.session().observe(seat).unwrap());
                let order = Command::ResearchTechnology {
                    player: seat,
                    technology: "tech_01".into(),
                };
                direct.dispatch(seat, order.clone()).unwrap();
                game.dispatch(order).unwrap();
                assert_eq!(
                    game.host().session().state_hash(),
                    direct.session().state_hash()
                );
                game.end_turn().unwrap();
                direct
                    .dispatch(seat, Command::TurnEnd { player: seat })
                    .unwrap();
                assert_eq!(
                    game.host().session().state_hash(),
                    direct.session().state_hash()
                );
                let resumed = ClientSession::hotseat_from_save(&game.save_bytes()).unwrap();
                assert_eq!(resumed.handoff_seat(), game.handoff_seat());
                assert_eq!(resumed.save_bytes(), game.save_bytes());
            }
            assert_eq!(game.observation().turn(), 1);
            assert_eq!(game.handoff_seat(), Some(PlayerId::new(1)));
        }
        for invalid in [1, 9] {
            assert!(ClientSession::hotseat(7, invalid).is_err());
        }
    }

    pub(crate) fn winning_capture() -> (ClientSession, Command) {
        let [seat, rival] = [2, 5].map(PlayerId::new);
        let board = FlatHex::new(5, 1, false, false).unwrap();
        let mut session = Session::with_map([seat, rival], MapState::blank(board));
        for (player, pos) in [(seat, 0), (rival, 4)] {
            let founder = UnitId::new(pos);
            session
                .dispatch(Command::UnitSpawn {
                    player,
                    unit: founder,
                    kind: UnitKind::Settler,
                    tile: TileId::new(pos),
                })
                .unwrap();
            session
                .dispatch(Command::CitySettle {
                    player,
                    unit: founder,
                })
                .unwrap();
        }
        let unit = UnitId::new(5);
        session
            .dispatch(Command::UnitSpawn {
                player: seat,
                unit,
                tile: TileId::new(3),
                kind: UnitKind::Warrior,
            })
            .unwrap();
        let game = ClientSession::from_save_bytes(&session.save_bytes()).unwrap();
        let city = game
            .observation()
            .visible_cities()
            .iter()
            .find(|city| city.owner == rival)
            .unwrap()
            .id;
        let order = Command::UnitCaptureCity {
            player: seat,
            unit,
            city,
        };
        (game, order)
    }

    #[test]
    fn capture_publishes_the_winner_and_closes_the_match() {
        let (mut game, order) = winning_capture();
        let seat = game.observation().seat();
        let mut hotseat = ClientSession::hotseat_from_save(&game.save_bytes()).unwrap();
        assert_eq!(hotseat.handoff_seat(), Some(seat));
        hotseat.confirm_handoff();
        hotseat.dispatch(order.clone()).unwrap();
        game.dispatch(order).unwrap();
        assert_eq!(hotseat.winner(), game.winner());
        assert!(hotseat.handoff_seat().is_none());
        assert!(hotseat.end_turn().is_err());
        let finished = ClientSession::hotseat_from_save(&hotseat.save_bytes()).unwrap();
        assert_eq!(finished.winner(), game.winner());
        assert!(finished.handoff_seat().is_none());
        assert_eq!(game.winner(), Some((seat, VictoryKind::Domination)));
        assert_eq!(game.replay(0).unwrap(), *game.observation());
        assert!(game.end_turn().is_err());
        assert_eq!(
            ClientSession::from_save_bytes(&game.save_bytes())
                .unwrap()
                .winner(),
            game.winner()
        );
    }
}
