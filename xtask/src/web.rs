//! Build a self-contained static distribution with matching wasm-bindgen glue.
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

pub(super) fn build(root: &Path) -> Result<(), String> {
    const VERSION: &str = "0.2.130";
    let executable = env::var_os("WASM_BINDGEN").unwrap_or_else(|| "wasm-bindgen".into());
    let version = Command::new(&executable)
        .arg("--version")
        .output()
        .map_err(|e| format!("install wasm-bindgen-cli {VERSION} or set WASM_BINDGEN: {e}"))?;
    if !version.status.success()
        || String::from_utf8_lossy(&version.stdout).trim() != format!("wasm-bindgen {VERSION}")
    {
        return Err(format!("web-build requires wasm-bindgen-cli {VERSION}"));
    }
    let status = Command::new("cargo")
        .current_dir(root)
        .args([
            "rustc",
            "--locked",
            "--release",
            "--target",
            "wasm32-unknown-unknown",
            "-p",
            "histeries-client",
            "--lib",
            "--crate-type",
            "cdylib",
        ])
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("client WebAssembly build failed".into());
    }
    let target = target_dir(root)?;
    let output = target.join("web");
    let status = Command::new(executable)
        .args(["--target", "web", "--no-typescript", "--out-dir"])
        .arg(output.join("pkg"))
        .arg(target.join("wasm32-unknown-unknown/release/histeries_client.wasm"))
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("generating browser bindings failed".into());
    }
    fs::copy(root.join("web/index.html"), output.join("index.html")).map_err(|e| e.to_string())?;
    println!("Serve {} over HTTPS or localhost.", output.display());
    Ok(())
}

fn target_dir(root: &Path) -> Result<PathBuf, String> {
    let metadata = Command::new("cargo")
        .current_dir(root)
        .args(["metadata", "--locked", "--no-deps", "--format-version", "1"])
        .output()
        .map_err(|e| e.to_string())?;
    if !metadata.status.success() {
        return Err("could not locate Cargo target directory".into());
    }
    let metadata: serde_json::Value =
        serde_json::from_slice(&metadata.stdout).map_err(|e| e.to_string())?;
    Ok(PathBuf::from(
        metadata["target_directory"]
            .as_str()
            .ok_or("missing target directory")?,
    ))
}

/// Preview only the three release files on a loopback socket.
pub(super) fn serve(root: &Path, args: &[String]) -> Result<(), String> {
    let (site, port) = match args {
        [] => (target_dir(root)?.join("web"), 8080),
        [site] => (PathBuf::from(site), 8080),
        [site, port] => (
            PathBuf::from(site),
            port.parse::<u16>().map_err(|e| e.to_string())?,
        ),
        _ => return Err("usage: cargo xtask web-serve [site-directory [port]]".into()),
    };
    super::web_preview::serve(&site, port)
}
