mod abi;

use super::*;
use crate::{Command, Feature, MapState, Session};
use histeries_hex::{FlatHex, TileId};

const MANIFEST: &[u8] = include_bytes!("../../../../data/extensions/woodland/manifest.toml");
const EXAMPLE: &[u8] = include_bytes!("../../../../data/extensions/woodland/plugin.wasm");

fn host() -> (PlayerId, MatchHost) {
    let seat = PlayerId::new(7);
    let map = MapState::blank(FlatHex::new(4, 3, false, false).unwrap());
    (
        seat,
        MatchHost::new(Session::with_map([seat, PlayerId::new(9)], map)),
    )
}

fn guest(wat: &str) -> Result<Package, String> {
    Package::from_bytes(MANIFEST, &wat::parse_str(wat).unwrap())
}

#[test]
fn example_source_matches_package_and_replays_without_the_guest() {
    let source = include_str!("../../../../data/extensions/woodland/plugin.wat");
    assert_eq!(wat::parse_str(source).unwrap(), EXAMPLE);
    let package = Package::from_bytes(MANIFEST, EXAMPLE).unwrap();
    let (seat, mut first) = host();
    let (_, mut second) = host();
    for game in [&mut first, &mut second] {
        assert_eq!(package.apply(game, seat).unwrap(), 1);
        assert_eq!(
            game.session().replay().commands(),
            &[Command::EditorSetFeature {
                player: seat,
                tile: TileId::new(0),
                feature: Feature::Forest,
            }]
        );
        assert_eq!(
            game.session().state_hash(),
            game.session().replay().replay().unwrap().state_hash()
        );
    }
    assert_eq!(first.session().save_bytes(), second.session().save_bytes());
}

#[test]
fn admission_checks_module_interface_and_limits() {
    let invalid = [
        r#"(module (import "wasi_snapshot_preview1" "random_get" (func)) (func (export "apply") (result i32) i32.const 0))"#,
        r#"(module (func (export "apply") (result i32) f32.const 1 i32.reinterpret_f32))"#,
        r#"(module (func $start) (start $start) (func (export "apply") (result i32) i32.const 0))"#,
        r#"(module (memory 5) (func (export "apply") (result i32) i32.const 0))"#,
        r#"(module (table 65 funcref) (func (export "apply") (result i32) i32.const 0))"#,
        r#"(module (func (export "apply") (param i32)))"#,
    ];
    for source in invalid {
        assert!(guest(source).is_err(), "admitted {source}");
    }
    let unknown_abi = std::str::from_utf8(MANIFEST)
        .unwrap()
        .replace("abi = 1", "abi = 2");
    assert!(Package::from_bytes(unknown_abi.as_bytes(), EXAMPLE).is_err());
    assert!(Package::from_bytes(MANIFEST, &vec![0; MAX_MODULE_BYTES + 1]).is_err());
    assert!(Package::from_bytes(&vec![0; MAX_MANIFEST_BYTES + 1], EXAMPLE).is_err());
}

#[test]
fn traps_and_registry_rejections_discard_all_proposed_edits() {
    let (seat, mut game) = host();
    let save = game.session().save_bytes();
    let replay = game.session().replay().encode();
    let cases = [
        ("(loop $spin br $spin)", "fuel"),
        ("i32.const 5 memory.grow drop", "grow"),
        ("i32.const 12 i32.const 1 i32.const 1 call $emit", "tile"),
        ("i32.const 0 i32.const 8 i32.const 1 call $emit", "field"),
        (
            "(loop $many i32.const 0 i32.const 1 i32.const 1 call $emit br $many)",
            "edit limit",
        ),
        ("i32.const 1 return", "status"),
    ];
    for (tail, expected) in cases {
        let module = format!(
            r#"(module
            (import "histeries_editor_v1" "emit" (func $emit (param i32 i32 i32)))
            (memory 1) (func (export "apply") (result i32)
                i32.const 0 i32.const 1 i32.const 1 call $emit {tail} i32.const 0))"#
        );
        let error = guest(&module).unwrap().apply(&mut game, seat).unwrap_err();
        assert!(
            error.to_lowercase().contains(expected),
            "{expected}: {error}"
        );
        assert_eq!(save, game.session().save_bytes());
        assert_eq!(replay, game.session().replay().encode());
    }
}

#[test]
fn neighbors_are_sorted_and_pending_edits_are_invisible() {
    let (seat, mut game) = host();
    let package = guest(
        r#"(module
        (import "histeries_editor_v1" "neighbor" (func $neighbor (param i32 i32) (result i32)))
        (import "histeries_editor_v1" "emit" (func $emit (param i32 i32 i32)))
        (import "histeries_editor_v1" "read" (func $read (param i32 i32) (result i32)))
        (func (export "apply") (result i32)
            i32.const 0 i32.const 0 call $neighbor i32.const 1 i32.const 1 call $emit
            i32.const 0 i32.const 0 call $neighbor i32.const 1 call $read))"#,
    )
    .unwrap();
    assert_eq!(package.apply(&mut game, seat).unwrap(), 1);
    assert_eq!(
        game.session().replay().commands(),
        &[Command::EditorSetFeature {
            player: seat,
            tile: TileId::new(1),
            feature: Feature::Forest
        }]
    );
}

#[test]
fn manifest_fields_reject_unsupported_or_ambiguous_metadata() {
    let template = std::str::from_utf8(MANIFEST).unwrap();
    for manifest in [
        template.replace("woodland", "Woodland"),
        template.replace("woodland", "-woodland"),
        template.replace("woodland", "woodland-"),
        template.replace("woodland", &"w".repeat(65)),
        template.replace("Plant a woodland", "Plant\\na woodland"),
        format!("{template}unknown = true\n"),
    ] {
        assert!(Package::from_bytes(manifest.as_bytes(), EXAMPLE).is_err());
    }
}

#[test]
fn woodland_fuel_is_identical_for_fresh_and_reused_modules() {
    let (seat, game) = host();
    let map = game.session().editor_map(seat).unwrap();
    let measure = |package: &Package| {
        let (mut store, entry) = runtime::prepare(&package.module, Some((seat, map))).unwrap();
        let initial = store.get_fuel().unwrap();
        assert_eq!(entry.call(&mut store, ()).unwrap(), 0);
        initial - store.get_fuel().unwrap()
    };
    let package = Package::from_bytes(MANIFEST, EXAMPLE).unwrap();
    let cost = measure(&package);
    assert_eq!(cost, measure(&package));
    assert_eq!(
        cost,
        measure(&Package::from_bytes(MANIFEST, EXAMPLE).unwrap())
    );
    assert_eq!(cost, 29);
}

#[test]
fn woodland_with_no_open_plains_is_a_successful_empty_batch() {
    let (seat, mut game) = host();
    let package = Package::from_bytes(MANIFEST, EXAMPLE).unwrap();
    for _ in 0..12 {
        assert_eq!(package.apply(&mut game, seat).unwrap(), 1);
    }
    let save = game.session().save_bytes();
    let replay = game.session().replay().encode();
    assert_eq!(package.apply(&mut game, seat).unwrap(), 0);
    assert_eq!(save, game.session().save_bytes());
    assert_eq!(replay, game.session().replay().encode());
}
