use super::{guest, host};
use crate::{
    Command, Feature, MapState, MatchHost, PlayerId, ResourceKind, RiverDirection, Session, Terrain,
};
use histeries_hex::{FlatHex, TileId};

fn check_field(field: i32, value: i32, expected: Command) {
    let (seat, mut game) = host();
    let setter = guest(&format!(
        r#"(module
        (import "histeries_editor_v1" "emit" (func $emit (param i32 i32 i32)))
        (func (export "apply") (result i32)
            i32.const 0 i32.const {field} i32.const {value} call $emit i32.const 0))"#
    ))
    .unwrap();
    assert_eq!(setter.apply(&mut game, seat).unwrap(), 1);
    assert_eq!(game.session().replay().commands(), &[expected]);
    let reader = guest(&format!(
        r#"(module
        (import "histeries_editor_v1" "read" (func $read (param i32 i32) (result i32)))
        (func (export "apply") (result i32)
            i32.const 0 i32.const {field} call $read i32.const {value} i32.ne))"#
    ))
    .unwrap();
    assert_eq!(reader.apply(&mut game, seat).unwrap(), 0);
}

#[test]
fn abi_v1_field_values_match_named_commands_and_survive_a_second_invocation() {
    let player = PlayerId::new(7);
    let tile = TileId::new(0);
    for (value, terrain) in [
        Terrain::Plains,
        Terrain::Grassland,
        Terrain::Desert,
        Terrain::Tundra,
        Terrain::Coast,
        Terrain::Ocean,
        Terrain::Hill,
        Terrain::Mountain,
    ]
    .into_iter()
    .enumerate()
    {
        check_field(
            0,
            value as i32,
            Command::EditorSetTerrain {
                player,
                tile,
                terrain,
            },
        );
    }
    for (value, feature) in [
        Feature::None,
        Feature::Forest,
        Feature::Jungle,
        Feature::Marsh,
    ]
    .into_iter()
    .enumerate()
    {
        check_field(
            1,
            value as i32,
            Command::EditorSetFeature {
                player,
                tile,
                feature,
            },
        );
    }
    for (value, river) in [
        None,
        Some(RiverDirection::NorthEast),
        Some(RiverDirection::East),
        Some(RiverDirection::SouthEast),
        Some(RiverDirection::SouthWest),
        Some(RiverDirection::West),
        Some(RiverDirection::NorthWest),
    ]
    .into_iter()
    .enumerate()
    {
        check_field(
            2,
            value as i32 - 1,
            Command::EditorSetRiver {
                player,
                tile,
                river,
            },
        );
    }
    for (value, resource) in [
        None,
        Some(ResourceKind::Wheat),
        Some(ResourceKind::Stone),
        Some(ResourceKind::Iron),
        Some(ResourceKind::Gold),
    ]
    .into_iter()
    .enumerate()
    {
        check_field(
            3,
            value as i32 - 1,
            Command::EditorSetResource {
                player,
                tile,
                resource,
            },
        );
    }
}

fn neighbor_probe(tile: i32, slot: i32, expected: i32) -> super::Package {
    guest(&format!(
        r#"(module
        (import "histeries_editor_v1" "neighbor" (func $neighbor (param i32 i32) (result i32)))
        (func (export "apply") (result i32)
            i32.const {tile} i32.const {slot} call $neighbor i32.const {expected} i32.ne))"#
    ))
    .unwrap()
}

#[test]
fn abi_v1_neighbors_sort_pad_and_reject_invalid_indices() {
    let seat = PlayerId::new(7);
    for (wrap, tile, expected) in [
        (false, 5, [1, 2, 4, 6, 8, 9]),
        (false, 0, [1, 4, -1, -1, -1, -1]),
        (true, 0, [1, 3, 4, 7, 8, 9]),
    ] {
        let map = MapState::blank(FlatHex::new(4, 3, wrap, wrap).unwrap());
        let mut game = MatchHost::new(Session::with_map([seat, PlayerId::new(9)], map));
        for (slot, value) in expected.into_iter().enumerate() {
            assert_eq!(
                neighbor_probe(tile, slot as i32, value)
                    .apply(&mut game, seat)
                    .unwrap(),
                0
            );
        }
        for (tile, slot) in [(tile, 6), (12, 0)] {
            let error = neighbor_probe(tile, slot, 0)
                .apply(&mut game, seat)
                .unwrap_err();
            assert!(error.contains("invalid tile or neighbor slot"), "{error}");
        }
    }
}

