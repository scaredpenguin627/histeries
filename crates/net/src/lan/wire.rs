//! A big-endian u32 length precedes each payload. The JSON opening header
//! carries an empty save field; a second binary frame carries the save bytes.

use std::{
    io::{self, Read, Write},
    net::TcpStream,
    time::{Duration, Instant},
};

use histeries_rules_core::PlayerId;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use super::{LanError, Refusal, SeatReplay, SubmissionOutcome};
use crate::{Commit, Opening, Position};

pub(super) const TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_LIMIT: usize = 16 * 1024;
const REPLY_LIMIT: usize = 8 * 1024 * 1024;
const SAVE_LIMIT: usize = 40 * 1024 * 1024;

pub(super) const VERSION: u16 = 2;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Permit {
    pub host: [u8; 16],
    pub seat: PlayerId,
    pub generation: u64,
}

#[derive(Serialize, Deserialize)]
pub(super) struct Request {
    pub version: u16,
    pub seat: PlayerId,
    pub token: String,
    pub action: Action,
}

#[derive(Serialize, Deserialize)]
pub(super) enum Action {
    Join,
    Replay {
        host: [u8; 16],
        turn: u32,
    },
    Sync {
        host: [u8; 16],
        from: Position,
    },
    Recover {
        previous: Permit,
        proposal: Option<Position>,
    },
    Submit {
        permit: Permit,
        from: Position,
        command: Vec<u8>,
    },
}

#[derive(Serialize, Deserialize)]
pub(super) enum Reply {
    Joined {
        opening: Opening,
        permit: Permit,
    },
    Opening(Opening),
    Recovered {
        previous: Permit,
        proposal: Option<Position>,
        permit: Permit,
        outcome: Option<SubmissionOutcome>,
        through: Position,
    },
    Replay(SeatReplay),
    Updates {
        commits: Vec<Commit>,
        through: Position,
    },
    Accepted(Commit),
    Refused(Refusal),
}

pub(super) struct FrameStream {
    socket: TcpStream,
    deadline: Instant,
}

impl FrameStream {
    pub fn new(socket: TcpStream) -> Result<Self, LanError> {
        socket.set_nonblocking(false)?;
        socket.set_nodelay(true)?;
        Ok(Self {
            socket,
            deadline: Instant::now() + TIMEOUT,
        })
    }

    pub fn read_request(&mut self) -> Result<Request, LanError> {
        self.read_json(REQUEST_LIMIT)
    }

    pub fn write_request(&mut self, req: &Request) -> Result<(), LanError> {
        self.write_json(req, REQUEST_LIMIT)
    }

    pub fn read_reply(&mut self) -> Result<Reply, LanError> {
        let mut reply = self.read_json(REPLY_LIMIT)?;
        if let Reply::Opening(opening) | Reply::Joined { opening, .. } = &mut reply {
            if !opening.save.is_empty() {
                return Err(LanError::Protocol(
                    "opening header contains inline save bytes".into(),
                ));
            }
            opening.save = self.read_frame(SAVE_LIMIT)?;
        }
        Ok(reply)
    }

    pub fn write_reply(&mut self, mut reply: Reply) -> Result<(), LanError> {
        if let Reply::Opening(opening) | Reply::Joined { opening, .. } = &mut reply {
            let save = std::mem::take(&mut opening.save);
            if save.len() > SAVE_LIMIT {
                let refused =
                    Reply::Refused(Refusal::Rejected("save exceeds LAN transfer limit".into()));
                return self.write_json(&refused, REPLY_LIMIT);
            }
            self.write_json(&reply, REPLY_LIMIT)?;
            self.write_frame(&save, SAVE_LIMIT)
        } else {
            self.write_json(&reply, REPLY_LIMIT)
        }
    }

    fn read_json<T: DeserializeOwned>(&mut self, limit: usize) -> Result<T, LanError> {
        serde_json::from_slice(&self.read_frame(limit)?)
            .map_err(|e| LanError::Protocol(e.to_string()))
    }

    fn write_json<T: Serialize>(&mut self, value: &T, limit: usize) -> Result<(), LanError> {
        let bytes = serde_json::to_vec(value).map_err(|e| LanError::Protocol(e.to_string()))?;
        self.write_frame(&bytes, limit)
    }

    fn read_frame(&mut self, limit: usize) -> Result<Vec<u8>, LanError> {
        let mut header = [0; 4];
        self.read_exact(&mut header)?;
        let len = u32::from_be_bytes(header) as usize;
        if len > limit {
            return Err(LanError::Protocol(format!(
                "frame length {len} exceeds limit {limit}"
            )));
        }
        let mut bytes = vec![0; len];
        self.read_exact(&mut bytes)?;
        Ok(bytes)
    }

    fn write_frame(&mut self, bytes: &[u8], limit: usize) -> Result<(), LanError> {
        if bytes.len() > limit {
            return Err(LanError::Protocol(format!(
                "outgoing frame exceeds limit {limit}"
            )));
        }
        self.write_all(&(bytes.len() as u32).to_be_bytes())?;
        self.write_all(bytes)?;
        Ok(())
    }

    fn remaining(&self) -> io::Result<Duration> {
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "LAN exchange deadline elapsed"))
    }
}

// Reapply the absolute deadline before every syscall, including every partial
// read/write inside read_exact/write_all, so a slow peer cannot retain a worker.
impl Read for FrameStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.socket.set_read_timeout(Some(self.remaining()?))?;
        self.socket.read(buf)
    }
}

impl Write for FrameStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.socket.set_write_timeout(Some(self.remaining()?))?;
        self.socket.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.socket.flush()
    }
}
