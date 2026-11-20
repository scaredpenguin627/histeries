//! Full-board authoring tools for a local scenario.
use super::{
    atlas::{self, Camera, GOLD, INK, PAPER},
    frame, visible,
};
use eframe::egui::{self, Button, Color32, ComboBox, Frame, Panel, RichText, Ui};
use histeries_hex::TileId;
use histeries_rules_core::PlayerId;
use histeries_sim::{
    Feature, ResourceKind, RiverDirection, ScenarioBrush as Brush, ScenarioDraft, Terrain, UnitKind,
};

pub(crate) struct EditorPresenter {
    source: String,
    camera: Camera,
    brush: Brush,
    owner: PlayerId,
    feedback: String,
    selected: Option<TileId>,
    fit: bool,
}

impl EditorPresenter {
    pub(crate) fn new(owner: PlayerId, source: String) -> Self {
        Self {
            owner,
            source,
            camera: Camera {
                min_radius: 0.01,
                ..Camera::default()
            },
            brush: Brush::Inspect,
            feedback: String::new(),
            selected: None,
            fit: true,
        }
    }

    pub(crate) fn owner(&self) -> PlayerId {
        self.owner
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn set_source(&mut self, name: String) {
        self.source = name;
    }

    fn brushes(&mut self, ui: &mut Ui) {
        ui.selectable_value(&mut self.brush, Brush::Inspect, "Inspect");
        brush_menu(
            ui,
            "Terrain",
            &mut self.brush,
            (0..=u8::MAX)
                .filter_map(Terrain::from_u8)
                .map(Brush::Terrain),
        );
        brush_menu(
            ui,
            "Feature",
            &mut self.brush,
            (0..=u8::MAX)
                .filter_map(Feature::from_u8)
                .map(Brush::Feature),
        );
        brush_menu(
            ui,
            "Resource",
            &mut self.brush,
            std::iter::once(None)
                .chain((0..=u8::MAX).filter_map(ResourceKind::from_u8).map(Some))
                .map(Brush::Resource),
        );
        brush_menu(
            ui,
            "River",
            &mut self.brush,
            std::iter::once(None)
                .chain((0..=u8::MAX).filter_map(RiverDirection::from_u8).map(Some))
                .map(Brush::River),
        );
        brush_menu(
            ui,
            "Place",
            &mut self.brush,
            (0..=u8::MAX)
                .filter_map(UnitKind::from_u8)
                .map(Brush::Unit)
                .chain([Brush::City, Brush::Road]),
        );
    }

    pub(crate) fn show(
        &mut self,
        root: &mut Ui,
        draft: &mut ScenarioDraft,
        menu: impl FnOnce(&mut Ui),
    ) {
        Panel::top("editor_heading")
            .frame(frame(INK, 12, 8))
            .show(root, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new("Scenario editor")
                            .size(24.0)
                            .color(PAPER)
                            .strong(),
                    );
                    menu(ui);
                    ui.weak(format!("{} · Turn {}", self.source, draft.board().turn()));
                    ui.separator();
                    if ui
                        .add_enabled(draft.can_undo(), Button::new("Undo"))
                        .on_disabled_hover_text("No earlier position is available. Undo history keeps up to 32 previous positions within 16 MiB.")
                        .clicked()
                    {
                        self.feedback = draft.travel(false).err().unwrap_or_default();
                    }
                    if ui
                        .add_enabled(draft.can_redo(), Button::new("Redo"))
                        .clicked()
                    {
                        self.feedback = draft.travel(true).err().unwrap_or_default();
                    }
                    if ui.button("Fit map").clicked() {
                        self.fit = true;
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    self.brushes(ui);
                    ComboBox::from_id_salt("editor_owner")
                        .selected_text(format!("Seat {}", self.owner.index()))
                        .show_ui(ui, |ui| {
                            for seat in draft.seats() {
                                ui.selectable_value(
                                    &mut self.owner,
                                    *seat,
                                    format!("Seat {}", seat.index()),
                                );
                            }
                        });
                });
            });
        Panel::bottom("editor_status")
            .frame(frame(INK, 12, 6))
            .show(root, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(GOLD, format!("Brush · {}", label(self.brush)));
                    if let Some(tile) = self.selected.and_then(|id| visible(draft.board(), id)) {
                        ui.label(format!("Tile {} · {:?}", tile.tile.index(), tile.terrain));
                        if tile.feature != Feature::None {
                            ui.label(format!("{:?}", tile.feature));
                        }
                        if let Some(resource) = tile.resource {
                            ui.label(format!("{resource:?}"));
                        }
                        if let Some(edge) = tile.river {
                            ui.label(format!("River · {edge:?}"));
                        }
                        if tile.road {
                            ui.label("Road");
                        }
                        for city in draft
                            .board()
                            .visible_cities()
                            .iter()
                            .filter(|c| c.tile == tile.tile)
                        {
                            ui.label(format!(
                                "City {} · Seat {}",
                                city.id.index(),
                                city.owner.index()
                            ));
                        }
                        for unit in draft
                            .board()
                            .visible_units()
                            .iter()
                            .filter(|u| u.tile == tile.tile)
                        {
                            let owner = unit
                                .owner
                                .map(|p| format!(" · Seat {}", p.index()))
                                .unwrap_or_default();
                            ui.label(format!("{:?} {}{owner}", unit.kind, unit.id.index()));
                        }
                    }
                    if self.feedback.is_empty() {
                        ui.weak(if self.brush == Brush::Inspect {
                            "Click to inspect · Drag to pan · Scroll to zoom"
                        } else {
                            "Click to apply · Drag to pan · Scroll to zoom"
                        });
                    } else {
                        ui.colored_label(Color32::LIGHT_RED, &self.feedback);
                    }
                });
            });
        egui::CentralPanel::default()
            .frame(Frame::NONE)
            .show(root, |ui| {
                let view = draft.board();
                if self.fit && ui.available_rect_before_wrap().is_positive() {
                    self.camera.fit(view, ui.available_rect_before_wrap());
                    self.fit = false;
                }
                let (response, painter, hovered) = atlas::surface(ui, view, &mut self.camera);
                atlas::occupants(&painter, response.rect, view, &self.camera, self.owner);
                for (id, color) in [(self.selected, GOLD), (hovered, PAPER)] {
                    if let Some(tile) = id.and_then(|id| visible(view, id)) {
                        atlas::outline(
                            &painter,
                            self.camera.project(tile.embedding, response.rect),
                            self.camera.radius,
                            color,
                        );
                    }
                }
                atlas::compass(&painter, response.rect);
                if response.clicked()
                    && let Some(tile) = hovered
                {
                    self.selected = Some(tile);
                    self.feedback = draft
                        .apply(self.brush, tile, self.owner)
                        .err()
                        .unwrap_or_default();
                }
            });
    }
}

