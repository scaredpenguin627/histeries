//! Origin-local storage for game and scenario save bytes.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use web_sys::Storage;

pub(crate) const GAME: &str = "histeries.world.v1";
pub(crate) const SCENARIO: &str = "histeries.scenario.v1";

fn storage() -> Result<Storage, String> {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .ok_or_else(|| "This browser has disabled local storage for this site.".into())
}

pub(crate) fn write(slot: &str, bytes: &[u8]) -> Result<(), String> {
    storage()?
        .set_item(slot, &STANDARD.encode(bytes))
        .map_err(|_| {
            "The browser could not write the save. Check storage permissions and available space."
                .into()
        })
}

pub(crate) fn read(slot: &str) -> Result<Vec<u8>, String> {
    let value = storage()?
        .get_item(slot)
        .map_err(|_| "The browser could not read the save.")?
        .ok_or("This save slot is empty.")?;
    STANDARD
        .decode(value)
        .map_err(|e| format!("Invalid saved data: {e}"))
}
