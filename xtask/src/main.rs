//! Workspace maintenance commands for Histeries.

#![forbid(unsafe_code)]

mod parity;
mod soak;
mod web;
mod web_preview;

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
    time::{Duration, Instant},
};

const ORDER: &[&str] = &[
    "hex",
    "rules-core",
    "ruleset",
    "sim",
    "ai",
    "net",
    "mcp",
    "cli",
    "client",
];
const FORBIDDEN_BELOW_CLIENT: &[&str] =
    &["wgpu", "egui", "eframe", "winit", "glutin", "sdl2", "-sys"];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask has a workspace parent")
        .to_path_buf()
}

fn run(program: &str, args: &[&str], root: &Path) -> bool {
    println!("+ {program} {}", args.join(" "));
    match Command::new(program).args(args).current_dir(root).status() {
        Ok(status) if status.success() => true,
        Ok(status) => {
            eprintln!("{program} exited with {status}");
            false
        }
        Err(error) => {
            eprintln!("could not run {program}: {error}");
            false
        }
    }
}

fn dependency_names(manifest: &str) -> Vec<String> {
    let mut in_dependencies = false;
    let mut names = Vec::new();
    for raw_line in manifest.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.starts_with('[') {
            in_dependencies = line.contains("dependencies]");
            continue;
        }
        if !in_dependencies || line.is_empty() {
            continue;
        }
        let Some((alias, value)) = line.split_once('=') else {
            continue;
        };
        let alias = alias.trim();
        let value = value.trim();
        if let Some(package) = quoted_value(value, "package") {
            names.push(package);
        } else if alias.starts_with("histeries-") {
            names.push(alias.to_owned());
        } else if value.contains("path") {
            for crate_name in ORDER {
                if value.contains(crate_name) {
                    names.push(format!("histeries-{crate_name}"));
                    break;
                }
            }
        }
    }
    names
}

fn quoted_value(value: &str, key: &str) -> Option<String> {
    let marker = format!("{key} = ");
    let start = value.find(&marker)? + marker.len();
    let quoted = value[start..].trim().strip_prefix('"')?;
    Some(quoted.split('"').next()?.to_owned())
}