#[test]
fn bounded_calls_fit_until_the_call_or_value_stack_limit() {
    let (seat, mut game) = host();
    let save = game.session().save_bytes();
    let replay = game.session().replay().encode();
    for (depth, locals, fits) in [
        (62, 1, true),
        (63, 1, false),
        (64, 1, false),
        (29, 256, true),
        (30, 256, false),
        (31, 256, false),
        (32, 256, false),
    ] {
        let locals_decl = " i32".repeat(locals);
        let module = guest(&format!(
            r#"(module
            (func $visit (param $n i32) (result i32) (local {locals_decl})
                local.get $n local.set {locals}
                local.get $n if
                    local.get $n i32.const 1 i32.sub call $visit drop
                end
                local.get {locals})
            (func (export "apply") (result i32) i32.const {depth} call $visit drop i32.const 0))"#
        ))
        .unwrap();
        let result = module.apply(&mut game, seat);
        if fits {
            assert_eq!(
                result.unwrap_or_else(|e| panic!("depth {depth}, locals {locals}: {e}")),
                0
            );
        } else {
            let error = result
                .err()
                .unwrap_or_else(|| panic!("unexpected success for depth {depth}, locals {locals}"));
            assert!(error.to_lowercase().contains("stack"), "{error}");
        }
        assert_eq!(save, game.session().save_bytes());
        assert_eq!(replay, game.session().replay().encode());
    }
}

#[test]
fn abi_v1_rejects_unassigned_field_values() {
    let (seat, mut game) = host();
    let opening = game.session().save_bytes();
    for (field, invalid) in [(0, [-1, 8]), (1, [-1, 4]), (2, [-2, 6]), (3, [-2, 4])] {
        for value in invalid {
            let package = guest(&format!(
                r#"(module
                (import "histeries_editor_v1" "emit" (func $emit (param i32 i32 i32)))
                (func (export "apply") (result i32)
                    i32.const 0 i32.const {field} i32.const {value} call $emit i32.const 0))"#
            ))
            .unwrap();
            let error = package.apply(&mut game, seat).unwrap_err();
            assert!(error.contains("invalid tile field or value"), "{error}");
            assert_eq!(game.session().save_bytes(), opening);
        }
    }
}

#[test]
fn abi_v1_proposal_and_signature_limits_are_fixed() {
    let run = r#"(func (export "apply") (result i32) i32.const 0)"#;
    for (item, admitted) in [
        ("(global (mut i32) (i32.const 0))", true),
        (
            "(func (param i32) (result i32) local.get 0 i32.extend8_s)",
            true,
        ),
        ("(func (result i32 i32) i32.const 0 i32.const 1)", true),
        (
            "(memory 1) (func i32.const 0 i32.const 0 i32.const 0 memory.copy)",
            true,
        ),
        ("(func (result funcref) ref.null func)", true),
        ("(func $a) (func return_call $a)", true),
        ("(global i32 (i32.add (i32.const 1) (i32.const 2)))", true),
        ("(memory i64 1)", false),
        ("(func v128.const i32x4 0 0 0 0 drop)", false),
        ("(func f32.const 0 i32.trunc_sat_f32_s drop)", false),
        ("(memory 1) (memory 1)", false),
    ] {
        assert_eq!(
            guest(&format!("(module {item} {run})")).is_ok(),
            admitted,
            "{item}"
        );
    }
    for count in [32, 33] {
        let params = " i32".repeat(count);
        let module = format!("(module (func (param {params})) {run})");
        assert_eq!(guest(&module).is_ok(), count == 32);
    }
}

#[test]
fn empty_editor_batches_reject_closed_turns() {
    let seat = PlayerId::new(7);
    let empty = guest(r#"(module (func (export "apply") (result i32) i32.const 0))"#).unwrap();
    for victory in [false, true] {
        let map = MapState::blank(FlatHex::new(4, 3, false, false).unwrap());
        let mut session = Session::with_map([seat, PlayerId::new(9)], map);
        if victory {
            session
                .state
                .victories
                .insert(seat, crate::VictoryKind::Science);
        }
        let mut host = MatchHost::new(session);
        if !victory {
            host.dispatch(seat, Command::TurnEnd { player: seat })
                .unwrap();
        }
        let before = host.session().save_bytes();
        assert!(host.dispatch_editor_batch(seat, []).is_err());
        assert!(empty.apply(&mut host, seat).is_err());
        assert_eq!(host.session().save_bytes(), before);
    }
}
