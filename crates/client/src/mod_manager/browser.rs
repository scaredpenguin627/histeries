//! A native folder picker for packages. Reading a directory happens only on navigation.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use eframe::egui;

pub(super) struct Folders {
    dir: PathBuf,
    input: String,
    children: Vec<OsString>,
    more: bool,
    error: Option<String>,
}

impl Folders {
    pub(super) fn open() -> Self {
        let dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut picker = Self {
            dir: dir.clone(),
            input: String::new(),
            children: Vec::new(),
            more: false,
            error: None,
        };
        picker.navigate(dir);
        picker
    }

    fn navigate(&mut self, path: PathBuf) {
        let result = (|| -> Result<_, String> {
            let dir = package_directory(&path)?
                .canonicalize()
                .map_err(|e| e.to_string())?;
            let mut children = Vec::new();
            let mut more = false;
            for (index, entry) in std::fs::read_dir(&dir)
                .map_err(|e| e.to_string())?
                .enumerate()
            {
                if index == 4096 {
                    more = true;
                    break;
                }
                let entry = entry.map_err(|e| e.to_string())?;
                if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
                    if children.len() == 256 {
                        more = true;
                        break;
                    }
                    children.push(entry.file_name());
                }
            }
            children.sort();
            Ok((dir, children, more))
        })();
        match result {
            Ok((dir, children, more)) => {
                self.input = dir.to_string_lossy().into_owned();
                self.dir = dir;
                self.children = children;
                self.more = more;
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }

    pub(super) fn show(&mut self, ui: &mut egui::Ui) -> Option<Result<Option<PathBuf>, String>> {
        let mut navigate = None;
        let mut result = None;
        ui.group(|ui| {
            ui.label("Package folder");
            let path = ui.text_edit_singleline(&mut self.input);
            ui.horizontal_wrapped(|ui| {
                if ui.button("Go").clicked()
                    || (path.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                {
                    navigate = Some(PathBuf::from(&self.input));
                }
                if let Some(parent) = self.dir.parent()
                    && ui.button("Up").clicked()
                {
                    navigate = Some(parent.to_owned());
                }
                if ui.button("Import").clicked() {
                    let path = if self.input == self.dir.to_string_lossy() {
                        self.dir.as_path()
                    } else {
                        Path::new(&self.input)
                    };
                    result = Some(package_directory(path).map(Some));
                }
                if ui.button("Cancel").clicked() {
                    result = Some(Ok(None));
                }
            });
            if let Some(error) = &self.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            egui::ScrollArea::vertical()
                .id_salt("package_folders")
                .max_height(130.0)
                .show(ui, |ui| {
                    for child in &self.children {
                        if ui
                            .selectable_label(false, child.to_string_lossy())
                            .clicked()
                        {
                            navigate = Some(self.dir.join(child));
                        }
                    }
                });
            if self.more {
                ui.weak("Enter a path to browse folders beyond this list.");
            }
        });
        if let Some(dir) = navigate {
            self.navigate(dir);
        }
        result
    }
}

fn package_directory(path: &Path) -> Result<PathBuf, String> {
    if path.is_dir() {
        return Ok(path.to_owned());
    }
    if matches!(
        path.file_name().and_then(|s| s.to_str()),
        Some("manifest.toml" | "plugin.wasm")
    ) {
        return Ok(path.parent().unwrap_or(Path::new(".")).to_owned());
    }
    Err("Choose a package folder, manifest.toml or plugin.wasm.".into())
}

pub(super) fn dropped_directory<'a>(
    mut paths: impl Iterator<Item = &'a Path>,
) -> Result<PathBuf, String> {
    let first = package_directory(paths.next().ok_or("Select a package folder.")?)?;
    for path in paths {
        if package_directory(path)? != first {
            return Err("Drop files from one package at a time.".into());
        }
    }
    Ok(first)
}
