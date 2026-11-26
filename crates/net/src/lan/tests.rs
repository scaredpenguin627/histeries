use super::*;
use histeries_sim::Session;
use std::net::Ipv4Addr;

const FIRST: PlayerId = PlayerId::new(1);
const SECOND: PlayerId = PlayerId::new(2);

fn host() -> LockstepHost {
    LockstepHost::new(Session::new([FIRST, SECOND])).unwrap()
}

fn loopback() -> SocketAddr {
    (Ipv4Addr::LOCALHOST, 0).into()
}

fn with_server(run: impl FnOnce(&LanServer, SocketAddr)) {
    let server = LanServer::bind(
        loopback(),
        host(),
        BTreeMap::from([(FIRST, "a".into()), (SECOND, "b".into())]),
    )
    .unwrap();
    let addr = server.local_addr().unwrap();
    let stop = AtomicBool::new(false);
    thread::scope(|scope| {
        // Stop before scope exit even when an assertion unwinds.
        struct Shutdown<'a>(&'a AtomicBool);
        impl Drop for Shutdown<'_> {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
            }
        }
        scope.spawn(|| server.serve_until(&stop).unwrap());
        let _shutdown = Shutdown(&stop);
        run(&server, addr);
    });
}

#[test]
fn validates_frame_version_and_position() {
    with_server(|_, addr| {
        use std::io::{Read, Write};

        let mut oversized = TcpStream::connect(addr).unwrap();
        oversized
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        oversized.write_all(&u32::MAX.to_be_bytes()).unwrap();
        let closed = oversized.read(&mut [0]);
        assert!(
            matches!(closed, Ok(0))
                || matches!(closed, Err(ref e) if e.kind() == io::ErrorKind::ConnectionReset)
        );

        let mut legacy = FrameStream::new(TcpStream::connect(addr).unwrap()).unwrap();
        legacy
            .write_request(&Request {
                version: wire::VERSION - 1,
                seat: FIRST,
                token: "a".into(),
                action: Action::Join,
            })
            .unwrap();
        assert!(matches!(
            legacy.read_reply().unwrap(),
            Reply::Refused(Refusal::Version {
                supported: wire::VERSION,
                ..
            })
        ));

        let peer = LanClient::connect(addr, FIRST, "a".into()).unwrap();
        let from = peer.replica.position();
        let corrupt = Position {
            hash: from.hash ^ 1,
            ..from
        };
        let command = Command::TurnEnd { player: FIRST }.encode();
        assert!(
            matches!(exchange(addr, FIRST, "a", Action::Submit { permit: peer.permit, from: corrupt, command }),
            Err(LanError::Refused(Refusal::Desync { expected, received })) if expected == from && received == corrupt)
        );
        assert!(
            matches!(exchange(addr, FIRST, "a", Action::Sync { host: peer.permit.host, from: corrupt }),
            Err(LanError::Refused(Refusal::Desync { expected, received })) if expected == from && received == corrupt)
        );
    });
}

// Capture the actual client submission, then close without replying. The test decides when the
// captured request reaches the real host, independently of thread and socket scheduling.
fn interrupt(peer: &mut LanClient, command: Command) -> Action {
    let relay = TcpListener::bind(loopback()).unwrap();
    let address = peer.address;
    peer.address = relay.local_addr().unwrap();
    let captured = thread::spawn(move || {
        let mut stream = FrameStream::new(relay.accept().unwrap().0).unwrap();
        stream.read_request().unwrap().action
    });
    assert!(matches!(peer.submit(command), Err(LanError::Io(_))));
    peer.address = address;
    assert!(matches!(
        peer.submit(Command::TurnEnd { player: FIRST }),
        Err(LanError::SubmissionPending)
    ));
    captured.join().unwrap()
}