fn check_layers(root: &Path) -> Result<(), String> {
    let mut errors = Vec::new();
    for (index, crate_name) in ORDER.iter().enumerate() {
        let path = root.join("crates").join(crate_name).join("Cargo.toml");
        let content =
            fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        for dependency_name in dependency_names(&content) {
            if let Some(dependency) = dependency_name.strip_prefix("histeries-") {
                let dependency_index = ORDER.iter().position(|item| *item == dependency);
                let Some(dependency_index) = dependency_index else {
                    continue;
                };
                if dependency_index >= index {
                    errors.push(format!(
                        "{crate_name} depends on {dependency}, which is not lower in the graph"
                    ));
                }
            }
        }
        if *crate_name != "client" {
            for forbidden in FORBIDDEN_BELOW_CLIENT {
                if dependency_names(&content)
                    .iter()
                    .any(|name| name.contains(forbidden))
                {
                    errors.push(format!(
                        "{crate_name} uses forbidden client dependency {forbidden}"
                    ));
                }
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

fn ci_fast(root: &Path) -> bool {
    let start = Instant::now();
    let mut ok = run("cargo", &["xtask", "layers"], root);
    ok &= run("cargo", &["fmt", "--all", "--", "--check"], root);
    ok &= run(
        "cargo",
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--all-features",
            "--",
            "-D",
            "warnings",
        ],
        root,
    );
    ok &= run("cargo", &["deny", "check"], root);
    ok &= run("cargo", &["nextest", "run", "--workspace"], root);
    if let Err(error) = parity::run(root) {
        eprintln!("{error}");
        ok = false;
    }
    ok &= ohos_check(root);
    ok &= run(
        "cargo",
        &["check", "--workspace", "--target", "wasm32-unknown-unknown"],
        root,
    );
    if let Err(error) = soak::run(root, "fixture", &["fast".into()]) {
        eprintln!("{error}");
        ok = false;
    }
    let elapsed = start.elapsed();
    ok &= elapsed < Duration::from_secs(15 * 60);
    let report =
        serde_json::json!({"passed": ok, "wall_ms": elapsed.as_millis(), "budget_ms": 900000});
    println!("ci-fast {report}");
    let dir = root.join("target/gates");
    if let Err(error) = fs::create_dir_all(&dir)
        .and_then(|_| fs::write(dir.join("ci-fast.json"), format!("{report}\n")))
    {
        eprintln!("could not write ci-fast measurements: {error}");
        return false;
    }
    ok
}

fn ohos_check(root: &Path) -> bool {
    let mut ok = true;
    for target in ["aarch64-unknown-linux-ohos", "x86_64-unknown-linux-ohos"] {
        ok &= run("cargo", &["check", "--workspace", "--target", target], root);
    }
    ok
}

fn main() -> ExitCode {
    let root = workspace_root();
    match env::args().nth(1).as_deref() {
        Some(command @ ("web-build" | "web-serve")) => {
            let result = if command == "web-build" {
                web::build(&root)
            } else {
                web::serve(&root, &env::args().skip(2).collect::<Vec<_>>())
            };
            match result {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("{error}");
                    ExitCode::FAILURE
                }
            }
        }
        Some("layers") => match check_layers(&root) {
            Ok(()) => {
                println!("dependency layers are valid");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("{error}");
                ExitCode::FAILURE
            }
        },
        Some("parity") => match parity::run(&root) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("{error}");
                ExitCode::FAILURE
            }
        },
        Some("ci-fast") => {
            if ci_fast(&root) {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Some(command @ ("soak" | "fixture" | "compare-fast")) => {
            let args = env::args().skip(2).collect::<Vec<_>>();
            let result = if command == "compare-fast" {
                soak::compare_fast(&args)
            } else {
                soak::run(&root, command, &args)
            };
            match result {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("{error}");
                    ExitCode::FAILURE
                }
            }
        }
        Some("memgate") => {
            let status = Command::new("cargo")
                .current_dir(root)
                .args([
                    "run",
                    "--locked",
                    "--release",
                    "-p",
                    "histeries-cli",
                    "--features",
                    "heap-profile",
                    "--bin",
                    "histeries",
                    "--",
                    "memgate",
                ])
                .args(env::args().skip(2))
                .status();
            match status {
                Ok(code) if code.success() => ExitCode::SUCCESS,
                result => {
                    eprintln!("memory gate failed: {result:?}");
                    ExitCode::FAILURE
                }
            }
        }
        Some("ohos-check") => {
            if ohos_check(&root) {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Some(command) => {
            eprintln!("unknown xtask command: {command}");
            ExitCode::FAILURE
        }
        None => {
            eprintln!(
                "usage: cargo xtask <layers|ci-fast|parity|fixture fast|nightly|weekly|soak|memgate|ohos-check|web-build|web-serve>"
            );
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ORDER, check_layers, dependency_names, workspace_root};

    #[test]
    fn graph_order_is_explicit() {
        assert_eq!(ORDER.first(), Some(&"hex"));
        assert_eq!(ORDER.last(), Some(&"client"));
        assert_eq!(ORDER.len(), 9);
    }

    #[test]
    fn scaffold_passes_layer_validation() {
        check_layers(&workspace_root()).expect("the Phase 0 scaffold must satisfy the layer rule");
    }

    #[test]
    fn aliased_path_dependencies_are_visible() {
        let manifest =
            "[dependencies]\nhex_topology = { package = \"histeries-hex\", path = \"../hex\" }\n";
        assert_eq!(dependency_names(manifest), vec!["histeries-hex"]);
    }
}
