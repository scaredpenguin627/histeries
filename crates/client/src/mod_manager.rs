//! Installed editor packages and their user-selected activation state.

use std::collections::BTreeMap;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use eframe::{Storage, egui};
use histeries_sim::extensions::{MAX_MANIFEST_BYTES, MAX_MODULE_BYTES, Package};
use serde::{Deserialize, Serialize};

#[cfg(not(target_arch = "wasm32"))]
mod browser;
#[cfg(test)]
mod tests;
#[cfg(target_arch = "wasm32")]
mod web;

const LIMIT: usize = 16;
const STORAGE_KEY: &str = "histeries.editor-packages.v1";
const SAMPLE_MANIFEST: &[u8] = include_bytes!("../../../data/extensions/woodland/manifest.toml");
const SAMPLE_MODULE: &[u8] = include_bytes!("../../../data/extensions/woodland/plugin.wasm");

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    manifest: String,
    module: String,
    enabled: bool,
}

struct Installed {
    package: Package,
    source: Source,
}

#[derive(Default)]
pub(crate) struct ModManager {
    packages: BTreeMap<String, Installed>,
    open: bool,
    changed: bool,
    notice: Option<Result<String, String>>,
    #[cfg(not(target_arch = "wasm32"))]
    folders: Option<browser::Folders>,
    #[cfg(target_arch = "wasm32")]
    loading: Option<web::Upload>,
}

impl ModManager {
    pub(crate) fn restore(storage: Option<&dyn Storage>) -> Self {
        let mut manager = Self::default();
        if let Some(data) = storage.and_then(|s| s.get_string(STORAGE_KEY)) {
            if let Err(e) = manager.restore_sources(&data) {
                manager.notice = Some(Err(e));
            }
        } else if let Err(e) = manager.import(SAMPLE_MANIFEST, SAMPLE_MODULE) {
            manager.notice = Some(Err(e));
        }
        manager.changed = false;
        manager
    }

    fn restore_sources(&mut self, data: &str) -> Result<(), String> {
        // Includes escaped UTF-8 manifest text and the base64-encoded module.
        if data.len() > LIMIT * (MAX_MANIFEST_BYTES * 6 + MAX_MODULE_BYTES * 2 + 128) {
            return Err("Saved package library exceeds its size limit.".into());
        }
        let entries: Vec<Source> =
            serde_json::from_str(data).map_err(|e| format!("Read package library: {e}"))?;
        if entries.len() > LIMIT {
            return Err(format!("A library holds up to {LIMIT} packages."));
        }
        let mut invalid = Vec::new();
        for (i, source) in entries.into_iter().enumerate() {
            let restore = || -> Result<Package, String> {
                if source.module.len() > MAX_MODULE_BYTES.div_ceil(3) * 4 {
                    return Err("module exceeds size limit".into());
                }
                let module = STANDARD.decode(&source.module).map_err(|e| e.to_string())?;
                Package::from_bytes(source.manifest.as_bytes(), &module)
            };
            let result = restore().and_then(|p| self.insert(p, source));
            if let Err(reason) = result {
                invalid.push(format!("Package {}: {reason}", i + 1));
            }
        }
        if invalid.is_empty() {
            Ok(())
        } else {
            Err(invalid.join("\n"))
        }
    }

