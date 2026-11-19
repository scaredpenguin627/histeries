# Tooling

## Pinned tools

- jj `0.46.0`, installed from the release archive for the host or with `cargo install jj-cli --locked --version 0.46.0`.
- Rust `1.100.0` from `rust-toolchain.toml`.
- Blender `5.2 LTS` for authored assets.
- Cargo tools `cargo-nextest 0.9.149`, `cargo-deny 0.20.2`, and `cargo-about 0.9.2` in CI.

The jj release archive is verified by SHA-256 in `scripts/codex-cloud-setup.sh`. The workspace uses a non-colocated jj repository for local development. Set an identity before making a commit:

```text
jj config set --user user.name "Your Name"
jj config set --user user.email "you@example.com"
```

Commits use Conventional Commit messages and include a `Signed-off-by` trailer from the human author when they are ready for contribution. Configure the jj description template once with `jj config set --user templates.commit_description 'description ++ "\\n\\nSigned-off-by: " ++ author.name() ++ " <" ++ author.email() ++ ">"'`. Agents do not claim the human DCO sign-off.

## VS Code

`.vscode/extensions.json` recommends Rust Analyzer, TOML support, dependency auditing, LLDB, Error Lens, Markdown linting, and Jujutsu Kaizen (`jjk.jjk`). The latter identifier is confirmed by its Visual Studio Marketplace listing.

## Windows

Windows contributors need the MSVC Build Tools with the Desktop C++ workload. No full Visual Studio IDE is required. The Rust MSVC target, `cargo`, and jj are the supported command-line path.

