//! Version 1 editor packages run against an immutable map and return ordinary editor commands.

use super::{MatchHost, PlayerId};
use serde::Deserialize;
use wasmi::{CompilationMode, Config, EnforcedLimits, Engine, Module};

mod runtime;
#[cfg(test)]
mod tests;

/// Maximum encoded size of a v1 manifest, in bytes.
pub const MAX_MANIFEST_BYTES: usize = 4096;
/// Maximum encoded size of a v1 Wasm module, in bytes.
pub const MAX_MODULE_BYTES: usize = 64 * 1024;

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    abi: u32,
    id: String,
    name: String,
    version: String,
    #[serde(default)]
    description: String,
}

/// An inspected package. Every application gets a fresh, isolated interpreter store.
#[derive(Clone)]
pub struct Package {
    manifest: Manifest,
    module: Module,
}

impl Package {
    /// Reads and validates both package members, without running guest code.
    pub fn from_bytes(manifest: &[u8], wasm: &[u8]) -> Result<Self, String> {
        if manifest.len() > MAX_MANIFEST_BYTES || wasm.len() > MAX_MODULE_BYTES {
            return Err("package exceeds the manifest or module size limit".into());
        }
        let text = std::str::from_utf8(manifest).map_err(|_| "manifest must be UTF-8")?;
        let manifest: Manifest = toml::from_str(text).map_err(|e| format!("manifest: {e}"))?;
        if manifest.abi != 1 {
            return Err(format!("unsupported editor ABI {}", manifest.abi));
        }
        if !(1..=64).contains(&manifest.id.len())
            || !manifest
                .id
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
            || manifest.id.starts_with('-')
            || manifest.id.ends_with('-')
        {
            return Err(
                "package id requires 1–64 lowercase ASCII letters, digits or interior hyphens"
                    .into(),
            );
        }
        if manifest.name.trim().is_empty()
            || manifest.name.len() > 96
            || manifest.version.trim().is_empty()
            || manifest.version.len() > 32
            || manifest.description.len() > 512
            || [&manifest.name, &manifest.version, &manifest.description]
                .iter()
                .any(|s| s.chars().any(char::is_control))
        {
            return Err("invalid package name, version or description".into());
        }
        let mut config = Config::default();
        config
            .enforced_limits(EnforcedLimits::strict())
            .compilation_mode(CompilationMode::Eager)
            .consume_fuel(true)
            .allow_start_fn(false)
            .floats(false)
            .wasm_mutable_global(true)
            .wasm_sign_extension(true)
            .wasm_saturating_float_to_int(false)
            .wasm_multi_value(true)
            .wasm_multi_memory(false)
            .wasm_bulk_memory(true)
            .wasm_reference_types(true)
            .wasm_tail_call(true)
            .wasm_extended_const(true)
            .wasm_custom_page_sizes(false)
            .wasm_wide_arithmetic(false)
            .set_max_recursion_depth(64)
            .set_min_stack_height(1024)
            .set_max_stack_height(64 * 1024);
        let engine = Engine::new(&config);
        let module = Module::new(&engine, wasm).map_err(|e| format!("invalid Wasm module: {e}"))?;
        runtime::prepare(&module, None).map_err(|e| format!("invalid editor ABI: {e}"))?;
        Ok(Self { manifest, module })
    }

    /// Loads the fixed members of a package directory on a native host.
    #[cfg(all(feature = "extension-files", not(target_family = "wasm")))]
    pub fn load(directory: &std::path::Path) -> Result<Self, String> {
        let (manifest, wasm) = Self::read_files(directory)?;
        Self::from_bytes(&manifest, &wasm)
    }

    /// Reads the bounded manifest and module for inspection or copying into a local library.
    #[cfg(all(feature = "extension-files", not(target_family = "wasm")))]
    pub fn read_files(directory: &std::path::Path) -> Result<(Vec<u8>, Vec<u8>), String> {
        use std::{fs, io::Read};
        let dir = directory
            .canonicalize()
            .map_err(|e| format!("package directory: {e}"))?;
        let read = |name: &str, limit: usize| -> Result<Vec<u8>, String> {
            let path = dir.join(name);
            let meta = fs::symlink_metadata(&path).map_err(|e| format!("{name}: {e}"))?;
            if !meta.is_file() || meta.len() > limit as u64 {
                return Err(format!(
                    "{name} must be a regular file of at most {limit} bytes"
                ));
            }
            let mut bytes = Vec::new();
            fs::File::open(path)
                .and_then(|f| f.take(limit as u64 + 1).read_to_end(&mut bytes))
                .map_err(|e| format!("{name}: {e}"))?;
            Ok(bytes)
        };
        Ok((
            read("manifest.toml", MAX_MANIFEST_BYTES)?,
            read("plugin.wasm", MAX_MODULE_BYTES)?,
        ))
    }

    /// Stable package ID used by the mod manager.
    pub fn id(&self) -> &str {
        &self.manifest.id
    }
    /// Human-readable package title.
    pub fn name(&self) -> &str {
        &self.manifest.name
    }
    /// Release identifier displayed to the editor.
    pub fn version(&self) -> &str {
        &self.manifest.version
    }
    /// Optional short explanation of the package's changes.
    pub fn description(&self) -> &str {
        &self.manifest.description
    }

    /// Executes with local editor authority. Only a completely valid command batch is published.
    pub fn apply(&self, host: &mut MatchHost, seat: PlayerId) -> Result<usize, String> {
        self.apply_and_snapshot(host, seat).map(|(_, count)| count)
    }

    pub(crate) fn apply_and_snapshot(
        &self,
        host: &mut MatchHost,
        seat: PlayerId,
    ) -> Result<(Vec<u8>, usize), String> {
        let map = host.editable_map(seat)?;
        let (mut store, entry) =
            runtime::prepare(&self.module, Some((seat, map))).map_err(|e| e.to_string())?;
        let status = entry
            .call(&mut store, ())
            .map_err(|e| format!("package trapped: {e}"))?;
        if status != 0 {
            return Err(format!("package returned status {status}"));
        }
        let commands = store.into_data().commands;
        let count = commands.len();
        let bytes = host.dispatch_editor_batch(seat, commands)?;
        Ok((bytes, count))
    }
}
