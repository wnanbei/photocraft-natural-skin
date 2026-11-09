#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use eframe::egui::{self, Color32, RichText, TextureHandle};
use natural_skin_companion::{
    desktop::{self, Instance, Paths, Preferences, Start},
    handoff::{HandoffEvent, HostTarget, Watcher, result_path},
    startup::LoginStartup,
    worker::{Event, Job, PreviewPixels, Worker},
};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Clone, PartialEq)]
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
        for theme in [egui::Theme::Dark, egui::Theme::Light] {
            let mut style = (*ctx.style_of(theme)).clone();
            style.spacing.item_spacing = egui::vec2(8.0, 8.0);
            style.spacing.button_padding = egui::vec2(12.0, 7.0);
            style.spacing.slider_width = 150.0;
            style.visuals.error_fg_color = if theme == egui::Theme::Dark {
                Color32::from_rgb(255, 150, 150)
            } else {
                Color32::from_rgb(175, 32, 40)
            };
            ctx.set_style_of(theme, style);
        }
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
        if let Some(path) = rfd::FileDialog::new()
            .set_parent(frame)
            .add_filter("PhotoCraft or RGB TIFF", &["pcraft", "tif", "tiff"])
            .pick_file()
        {
            self.load(path);
        }
    }
    fn preview(&mut self) {
        self.pending = None;
        self.begin(
            Job::Preview(self.controls.parameters(false)),
            "Updating preview",
        );
    }
    fn texture(ctx: &egui::Context, p: PreviewPixels, name: &str) -> TextureHandle {
        ctx.load_texture(
            name,
            egui::ColorImage::from_rgba_unmultiplied([p.width, p.height], &p.rgba),
            egui::TextureOptions::LINEAR,
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
                    ..
                } => {
                    self.source_label = label;
                    self.info = format!("{width} × {height}  ·  {depth}-bit RGB");
                    self.original = Some(Self::texture(ctx, original, "Original"));
                    self.ready = true;
                    self.preview();
                }
                Event::Rendered {
                    pixels,
                    cache_hit,
                    milliseconds,
                    ..
                } => {
                    self.result = Some(Self::texture(ctx, pixels, "Retouched"));
                    self.busy = false;
                    self.stage = "Preview complete".into();
                    self.detail = format!(
                        "{milliseconds:.0} ms{}",
                        if cache_hit { " · cached filter" } else { "" }
                    );
                }
                Event::Exported {
                    path, milliseconds, ..
                } => {
                    self.busy = false;
                    self.exporting = false;
                    self.stage = "Export complete".into();
                    self.detail = format!("Saved in {:.2} s", milliseconds / 1000.0);
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
            .add_filter("PhotoCraft document", &["pcraft"])
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
    fn connection_ui(&mut self, ui: &mut egui::Ui, frame: &eframe::Frame) {
        ui.add_space(20.0);
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.set_max_width(680.0);
            ui.heading("Connect to PhotoCraft");
            ui.label("Send a selected pixel layer, adjust its skin, then open the result as a new PhotoCraft document.");
            ui.add_space(20.0);
            let Some(state) = &mut self.desktop else { return; };
            let mut enable = None;
            ui.strong("PhotoCraft application");
            let label = ui.label("Choose PhotoCraft.app on macOS or PhotoCraft.exe on Windows.");
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut state.host_draft).desired_width(460.0)).labelled_by(label.id);
                if ui.button("Choose…").clicked() {
                    let dialog = rfd::FileDialog::new().set_parent(frame);
                    #[cfg(target_os = "macos")]
                    let dialog = dialog.add_filter("PhotoCraft application", &["app"]);
                    #[cfg(target_os = "windows")]
                    let dialog = dialog.add_filter("PhotoCraft application", &["exe"]);
                    if let Some(path) = dialog.pick_file() { state.host_draft = path.to_string_lossy().into_owned(); }
                }
            });
            ui.horizontal(|ui| {
                if ui.add_enabled(!self.exporting, egui::Button::new(if self.watcher.is_some() { "Save connection" } else { "Enable connection" })).clicked() { enable = Some(true); }
                if self.watcher.is_some() && ui.add_enabled(!self.exporting,egui::Button::new("Stop connection")).clicked() { enable = Some(false); }
                ui.label(if self.watcher.is_some() { "Listening" } else { "Stopped" });
            });
            if !state.notice.is_empty() { ui.label(&state.notice); }
            ui.add_space(20.0);
            ui.strong("Install the Send action once");
            ui.label("In PhotoCraft, choose File → Scripts → Browse and select this installer. Then find Natural Skin Send in the Actions panel.");
            path_label(ui, &state.paths.installer);
            ui.horizontal(|ui| {
                if ui.add_enabled(state.paths.installer.exists(),egui::Button::new("Show installer folder")).clicked()
                    && let Err(e) = desktop::show_folder(&state.paths.root) { state.fail(e.to_string()); }
                if ui.button("Copy installer path").clicked() { ui.ctx().copy_text(state.paths.installer.display().to_string()); }
            });
            ui.label("If the action is already present, use it directly. To reinstall, delete only Natural Skin Send from PhotoCraft first.");
            ui.add_space(20.0);
            ui.strong("Keep Natural Skin available");
            ui.label("While connected, closing the window minimizes it and keeps receiving layers. Use Quit to stop the app.");
            let mut login = state.login_enabled;
            if ui.add_enabled(state.startup.is_some(),egui::Checkbox::new(&mut login,"Start at login")).changed() {
                match state.startup.as_ref().unwrap().set_enabled(login) {
                    Ok(()) => { state.login_enabled=login; state.notice="Login startup updated. Keep Natural Skin in its current location, or update this setting after moving it.".into(); state.error=None; }
                    Err(e) => state.fail(format!("Could not update login startup: {e}")),
                }
            }
            ui.add_space(20.0);
            ui.strong("Saved results");
            ui.label("Completed results are kept until you remove them. Save the returned document in PhotoCraft to a location of your choice.");
            let mut reopen=None;
            if let Some(path) = &state.prefs.last_result {
                path_label(ui, path);
                if ui.add_enabled(!self.exporting && path.is_file(),egui::Button::new("Open last result")).clicked() { reopen=Some(path.clone()); }
            }
            if ui.button("Show saved files").clicked()
                && let Err(e) = desktop::show_folder(&state.paths.root) { state.fail(e.to_string()); }
            ui.add_space(20.0);
            if ui.button("Back to image").clicked() { self.connection_page=false; }
            if let Some(enabled)=enable { self.set_connection(enabled,ui.ctx()); }
            if let Some(path)=reopen { self.load(path); }
        });
    }
    fn inspector(&mut self, ui: &mut egui::Ui) {
        let old = self.controls.clone();
        ui.heading("Skin retouching");
        ui.add_space(12.0);
        ui.checkbox(&mut self.controls.skin, "Skin only")
            .on_hover_text("Limit changes to the detected skin-colour range");
        ui.add_space(8.0);
        let label = ui.label("Strength");
        ui.add(egui::Slider::new(&mut self.controls.strength, 0..=100))
            .labelled_by(label.id);
        let label = ui.label("Radius (px)");
        ui.add(egui::Slider::new(&mut self.controls.radius, 1..=20))
            .labelled_by(label.id);
        let label = ui.label("Edge protection");
        ui.add(egui::Slider::new(&mut self.controls.edge, 0..=100))
            .labelled_by(label.id);
        let label = ui.label("Texture");
        ui.add(egui::Slider::new(&mut self.controls.texture, 0..=100))
            .labelled_by(label.id);
        ui.add_space(20.0);
        ui.strong("Skin range");
        let label = ui.label("Range");
        ui.add(egui::Slider::new(&mut self.controls.range, 0..=100))
            .labelled_by(label.id);
        ui.checkbox(&mut self.controls.mask, "Show skin mask");
        ui.add_space(20.0);
        ui.strong("Skin colour");
        let label = ui.label("Brightening");
        ui.add(egui::Slider::new(&mut self.controls.bright, 0..=100))
            .labelled_by(label.id);
        let label = ui.label("Rosiness");
        ui.add(egui::Slider::new(&mut self.controls.rose, -100..=100))
            .labelled_by(label.id);
        ui.add_space(20.0);
        if ui.button("Reset adjustments").clicked() {
            self.controls = Controls::default();
        }
        ui.add_space(20.0);
        ui.label(
            RichText::new("Fit preview is approximate. Export uses the full-resolution source.")
                .small(),
        );
        ui.add_space(8.0);
        ui.label(RichText::new("Current colour workflow: sRGB.").small());
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
                ui.label(RichText::new("Natural Skin").size(20.0).strong());
                ui.add_space(16.0);
                if ui
                    .add_enabled(!self.exporting, egui::Button::new("Open Image…"))
                    .clicked()
                {
                    self.open(frame);
                }
                if ui
                    .add_enabled(
                        self.ready && !self.exporting,
                        egui::Button::new("Export Result…"),
                    )
                    .clicked()
                {
                    self.export(frame);
                }
                if self.host.is_some()
                    && ui
                        .add_enabled(
                            self.ready && !self.exporting,
                            egui::Button::new("Apply to PhotoCraft"),
                        )
                        .clicked()
                {
                    self.apply_to_host();
                }
                if self.desktop.is_some()
                    && ui
                        .selectable_label(self.connection_page, "Connection…")
                        .clicked()
                {
                    self.connection_page = !self.connection_page;
                }
                if ui.button("Quit").clicked() {
                    self.request_quit(&ctx);
                }
                ui.label(RichText::new(&self.info).small());
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
            ui.add_space(6.0);
        });
        egui::Panel::bottom("task_status")
            .min_size(48.0)
            .show(ui, |ui| {
                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    if self.busy {
                        ui.spinner();
                    }
                    ui.strong(&self.stage);
                    if self.busy {
                        ui.label(format!(
                            "{:.1} s elapsed",
                            self.start.elapsed().as_secs_f64()
                        ));
                        if ui
                            .add_enabled(
                                self.opening.is_none() && !self.receiving,
                                egui::Button::new("Cancel"),
                            )
                            .on_disabled_hover_text(
                                "Wait for layer capture or the PhotoCraft open request to finish",
                            )
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
                        ui.label(&self.detail);
                    }
                });
                if self.busy
                    && let Some(progress) = self.progress
                {
                    ui.add(
                        egui::ProgressBar::new(progress)
                            .show_percentage()
                            .desired_width(220.0),
                    );
                }
                if self.watcher.is_some()
                    && self.desktop.is_none()
                    && !self.busy
                    && ui.button("Stop Listening").clicked()
                {
                    self.watcher = None;
                    self.stage = "Listener stopped".into();
                }
                if let Some(error) = &self.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                if let Some(state) = &self.desktop
                    && let Some(error) = &state.error
                {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                if let Some(path) = &self.saved {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!(
                                "Saved: {}",
                                path.file_name().unwrap_or_default().to_string_lossy()
                            ))
                            .small(),
                        );
                        if ui.button("Copy saved path").clicked() {
                            ui.ctx().copy_text(path.display().to_string());
                        }
                    });
                }
                if self.queued_open.is_some() {
                    ui.label("An image will open after the current export completes.");
                }
                ui.add_space(5.0);
            });
        if self.connection_page {
            egui::CentralPanel::default().show(ui, |ui| self.connection_ui(ui, frame));
            return;
        }
        egui::Panel::right("adjustments")
            .exact_size(300.0)
            .resizable(false)
            .show(ui, |ui| {
                ui.add_space(12.0);
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                egui::Frame::new().inner_margin(12.0).show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.spacing_mut().slider_width =
                                (ui.available_width() - 60.0).max(100.0);
                            ui.add_enabled_ui(self.ready && !self.exporting, |ui| {
                                self.inspector(ui)
                            });
                        });
                });
            });
        egui::CentralPanel::default().show(ui, |ui| {
            if self.original.is_some() {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.show_original, true, "Original");
                    ui.selectable_value(
                        &mut self.show_original,
                        false,
                        if self.controls.mask {
                            "Skin mask"
                        } else {
                            "Retouched"
                        },
                    );
                });
                let texture = if self.show_original {
                    self.original.as_ref()
                } else {
                    self.result.as_ref().or(self.original.as_ref())
                };
                let area = ui.available_size();
                egui::Frame::new()
                    .fill(Color32::from_rgb(35, 37, 39))
                    .inner_margin(16.0)
                    .show(ui, |ui| {
                        ui.set_min_size(area - egui::vec2(32.0, 32.0));
                        ui.centered_and_justified(|ui| {
                            if let Some(texture) = texture {
                                ui.add(
                                    egui::Image::new(texture).fit_to_exact_size(
                                        texture.size_vec2()
                                            * (ui.available_width() / texture.size_vec2().x)
                                                .min(ui.available_height() / texture.size_vec2().y),
                                    ),
                                );
                            }
                        });
                    });
            } else {
                ui.centered_and_justified(|ui| {
                    ui.vertical_centered(|ui| {
                        ui.heading("Open an image");
                        ui.add_space(12.0);
                        ui.label("Open a single-layer .pcraft document or an RGB TIFF.");
                        ui.label("Drag a file here, or choose Open Image.");
                        ui.add_space(20.0);
                        if ui
                            .add_enabled(!self.exporting, egui::Button::new("Open Image…"))
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