#[test]
fn interrupted_order_is_settled_in_both_arrival_orders() {
    for submit_first in [false, true] {
        with_server(|_, addr| {
            let mut peer = LanClient::connect(addr, FIRST, "a".into()).unwrap();
            let mut delayed = Some(interrupt(&mut peer, Command::TurnEnd { player: FIRST }));
            if submit_first {
                assert!(matches!(
                    exchange(addr, FIRST, "a", delayed.take().unwrap()),
                    Ok(Reply::Accepted(_))
                ));
            }
            peer.sync().unwrap();
            assert_eq!(
                peer.submission_outcome(),
                Some(if submit_first {
                    SubmissionOutcome::Accepted
                } else {
                    SubmissionOutcome::Canceled
                })
            );
            if let Some(action) = delayed {
                assert!(matches!(
                    exchange(addr, FIRST, "a", action),
                    Err(LanError::Refused(Refusal::Superseded))
                ));
                peer.submit(Command::TurnEnd { player: FIRST }).unwrap();
            }
            assert_eq!(peer.replica.position().sequence, 1);
            assert_eq!(peer.replica.active_seat(), Some(SECOND));
        });
    }
}

#[test]
fn rejoin_fences_old_work_and_an_identical_independent_order_has_its_own_receipt() {
    with_server(|_, addr| {
        let mut first = LanClient::connect(addr, FIRST, "a".into()).unwrap();
        let end = || Command::TurnEnd { player: FIRST };
        let delayed = interrupt(&mut first, end());
        let mut second = LanClient::connect(addr, FIRST, "a".into()).unwrap();
        assert!(matches!(
            exchange(addr, FIRST, "a", delayed),
            Err(LanError::Refused(Refusal::Superseded))
        ));
        second.submit(end()).unwrap();
        first.sync().unwrap();
        assert_eq!(
            first.submission_outcome(),
            Some(SubmissionOutcome::Canceled)
        );
        assert_eq!(first.replica.position(), second.replica.position());
    });
}

#[test]
fn a_forged_post_command_hash_is_reported_without_advancing_the_replica() {
    let listener = TcpListener::bind(loopback()).unwrap();
    let address = listener.local_addr().unwrap();
    let mut authority = host();
    let opening = authority.opening();
    let before = authority.position();
    let mut commit = authority
        .submit(FIRST, before, Command::TurnEnd { player: FIRST })
        .unwrap();
    let actual = commit.after;
    commit.after.hash ^= 1;
    let expected = commit.after;
    let worker = thread::spawn(move || {
        for reply in [
            Reply::Joined {
                opening,
                permit: Permit {
                    host: [0; 16],
                    seat: FIRST,
                    generation: 1,
                },
            },
            Reply::Accepted(commit),
        ] {
            let mut stream = FrameStream::new(listener.accept().unwrap().0).unwrap();
            stream.read_request().unwrap();
            stream.write_reply(reply).unwrap();
        }
    });
    let mut peer = LanClient::connect(address, FIRST, "a".into()).unwrap();
    let err = peer.submit(Command::TurnEnd { player: FIRST }).unwrap_err();
    assert!(
        matches!(err, LanError::Replica(report) if report.before == before && report.expected == expected && report.actual == actual)
    );
    assert_eq!(peer.replica.position(), before);
    worker.join().unwrap();
}