fn brush_menu(
    ui: &mut Ui,
    title: &str,
    selected: &mut Brush,
    choices: impl IntoIterator<Item = Brush>,
) {
    ui.menu_button(title, |ui| {
        for brush in choices {
            if ui
                .selectable_label(*selected == brush, label(brush))
                .clicked()
            {
                *selected = brush;
                ui.close();
            }
        }
    });
}

fn label(brush: Brush) -> String {
    match brush {
        Brush::Inspect => "Inspect".into(),
        Brush::Terrain(t) => format!("{t:?}"),
        Brush::Feature(Feature::None) => "Clear feature".into(),
        Brush::Feature(f) => format!("{f:?}"),
        Brush::Resource(None) => "Clear resource".into(),
        Brush::Resource(Some(r)) => format!("{r:?}"),
        Brush::River(None) => "Clear river".into(),
        Brush::River(Some(direction)) => format!("River · {direction:?}"),
        Brush::Unit(kind) => format!("{kind:?}"),
        Brush::City => "Found city".into(),
        Brush::Road => "Build road".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{
        Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, pos2, vec2,
    };

    #[test]
    fn map_click_applies_the_active_brush_to_a_blank_scenario() {
        let ctx = Context::default();
        crate::font::install(&ctx);
        let mut draft = ScenarioDraft::blank(1, 1, 2).unwrap();
        let mut editor = EditorPresenter::new(draft.seats()[0], "Test".into());
        editor.brush = Brush::Terrain(Terrain::Grassland);
        let mut frame = |events| {
            let input = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(840.0, 600.0))),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                editor.show(ui, &mut draft, |ui| {
                    ui.label("Scenario");
                })
            });
            output.textures_delta.clear();
        };
        frame(vec![]);
        frame(vec![]);
        let at = pos2(420.0, 320.0);
        for pressed in [true, false] {
            frame(vec![
                Event::PointerMoved(at),
                Event::PointerButton {
                    pos: at,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ]);
        }
        assert_eq!(editor.selected, Some(TileId::new(0)));
        assert!(draft.can_undo());
        assert_eq!(draft.board().visible_tiles()[0].terrain, Terrain::Grassland);
    }
}
