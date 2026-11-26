//! Tile inspection, production and seat-wide orders.

use super::{BoardView, Presenter, Target, atlas, visible};
use eframe::egui::{self, RichText, ScrollArea, Ui, vec2};
use histeries_sim::{
    BuildingKind, CityObservation, Command, DistrictKind, Feature, Observation, ProductionItem,
    UnitKind,
};

impl Presenter {
    pub(super) fn inspector(
        &mut self,
        ui: &mut Ui,
        board: &BoardView<'_>,
        order: &mut Option<Command>,
    ) {
        let view = board.observation;
        let Some(tile) = self.selected.and_then(|id| visible(view.board(), id)) else {
            ui.label("Select a visible tile on the map.");
            return;
        };
        ui.horizontal(|ui| {
            let (_, swatch) = ui.allocate_space(vec2(12.0, 12.0));
            ui.painter()
                .circle_filled(swatch.center(), 6.0, atlas::land_color(tile.terrain));
            ui.heading(format!("{:?}", tile.terrain));
        });
        ui.weak(format!(
            "Tile {}    ·    {} visible neighbors",
            tile.tile.index(),
            tile.neighbors.len()
        ));
        ui.horizontal_wrapped(|ui| {
            if tile.feature != Feature::None {
                ui.label(format!("{:?}", tile.feature));
            }
            if let Some(resource) = tile.resource {
                ui.label(format!("{resource:?}"));
            }
            if tile.river.is_some() {
                ui.label("River");
            }
            if tile.road {
                ui.label("Road");
            }
            if tile.district_reserved {
                ui.label("District site");
            }
        });
        for unit in view
            .visible_units()
            .iter()
            .filter(|unit| unit.tile == tile.tile)
        {
            ui.separator();
            ui.heading(format!("{:?}", unit.kind));
            let owner = unit.owner.map_or_else(
                || "Independent".to_owned(),
                |player| format!("Seat {}", player.index()),
            );
            ui.label(format!(
                "{owner}   ·   {} HP   ·   {} movement",
                unit.hp, unit.movement
            ));
            if unit.owner != Some(view.seat()) {
                continue;
            }
            let player = view.seat();
            let id = unit.id;
            ui.add_enabled_ui(board.can_order, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Move").clicked() {
                        self.start(Target::Move(id));
                    }
                    if unit.kind.is_military() && ui.button("Attack").clicked() {
                        self.start(Target::Attack(id));
                    }
                    if unit.kind.is_military() && ui.button("Capture city").clicked() {
                        self.start(Target::Capture(id));
                    }
                    if ui.button("Hold").clicked() {
                        *order = Some(Command::UnitHold { player, unit: id });
                    }
                    if ui.button("Heal").clicked() {
                        *order = Some(Command::UnitHeal { player, unit: id });
                    }
                    if unit.kind == UnitKind::Builder && ui.button("Build road").clicked() {
                        *order = Some(Command::UnitBuildRoad { player, unit: id });
                    }
                    if unit.kind == UnitKind::Settler && ui.button("Found city").clicked() {
                        *order = Some(Command::CitySettle { player, unit: id });
                    }
                });
            });
        }
        for city in view
            .visible_cities()
            .iter()
            .filter(|city| city.tile == tile.tile)
        {
            ui.separator();
            ui.heading(format!("City {}", city.id.index()));
            ui.label(format!(
                "Seat {}   ·   Population {}",
                city.owner.index(),
                city.population
            ));
            if let Some(own) = view.cities().iter().find(|own| own.id == city.id) {
                ui.add_enabled_ui(board.can_order, |ui| self.production(ui, view, own, order));
            }
        }
    }

    fn start(&mut self, target: Target) {
        self.target = Some(target);
        self.feedback = None;
    }

    fn production(
        &mut self,
        ui: &mut Ui,
        view: &Observation,
        city: &CityObservation,
        order: &mut Option<Command>,
    ) {
        let current = city
            .production
            .map(item_name)
            .unwrap_or_else(|| "Idle".into());
        ui.label(format!("Production   ·   {current}"));
        if let Some(item) = city.production {
            ui.add(
                egui::ProgressBar::new(city.production_stored as f32 / item.cost() as f32)
                    .text(format!("{} / {}", city.production_stored, item.cost())),
            );
        }
        if !city.waiting.is_empty() {
            ui.weak(format!(
                "Queue   ·   {}",
                city.waiting
                    .iter()
                    .copied()
                    .map(item_name)
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        ui.menu_button("Queue production", |ui| {
            ScrollArea::vertical().max_height(280.0).show(ui, |ui| {
                for item in [
                    UnitKind::Warrior,
                    UnitKind::Archer,
                    UnitKind::Settler,
                    UnitKind::Builder,
                ]
                .map(ProductionItem::Unit)
                .into_iter()
                .chain(BuildingKind::ALL.into_iter().map(ProductionItem::Building))
                {
                    if ui.button(item_name(item)).clicked() {
                        *order = Some(Command::CityEnqueueProduction {
                            player: view.seat(),
                            city: city.id,
                            item,
                        });
                        ui.close();
                    }
                }
            });
        });
        ui.menu_button("Place district", |ui| {
            for kind in [
                DistrictKind::Campus,
                DistrictKind::IndustrialZone,
                DistrictKind::Harbor,
            ] {
                if ui.button(words(kind.ruleset_id())).clicked() {
                    self.start(Target::District(city.id, kind));
                    ui.close();
                }
            }
        });
    }

    pub(super) fn realm_controls(
        &mut self,
        ui: &mut Ui,
        board: &BoardView<'_>,
        order: &mut Option<Command>,
    ) {
        let view = board.observation;
        ui.heading(format!("Seat {}", view.seat().index()));
        ui.label(format!(
            "{} cities   ·   {} units",
            view.cities().len(),
            view.units().len()
        ));
        ui.menu_button("Locate on map", |ui| {
            let cities = view
                .cities()
                .iter()
                .map(|city| (format!("City {}", city.id.index()), city.tile));
            let units = view
                .units()
                .iter()
                .map(|unit| (format!("{:?} {}", unit.kind, unit.id.index()), unit.tile));
            ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                for (name, id) in cities.chain(units) {
                    if ui.button(name).clicked() {
                        self.focus(view, id);
                        self.realm = false;
                        self.target = None;
                        ui.close();
                    }
                }
            });
        });
        ui.separator();
        let progress = view.progress();
        let rules = histeries_ruleset::base_ruleset();
        ui.add_enabled_ui(board.can_order, |ui| {
            let player = view.seat();
            if let Some(technology) = research_menu(
                ui,
                "Technology",
                progress.current_technology.as_deref(),
                progress.technology_progress,
                &progress.technologies,
                rules
                    .technologies()
                    .map(|t| (t.id(), t.cost(), t.prerequisites())),
            ) {
                *order = Some(Command::ResearchTechnology { player, technology });
            }
            if let Some(civic) = research_menu(
                ui,
                "Civic",
                progress.current_civic.as_deref(),
                progress.civic_progress,
                &progress.civics,
                rules
                    .civics()
                    .map(|c| (c.id(), c.cost(), c.prerequisites())),
            ) {
                *order = Some(Command::ResearchCivic { player, civic });
            }
        });
    }
}

