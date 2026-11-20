use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use histeries_sim::Session;

// Canonicalize the parent even when the output has not been created yet.
fn destination(name: &str) -> io::Result<PathBuf> {
    let path = Path::new(name);
    match fs::symlink_metadata(path) {
        Ok(_) => {
            let canonical = path.canonicalize()?;
            if !canonical.is_file() {
                return Err(io::Error::other("destination must be a regular file"));
            }
            Ok(canonical)
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {
            let parent = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            Ok(parent.canonicalize()?.join(
                path.file_name()
                    .ok_or_else(|| io::Error::other("missing filename"))?,
            ))
        }
        Err(error) => Err(error),
    }
}

struct Staged(PathBuf);

fn same_destination(left: &Path, right: &Path) -> bool {
    // Canonical path components are available for existing files. New files can
    // still have differently cased names in the same case-insensitive directory.
    #[cfg(any(windows, target_os = "macos"))]
    {
        left == right
            || (left.parent() == right.parent()
                && left
                    .file_name()
                    .zip(right.file_name())
                    .is_some_and(|(a, b)| {
                        a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
                    }))
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        left == right
    }
}

impl Staged {
    fn prepare(
        target: &Path,
        other: Option<&Path>,
        write: impl FnOnce(&mut File) -> io::Result<()>,
    ) -> io::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        loop {
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = target.with_file_name(format!(".histeries-{}-{id}.tmp", std::process::id()));
            if same_destination(&path, target) || other.is_some_and(|p| same_destination(&path, p))
            {
                continue;
            }
            match options.open(&path) {
                Ok(mut file) => {
                    let staged = Self(path);
                    write(&mut file)?;
                    if let Ok(meta) = target.metadata() {
                        if meta.permissions().readonly() {
                            return Err(io::Error::new(
                                ErrorKind::PermissionDenied,
                                "read-only output",
                            ));
                        }
                        file.set_permissions(meta.permissions())?;
                    }
                    file.sync_all()?;
                    return Ok(staged);
                }
                Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
    }
}

impl Drop for Staged {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

pub(crate) fn write_session<'a>(
    session: &Session,
    output: &str,
    replay_output: Option<&str>,
    input: Option<&str>,
    commands: impl IntoIterator<Item = &'a str>,
) -> Result<(), String> {
    let resolve = |name: &str| destination(name).map_err(|e| format!("{name}: {e}"));
    let state = resolve(output)?;
    let replay = replay_output.map(resolve).transpose()?;
    let input = input.map(resolve).transpose()?;
    if replay.as_ref().is_some_and(|r| {
        same_destination(r, &state) || input.as_ref().is_some_and(|i| same_destination(i, r))
    }) {
        return Err("save, replay and command files require separate destinations".into());
    }
    for name in commands {
        let path = resolve(name)?;
        if same_destination(&path, &state)
            || replay.as_ref().is_some_and(|r| same_destination(r, &path))
        {
            return Err("save, replay and command files require separate destinations".into());
        }
    }
    let staged = Staged::prepare(&state, replay.as_deref(), |f| {
        f.write_all(&session.state_snapshot().encode())
    })
    .map_err(|e| format!("stage {output}: {e}"))?;
    let Some(replay) = replay else {
        return fs::rename(&staged.0, &state).map_err(|e| format!("publish {output}: {e}"));
    };
    let publish = || -> io::Result<()> {
        let log = Staged::prepare(&replay, Some(&state), |f| {
            f.write_all(&session.replay().encode())
        })?;
        let backup = match File::open(&replay) {
            Ok(mut existing) => Some(Staged::prepare(&replay, Some(&state), |f| {
                io::copy(&mut existing, f).map(|_| ())
            })?),
            Err(error) if error.kind() == ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        fs::rename(&log.0, &replay)?;
        if let Err(error) = fs::rename(&staged.0, &state) {
            let restored = match &backup {
                Some(old) => fs::rename(&old.0, &replay),
                None => fs::remove_file(&replay),
            };
            if let Err(restore_error) = restored {
                let recovery_path = backup.map(|mut old| std::mem::take(&mut old.0));
                return Err(io::Error::other(format!(
                    "{error}; restoring replay: {restore_error}; backup: {recovery_path:?}"
                )));
            }
            return Err(error);
        }
        Ok(())
    };
    publish().map_err(|e| format!("publish {output} and {}: {e}", replay.display()))
}
