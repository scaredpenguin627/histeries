//! Immutable, checksummed replay segments for restarting an endurance shard.
//! The live and comparison snapshots are produced by independent worker runs.

use std::{
    fs::{self, File},
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
};

use histeries_sim::{ReplayLog, hash_bytes};
use serde::{Deserialize, Serialize};

use super::{Boundary, Fixture, Measurements, Segment, Workers, restore};

// Caps include two live saves, the replay initial state and up to 100 rounds of commands.
const MAX_RECORD_BYTES: u64 = 256 * 1024 * 1024;
const MAX_MANIFEST_BYTES: usize = 64 * 1024;
const FORMAT: &[u8; 8] = b"HSPCK001";

#[derive(Serialize, Deserialize)]
struct Manifest {
    fixture: Fixture,
    start: u32,
    segment: Segment,
    primary_objectives: Vec<u8>,
    comparison_objectives: Vec<u8>,
}

pub(super) struct Journal {
    path: PathBuf,
    // Held throughout load, planning, and publication. A process exit releases it.
    _lock: File,
}

impl Journal {
    pub(super) fn open(root: &Path, fixture: &Fixture) -> Result<Self, String> {
        let path = root.join(format!("seed-{}", fixture.seed));
        fs::create_dir_all(&path).map_err(|e| e.to_string())?;
        let lock = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path.join("writer.lock"))
            .map_err(|e| e.to_string())?;
        lock.try_lock()
            .map_err(|e| format!("lock checkpoint {}: {e}", path.display()))?;
        Ok(Self { path, _lock: lock })
    }

    pub(super) fn load(
        &self,
        fixture: &Fixture,
        workers: &mut Workers,
        totals: &mut Measurements,
    ) -> Result<(), String> {
        let mut files = Vec::new();
        for entry in fs::read_dir(&self.path).map_err(|e| e.to_string())? {
            let file = entry.map_err(|e| e.to_string())?.path();
            if file.extension().is_some_and(|ext| ext == "chk") {
                files.push(file);
            }
        }
        files.sort();
        // Opening commands belong to the first replay. Every later record
        // begins at the preceding segment's verified final state.
        for file in files {
            let start = totals.hashes.len() as u32;
            if file != self.segment_path(start) {
                return Err(format!("checkpoint gap or overlap at round {start}"));
            }
            if file.metadata().map_err(|e| e.to_string())?.len() > MAX_RECORD_BYTES {
                return Err(format!("oversized checkpoint {}", file.display()));
            }
            let bytes = fs::read(&file).map_err(|e| format!("read {}: {e}", file.display()))?;
            let (manifest, primary, comparison, replay) = unpack(&bytes)?;
            if manifest.fixture != *fixture || manifest.start != start {
                return Err(format!("checkpoint fixture mismatch in {}", file.display()));
            }
            let segment = manifest.segment;
            let count =
                u32::try_from(segment.rounds.len()).map_err(|_| "too many checkpoint rounds")?;
            let end = start
                .checked_add(count)
                .ok_or("checkpoint round overflow")?;
            if !(1..=100).contains(&count) || end > fixture.limit {
                return Err(format!("invalid checkpoint round count at {start}"));
            }
            let expected_checkpoint = (start + 1..=end)
                .rev()
                .find(|&r| r.is_power_of_two() && r < fixture.limit);
            if segment.controller_checkpoint != expected_checkpoint
                || segment.save_bytes < primary.len()
            {
                return Err(format!("invalid checkpoint measurements at {end}"));
            }
            check_replay_prefix(replay, &segment.rounds, workers.primary.session.replay())?;
            let anchor = segment.rounds.last().ok_or("empty checkpoint")?.hash;
            let game = restore(primary, &manifest.primary_objectives, anchor)?;
            let parallel = restore(comparison, &manifest.comparison_objectives, anchor)?;
            if game.session.turn() != end
                || game.session.winner().is_some()
                || manifest.primary_objectives.len() != fixture.players as usize
                || manifest.primary_objectives != manifest.comparison_objectives
            {
                return Err(format!(
                    "invalid checkpoint world or controllers at round {end}"
                ));
            }
            totals.append(segment);
            *workers = Workers {
                primary: game,
                comparison: parallel,
            };
        }
        Ok(())
    }

    fn segment_path(&self, start: u32) -> PathBuf {
        self.path.join(format!("from-{start:010}.chk"))
    }

    pub(super) fn publish(
        &self,
        fixture: &Fixture,
        start: u32,
        segment: &Segment,
        workers: &Workers,
        replay: &[u8],
    ) -> Result<(), String> {
        let manifest = Manifest {
            fixture: fixture.clone(),
            start,
            segment: segment.clone(),
            primary_objectives: workers.primary.objectives(),
            comparison_objectives: workers.comparison.objectives(),
        };
        let meta = serde_json::to_vec(&manifest).map_err(|e| e.to_string())?;
        let primary = workers.primary.session.state_snapshot().encode();
        let comparison = workers.comparison.session.state_snapshot().encode();
        let mut payload = Vec::new();
        for field in [&meta[..], &primary, &comparison, replay] {
            payload.extend_from_slice(&(field.len() as u64).to_le_bytes());
            payload.extend_from_slice(field);
        }
        if payload.len() as u64 + 16 > MAX_RECORD_BYTES || meta.len() > MAX_MANIFEST_BYTES {
            return Err(format!("checkpoint segment too large at round {start}"));
        }
        let to = self.segment_path(start);
        let tmp = self.path.join("segment.pending");
        let write = || -> std::io::Result<()> {
            if to.try_exists()? {
                return Err(std::io::Error::new(
                    ErrorKind::AlreadyExists,
                    "checkpoint already published",
                ));
            }
            let mut f = File::create(&tmp)?;
            f.write_all(FORMAT)?;
            f.write_all(&hash_bytes(&payload).value().to_le_bytes())?;
            f.write_all(&payload)?;
            f.sync_all()?;
            drop(f);
            fs::rename(&tmp, &to)?;
            #[cfg(unix)]
            File::open(&self.path)?.sync_all()?;
            Ok(())
        };
        write().map_err(|e| format!("publish checkpoint {}: {e}", to.display()))
    }
}