fn research_menu<'a>(
    ui: &mut Ui,
    label: &str,
    current: Option<&str>,
    points: i64,
    known: &[String],
    options: impl Iterator<Item = (&'a str, u16, &'a [String])>,
) -> Option<String> {
    ui.label(RichText::new(label).strong());
    let status = current.map(words).unwrap_or_else(|| "Unassigned".into());
    ui.weak(format!("{status}   ·   {points} stored"));
    let mut selected = None;
    ui.menu_button(format!("Choose {}", label.to_lowercase()), |ui| {
        ScrollArea::vertical().max_height(270.0).show(ui, |ui| {
            for (id, cost, prerequisites) in
                options.filter(|(id, _, _)| !known.iter().any(|already| already == id))
            {
                let mut button = ui.button(format!("{}  ·  {cost}", words(id)));
                if !prerequisites.is_empty() {
                    button = button.on_hover_text(format!("Requires {}", prerequisites.join(", ")));
                }
                if button.clicked() {
                    selected = Some(id.to_owned());
                    ui.close();
                }
            }
        });
    });
    selected
}

fn item_name(item: ProductionItem) -> String {
    match item {
        ProductionItem::Unit(unit) => words(unit.ruleset_id()),
        ProductionItem::Building(building) => words(building.ruleset_id()),
        ProductionItem::District { kind, tile } => {
            format!("{} · tile {}", words(kind.ruleset_id()), tile.index())
        }
    }
}

fn words(id: &str) -> String {
    let mut chars = id.chars();
    let first = chars
        .next()
        .into_iter()
        .flat_map(char::to_uppercase)
        .collect::<String>();
    first + &chars.as_str().replace('_', " ")
}
