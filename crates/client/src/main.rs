//! Native desktop entry point.

#![forbid(unsafe_code)]

#[cfg(not(any(target_env = "ohos", target_arch = "wasm32")))]
fn main() -> std::process::ExitCode {
    match histeries_client::run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Histeries: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(any(target_env = "ohos", target_arch = "wasm32"))]
fn main() {}
