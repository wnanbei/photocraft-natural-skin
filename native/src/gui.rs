#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod appearance;
#[cfg(target_os = "macos")]
mod native_menu;

#[cfg(test)]
use eframe::egui::Color32;
use eframe::egui::{self, RichText, TextureHandle};
use natural_skin_companion::{
    desktop::{self, Appearance, Instance, Paths, Preferences, Start},
    handoff::{HandoffEvent, HostTarget, Watcher, result_path},
    i18n::Language,
    processing::Crop,
    startup::LoginStartup,
    worker::{Event, Job, PreviewPixels, Worker},
};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq)]
struct Controls {
    strength: u32,
    radius: u32,
    edge: u32,
    texture: u32,
    range: u32,
    bright: u32,
    rose: i32,
    skin: bool,
    mask: bool,
}
impl Default for Controls {
    fn default() -> Self {
        Self {
            strength: 55,
            radius: 6,
            edge: 60,
            texture: 65,
            range: 60,
            bright: 0,
            rose: 0,
            skin: true,
            mask: false,
        }
    }
}
impl Controls {
    fn parameters(&self, export: bool) -> Value {
        json!({"Strength":self.strength,"Radius":self.radius,"Edge_Protection":self.edge,"Texture":self.texture,"Skin_Range":self.range,"Skin_Only":self.skin,"Show_Mask":self.mask && !export,"skinBrightening":self.bright,"skinRosiness":self.rose})
    }
}
struct DesktopSession {
    paths: Paths,
    prefs: Preferences,
    instance: Instance,
    host_draft: String,
    startup: Option<LoginStartup>,
    login_enabled: bool,
    notice: String,
    error: Option<String>,
    prefs_valid: bool,
}
impl DesktopSession {
    fn new(paths: Paths, instance: Instance) -> Self {
        let (prefs, error) = match paths.load() {
            Ok(p) => (p, None),
            Err(e) => (
                Preferences::default(),
                Some(format!(
                    "Could not read settings: {e}. The existing settings file has been kept."
                )),
            ),
        };
        let prefs_valid = error.is_none();
        let host_draft = prefs
            .host
            .clone()
            .or_else(desktop::discover_host)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let startup = LoginStartup::new(&paths.root).ok();
        let login_enabled = startup
            .as_ref()
            .and_then(|s| s.enabled().ok())
            .unwrap_or(false);
        Self {
            paths,
            prefs,
            instance,
            host_draft,
            startup,
            login_enabled,
            notice: String::new(),
            error,
            prefs_valid,
        }
    }
    fn fail(&mut self, message: impl Into<String>) {
        self.notice.clear();
        self.error = Some(message.into());
    }
}
struct App {
    desktop: Option<DesktopSession>,
    language: Language,
    appearance: Appearance,
    interface_error: Option<String>,
    connection_page: bool,
    quitting: bool,
    progress: Option<f32>,
    #[cfg(target_os = "macos")]
    mac_events: Option<fmv_macos_events::Inbox>,
    worker: Worker,
    id: u64,
    controls: Controls,
    path: Option<PathBuf>,
    info: String,
    source_label: String,
    receiving: bool,
    original: Option<TextureHandle>,
    result: Option<TextureHandle>,
    show_original: bool,
    fit_original: Option<TextureHandle>,
    source_size: [u32; 2],
    detail_mode: bool,
    detail_center: [f32; 2],
    detail_crop: Option<Crop>,
    detail_viewport: Option<egui::Vec2>,
    detail_anchor: [f32; 2],
    colour: String,
    display_profile: Option<PathBuf>,
    ready: bool,
    busy: bool,
    exporting: bool,
    stage: String,
    start: Instant,
    detail: String,
    error: Option<String>,
    pending: Option<Instant>,
    saved: Option<PathBuf>,
    watcher: Option<Watcher>,
    queued_handoff: Option<HandoffEvent>,
    queued_open: Option<PathBuf>,
    host: Option<HostTarget>,
    returning: bool,
    opening: Option<std::sync::mpsc::Receiver<std::result::Result<(), String>>>,
}
impl App {
    fn new(
        cc: &eframe::CreationContext<'_>,
        mut options: LaunchOptions,
    ) -> natural_skin_companion::Result<Self> {
        let ctx = cc.egui_ctx.clone();
        let language = options
            .desktop
            .as_ref()
            .map(|s| s.prefs.language)
            .unwrap_or_default();
        let appearance = options
            .desktop
            .as_ref()
            .map(|s| s.prefs.appearance)
            .unwrap_or_default();
        appearance::install_fonts(&ctx);
        appearance::apply(&ctx, appearance);
        #[cfg(target_os = "macos")]
        native_menu::translate(language);
        if let Some(state) = &mut options.desktop {
            if let Some(path) = &state.prefs.host {
                match HostTarget::new(path) {
                    Ok(host) => options.host = Some(host),
                    Err(e) => state.fail(format!(
                        "PhotoCraft could not be found: {e}. Choose its current location."
                    )),
                }
            }
            if state.prefs.listening && options.host.is_some() {
                options.watch = Some(state.paths.exchange.clone());
            }
        }
        let mut connection_page = options
            .desktop
            .as_ref()
            .is_some_and(|s| s.error.is_some() || s.prefs.host.is_none());
        let worker = Worker::new(move || ctx.request_repaint());
        let watcher = if let Some(root) = &options.watch {
            let ctx = cc.egui_ctx.clone();
            match Watcher::new(root, move || ctx.request_repaint()) {
                Ok(watcher) => Some(watcher),
                Err(e) => {
                    if let Some(state) = &mut options.desktop {
                        state.fail(format!(
                            "Could not start the connection: {e}. Check the exchange folder, then enable the connection again."
                        ));
                        None
                    } else {
                        return Err(e);
                    }
                }
            }
        } else {
            None
        };
        connection_page |= options.desktop.as_ref().is_some_and(|s| s.error.is_some());
        let mut app = Self {
            language,
            appearance,
            interface_error: None,
            desktop: options.desktop,
            connection_page,
            quitting: false,
            progress: None,
            #[cfg(target_os = "macos")]
            mac_events: options.mac_events,
            worker,
            id: 0,
            controls: Controls::default(),
            path: None,
            info: String::new(),
            source_label: String::new(),
            receiving: false,
            original: None,
            result: None,
            show_original: false,
            fit_original: None,
            source_size: [0; 2],
            detail_mode: false,
            detail_center: [0.5; 2],
            detail_crop: None,
            detail_viewport: None,
            detail_anchor: [0.5; 2],
            colour: String::new(),
            display_profile: None,
            ready: false,
            busy: false,
            exporting: false,
            stage: "Ready".into(),
            start: Instant::now(),
            detail: "Open an image to begin".into(),
            error: None,
            pending: None,
            saved: None,
            watcher,
            queued_handoff: None,
            queued_open: None,
            host: options.host,
            returning: false,
            opening: None,
        };
        if app.connection_page || (options.background && app.watcher.is_none()) {
            cc.egui_ctx
                .send_viewport_cmd(egui::ViewportCommand::Visible(true));
        }
        if let Some(path) = options.input {
            app.load(path);
        } else if app.watcher.is_some() {
            app.stage = "Waiting for PhotoCraft".into();
            app.detail = "Click the Natural Skin Send action in PhotoCraft".into();
        }
        Ok(app)
    }
    fn begin(&mut self, job: Job, stage: &str) {
        if let Some(id) = self.worker.submit(job) {
            self.id = id;
            self.busy = true;
            self.progress = None;
            self.stage = stage.into();
            self.start = Instant::now();
            self.error = None;
            self.detail.clear();
        }
    }
    fn load(&mut self, path: PathBuf) {
        self.connection_page = false;
        self.receiving = false;
        self.source_label.clear();
        self.fit_original = None;
        self.detail_mode = false;
        self.detail_crop = None;
        self.detail_viewport = None;
        self.colour.clear();
        self.pending = None;
        self.ready = false;
        self.original = None;
        self.result = None;
        self.info.clear();
        self.saved = None;
        self.show_original = false;
        self.path = Some(path.clone());
        self.begin(Job::Load(path), "Reading image");
    }
    fn open(&mut self, frame: &eframe::Frame) {
        let l = self.language;
        if let Some(path) = rfd::FileDialog::new()
            .set_parent(frame)
            .set_title(l.text("Open Image…"))
            .add_filter(l.text("PhotoCraft or RGB TIFF"), &["pcraft", "tif", "tiff"])
            .pick_file()
        {
            self.load(path);
        }
    }
    fn preview(&mut self) {
        self.pending = None;
        let mut parameters = self.controls.parameters(false);
        if let Some(path) = &self.display_profile {
            parameters["_displayProfile"] = json!(path);
        }
        self.begin(
            if let Some(crop) = self.detail_crop.filter(|_| self.detail_mode) {
                Job::Detail(crop, parameters)
            } else {
                Job::Preview(parameters)
            },
            "Updating preview",
        );
    }
    fn texture(ctx: &egui::Context, p: PreviewPixels, name: &str, detail: bool) -> TextureHandle {
        ctx.load_texture(
            name,
            egui::ColorImage::from_rgba_unmultiplied([p.width, p.height], &p.rgba),
            if detail {
                egui::TextureOptions::NEAREST
            } else {
                egui::TextureOptions::LINEAR
            },
        )
    }
    fn events(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.worker.events.try_recv() {
            if event.id() != self.id {
                continue;
            }
            match event {
                Event::Progress {
                    stage,
                    completed,
                    total,
                    ..
                } => {
                    self.stage = stage;
                    self.progress = (total > 0).then(|| completed as f32 / total as f32);
                }
                Event::Loaded {
                    width,
                    height,
                    depth,
                    original,
                    label,
                    colour,
                    ..
                } => {
                    self.source_label = label;
                    self.colour = colour;
                    self.source_size = [width, height];
                    self.info = format!("{width} × {height}  ·  {depth}-bit RGB");
                    self.original = Some(Self::texture(ctx, original, "Original", false));
                    self.fit_original = self.original.clone();
                    self.ready = true;
                    self.preview();
                }
                Event::Rendered {
                    pixels,
                    cache_hit,
                    original,
                    milliseconds,
                    ..
                } => {
                    if let Some(original) = original {
                        self.original =
                            Some(Self::texture(ctx, original, "Original", self.detail_mode));
                    }
                    self.result = Some(Self::texture(ctx, pixels, "Retouched", self.detail_mode));

                    self.busy = false;
                    self.stage = "Preview complete".into();
                    self.detail = format!(
                        "{milliseconds:.0} ms{}",
                        if cache_hit { " · cached filter" } else { "" }
                    );
                }
                Event::Exported {
                    path,
                    milliseconds,
                    processing_ms,
                    write_ms,
                    tile_buffer_bytes,
                    ..
                } => {
                    self.busy = false;
                    self.exporting = false;
                    self.stage = "Export complete".into();
                    self.detail = format!(
                        "Saved in {:.2} s · filter {:.2} s · write {:.2} s · tile buffers ≤ {:.1} MiB",
                        milliseconds / 1000.0,
                        processing_ms / 1000.0,
                        write_ms / 1000.0,
                        tile_buffer_bytes as f64 / 1048576.0
                    );
                    self.saved = Some(path.clone());
                    if let Some(state) = &mut self.desktop {
                        state.prefs.last_result = Some(path.clone());
                        if state.prefs_valid
                            && let Err(e) = state.paths.save(&state.prefs)
                        {
                            state.error = Some(format!(
                                "Result saved, but its recovery link could not be remembered: {e}"
                            ));
                        }
                    }
                    if self.returning {
                        self.returning = false;
                        self.stage = "Opening result in PhotoCraft".into();
                        self.busy = true;
                        self.exporting = true;
                        let host = self.host.clone().expect("Return target configured");
                        let (sender, receiver) = std::sync::mpsc::channel();
                        self.opening = Some(receiver);
                        let ctx = ctx.clone();
                        std::thread::spawn(move || {
                            let _ = sender.send(host.open(&path).map_err(|e| e.to_string()));
                            ctx.request_repaint();
                        });
                    }
                }
                Event::Failed { message, .. } => {
                    self.busy = false;
                    self.exporting = false;
                    self.returning = false;
                    self.stage = "Operation failed".into();
                    self.detail =
                        format!("Failed after {:.2} s", self.start.elapsed().as_secs_f64());
                    self.error = Some(message);
                }
            }
        }
    }
    fn export(&mut self, frame: &eframe::Frame) {
        let l = self.language;
        self.returning = false;
        let stem = self
            .path
            .as_ref()
            .and_then(|p| p.file_stem())
            .unwrap_or_default()
            .to_string_lossy();
        let file = format!("{stem}-natural-skin.pcraft");
        if let Some(path) = rfd::FileDialog::new()
            .set_parent(frame)
            .set_title(l.text("Export Result…"))
            .add_filter(l.text("PhotoCraft document"), &["pcraft"])
            .set_file_name(file)
            .save_file()
        {
            self.saved = None;
            self.pending = None;
            self.exporting = true;
            self.begin(
                Job::Export(path, self.controls.parameters(true)),
                "Processing full image",
            );
        }
    }
    fn handoff_events(&mut self, ctx: &egui::Context) {
        if let Some(receiver) = &self.opening
            && let Ok(result) = receiver.try_recv()
        {
            self.opening = None;
            self.busy = false;
            self.exporting = false;
            match result {
                Ok(()) => {
                    self.stage = "Sent to PhotoCraft".into();
                    self.detail = "Result opens as a new document".into();
                }
                Err(error) => {
                    self.stage = "Result saved; could not open PhotoCraft".into();
                    self.error = Some(error);
                }
            }
        }
        let events: Vec<_> = self
            .watcher
            .as_ref()
            .map(|w| w.events.try_iter().collect())
            .unwrap_or_default();
        for event in events {
            if let Some(HandoffEvent::Captured(path)) = self.queued_handoff.replace(event) {
                let _ = std::fs::remove_file(path);
            }
        }
        if !self.exporting
            && let Some(event) = self.queued_handoff.take()
        {
            match event {
                HandoffEvent::Receiving => {
                    self.worker.cancel();
                    self.id = u64::MAX;
                    self.pending = None;
                    self.receiving = true;
                    self.ready = false;
                    self.busy = true;
                    self.progress = None;
                    self.stage = "Receiving layer from PhotoCraft".into();
                    self.start = Instant::now();
                    self.detail.clear();
                    self.error = None;
                }
                HandoffEvent::Captured(path) => {
                    if let Some(state) = &mut self.desktop {
                        state.notice =
                            "Layer received from PhotoCraft. The connection is working.".into();
                    }
                    self.load(path);
                }
                HandoffEvent::Rejected(error) => {
                    self.receiving = false;
                    self.worker.cancel();
                    self.id = u64::MAX;
                    self.pending = None;
                    self.ready = false;
                    self.busy = false;
                    self.original = None;
                    self.result = None;
                    self.path = None;
                    self.info.clear();
                    self.stage = "Capture rejected".into();
                    self.detail.clear();
                    self.error = Some(error);
                }
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        if !self.quitting && ctx.input(|i| i.viewport().close_requested()) {
            if self.watcher.is_some() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            } else if self.exporting {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.request_quit(ctx);
            }
        }
    }
    fn apply_to_host(&mut self) {
        if let Some(input) = &self.path {
            let output_source = self
                .desktop
                .as_ref()
                .map(|s| s.paths.root.join("result.pcraft"));
            match result_path(output_source.as_deref().unwrap_or(input)) {
                Ok(path) => {
                    self.pending = None;
                    self.saved = None;
                    self.returning = true;
                    self.exporting = true;
                    self.begin(
                        Job::Export(path, self.controls.parameters(true)),
                        "Processing full image",
                    );
                }
                Err(error) => self.error = Some(error.to_string()),
            }
        }
    }
    fn request_quit(&mut self, ctx: &egui::Context) {
        self.quitting = true;
        if self.opening.is_some() {
            return;
        }
        if self.exporting && !self.worker.cancel() {
            self.stage = "Finishing file write before quitting".into();
            return;
        }
        self.watcher = None;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
    fn set_connection(&mut self, enabled: bool, ctx: &egui::Context) {
        let Some(state) = &mut self.desktop else {
            return;
        };
        let result = (|| -> natural_skin_companion::Result<()> {
            let mut prefs = state.prefs.clone();
            let mut candidate_host = None;
            let mut candidate_watcher = None;
            if enabled {
                let host = HostTarget::new(&PathBuf::from(state.host_draft.trim()))?;
                state.paths.generate_installer()?;
                if self.watcher.is_none() {
                    let ctx = ctx.clone();
                    candidate_watcher = Some(Watcher::new(&state.paths.exchange, move || {
                        ctx.request_repaint()
                    })?);
                }
                prefs.host = Some(host.path().to_path_buf());
                candidate_host = Some(host);
            }
            prefs.listening = enabled;
            state.paths.save(&prefs)?;
            state.prefs = prefs;
            state.prefs_valid = true;
            if enabled {
                self.host = candidate_host;
                if candidate_watcher.is_some() {
                    self.watcher = candidate_watcher;
                }
            } else {
                self.watcher = None;
                self.queued_handoff = None;
                if self.receiving {
                    self.receiving = false;
                    self.busy = false;
                }
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                state.error = None;
                state.notice = if enabled {
                    "Listening. Import the action below if this is your first connection."
                } else {
                    "Connection stopped. Enable it here when needed."
                }
                .into();
                if !self.busy {
                    self.stage = if enabled {
                        "Waiting for PhotoCraft"
                    } else {
                        "Connection stopped"
                    }
                    .into();
                    self.detail = if enabled { "Run Natural Skin Send in PhotoCraft" } else { "Open a file manually or enable the connection again" }.into();
                }
            }
            Err(e) => state.fail(format!("Could not update the connection: {e}. Choose PhotoCraft again, then save the connection.")),
        }
    }
    fn save_interface(&mut self, ctx: &egui::Context) {
        appearance::apply(ctx, self.appearance);
        #[cfg(target_os = "macos")]
        native_menu::translate(self.language);
        self.interface_error = None;
        if let Some(state) = &mut self.desktop {
            if !state.prefs_valid {
                self.interface_error = Some("Settings file needs repair before preferences can be saved. Your selection is active for this session.".into());
                return;
            }
            let mut prefs = state.prefs.clone();
            prefs.language = self.language;
            prefs.appearance = self.appearance;
            match state.paths.save(&prefs) {
                Ok(()) => {
                    state.prefs = prefs;
                }
                Err(e) => {
                    self.interface_error = Some(format!(
                        "Could not save interface settings: {e}. Your selection is active for this session; try again after checking the settings folder."
                    ))
                }
            }
        }
        ctx.request_repaint();
    }
    fn interface_ui(&mut self, ui: &mut egui::Ui) {
        let l = self.language;
        let before = (self.language, self.appearance);
        ui.horizontal_wrapped(|ui| {
            let label = ui.label(l.text("Language"));
            egui::ComboBox::from_id_salt("interface_language")
                .width(110.0)
                .selected_text(self.language.name())
                .show_ui(ui, |ui| {
                    for language in Language::ALL {
                        if ui
                            .selectable_value(&mut self.language, language, language.name())
                            .clicked()
                        {
                            ui.close();
                        }
                    }
                })
                .response
                .labelled_by(label.id)
                .on_hover_text(l.text(
                    "Language applies immediately and is saved independently of PhotoCraft.",
                ));
            ui.add_space(12.0);
            let label = ui.label(l.text("Appearance"));
            egui::ComboBox::from_id_salt("interface_appearance")
                .width(170.0)
                .selected_text(l.text(self.appearance.label()))
                .show_ui(ui, |ui| {
                    for appearance in Appearance::ALL {
                        if ui
                            .selectable_value(
                                &mut self.appearance,
                                appearance,
                                l.text(appearance.label()),
                            )
                            .clicked()
                        {
                            ui.close();
                        }
                    }
                })
                .response
                .labelled_by(label.id)
                .on_hover_text(
                    l.text("Appearance applies immediately and is saved for the next launch."),
                );
        });
        if before != (self.language, self.appearance) {
            self.save_interface(ui.ctx());
        }
    }
    fn connection_ui(&mut self, ui: &mut egui::Ui, frame: &eframe::Frame) {
        let l = self.language;
        ui.add_space(appearance::SECTION_GAP);
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.set_max_width(680.0);
            ui.heading(l.text("Connect to PhotoCraft"));
            ui.label(l.text("Send a selected pixel layer, adjust its skin, then open the result as a new PhotoCraft document."));
            ui.add_space(appearance::SECTION_GAP);
            let Some(state) = &mut self.desktop else { return; };
            let mut enable = None;
            ui.strong(l.text("PhotoCraft application"));
            let label = ui.label(l.text("Choose PhotoCraft.app on macOS or PhotoCraft.exe on Windows."));
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut state.host_draft).desired_width(460.0)).labelled_by(label.id);
                if ui.button(l.text("Choose…")).clicked() {
                    let dialog = rfd::FileDialog::new().set_parent(frame);
                    #[cfg(target_os = "macos")]
                    let dialog = dialog.add_filter(l.text("PhotoCraft application"), &["app"]);
                    #[cfg(target_os = "windows")]
                    let dialog = dialog.add_filter(l.text("PhotoCraft application"), &["exe"]);
                    if let Some(path) = dialog.pick_file() { state.host_draft = path.to_string_lossy().into_owned(); }
                }
            });
            ui.horizontal(|ui| {
                if ui.add_enabled(!self.exporting, egui::Button::new(l.text(if self.watcher.is_some() { "Save connection" } else { "Enable connection" }))).clicked() { enable = Some(true); }
                if self.watcher.is_some() && ui.add_enabled(!self.exporting,egui::Button::new(l.text("Stop connection"))).clicked() { enable = Some(false); }
                ui.label(l.text(if self.watcher.is_some() { "Listening" } else { "Stopped" }));
            });
            if !state.notice.is_empty() { ui.label(l.text(&state.notice)); }
            ui.add_space(appearance::SECTION_GAP);
            ui.strong(l.text("Install the Send action once"));
            ui.label(l.text("In PhotoCraft, choose File → Scripts → Browse and select this installer. Then find Natural Skin Send in the Actions panel."));
            path_label(ui, &state.paths.installer);
            ui.horizontal(|ui| {
                if ui.add_enabled(state.paths.installer.exists(),egui::Button::new(l.text("Show installer folder"))).clicked()
                    && let Err(e) = desktop::show_folder(&state.paths.root) { state.fail(e.to_string()); }
                if ui.button(l.text("Copy installer path")).clicked() { ui.ctx().copy_text(state.paths.installer.display().to_string()); }
            });
            ui.label(l.text("If the action is already present, use it directly. To reinstall, delete only Natural Skin Send from PhotoCraft first."));
            ui.add_space(appearance::SECTION_GAP);
            ui.strong(l.text("Keep Natural Skin available"));
            ui.label(l.text("While connected, closing the window minimizes it and keeps receiving layers. Use Quit to stop the app."));
            let mut login = state.login_enabled;
            if ui.add_enabled(state.startup.is_some(),egui::Checkbox::new(&mut login, l.text("Start at login"))).changed() {
                match state.startup.as_ref().unwrap().set_enabled(login) {
                    Ok(()) => { state.login_enabled=login; state.notice="Login startup updated. Keep Natural Skin in its current location, or update this setting after moving it.".into(); state.error=None; }
                    Err(e) => state.fail(format!("Could not update login startup: {e}")),
                }
            }
            ui.add_space(appearance::SECTION_GAP);
            ui.strong(l.text("Saved results"));
            ui.label(l.text("Completed results are kept until you remove them. Save the returned document in PhotoCraft to a location of your choice."));
            let mut reopen=None;
            if let Some(path) = &state.prefs.last_result {
                path_label(ui, path);
                if ui.add_enabled(!self.exporting && path.is_file(),egui::Button::new(l.text("Open last result"))).clicked() { reopen=Some(path.clone()); }
            }
            if ui.button(l.text("Show saved files")).clicked()
                && let Err(e) = desktop::show_folder(&state.paths.root) { state.fail(e.to_string()); }
            ui.add_space(appearance::SECTION_GAP);
            if ui.button(l.text("Back to image")).clicked() { self.connection_page=false; }
            if let Some(enabled)=enable { self.set_connection(enabled,ui.ctx()); }
            if let Some(path)=reopen { self.load(path); }
        });
    }
    fn inspector(&mut self, ui: &mut egui::Ui, frame: &eframe::Frame) {
        let l = self.language;
        let old = self.controls.clone();
        appearance::strip(ui, l.text("Skin retouching"));
        ui.checkbox(&mut self.controls.skin, l.text("Skin only"))
            .on_hover_text(l.text("Limit changes to the detected skin-colour range"));
        ui.add_space(8.0);
        adjustment(ui, l, "Strength", &mut self.controls.strength, 0..=100);
        adjustment(ui, l, "Radius (px)", &mut self.controls.radius, 1..=20);
        adjustment(ui, l, "Edge protection", &mut self.controls.edge, 0..=100);
        adjustment(ui, l, "Texture", &mut self.controls.texture, 0..=100);
        ui.add_space(appearance::SECTION_GAP);
        appearance::strip(ui, l.text("Skin range"));
        adjustment(ui, l, "Range", &mut self.controls.range, 0..=100);
        ui.checkbox(&mut self.controls.mask, l.text("Show skin mask"));
        ui.add_space(appearance::SECTION_GAP);
        appearance::strip(ui, l.text("Skin colour"));
        adjustment(ui, l, "Brightening", &mut self.controls.bright, 0..=100);
        adjustment(ui, l, "Rosiness", &mut self.controls.rose, -100..=100);
        ui.add_space(appearance::SECTION_GAP);
        if ui.button(l.text("Reset adjustments")).clicked() {
            self.controls = Controls::default();
        }
        ui.add_space(appearance::SECTION_GAP);
        preview_note(
            ui,
            l,
            "Fit is approximate. Click the image to inspect 1:1 detail; drag detail to pan. Export uses the full source.",
        );
        ui.add_space(8.0);
        preview_note(ui, l, &self.colour);
        preview_note(
            ui,
            l,
            if self.display_profile.is_some() {
                "Display output: selected ICC profile"
            } else {
                "Display output: sRGB"
            },
        );
        if ui.button(l.text("Display profile…")).on_hover_text(l.text("Choose the ICC profile matching your display. Use sRGB output if your OS already converts this window to the display profile.")).clicked()
            && let Some(path)=rfd::FileDialog::new().set_parent(frame).set_title(l.text("Display profile…")).add_filter(l.text("ICC display profile"), &["icc","icm"]).pick_file() {
            match natural_skin_companion::color::DisplayTransform::from_path(Some(&path)) {
                Ok(_)=>{self.display_profile=Some(path);self.pending=Some(Instant::now());},
                Err(e)=>self.error=Some(format!("Display profile was not changed: {e}. Choose a supported SDR RGB matrix/TRC display ICC, or choose Use sRGB output")),
            }
        }
        if let Some(path) = &self.display_profile {
            preview_note(
                ui,
                Language::English,
                path.file_name().unwrap_or_default().to_string_lossy(),
            );
            if ui.button(l.text("Use sRGB output")).clicked() {
                self.display_profile = None;
                self.pending = Some(Instant::now());
            }
        }
        preview_note(
            ui,
            l,
            "SDR only. Monitor calibration and automatic profile switching are not verified.",
        );
        if self.controls != old {
            // Invalidate the previous result immediately, before the debounce.
            self.worker.cancel();
            self.id = u64::MAX;
            self.pending = Some(Instant::now() + Duration::from_millis(90));
            self.stage = "Updating preview".into();
            self.progress = None;
            self.busy = true;
            self.start = Instant::now();
            self.saved = None;
        }
    }
}
impl eframe::App for App {
    // eframe intentionally skips ui() for a hidden viewport. File handoff and
    // job completion must continue in logic() so a resident window can reappear.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.events(ctx);
        self.handoff_events(ctx);
        #[cfg(target_os = "macos")]
        if let Some(events) = &self.mac_events {
            for event in events.drain() {
                match event {
                    fmv_macos_events::Event::Open(paths) => {
                        if let Some(path) = paths.into_iter().next() {
                            self.queued_open = Some(path);
                        }
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    }
                    fmv_macos_events::Event::Quit => self.request_quit(ctx),
                }
            }
        }
        if let Some(state) = &self.desktop {
            let activation = state.instance.poll();
            match activation {
                Ok(Some(activation)) => {
                    if let Some(path) = activation.input {
                        self.queued_open = Some(path);
                    }
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                Err(e) => self.error = Some(format!("Could not receive the open request: {e}")),
                Ok(None) => (),
            }
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        if !self.exporting
            && !self.quitting
            && let Some(path) = self.queued_open.take()
        {
            self.load(path);
        }
        if self.quitting && !self.exporting {
            self.request_quit(ctx);
        }
        if !self.quitting && self.pending.is_some_and(|time| Instant::now() >= time) {
            self.preview();
        }
        if self.busy || self.pending.is_some() {
            ctx.request_repaint_after(Duration::from_millis(30));
        }
    }
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let l = self.language;
        let ctx = ui.ctx().clone();
        if !self.exporting
            && let Some(path) =
                ctx.input(|i| i.raw.dropped_files.first().map(|f| f.path().to_path_buf()))
        {
            self.load(path);
        }
        let shortcut = ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::O));
        if shortcut && !self.exporting {
            self.open(frame);
        }
        egui::Panel::top("file_bar").show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Natural Skin").size(16.0).strong());
                ui.add_space(16.0);
                if ui
                    .add_enabled(!self.exporting, egui::Button::new(l.text("Open Image…")))
                    .clicked()
                {
                    self.open(frame);
                }
                if ui
                    .add_enabled(
                        self.ready && !self.exporting,
                        egui::Button::new(l.text("Export Result…")),
                    )
                    .clicked()
                {
                    self.export(frame);
                }
                if self.host.is_some()
                    && ui
                        .add_enabled(
                            self.ready && !self.exporting,
                            appearance::primary(l.text("Apply to PhotoCraft")),
                        )
                        .clicked()
                {
                    self.apply_to_host();
                }
                if self.desktop.is_some()
                    && ui
                        .selectable_label(self.connection_page, l.text("Connection…"))
                        .clicked()
                {
                    self.connection_page = !self.connection_page;
                }
                if ui.button(l.text("Quit")).clicked() {
                    self.request_quit(&ctx);
                }
                ui.label(RichText::new(l.text(&self.info)).small());
            });
            if let Some(path) = &self.path {
                ui.label(
                    RichText::new(if self.source_label.is_empty() {
                        path.file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned()
                    } else {
                        self.source_label.clone()
                    })
                    .small(),
                );
            }
            ui.separator();
            self.interface_ui(ui);
            ui.add_space(4.0);
        });
        egui::Panel::bottom("task_status")
            .min_size(48.0)
            .show(ui, |ui| {
                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    ui.set_min_height(32.0);
                    if self.busy {
                        ui.spinner();
                    }
                    ui.strong(l.text(&self.stage));
                    if self.busy {
                        ui.label(l.text(&format!(
                            "{:.1} s elapsed",
                            self.start.elapsed().as_secs_f64()
                        )));
                        if ui
                            .add_enabled(
                                self.opening.is_none() && !self.receiving,
                                egui::Button::new(l.text("Cancel")),
                            )
                            .on_disabled_hover_text(l.text(
                                "Wait for layer capture or the PhotoCraft open request to finish",
                            ))
                            .clicked()
                        {
                            if self.worker.cancel() {
                                self.id = u64::MAX;
                                self.pending = None;
                                self.busy = false;
                                self.exporting = false;
                                self.returning = false;
                                self.stage = "Cancelled".into();
                                self.detail = "The source is unchanged".into();
                            } else {
                                self.stage = "Finishing file write".into();
                            }
                        }
                    } else {
                        ui.label(l.text(&self.detail));
                    }
                });
                task_progress(ui, self.progress.filter(|_| self.busy));
                if self.watcher.is_some()
                    && self.desktop.is_none()
                    && !self.busy
                    && ui.button(l.text("Stop Listening")).clicked()
                {
                    self.watcher = None;
                    self.stage = "Listener stopped".into();
                }
                if let Some(error) = &self.error {
                    error_label(ui, l, error);
                }
                if let Some(error) = &self.interface_error {
                    error_label(ui, l, error);
                }
                if let Some(state) = &self.desktop
                    && let Some(error) = &state.error
                {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                    error_label(ui, l, error);
                }
                if let Some(path) = &self.saved {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(l.text(&format!(
                                "Saved: {}",
                                path.file_name().unwrap_or_default().to_string_lossy()
                            )))
                            .small(),
                        );
                        if ui.button(l.text("Copy saved path")).clicked() {
                            ui.ctx().copy_text(path.display().to_string());
                        }
                    });
                }
                if self.queued_open.is_some() {
                    ui.label(l.text("An image will open after the current export completes."));
                }
                ui.add_space(5.0);
            });
        if self.connection_page {
            egui::CentralPanel::default().show(ui, |ui| self.connection_ui(ui, frame));
            return;
        }
        egui::Panel::right("adjustments")
            .exact_size(appearance::INSPECTOR_WIDTH)
            .resizable(false)
            .show(ui, |ui| {
                ui.add_space(12.0);
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                egui::Frame::new()
                    .inner_margin(appearance::INSET)
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.spacing_mut().slider_width =
                                    (ui.available_width() - 60.0).max(100.0);
                                ui.add_enabled_ui(self.ready && !self.exporting, |ui| {
                                    self.inspector(ui, frame)
                                });
                            });
                    });
            });
        egui::CentralPanel::default().show(ui, |ui| {
            if self.fit_original.is_some() {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.show_original, true, l.text("Original"));
                    ui.selectable_value(
                        &mut self.show_original,
                        false,
                        l.text(if self.controls.mask {
                            "Skin mask"
                        } else {
                            "Retouched"
                        }),
                    );
                });
                let mut view_changed = false;
                ui.horizontal_wrapped(|ui| {
                    ui.add_enabled_ui(!self.exporting, |ui| {
                        view_changed |= ui
                            .selectable_value(&mut self.detail_mode, false, l.text("Fit"))
                            .changed();
                        view_changed |= ui
                            .selectable_value(&mut self.detail_mode, true, l.text("1:1 detail"))
                            .changed();
                    });
                    if self.detail_mode {
                        if let Some(crop) = self.detail_crop {
                            preview_note(
                                ui,
                                l,
                                format!(
                                    "x {} · y {} · {} × {} px",
                                    crop.x, crop.y, crop.width, crop.height
                                ),
                            );
                        }
                        preview_note(ui, l, "Drag or use arrow keys to pan");
                    } else {
                        preview_note(ui, l, "Click image to inspect detail");
                    }
                });
                let area = ui.available_size();
                let pixels_per_point = ctx.pixels_per_point();
                let mut center_changed = false;
                if self.detail_mode && !self.exporting && !ctx.egui_wants_keyboard_input() {
                    let delta = ctx.input(|i| {
                        let step = if i.modifiers.shift { 256.0 } else { 64.0 };
                        [
                            if i.key_pressed(egui::Key::ArrowRight) {
                                step
                            } else if i.key_pressed(egui::Key::ArrowLeft) {
                                -step
                            } else {
                                0.0
                            },
                            if i.key_pressed(egui::Key::ArrowDown) {
                                step
                            } else if i.key_pressed(egui::Key::ArrowUp) {
                                -step
                            } else {
                                0.0
                            },
                        ]
                    });
                    for (axis, amount) in delta.iter().enumerate() {
                        if *amount != 0.0 {
                            self.detail_center[axis] = (self.detail_center[axis]
                                + amount / self.source_size[axis] as f32)
                                .clamp(0.0, 1.0);
                            center_changed = true;
                        }
                    }
                }
                egui::Frame::new()
                    .fill(appearance::canvas())
                    .inner_margin(16.0)
                    .show(ui, |ui| {
                        ui.set_min_size(area - egui::vec2(32.0, 32.0));
                        let viewport = ctx.viewport_rect().size();
                        if self.detail_mode
                            && !self.exporting
                            && (self.detail_viewport != Some(viewport)
                                || self.detail_anchor != self.detail_center
                                || view_changed
                                || self.detail_crop.is_none())
                        {
                            let crop = detail_region(
                                self.source_size,
                                self.detail_center,
                                [
                                    ui.available_width() * pixels_per_point,
                                    ui.available_height() * pixels_per_point,
                                ],
                            );
                            if self.detail_crop != Some(crop) {
                                self.detail_crop = Some(crop);
                                view_changed = true;
                            }
                            self.detail_center = [
                                (crop.x as f32 + crop.width as f32 / 2.0)
                                    / self.source_size[0] as f32,
                                (crop.y as f32 + crop.height as f32 / 2.0)
                                    / self.source_size[1] as f32,
                            ];
                            self.detail_viewport = Some(viewport);
                            self.detail_anchor = self.detail_center;
                        } else if !self.detail_mode {
                            self.detail_viewport = None;
                            self.detail_crop = None;
                        }
                        let texture = if self.show_original {
                            self.original.as_ref()
                        } else {
                            self.result.as_ref().or(self.original.as_ref())
                        };
                        ui.centered_and_justified(|ui| {
                            if let Some(texture) = texture.filter(|_| !view_changed) {
                                let size = if self.detail_mode {
                                    texture.size_vec2() / pixels_per_point
                                } else {
                                    texture.size_vec2()
                                        * (ui.available_width() / texture.size_vec2().x)
                                            .min(ui.available_height() / texture.size_vec2().y)
                                };
                                let response = ui.add(
                                    egui::Image::new(texture)
                                        .fit_to_exact_size(size)
                                        .texture_options(if self.detail_mode {
                                            egui::TextureOptions::NEAREST
                                        } else {
                                            egui::TextureOptions::LINEAR
                                        })
                                        .sense(egui::Sense::click_and_drag()),
                                );
                                if !self.exporting {
                                    if !self.detail_mode
                                        && response.clicked()
                                        && let Some(pos) = response.interact_pointer_pos()
                                    {
                                        self.detail_center = [
                                            ((pos.x - response.rect.left())
                                                / response.rect.width())
                                            .clamp(0.0, 1.0),
                                            ((pos.y - response.rect.top())
                                                / response.rect.height())
                                            .clamp(0.0, 1.0),
                                        ];
                                        self.detail_mode = true;
                                        center_changed = true;
                                    } else if self.detail_mode && response.dragged() {
                                        let delta = response.drag_delta() * pixels_per_point;
                                        self.detail_center[0] = (self.detail_center[0]
                                            - delta.x / self.source_size[0] as f32)
                                            .clamp(0.0, 1.0);
                                        self.detail_center[1] = (self.detail_center[1]
                                            - delta.y / self.source_size[1] as f32)
                                            .clamp(0.0, 1.0);
                                        center_changed = true;
                                    }
                                }
                            } else {
                                ui.label(l.text("Preparing preview…"));
                            }
                        });
                    });
                if (view_changed || center_changed) && !self.exporting {
                    self.worker.cancel();
                    self.id = u64::MAX;
                    if !center_changed {
                        self.original = None;
                        self.result = None;
                    }
                    self.pending = Some(Instant::now() + Duration::from_millis(90));
                    self.busy = true;
                    self.progress = None;
                    self.stage = "Updating preview".into();
                    self.start = Instant::now();
                    if !self.detail_mode {
                        self.original = self.fit_original.clone();
                    }
                }
            } else {
                ui.centered_and_justified(|ui| {
                    ui.vertical_centered(|ui| {
                        ui.heading(l.text("Open an image"));
                        ui.add_space(12.0);
                        ui.label(l.text("Open a single-layer .pcraft document or an RGB TIFF."));
                        ui.label(l.text("Drag a file here, or choose Open Image."));
                        ui.add_space(appearance::SECTION_GAP);
                        if ui
                            .add_enabled(!self.exporting, egui::Button::new(l.text("Open Image…")))
                            .clicked()
                        {
                            self.open(frame);
                        }
                    });
                });
            }
        });
    }
}
fn adjustment<T: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    l: Language,
    text: &str,
    value: &mut T,
    range: std::ops::RangeInclusive<T>,
) {
    let text: egui::WidgetText = l.text(text).into();
    ui.horizontal(|ui| {
        let label = ui
            .add_sized([104.0, 26.0], egui::Label::new(text.clone()).truncate().halign(egui::Align::LEFT))
            .on_hover_text(text);
        ui.spacing_mut().slider_width = (ui.available_width() - 50.0).max(70.0);
        ui.add(egui::Slider::new(value, range))
            .labelled_by(label.id)
            .on_hover_text(l.text("Drag to adjust, or click the number to type a value. Hold Shift while dragging for finer control."));
    });
}
fn preview_note(ui: &mut egui::Ui, l: Language, text: impl Into<String>) -> egui::Response {
    ui.label(
        RichText::new(l.text(&text.into()))
            .small()
            .color(ui.visuals().weak_text_color()),
    )
}
fn error_label(ui: &mut egui::Ui, l: Language, error: &str) {
    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
    ui.colored_label(ui.visuals().error_fg_color, l.error(error));
    if l.error(error) != error {
        ui.push_id(error, |ui| {
            egui::CollapsingHeader::new(l.text("Technical details"))
                .default_open(false)
                .show(ui, |ui| {
                    ui.label(error);
                });
        });
    }
}
fn task_progress(ui: &mut egui::Ui, progress: Option<f32>) {
    // Reserve identical space through indeterminate/numeric/idle states. Detail
    // crops depend on canvas size; a changing status height would restart them.
    ui.allocate_ui_with_layout(
        egui::vec2(220.0, 18.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_height(18.0);
            if let Some(progress) = progress {
                ui.add(
                    egui::ProgressBar::new(progress)
                        .show_percentage()
                        .desired_width(220.0)
                        .desired_height(18.0),
                );
            }
        },
    );
}
fn detail_region(size: [u32; 2], center: [f32; 2], available: [f32; 2]) -> Crop {
    let width = (available[0].floor() as usize)
        .clamp(1, 1024)
        .min(size[0] as usize);
    let height = (available[1].floor() as usize)
        .clamp(1, 1024)
        .min(size[1] as usize);
    Crop {
        x: ((center[0] * size[0] as f32) as usize)
            .saturating_sub(width / 2)
            .min(size[0] as usize - width),
        y: ((center[1] * size[1] as f32) as usize)
            .saturating_sub(height / 2)
            .min(size[1] as usize - height),
        width,
        height,
    }
}
fn path_label(ui: &mut egui::Ui, path: &std::path::Path) {
    let mut job = egui::text::LayoutJob::simple(
        path.display().to_string(),
        egui::TextStyle::Small.resolve(ui.style()),
        ui.visuals().text_color(),
        ui.available_width(),
    );
    job.wrap.break_anywhere = true;
    ui.label(job);
}