#[test]
fn expired_submission_is_fenced_and_play_resumes_only_from_a_verified_checkpoint() {
    with_server(|server, addr| {
        let mut laggard = LanClient::connect(addr, SECOND, "b".into()).unwrap();
        let mut interrupted = LanClient::connect(addr, FIRST, "a".into()).unwrap();
        let delayed = interrupt(&mut interrupted, Command::TurnEnd { player: FIRST });
        let previous = laggard.replica.position();
        let mut game = server.game.lock().unwrap();
        let permit = game.issue_permit(FIRST).unwrap();
        let mut earlier_checkpoint = None;
        for i in 0..=HISTORY {
            let from = game.host.position();
            let command = if i == HISTORY {
                Command::TurnEnd { player: FIRST }
            } else {
                Command::ResearchTechnology {
                    player: FIRST,
                    technology: "tech_01".into(),
                }
            }
            .encode();
            game.execute(
                FIRST,
                Action::Submit {
                    permit,
                    from,
                    command,
                },
            )
            .unwrap();
            if i == 0 {
                earlier_checkpoint = Some(game.host.opening());
            }
        }
        let current = game.host.position();
        assert_eq!(game.history.len(), HISTORY);
        drop(game);
        assert_eq!(
            laggard.sync().unwrap(),
            SyncOutcome::Checkpoint { previous, current }
        );
        assert_eq!(laggard.replica.position(), current);
        assert_eq!(laggard.replica.session().state_hash().value(), current.hash);
        assert_eq!(laggard.submission_outcome(), None);
        // Deliver the real fence receipt, followed by a valid checkpoint from before the fence.
        let relay = TcpListener::bind(loopback()).unwrap();
        interrupted.address = relay.local_addr().unwrap();
        let worker = thread::spawn(move || {
            let mut recover = FrameStream::new(relay.accept().unwrap().0).unwrap();
            let request = recover.read_request().unwrap();
            let reply = exchange(addr, FIRST, "a", request.action).unwrap();
            assert!(
                matches!(reply, Reply::Recovered { outcome: Some(SubmissionOutcome::Unknown), through, .. } if through == current)
            );
            recover.write_reply(reply).unwrap();
            let mut sync = FrameStream::new(relay.accept().unwrap().0).unwrap();
            sync.read_request().unwrap();
            sync.write_reply(Reply::Opening(earlier_checkpoint.unwrap()))
                .unwrap();
        });
        assert!(matches!(interrupted.sync(), Err(LanError::Protocol(_))));
        worker.join().unwrap();
        interrupted.address = addr;
        assert_eq!(interrupted.replica.position(), previous);
        assert_eq!(interrupted.submission_outcome(), None);
        assert!(matches!(
            interrupted.submit(Command::TurnEnd { player: FIRST }),
            Err(LanError::SubmissionPending)
        ));
        assert!(matches!(
            exchange(addr, FIRST, "a", delayed),
            Err(LanError::Refused(Refusal::Superseded))
        ));
        assert!(matches!(
            exchange(
                addr,
                FIRST,
                "a",
                Action::Submit {
                    permit,
                    from: current,
                    command: Command::TurnEnd { player: FIRST }.encode()
                }
            ),
            Err(LanError::Refused(Refusal::Superseded))
        ));
        laggard.submit(Command::TurnEnd { player: SECOND }).unwrap();
        let advanced = laggard.replica.position();
        assert!(advanced.sequence > current.sequence);
        assert_eq!(
            interrupted.sync().unwrap(),
            SyncOutcome::Checkpoint {
                previous,
                current: advanced
            }
        );
        assert_eq!(
            interrupted.submission_outcome(),
            Some(SubmissionOutcome::Unknown)
        );
        interrupted
            .submit(Command::TurnEnd { player: FIRST })
            .unwrap();
        assert_eq!(
            interrupted.replica.position().sequence,
            advanced.sequence + 1
        );
        assert_eq!(interrupted.replica.active_seat(), Some(SECOND));
    });
}

#[test]
fn equal_permit_generations_remain_bound_to_their_authenticated_seats() {
    with_server(|server, _| {
        let mut game = server.game.lock().unwrap();
        let first = game.issue_permit(FIRST).unwrap();
        let second = game.issue_permit(SECOND).unwrap();
        assert_eq!(first.generation, second.generation);
        assert_ne!(first, second);
        let from = game.host.position();
        assert!(matches!(
            game.execute(
                FIRST,
                Action::Recover {
                    previous: second,
                    proposal: Some(from)
                }
            ),
            Err(Refusal::RecoveryExpired)
        ));
        assert!(matches!(
            game.execute(
                FIRST,
                Action::Submit {
                    permit: second,
                    from,
                    command: Command::TurnEnd { player: FIRST }.encode()
                }
            ),
            Err(Refusal::Superseded)
        ));
        assert_eq!(game.host.position(), from);
        assert!(
            game.execute(
                FIRST,
                Action::Submit {
                    permit: first,
                    from,
                    command: Command::TurnEnd { player: FIRST }.encode()
                }
            )
            .is_ok()
        );
    });
}

