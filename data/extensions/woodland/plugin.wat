;; Original Histeries sample program. SPDX-License-Identifier: MIT OR Apache-2.0
(module
  (import "histeries_editor_v1" "tile_count" (func $count (result i32)))
  (import "histeries_editor_v1" "read" (func $read (param i32 i32) (result i32)))
  (import "histeries_editor_v1" "emit" (func $emit (param i32 i32 i32)))
  (func (export "apply") (result i32)
    (local $tile i32)
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $tile) (call $count)))
        (if (i32.and
              (i32.eqz (call $read (local.get $tile) (i32.const 0)))
              (i32.eqz (call $read (local.get $tile) (i32.const 1))))
          (then
            (call $emit (local.get $tile) (i32.const 1) (i32.const 1))
            (br $done)))
        (local.set $tile (i32.add (local.get $tile) (i32.const 1)))
        (br $next)))
    (i32.const 0)))
