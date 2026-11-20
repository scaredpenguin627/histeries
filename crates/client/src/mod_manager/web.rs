//! User-selected browser files. The DOM input owns callbacks until completion or dismissal.

use eframe::{
    egui,
    wasm_bindgen::{JsCast, closure::Closure},
    web_sys::{self, File, HtmlInputElement, js_sys::Uint8Array},
};
use histeries_sim::extensions::{MAX_MANIFEST_BYTES, MAX_MODULE_BYTES};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use wasm_bindgen_futures::{JsFuture, spawn_local};

type Members = (Vec<u8>, Vec<u8>);
type ResultMessage = Result<Option<Members>, String>;

struct Picker {
    input: HtmlInputElement,
    _change: Closure<dyn FnMut()>,
    _cancel: Closure<dyn FnMut()>,
}

impl Drop for Picker {
    fn drop(&mut self) {
        self.input.set_onchange(None);
        self.input.set_oncancel(None);
        self.input.remove();
    }
}

pub(super) struct Upload {
    receive: Receiver<ResultMessage>,
    _picker: Option<Picker>,
}

impl Upload {
    pub(super) fn from_drop(
        handles: &[egui::DroppedFileHandle],
        ctx: egui::Context,
    ) -> Result<Self, String> {
        if handles.len() != 2 {
            return Err("Drop manifest.toml and plugin.wasm together.".into());
        }
        let files = handles
            .iter()
            .map(|h| {
                h.web_file()
                    .cloned()
                    .ok_or_else(|| "Dropped file is unavailable.".to_owned())
            })
            .collect::<Result<_, _>>()?;
        let (send, receive) = mpsc::channel();
        read(files, ctx, send);
        Ok(Self {
            receive,
            _picker: None,
        })
    }

    pub(super) fn choose(ctx: egui::Context) -> Result<Self, String> {
        let document = web_sys::window()
            .and_then(|w| w.document())
            .ok_or("File picker unavailable.")?;
        let input: HtmlInputElement = document
            .create_element("input")
            .map_err(|_| "Could not create file picker.")?
            .dyn_into()
            .map_err(|_| "File input unavailable.")?;
        input.set_type("file");
        input.set_accept(".toml,.wasm");
        input.set_multiple(true);
        input.set_hidden(true);
        let (send, receive) = mpsc::channel();
        let change = Closure::wrap(Box::new({
            let input = input.clone();
            let ctx = ctx.clone();
            let send = send.clone();
            move || {
                let files = input
                    .files()
                    .filter(|list| list.length() == 2)
                    .map(|list| (0..2).filter_map(|i| list.get(i)).collect())
                    .unwrap_or_default();
                read(files, ctx.clone(), send.clone());
            }
        }) as Box<dyn FnMut()>);
        let cancel = Closure::wrap(Box::new(move || {
            let _ = send.send(Ok(None));
            ctx.request_repaint();
        }) as Box<dyn FnMut()>);
        input.set_onchange(Some(change.as_ref().unchecked_ref()));
        input.set_oncancel(Some(cancel.as_ref().unchecked_ref()));
        let picker = Picker {
            input,
            _change: change,
            _cancel: cancel,
        };
        document
            .body()
            .ok_or("File picker unavailable.")?
            .append_child(&picker.input)
            .map_err(|_| "Could not open file picker.")?;
        picker.input.click();
        Ok(Self {
            receive,
            _picker: Some(picker),
        })
    }

    pub(super) fn poll(&self) -> Option<ResultMessage> {
        match self.receive.try_recv() {
            Ok(done) => Some(done),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                Some(Err("Could not read the selected files.".into()))
            }
        }
    }
}

fn read(files: Vec<File>, ctx: egui::Context, send: Sender<ResultMessage>) {
    spawn_local(async move {
        let bytes = async {
            if files.len() != 2 {
                return Err("Select manifest.toml and plugin.wasm together.".to_owned());
            }
            let member = |name: &str, limit: usize| -> Result<&File, String> {
                let file = files
                    .iter()
                    .find(|file| file.name() == name)
                    .ok_or_else(|| format!("Missing {name}."))?;
                if file.size() > limit as f64 {
                    return Err(format!("{name} exceeds {limit} bytes."));
                }
                Ok(file)
            };
            let manifest = member("manifest.toml", MAX_MANIFEST_BYTES)?;
            let module = member("plugin.wasm", MAX_MODULE_BYTES)?;
            let bytes = |file: &File| JsFuture::from(file.array_buffer());
            let manifest = bytes(manifest)
                .await
                .map_err(|_| "Could not read manifest.toml.")?;
            let wasm = bytes(module)
                .await
                .map_err(|_| "Could not read plugin.wasm.")?;
            Ok(Some((
                Uint8Array::new(&manifest).to_vec(),
                Uint8Array::new(&wasm).to_vec(),
            )))
        }
        .await;
        let _ = send.send(bytes);
        ctx.request_repaint();
    });
}
