//! Multiplayer behavior over real loopback sockets.
#![cfg(not(target_arch = "wasm32"))]

use std::{
    collections::BTreeMap,
    net::{SocketAddr, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

use histeries_net::{
    LockstepHost,
    lan::{LanClient, LanError, LanServer, Refusal},
};
use histeries_rules_core::PlayerId;
use histeries_sim::{Command, Session};

const ALICE: PlayerId = PlayerId::new(2);
const BOB: PlayerId = PlayerId::new(5);
const CAROL: PlayerId = PlayerId::new(7);

struct Running {
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<Result<(), LanError>>>,
}

impl Running {
    fn start() -> Self {
        let seats = [ALICE, BOB, CAROL];
        let host = LockstepHost::new(Session::new(seats)).unwrap();
        let tokens = seats
            .into_iter()
            .map(|id| (id, format!("seat-{}", id.index())))
            .collect();
        let server = LanServer::bind("127.0.0.1:0".parse().unwrap(), host, tokens).unwrap();
        let addr = server.local_addr().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let signal = stop.clone();
        Self {
            addr,
            stop,
            worker: Some(thread::spawn(move || server.serve_until(&signal))),
        }
    }

    fn join(&self, seat: PlayerId) -> LanClient {
        LanClient::connect(self.addr, seat, format!("seat-{}", seat.index())).unwrap()
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.worker.take().unwrap().join().unwrap().unwrap();
    }
}

#[test]
fn three_peers_verify_remote_orders_recover_and_share_a_round_hash() {
    let server = Running::start();
    // The silent unauthenticated connection occupies no simulation lock.
    let _idle = TcpStream::connect(server.addr).unwrap();
    let mut alice = server.join(ALICE);
    let mut bob = server.join(BOB);
    let mut carol = server.join(CAROL);
    assert!(matches!(
        LanClient::connect(server.addr, BOB, "seat-2".into()),
        Err(LanError::Refused(Refusal::Credentials))
    ));
    assert!(matches!(
        bob.submit(Command::TurnEnd { player: BOB }),
        Err(LanError::Refused(Refusal::Rejected(_)))
    ));
    assert!(matches!(
        alice.submit(Command::TurnEnd { player: CAROL }),
        Err(LanError::Refused(Refusal::Rejected(_)))
    ));

    let mut duplicate = alice;
    let mut alice = server.join(ALICE);
    alice
        .submit(Command::ResearchTechnology {
            player: ALICE,
            technology: "tech_01".into(),
        })
        .unwrap();
    assert!(matches!(
        duplicate.submit(Command::ResearchTechnology {
            player: ALICE,
            technology: "tech_01".into()
        }),
        Err(LanError::Refused(Refusal::Superseded))
    ));
    bob.sync().unwrap();
    assert_eq!(bob.replica().position(), alice.replica().position());
    assert_eq!(bob.replica().active_seat(), Some(ALICE));

    drop(alice);
    let mut rejoined = server.join(ALICE);
    assert_eq!(rejoined.replica().position(), bob.replica().position());
    rejoined.submit(Command::TurnEnd { player: ALICE }).unwrap();
    bob.sync().unwrap();
    bob.submit(Command::TurnEnd { player: BOB }).unwrap();
    carol.sync().unwrap();
    assert_eq!(carol.replica().active_seat(), Some(CAROL));
    carol.submit(Command::TurnEnd { player: CAROL }).unwrap();
    for peer in [&mut rejoined, &mut bob, &mut duplicate] {
        peer.sync().unwrap();
        assert_eq!(peer.replica().position(), carol.replica().position());
        assert_eq!(
            peer.replica().session().state_hash(),
            carol.replica().session().state_hash()
        );
    }
    assert_eq!(carol.replica().position().turn, 1);
    assert_eq!(carol.replica().position().sequence, 4);
}

#[test]
fn rejects_missing_or_shared_credentials_at_bind_time() {
    for tokens in [
        BTreeMap::from([(ALICE, "a".into())]),
        BTreeMap::from([(ALICE, "a".into()), (BOB, "a".into())]),
        BTreeMap::from([(ALICE, "a".into()), (CAROL, "c".into())]),
    ] {
        let host = LockstepHost::new(Session::new([BOB, ALICE])).unwrap();
        assert!(matches!(
            LanServer::bind("127.0.0.1:0".parse().unwrap(), host, tokens),
            Err(LanError::Protocol(_))
        ));
    }
}