    pub(crate) fn save_changes(&mut self, storage: Option<&mut (dyn Storage + 'static)>) {
        if !self.changed {
            return;
        }
        let Some(storage) = storage else { return };
        let sources: Vec<_> = self.packages.values().map(|e| &e.source).collect();
        match serde_json::to_string(&sources) {
            Ok(data) => {
                storage.set_string(STORAGE_KEY, data);
                storage.flush();
                self.changed = false;
            }
            Err(e) => self.notice = Some(Err(format!("Save package library: {e}"))),
        }
    }

    fn import(&mut self, manifest: &[u8], module: &[u8]) -> Result<String, String> {
        let package = Package::from_bytes(manifest, module)?;
        let source = Source {
            manifest: std::str::from_utf8(manifest)
                .map_err(|e| e.to_string())?
                .into(),
            module: STANDARD.encode(module),
            enabled: false,
        };
        let name = package.name().to_owned();
        self.insert(package, source)?;
        self.changed = true;
        Ok(format!("Imported {name}."))
    }

    fn insert(&mut self, package: Package, source: Source) -> Result<(), String> {
        if self.packages.contains_key(package.id()) {
            return Err(format!(
                "{} is already installed. Remove that copy before importing another version.",
                package.id()
            ));
        }
        if self.packages.len() >= LIMIT {
            return Err(format!("A library holds up to {LIMIT} packages."));
        }
        self.packages
            .insert(package.id().to_owned(), Installed { package, source });
        Ok(())
    }

    fn apply(
        &self,
        id: &str,
        mut apply: impl FnMut(&Package) -> Result<usize, String>,
    ) -> Result<String, String> {
        let entry = self
            .packages
            .get(id)
            .ok_or("Select an installed package.")?;
        if !entry.source.enabled {
            return Err("Enable this package before applying it.".into());
        }
        let count = apply(&entry.package)?;
        Ok(format!(
            "{} applied {count} editor {}.",
            entry.package.name(),
            if count == 1 { "command" } else { "commands" }
        ))
    }

    pub(crate) fn open(&mut self) {
        self.open = true;
    }

    pub(crate) fn show(
        &mut self,
        ctx: &egui::Context,
        apply: impl FnMut(&Package) -> Result<usize, String>,
    ) {
        if !self.open {
            return;
        }
        self.drops(ctx);
        let mut run = None;
        let mut remove = None;
        let frame = egui::Frame::popup(&ctx.global_style())
            .fill(egui::Color32::from_rgb(36, 53, 52))
            .inner_margin(14);
        let response = egui::Modal::new(egui::Id::unique("mod_manager"))
            .frame(frame)
            .show(ctx, |ui| {
                let style = ui.style_mut();
                style.visuals = egui::Visuals::dark();
                style.visuals.override_text_color = Some(egui::Color32::from_rgb(243, 237, 214));
                style.visuals.widgets.inactive.weak_bg_fill = egui::Color32::from_rgb(57, 77, 73);
                ui.set_width((ctx.content_rect().width() - 56.0).clamp(160.0, 455.0));
                ui.heading("Mod manager");
                ui.weak("Use Undo in the scenario editor to reverse package edits.");
                ui.separator();
                if let Some(result) = &self.notice {
                    match result {
                        Ok(msg) => {
                            ui.label(msg);
                        }
                        Err(msg) => {
                            ui.colored_label(ui.visuals().error_fg_color, msg);
                        }
                    }
                }
                egui::ScrollArea::vertical()
                    .max_height((ctx.content_rect().height() * 0.43).max(80.0))
                    .show(ui, |ui| {
                        if self.packages.is_empty() {
                            ui.weak("No packages installed.");
                        }
                        for (id, entry) in &mut self.packages {
                            ui.push_id(id, |ui| {
                                ui.label(
                                    egui::RichText::new(entry.package.name())
                                        .size(17.0)
                                        .strong(),
                                );
                                ui.weak(format!(
                                    "{}   ·   {}",
                                    entry.package.id(),
                                    entry.package.version()
                                ));
                                if !entry.package.description().is_empty() {
                                    ui.label(entry.package.description());
                                }
                                ui.horizontal_wrapped(|ui| {
                                    if ui.checkbox(&mut entry.source.enabled, "Enabled").changed() {
                                        self.changed = true;
                                    }
                                    if ui
                                        .add_enabled(
                                            entry.source.enabled,
                                            egui::Button::new("Apply to scenario"),
                                        )
                                        .clicked()
                                    {
                                        run = Some(id.clone());
                                    }
                                    if ui.small_button("Remove").clicked() {
                                        remove = Some(id.clone());
                                    }
                                });
                                ui.separator();
                            });
                        }
                    });
                #[cfg(not(target_arch = "wasm32"))]
                {
                    if ui.button("Browse folder…").clicked() {
                        self.folders = Some(browser::Folders::open());
                    }
                    if let Some(folders) = &mut self.folders {
                        match folders.show(ui) {
                            Some(Ok(Some(dir))) => {
                                self.notice = Some(
                                    Package::read_files(&dir)
                                        .and_then(|(m, w)| self.import(&m, &w)),
                                );
                                self.folders = None;
                            }
                            Some(Ok(None)) => self.folders = None,
                            Some(Err(e)) => self.notice = Some(Err(e)),
                            None => {}
                        }
                    }
                    ui.weak("Or drop a package folder here.");
                }
                #[cfg(target_arch = "wasm32")]
                {
                    if ui
                        .add_enabled(
                            self.loading.is_none(),
                            egui::Button::new("Choose package files…"),
                        )
                        .clicked()
                    {
                        match web::Upload::choose(ctx.clone()) {
                            Ok(upload) => self.loading = Some(upload),
                            Err(e) => self.notice = Some(Err(e)),
                        }
                    }
                    ui.weak("Choose or drop manifest.toml and plugin.wasm together.");
                }
                ui.horizontal_wrapped(|ui| {
                    if !self.packages.contains_key("woodland")
                        && ui.button("Import bundled woodland").clicked()
                    {
                        self.notice = Some(self.import(SAMPLE_MANIFEST, SAMPLE_MODULE));
                    }
                    if ui.button("Close").clicked() {
                        self.open = false;
                    }
                });
            });
        if let Some(id) = run {
            self.notice = Some(self.apply(&id, apply));
        }
        if let Some(id) = remove {
            self.packages.remove(&id);
            self.notice = Some(Ok(format!("Removed {id} from this library.")));
            self.changed = true;
        }
        if response.should_close() {
            self.open = false;
        }
        if !self.open {
            #[cfg(target_arch = "wasm32")]
            {
                self.loading = None;
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                self.folders = None;
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn drops(&mut self, ctx: &egui::Context) {
        let files = ctx.input_mut(|i| std::mem::take(&mut i.raw.dropped_files));
        if files.is_empty() {
            return;
        }
        self.notice = Some(
            browser::dropped_directory(files.iter().map(|f| f.path()))
                .and_then(|dir| Package::read_files(&dir))
                .and_then(|(m, w)| self.import(&m, &w)),
        );
    }

    #[cfg(target_arch = "wasm32")]
    fn drops(&mut self, ctx: &egui::Context) {
        if let Some(loading) = &self.loading {
            if let Some(result) = loading.poll() {
                self.loading = None;
                match result {
                    Ok(Some((m, w))) => self.notice = Some(self.import(&m, &w)),
                    Err(e) => self.notice = Some(Err(e)),
                    Ok(None) => {}
                }
            }
            return;
        }
        let files = ctx.input_mut(|i| std::mem::take(&mut i.raw.dropped_files));
        if !files.is_empty() {
            match web::Upload::from_drop(&files, ctx.clone()) {
                Ok(upload) => self.loading = Some(upload),
                Err(e) => self.notice = Some(Err(e)),
            }
        }
    }
}
