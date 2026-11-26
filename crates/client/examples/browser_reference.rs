//! Generate native saves for the browser acceptance test on the same seed and orders.
#![forbid(unsafe_code)]

use histeries_client::ClientSession;
use histeries_hex::TileId;
use histeries_sim::{Command, Feature, MatchHost, Session, extensions::Package};
use std::{env, error::Error, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let directory = PathBuf::from(args.next().ok_or("expected an output directory")?);
    fs::create_dir_all(&directory)?;
    let mut game = ClientSession::new(7)?;
    let original = game.save_bytes();
    fs::write(directory.join("before-woodland.hst"), &original)?;
    game.end_turn()?;
    fs::write(directory.join("turn-1.hst"), game.save_bytes())?;

    let mut globe = ClientSession::globe(7, 3)?;
    globe.end_turn()?;
    fs::write(directory.join("globe-turn-1.hst"), globe.save_bytes())?;

    if let Some(opening) = args.next() {
        let mut expedition = ClientSession::from_save_bytes(&fs::read(opening)?)?;
        let seat = expedition.observation().seat();
        let city = expedition.observation().cities()[0].id;
        expedition.dispatch(Command::TheatreLaunch {
            player: seat,
            theatre: "orbit".to_owned(),
            city,
        })?;
        expedition.end_turn()?;
        // Tile one borders the entry on the bundled orbital chart.
        expedition.dispatch(Command::TheatreMove {
            player: seat,
            theatre: "orbit".to_owned(),
            tile: TileId::new(1),
        })?;
        let chart = expedition
            .observation()
            .theatres()
            .iter()
            .find(|board| board.id == "orbit")
            .unwrap();
        assert_eq!(chart.expedition, Some(TileId::new(1)));
        assert_eq!(chart.tiles.iter().filter(|site| site.visited).count(), 2);
        fs::write(
            directory.join("orbital-survey.hst"),
            expedition.save_bytes(),
        )?;
    }

    let session = Session::from_save_bytes(&original)?;
    let seat = session.seat_ids().next().ok_or("expected a player")?;
    let mut host = MatchHost::new(session);
    let package = Package::from_bytes(
        include_bytes!("../../../data/extensions/woodland/manifest.toml"),
        include_bytes!("../../../data/extensions/woodland/plugin.wasm"),
    )?;
    assert_eq!(package.apply(&mut host, seat)?, 1);
    assert_eq!(
        host.session().replay().commands(),
        &[Command::EditorSetFeature {
            player: seat,
            tile: TileId::new(1),
            feature: Feature::Forest,
        }]
    );
    fs::write(
        directory.join("after-woodland.hst"),
        host.session().save_bytes(),
    )?;
    Ok(())
}
