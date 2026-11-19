use super::*;

#[derive(Default)]
struct MemoryStorage(BTreeMap<String, String>);
impl Storage for MemoryStorage {
    fn get_string(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }
    fn set_string(&mut self, key: &str, value: String) {
        self.0.insert(key.into(), value);
    }
    fn remove_string(&mut self, key: &str) {
        self.0.remove(key);
    }
    fn flush(&mut self) {}
}

#[test]
fn installed_source_and_enablement_survive_restart_and_removal() {
    let mut manager = ModManager::restore(None);
    assert!(
        manager
            .apply("woodland", |_| panic!("disabled package ran"))
            .is_err()
    );
    assert!(manager.import(SAMPLE_MANIFEST, SAMPLE_MODULE).is_err());
    assert_eq!(manager.packages.len(), 1);
    manager.packages.get_mut("woodland").unwrap().source.enabled = true;
    manager.changed = true;
    let mut storage = MemoryStorage::default();
    manager.save_changes(Some(&mut storage));
    let mut restored = ModManager::restore(Some(&storage));
    let status = restored
        .apply("woodland", |p| {
            assert_eq!(p.version(), "1.0.0");
            assert!(!p.description().is_empty());
            Ok(2)
        })
        .unwrap();
    assert!(status.contains("2 editor commands"));
    restored.packages.remove("woodland");
    restored.changed = true;
    restored.save_changes(Some(&mut storage));
    assert!(ModManager::restore(Some(&storage)).packages.is_empty());
}

#[test]
fn a_failed_import_preserves_the_installed_library_and_corrupt_settings_are_reported() {
    let mut manager = ModManager::restore(None);
    assert!(manager.import(SAMPLE_MANIFEST, b"invalid").is_err());
    assert_eq!(manager.packages.len(), 1);
    let mut storage = MemoryStorage::default();
    storage.set_string(STORAGE_KEY, "not json".into());
    let restored = ModManager::restore(Some(&storage));
    assert!(restored.notice.is_some_and(|r| r.is_err()));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn native_folder_and_member_import_resolve_the_same_package() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    std::fs::write(dir.join("manifest.toml"), SAMPLE_MANIFEST).unwrap();
    std::fs::write(dir.join("plugin.wasm"), SAMPLE_MODULE).unwrap();
    let module = dir.join("plugin.wasm");
    for selected in [dir, module.as_path()] {
        let source = browser::dropped_directory([selected].into_iter()).unwrap();
        let (manifest, wasm) = Package::read_files(&source).unwrap();
        let mut manager = ModManager::default();
        manager.import(&manifest, &wasm).unwrap();
        assert!(!manager.packages["woodland"].source.enabled);
    }
}

#[test]
fn editor_manager_enables_and_applies_the_bundled_package_through_mouse_input() {
    use crate::editor::Editor;
    use egui::{Context, Event, FullOutput, PointerButton, Pos2, RawInput, Rect, Shape, vec2};
    let ctx = Context::default();
    crate::font::install(&ctx);
    let mut manager = ModManager::restore(None);
    manager.open();
    let mut editor = Editor::blank(4, 3, 2).unwrap();
    let opening = editor.bytes().to_vec();
    let input = |events: Vec<Event>| RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(860.0, 680.0))),
        events,
        ..Default::default()
    };
    let mut frame = |events| -> FullOutput {
        let mut out = ctx.run_ui(input(events), |ui| {
            manager.show(ui.ctx(), |p| editor.apply_package(p))
        });
        out.textures_delta.clear();
        out
    };
    let center = |out: &FullOutput, label: &str| {
        out.shapes
            .iter()
            .find_map(|s| match &s.shape {
                Shape::Text(t) if t.galley.text() == label => {
                    Some(t.visual_bounding_rect().center())
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing {label}"))
    };
    let press = |at: Pos2, pressed| {
        vec![
            Event::PointerMoved(at),
            Event::PointerButton {
                pos: at,
                button: PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    };
    frame(vec![]);
    let output = frame(vec![]);
    let enable = center(&output, "Enabled");
    frame(press(enable, true));
    frame(press(enable, false));
    let output = frame(vec![]);
    let apply = center(&output, "Apply to scenario");
    frame(press(apply, true));
    frame(press(apply, false));
    assert_ne!(editor.bytes(), opening.as_slice());
    let draft = histeries_sim::ScenarioDraft::open(editor.bytes()).unwrap();
    assert_eq!(
        draft.board().visible_tiles()[0].feature,
        histeries_sim::Feature::Forest
    );
    assert!(
        manager
            .notice
            .as_ref()
            .is_some_and(|r| r.as_ref().is_ok_and(|m| m.contains("1 editor command")))
    );
    let mut edit_frame = |events| {
        let mut out = ctx.run_ui(input(events), |ui| editor.show(ui, |_| {}));
        out.textures_delta.clear();
        out
    };
    edit_frame(vec![]);
    let undo = center(&edit_frame(vec![]), "Undo");
    edit_frame(press(undo, true));
    edit_frame(press(undo, false));
    assert_eq!(editor.bytes(), opening.as_slice());
}