// Every round in this immutable record was replayed before atomic publication.
// Bind the decoded log to the preceding world and validate all command ranges;
// restarting a shard does not dispatch an already checked command.
fn check_replay_prefix(
    bytes: &[u8],
    rounds: &[Boundary],
    previous: &ReplayLog,
) -> Result<(), String> {
    let replay = ReplayLog::decode(bytes).map_err(|e| e.to_string())?;
    if replay.initial_state().state_hash() != previous.initial_state().state_hash()
        || !replay.commands().starts_with(previous.commands())
    {
        return Err("checkpoint replay does not continue its verified predecessor".into());
    }
    let mut begin = previous.commands().len();
    for round in rounds {
        let end = round.commands_end;
        if !(begin < end && end <= replay.commands().len()) {
            return Err("invalid checkpoint replay command boundary".into());
        }
        begin = end;
    }
    if begin != replay.commands().len() {
        return Err("unconsumed checkpoint replay commands".into());
    }
    Ok(())
}

type Record<'a> = (Manifest, &'a [u8], &'a [u8], &'a [u8]);

fn unpack(bytes: &[u8]) -> Result<Record<'_>, String> {
    let mut data = bytes
        .strip_prefix(FORMAT)
        .ok_or("unknown checkpoint format")?;
    let checksum = read_u64(&mut data)?;
    if hash_bytes(data).value() != checksum {
        return Err("checkpoint checksum mismatch".into());
    }
    fn field<'a>(data: &mut &'a [u8]) -> Result<&'a [u8], String> {
        let size = usize::try_from(read_u64(data)?).map_err(|_| "checkpoint length overflow")?;
        data.split_off(..size)
            .ok_or_else(|| "truncated checkpoint field".into())
    }
    let meta = field(&mut data)?;
    if meta.len() > MAX_MANIFEST_BYTES {
        return Err("oversized checkpoint manifest".into());
    }
    let manifest = serde_json::from_slice(meta).map_err(|e| e.to_string())?;
    let primary = field(&mut data)?;
    let comparison = field(&mut data)?;
    let replay = field(&mut data)?;
    if !data.is_empty() {
        return Err("trailing checkpoint data".into());
    }
    Ok((manifest, primary, comparison, replay))
}

fn read_u64(data: &mut &[u8]) -> Result<u64, String> {
    data.split_off(..8)
        .and_then(|bytes| bytes.try_into().ok())
        .map(u64::from_le_bytes)
        .ok_or_else(|| "truncated checkpoint header".into())
}