#[derive(Default)]
struct LaunchOptions {
    input: Option<PathBuf>,
    watch: Option<PathBuf>,
    host: Option<HostTarget>,
    data_dir: Option<PathBuf>,
    desktop: Option<DesktopSession>,
    background: bool,
    #[cfg(target_os = "macos")]
    mac_events: Option<fmv_macos_events::Inbox>,
}
impl LaunchOptions {
    fn parse() -> natural_skin_companion::Result<Self> {
        let mut result = Self::default();
        let mut args = std::env::args_os().skip(1);
        while let Some(arg) = args.next() {
            if arg == "--data-dir" {
                result.data_dir = Some(args.next().ok_or("--data-dir needs a folder")?.into());
            } else if arg == "--background" {
                result.background = true;
            } else if arg == "--watch" {
                result.watch = Some(
                    args.next()
                        .ok_or("--watch needs an exchange folder")?
                        .into(),
                );
            } else if arg == "--return-to" {
                result.host = Some(HostTarget::new(&PathBuf::from(
                    args.next()
                        .ok_or("--return-to needs a PhotoCraft application path")?,
                ))?);
            } else if result.input.is_none() && !arg.to_string_lossy().starts_with("--") {
                result.input = Some(arg.into());
            } else {
                return Err("Usage: natural-skin [IMAGE] [--watch EXCHANGE_DIR] [--return-to PHOTOCRAFT_APP] [--data-dir FOLDER] [--background]".into());
            }
        }
        Ok(result)
    }
}
fn main() -> eframe::Result {
    if std::env::args_os().any(|a| a == "--help") {
        println!(
            "Natural Skin desktop\nUsage: natural-skin [IMAGE] [--watch EXCHANGE_DIR] [--return-to PHOTOCRAFT_APP] [--data-dir FOLDER] [--background]\nNormal launch remembers the PhotoCraft connection. --background starts a configured connection minimized. --watch is an isolated developer mode. Use Quit to exit."
        );
        return Ok(());
    }
    let mut options = LaunchOptions::parse().map_err(eframe::Error::AppCreation)?;
    if options.watch.is_none() {
        let root = options
            .data_dir
            .clone()
            .map(Ok)
            .unwrap_or_else(Paths::default_root)
            .map_err(eframe::Error::AppCreation)?;
        let paths = Paths::new(&root).map_err(eframe::Error::AppCreation)?;
        match Instance::start(&paths, options.input.as_deref(), options.background)
            .map_err(eframe::Error::AppCreation)?
        {
            Start::Forwarded => return Ok(()),
            Start::Primary(instance) => {
                options.desktop = Some(DesktopSession::new(paths, instance))
            }
        }
    }
    if options.background
        && options.input.is_none()
        && options
            .desktop
            .as_ref()
            .is_some_and(|s| !s.prefs.listening && s.error.is_none())
    {
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    let (_registration, inbox) = fmv_macos_events::Registration::install();
    #[cfg(target_os = "macos")]
    {
        options.mac_events = Some(inbox);
    }
    let visible = !options.background && (options.watch.is_none() || options.input.is_some());
    eframe::run_native(
        "Natural Skin",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1120.0, 760.0])
                .with_min_inner_size([880.0, 620.0])
                .with_visible(visible),
            ..Default::default()
        },
        Box::new(|cc| {
            #[cfg(target_os = "macos")]
            fmv_macos_events::install_edit_menu();
            Ok(Box::new(App::new(cc, options)?))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn painted_text(shapes: &[egui::epaint::ClippedShape]) -> Vec<String> {
        fn visit(shape: &egui::Shape, output: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(text) => output.push(text.galley.text().to_owned()),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| visit(shape, output)),
                _ => (),
            }
        }
        let mut output = Vec::new();
        for shape in shapes {
            visit(&shape.shape, &mut output);
        }
        output
    }
    #[test]
    fn switching_language_updates_the_actual_workbench_and_preserves_the_image() {
        let (mut app, ctx, root) = managed();
        app.connection_page = false;
        app.ready = true;
        app.path = Some(root.join("Original 原图.pcraft"));
        app.source_label = "Original 原图".into();
        app.source_size = [64, 48];
        app.stage = "Preview complete".into();
        app.detail = "12 ms · cached filter".into();
        app.desktop.as_mut().unwrap().error = Some("Output already exists".into());
        app.info = "64 × 48  ·  16-bit RGB".into();
        app.fit_original = Some(ctx.load_texture(
            "fixture",
            egui::ColorImage::filled([64, 48], Color32::WHITE),
            egui::TextureOptions::LINEAR,
        ));
        app.original = app.fit_original.clone();
        app.result = app.fit_original.clone();
        let original_id = app.original.as_ref().unwrap().id();
        let mut frame = eframe::Frame::_new_kittest();
        for size in [[880.0, 620.0], [1120.0, 760.0]] {
            for appearance in Appearance::ALL {
                for language in Language::ALL {
                    app.language = language;
                    app.appearance = appearance;
                    app.save_interface(&ctx);
                    let mut labels = Vec::new();
                    for _ in 0..2 {
                        let mut out = ctx.run_ui(
                            egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    egui::vec2(size[0], size[1]),
                                )),
                                ..Default::default()
                            },
                            |ui| eframe::App::ui(&mut app, ui, &mut frame),
                        );
                        labels = painted_text(&out.shapes);
                        out.textures_delta.clear();
                    }
                    for key in [
                        "Open Image…",
                        "Language",
                        "Skin retouching",
                        "Strength",
                        "Original",
                        "Preview complete",
                        "12 ms · cached filter",
                        "Output already exists",
                    ] {
                        assert!(
                            labels.iter().any(|label| label == &language.text(key)),
                            "{language:?} {appearance:?} {size:?}: missing {key}: {labels:?}"
                        );
                        if language == Language::Chinese {
                            assert!(
                                !labels.iter().any(|label| label == key),
                                "Untranslated: {key}"
                            );
                        }
                    }
                    assert!(labels.iter().any(|label| label == "Original 原图"));
                    assert_eq!(app.original.as_ref().unwrap().id(), original_id);
                    assert!(app.ready && !app.busy && app.pending.is_none());
                    assert_eq!(app.controls, Controls::default());
                }
            }
        }
        drop(app);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn language_restarts_and_invalid_preferences_are_never_overwritten() {
        let (mut app, ctx, root) = managed();
        app.language = Language::Chinese;
        app.appearance = Appearance::Light;
        app.save_interface(&ctx);
        drop(app);
        let paths = Paths::new(&root).unwrap();
        let Start::Primary(instance) = Instance::start(&paths, None, false).unwrap() else {
            panic!()
        };
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = App::new(
            &cc,
            LaunchOptions {
                desktop: Some(DesktopSession::new(paths, instance)),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(app.language, Language::Chinese);
        assert_eq!(app.appearance, Appearance::Light);
        let settings = root.join("settings.json");
        std::fs::write(&settings, b"{broken settings").unwrap();
        let state = app.desktop.as_mut().unwrap();
        state.prefs_valid = false;
        app.language = Language::English;
        app.save_interface(&ctx);
        assert_eq!(std::fs::read(&settings).unwrap(), b"{broken settings");
        assert!(app.interface_error.is_some());
        drop(app);
        std::fs::remove_dir_all(root).unwrap();
    }
    fn managed() -> (App, egui::Context, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "natural-skin-gui-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let paths = Paths::new(&root).unwrap();
        let Start::Primary(instance) = Instance::start(&paths, None, false).unwrap() else {
            panic!()
        };
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let app = App::new(
            &cc,
            LaunchOptions {
                desktop: Some(DesktopSession::new(paths, instance)),
                ..Default::default()
            },
        )
        .unwrap();
        (app, ctx, root.canonicalize().unwrap())
    }
    #[test]
    fn detail_view_stays_ready_after_render_and_progress_transitions() {
        let (mut app, ctx, root) = managed();
        app.connection_page = false;
        app.ready = true;
        app.source_size = [1536, 1024];
        app.detail_mode = true;
        ctx.set_pixels_per_point(2.0);
        app.fit_original = Some(ctx.load_texture(
            "fit",
            egui::ColorImage::filled([1024, 682], Color32::WHITE),
            egui::TextureOptions::LINEAR,
        ));
        let mut frame = eframe::Frame::_new_kittest();
        let mut crops = Vec::new();
        for i in 0..8 {
            let crop = app.detail_crop.unwrap_or(Crop {
                x: 256,
                y: 0,
                width: 1024,
                height: 1024,
            });
            app.original = Some(ctx.load_texture(
                "original",
                egui::ColorImage::filled([crop.width, crop.height], Color32::WHITE),
                egui::TextureOptions::NEAREST,
            ));
            app.result = app.original.clone();
            app.pending = None;
            app.busy = i % 2 == 0;
            app.progress = if app.busy { Some(0.5) } else { None };
            if i == 4 {
                app.error=Some("Display profile was not changed: unsupported grayscale ICC. Keep the previous working output profile.".repeat(3));
            }
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1120.0, 760.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    eframe::App::ui(&mut app, ui, &mut frame);
                },
            );
            output.textures_delta.clear();
            crops.push(app.detail_crop);
            if i > 2 {
                assert!(app.pending.is_none(), "crop invalidation at {i}: {crops:?}");
            }
        }
        drop(app);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn progress_visibility_does_not_resize_the_detail_canvas() {
        let ctx = egui::Context::default();
        let mut heights = Vec::new();
        for progress in [None, Some(0.0), Some(0.5), Some(1.0), None] {
            let mut output = ctx.run_ui(egui::RawInput::default(), |root_ui| {
                egui::CentralPanel::default().show(root_ui, |ui| {
                    heights.push(
                        ui.scope(|ui| task_progress(ui, progress))
                            .response
                            .rect
                            .height(),
                    );
                });
            });
            output.textures_delta.clear();
        }
        assert!(
            heights.iter().all(|height| *height == heights[0]),
            "{heights:?}"
        );
    }
    #[test]
    fn detail_region_maps_physical_pixels_and_clamps_source_edges() {
        assert_eq!(
            detail_region([6000, 4000], [0.5, 0.5], [960.0, 800.0]),
            Crop {
                x: 2520,
                y: 1600,
                width: 960,
                height: 800
            }
        );
        assert_eq!(
            detail_region([64, 48], [1.0, 0.0], [1600.0, 1200.0]),
            Crop {
                x: 0,
                y: 0,
                width: 64,
                height: 48
            }
        );
        assert_eq!(
            detail_region([6000, 4000], [1.0, 1.0], [2000.0, 2000.0]),
            Crop {
                x: 4976,
                y: 2976,
                width: 1024,
                height: 1024
            }
        );
    }
    #[test]
    fn managed_connection_persists_and_invalid_changes_keep_working_target() {
        let (mut app, ctx, root) = managed();
        assert!(app.connection_page);
        #[cfg(target_os = "macos")]
        let host = root.join("PhotoCraft.app");
        #[cfg(not(target_os = "macos"))]
        let host = root.join("PhotoCraft.exe");
        #[cfg(target_os = "macos")]
        std::fs::create_dir(&host).unwrap();
        #[cfg(not(target_os = "macos"))]
        std::fs::write(&host, b"test").unwrap();
        app.desktop.as_mut().unwrap().host_draft = host.display().to_string();
        app.set_connection(true, &ctx);
        assert!(app.watcher.is_some());
        let state = app.desktop.as_ref().unwrap();
        assert!(state.paths.load().unwrap().listening);
        assert!(state.paths.installer.is_file());
        app.desktop.as_mut().unwrap().host_draft = root.join("missing").display().to_string();
        app.set_connection(true, &ctx);
        assert!(app.desktop.as_ref().unwrap().error.is_some());
        assert!(app.desktop.as_ref().unwrap().notice.is_empty());
        assert_eq!(
            app.host.as_ref().unwrap().path(),
            host.canonicalize().unwrap()
        );
        app.set_connection(false, &ctx);
        assert!(app.watcher.is_none());
        assert!(
            !app.desktop
                .as_ref()
                .unwrap()
                .paths
                .load()
                .unwrap()
                .listening
        );
        drop(app);
        let _ = std::fs::remove_dir_all(root);
    }
    #[test]
    fn second_launch_wakes_minimized_owner_without_interrupting_export() {
        let (mut app, ctx, root) = managed();
        let input = root.join("queued.pcraft");
        std::fs::write(&input, b"test").unwrap();
        let paths = app.desktop.as_ref().unwrap().paths.clone();
        assert!(matches!(
            Instance::start(&paths, Some(&input), false).unwrap(),
            Start::Forwarded
        ));
        app.exporting = true;
        let raw = egui::RawInput::default();
        let mut frame = eframe::Frame::_new_kittest();
        let out = ctx.run_logic(&raw, |ctx| eframe::App::logic(&mut app, ctx, &mut frame));
        assert_eq!(app.queued_open, Some(input.clone()));
        assert!(app.path.is_none());
        assert!(
            out.viewport_commands[&egui::ViewportId::ROOT]
                .iter()
                .any(|c| matches!(c, egui::ViewportCommand::Minimized(false)))
        );
        app.exporting = false;
        let _ = ctx.run_logic(&raw, |ctx| eframe::App::logic(&mut app, ctx, &mut frame));
        assert_eq!(app.path, Some(input));
        drop(app);
        let _ = std::fs::remove_dir_all(root);
    }
    #[test]
    fn receiving_capture_shows_activity_before_worker_loading() {
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = App::new(&cc, LaunchOptions::default()).unwrap();
        app.queued_handoff = Some(HandoffEvent::Receiving);
        let raw = egui::RawInput::default();
        let mut frame = eframe::Frame::_new_kittest();
        let _ = ctx.run_logic(&raw, |ctx| eframe::App::logic(&mut app, ctx, &mut frame));
        assert!(app.busy && app.receiving && !app.ready);
        assert_eq!(app.stage, "Receiving layer from PhotoCraft");
        assert!(app.path.is_none());
    }
    #[test]
    fn hidden_viewport_receives_capture_without_running_ui() {
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = App::new(
            &cc,
            LaunchOptions {
                input: None,
                watch: None,
                host: None,
                ..Default::default()
            },
        )
        .unwrap();
        let input = PathBuf::from("private-capture.pcraft");
        app.queued_handoff = Some(HandoffEvent::Captured(input.clone()));
        let mut raw = egui::RawInput::default();
        raw.viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .minimized = Some(true);
        let mut frame = eframe::Frame::_new_kittest();
        let out = ctx.run_logic(&raw, |ctx| eframe::App::logic(&mut app, ctx, &mut frame));
        assert_eq!(app.path, Some(input));
        assert!(app.busy);
        let commands = &out.viewport_commands[&egui::ViewportId::ROOT];
        assert!(
            commands
                .iter()
                .any(|c| matches!(c, egui::ViewportCommand::Visible(true)))
        );
        assert!(
            commands
                .iter()
                .any(|c| matches!(c, egui::ViewportCommand::Focus))
        );
    }
    #[test]
    fn resident_close_minimizes_window_instead_of_exiting() {
        let path = std::env::temp_dir().join(format!("natural-skin-hidden-{}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = App::new(
            &cc,
            LaunchOptions {
                input: None,
                watch: Some(path.clone()),
                host: None,
                ..Default::default()
            },
        )
        .unwrap();
        let mut raw = egui::RawInput::default();
        raw.viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .events
            .push(egui::ViewportEvent::Close);
        let mut frame = eframe::Frame::_new_kittest();
        let out = ctx.run_logic(&raw, |ctx| eframe::App::logic(&mut app, ctx, &mut frame));
        let commands = &out.viewport_commands[&egui::ViewportId::ROOT];
        assert!(
            commands
                .iter()
                .any(|c| matches!(c, egui::ViewportCommand::CancelClose))
        );
        assert!(
            commands
                .iter()
                .any(|c| matches!(c, egui::ViewportCommand::Minimized(true)))
        );
        drop(app);
        let _ = std::fs::remove_dir_all(path);
    }
}