#[test]
fn a_replacement_host_at_the_same_address_requires_rejoin_for_replays_and_recovery() {
    with_server(|server, addr| {
        let mut old = LanClient::connect(addr, FIRST, "a".into()).unwrap();
        let mut idle = LanClient::connect(addr, SECOND, "b".into()).unwrap();
        old.replay(0).unwrap();
        let _delayed = interrupt(&mut old, Command::TurnEnd { player: FIRST });
        assert!(old.needs_recovery());
        let replacement = LanServer::bind(loopback(), host(), server.credentials.clone()).unwrap();
        *server.game.lock().unwrap() = replacement.game.into_inner().unwrap();
        let fresh = LanClient::connect(addr, FIRST, "a".into()).unwrap();
        fresh.replay(0).unwrap();
        let before = fresh.replica.position();
        assert!(matches!(
            idle.sync(),
            Err(LanError::Refused(Refusal::RecoveryExpired))
        ));
        assert_eq!(idle.submission_outcome(), None);
        assert!(matches!(
            old.replay(0),
            Err(LanError::Refused(Refusal::RecoveryExpired))
        ));
        assert_eq!(old.submission_outcome(), None);
        assert!(matches!(
            old.sync(),
            Err(LanError::InterruptedSubmissionRequiresRejoin)
        ));
        assert_eq!(old.submission_outcome(), Some(SubmissionOutcome::Unknown));
        assert!(!old.needs_recovery());
        assert!(matches!(
            old.sync(),
            Err(LanError::InterruptedSubmissionRequiresRejoin)
        ));
        assert!(matches!(
            old.submit(Command::TurnEnd { player: FIRST }),
            Err(LanError::InterruptedSubmissionRequiresRejoin)
        ));
        assert_eq!(old.submission_outcome(), Some(SubmissionOutcome::Unknown));
        assert_eq!(server.game.lock().unwrap().host.position(), before);
    });
}

#[test]
fn recovery_retries_transport_loss_but_requires_rejoin_after_token_or_version_changes() {
    for upgraded in [false, true] {
        let original = LanServer::bind(
            loopback(),
            host(),
            BTreeMap::from([(FIRST, "a".into()), (SECOND, "b".into())]),
        )
        .unwrap();
        let address = original.local_addr().unwrap();
        original.listener.set_nonblocking(false).unwrap();
        let worker = thread::spawn(move || {
            for _ in 0..2 {
                original
                    .handle(original.listener.accept().unwrap().0)
                    .unwrap();
            }
            // Interrupt the first recovery attempt before the original host handles it.
            let mut lost = FrameStream::new(original.listener.accept().unwrap().0).unwrap();
            lost.read_request().unwrap();
            drop(lost);

            let mut replacement = LanServer::bind(
                loopback(),
                host(),
                BTreeMap::from([(FIRST, "new-a".into()), (SECOND, "new-b".into())]),
            )
            .unwrap();
            replacement.listener = original.listener;
            let before = replacement.game.lock().unwrap().host.position();
            for _ in 0..2 {
                let socket = replacement.listener.accept().unwrap().0;
                if upgraded {
                    let mut stream = FrameStream::new(socket).unwrap();
                    let request = stream.read_request().unwrap();
                    stream
                        .write_reply(Reply::Refused(Refusal::Version {
                            requested: request.version,
                            supported: wire::VERSION + 1,
                        }))
                        .unwrap();
                } else {
                    replacement.handle(socket).unwrap();
                }
            }
            assert_eq!(replacement.game.lock().unwrap().host.position(), before);
        });
        let mut pending = LanClient::connect(address, FIRST, "a".into()).unwrap();
        let mut idle = LanClient::connect(address, SECOND, "b".into()).unwrap();
        let _delayed = interrupt(&mut pending, Command::TurnEnd { player: FIRST });
        assert!(matches!(pending.sync(), Err(LanError::Io(_))));
        assert!(pending.needs_recovery());
        assert_eq!(pending.submission_outcome(), None);

        assert!(matches!(
            pending.sync(),
            Err(LanError::InterruptedSubmissionRequiresRejoin)
        ));
        assert_eq!(
            pending.submission_outcome(),
            Some(SubmissionOutcome::Unknown)
        );
        assert!(matches!(
            pending.submit(Command::TurnEnd { player: FIRST }),
            Err(LanError::InterruptedSubmissionRequiresRejoin)
        ));
        assert!(matches!(
            (upgraded, idle.sync()),
            (true, Err(LanError::Refused(Refusal::Version { .. })))
                | (false, Err(LanError::Refused(Refusal::Credentials)))
        ));
        assert_eq!(idle.submission_outcome(), None);
        worker.join().unwrap();
    }
}
