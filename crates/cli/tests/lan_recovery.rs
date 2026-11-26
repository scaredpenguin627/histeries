//! Recover a lost LAN acknowledgement through the public line-oriented CLI.
#![cfg(not(target_family = "wasm"))]

use std::{
    collections::BTreeMap,
    io::{self, Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

use histeries_net::{LockstepHost, lan::LanServer};
use histeries_rules_core::PlayerId;
use histeries_sim::Session;
use serde_json::Value;

const TIMEOUT: Duration = Duration::from_secs(15);

fn relay(mut client: TcpStream, address: SocketAddr, drop_reply: bool) {
    client.set_nonblocking(false).unwrap();
    client.set_read_timeout(Some(TIMEOUT)).unwrap();
    let mut header = [0; 4];
    client.read_exact(&mut header).unwrap();
    let mut host = TcpStream::connect(address).unwrap();
    host.set_read_timeout(Some(TIMEOUT)).unwrap();
    host.write_all(&header).unwrap();
    let length = u64::from(u32::from_be_bytes(header));
    assert_eq!(
        io::copy(&mut Read::by_ref(&mut client).take(length), &mut host).unwrap(),
        length
    );
    // The host closes the exchange after writing its complete reply.
    if drop_reply {
        assert!(io::copy(&mut host, &mut io::sink()).unwrap() > 0);
    } else {
        io::copy(&mut host, &mut client).unwrap();
    }
}

fn batch_after_lost_reply(batch: &str) -> Output {
    let local = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
    let seats = [PlayerId::new(1), PlayerId::new(2)];
    let server = LanServer::bind(
        local,
        LockstepHost::new(Session::new(seats)).unwrap(),
        BTreeMap::from([(seats[0], "one".into()), (seats[1], "two".into())]),
    )
    .unwrap();
    let address = server.local_addr().unwrap();
    let listener = TcpListener::bind(local).unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut client = Command::new(env!("CARGO_BIN_EXE_histeries"))
        .args([
            "lan",
            "join",
            "--connect",
            &listener.local_addr().unwrap().to_string(),
            "--seat",
            "1",
        ])
        .env("HISTERIES_LAN_TOKEN", "one")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    client
        .stdin
        .take()
        .unwrap()
        .write_all(batch.as_bytes())
        .unwrap();
    let stop = AtomicBool::new(false);
    thread::scope(|scope| {
        scope.spawn(|| server.serve_until(&stop).unwrap());
        struct StopServer<'a>(&'a AtomicBool);
        impl Drop for StopServer<'_> {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
            }
        }
        let _stop = StopServer(&stop);
        let deadline = Instant::now() + TIMEOUT;
        let mut exchange = 0;
        while client.try_wait().unwrap().is_none() {
            if Instant::now() >= deadline {
                client.kill().unwrap();
                client.wait().unwrap();
                panic!("piped LAN client did not finish the batch");
            }
            match listener.accept() {
                // Forward the join and all recovery traffic. Lose only the first submit reply.
                Ok((stream, _)) => {
                    relay(stream, address, exchange == 1);
                    exchange += 1;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("{error}"),
            }
        }
    });
    client.wait_with_output().unwrap()
}

#[test]
fn piped_sync_resolves_the_original_order_exactly_once() {
    let output = batch_after_lost_reply("end\nend\nsync\nsync\n");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let replies: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(replies.len(), 5);
    assert_eq!(replies[0]["sequence"], 0);
    for error in &replies[1..3] {
        assert!(error["error"].is_string());
        assert_eq!(error["recovery_required"], true);
    }
    assert_eq!(replies[3]["recovered_submission"], "accepted");
    assert_eq!(replies[3]["active_seat"], 2);
    assert_eq!(replies[3]["sequence"], 1);
    assert_eq!(replies[4]["sync"]["replayed"], 0);
    assert!(replies[4].get("recovered_submission").is_none());
}

#[test]
fn piped_eof_with_an_unresolved_order_fails() {
    let output = batch_after_lost_reply("end\n");
    assert!(!output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    let reply: Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
    assert_eq!(reply["recovery_required"], true);
    assert!(String::from_utf8_lossy(&output.stderr).contains("pending submission"));
}
