#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ffmpeg;
mod jobs;
mod multicut;
mod settings;
mod time;

use eframe::egui;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Combine,
    Compress,
    Cut,
    MultiCut,
    Rename,
}

struct App {
    settings: settings::Settings,
    tab: Tab,
    busy: bool,
    rx: Option<Receiver<jobs::UiEvent>>,
    status: String,
    progress: Option<f32>,
    toast: Option<(String, bool)>,
    toast_at: Option<std::time::Instant>,
    error: Option<String>,
    show_settings: bool,
    first_frame: bool,

    visual_path: Option<PathBuf>,
    is_visual_video: bool,
    audio_path: Option<PathBuf>,
    videos_folder: Option<PathBuf>,
    video_for_cut: Option<PathBuf>,
    multi_cut_folder: Option<PathBuf>,
    rename_folder: Option<PathBuf>,

    start_time: String,
    end_time: String,
    multi_cut_input: String,
    rename_length: String,
    find_text: String,
    replace_text: String,

    // Estado de edición de Ajustes (persistente entre frames)
    s_ffmpeg_path: String,
    s_crf: i32,
    s_preset: String,
    s_gen_report: bool,
    s_accent: String,
}

impl Default for App {
    fn default() -> Self {
        let settings = settings::load();
        let tab = match settings.last_tab {
            1 => Tab::Compress,
            2 => Tab::Cut,
            3 => Tab::MultiCut,
            4 => Tab::Rename,
            _ => Tab::Combine,
        };
        let visual_path = existing_path(&settings.last_visual);
        let is_visual_video = visual_path.as_deref().map(ffmpeg::is_video_ext).unwrap_or(false);
        let audio_path = existing_path(&settings.last_audio);
        let videos_folder = existing_path(&settings.last_videos_folder);
        let video_for_cut = existing_path(&settings.last_video_for_cut);
        let multi_cut_folder = existing_path(&settings.last_multi_cut_folder);
        let rename_folder = existing_path(&settings.last_rename_folder);
        Self {
            settings,
            tab,
            busy: false,
            rx: None,
            status: String::new(),
            progress: None,
            toast: None,
            toast_at: None,
            error: None,
            show_settings: false,
            first_frame: true,
            visual_path,
            is_visual_video,
            audio_path,
            videos_folder,
            video_for_cut,
            multi_cut_folder,
            rename_folder,
            start_time: String::new(),
            end_time: String::new(),
            multi_cut_input: String::new(),
            rename_length: "8".to_string(),
            find_text: String::new(),
            replace_text: String::new(),
            s_ffmpeg_path: "ffmpeg".to_string(),
            s_crf: 25,
            s_preset: "medium".to_string(),
            s_gen_report: true,
            s_accent: "amber400".to_string(),
        }
    }
}

impl App {
    fn accent(&self) -> egui::Color32 {
        settings::accent_color(&self.settings.accent)
    }

    fn gray(&self) -> egui::Color32 {
        egui::Color32::from_gray(180)
    }

    fn red(&self) -> egui::Color32 {
        egui::Color32::from_rgb(239, 83, 80)
    }

    fn show_toast(&mut self, msg: impl Into<String>, is_err: bool) {
        self.toast = Some((msg.into(), is_err));
        self.toast_at = Some(std::time::Instant::now());
    }

    fn start_job<F>(&mut self, f: F)
    where
        F: FnOnce(&Sender<jobs::UiEvent>) + Send + 'static,
    {
        if self.busy {
            return;
        }
        self.busy = true;
        self.progress = None;
        self.status = String::new();
        let (tx, rx) = channel();
        self.rx = Some(rx);
        let tx_fin = tx.clone();
        std::thread::spawn(move || {
            f(&tx_fin);
            let _ = tx.send(jobs::UiEvent::Finished);
        });
    }

    fn drain_events(&mut self, ctx: &egui::Context) {
        let mut events: Vec<jobs::UiEvent> = Vec::new();
        if let Some(rx) = &self.rx {
            while let Ok(ev) = rx.try_recv() {
                events.push(ev);
            }
        }
        let mut finished = false;
        for ev in events {
            match ev {
                jobs::UiEvent::Status(s) => self.status = s,
                jobs::UiEvent::Progress(p) => self.progress = Some(p),
                jobs::UiEvent::Toast(msg) => self.show_toast(msg, false),
                jobs::UiEvent::Error(msg) => {
                    self.error = Some(msg);
                    self.show_toast("Error", true);
                }
                jobs::UiEvent::Finished => finished = true,
            }
        }
        if finished {
            self.busy = false;
            self.rx = None;
            self.progress = None;
        }
        if self.busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    fn apply_theme(&self, ctx: &egui::Context) {
        let accent = self.accent();
        let mut style = (*ctx.style()).clone();

        // Tipografía más grande y legible
        style.text_styles.insert(
            egui::TextStyle::Heading,
            egui::FontId::proportional(22.0),
        );
        style.text_styles.insert(egui::TextStyle::Body, egui::FontId::proportional(16.0));
        style.text_styles.insert(egui::TextStyle::Button, egui::FontId::proportional(15.0));
        style.text_styles.insert(egui::TextStyle::Small, egui::FontId::proportional(12.0));
        style.text_styles.insert(
            egui::TextStyle::Monospace,
            egui::FontId::monospace(14.0),
        );

        // Espaciado más aireado
        style.spacing.item_spacing = egui::vec2(10.0, 12.0);
        style.spacing.button_padding = egui::vec2(16.0, 10.0);
        style.spacing.interact_size.y = 34.0;
        style.spacing.slider_width = 170.0;
        style.spacing.text_edit_width = 230.0;

        // Paleta oscura con mejor contraste
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = egui::Color32::from_rgb(15, 16, 20);
        visuals.window_fill = egui::Color32::from_rgb(24, 25, 31);
        visuals.faint_bg_color = egui::Color32::from_rgb(20, 21, 26);
        visuals.extreme_bg_color = egui::Color32::from_rgb(12, 13, 16);
        visuals.override_text_color = Some(egui::Color32::from_gray(245));
        visuals.hyperlink_color = accent;
        visuals.selection.bg_fill = accent;
        visuals.selection.stroke = egui::Stroke::new(1.0_f32, accent);

        // Widgets redondeados y con estados bien diferenciados
        let round = egui::Rounding::same(8.0);
        visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(30, 31, 38);
        visuals.widgets.noninteractive.bg_stroke =
            egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(52, 54, 62));
        visuals.widgets.noninteractive.rounding = round;
        visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0_f32, egui::Color32::from_gray(218));
        visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(38, 40, 48);
        visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, egui::Color32::from_gray(235));
        visuals.widgets.inactive.rounding = round;
        visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(58, 60, 70);
        visuals.widgets.hovered.rounding = round;
        visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0_f32, egui::Color32::WHITE);
        visuals.widgets.active.bg_fill = egui::Color32::from_rgb(72, 74, 86);
        visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0_f32, egui::Color32::WHITE);
        visuals.widgets.active.rounding = round;
        visuals.widgets.open.bg_fill = egui::Color32::from_rgb(48, 50, 60);
        visuals.widgets.open.rounding = round;
        visuals.widgets.open.fg_stroke = egui::Stroke::new(1.0_f32, egui::Color32::WHITE);

        style.visuals = visuals;
        ctx.set_style(style);
    }

    fn toast_expire(&mut self) {
        if let Some(t) = self.toast_at {
            if t.elapsed().as_secs_f32() > 4.0 {
                self.toast = None;
                self.toast_at = None;
            }
        }
    }

    fn require_ffmpeg(&mut self) -> bool {
        if ffmpeg::ffmpeg_available(&self.settings.ffmpeg_path) {
            true
        } else {
            self.error = Some(format!(
                "FFmpeg no está instalado o no se encuentra en '{}'. Ve a Ajustes para configurarlo.",
                self.settings.ffmpeg_path
            ));
            false
        }
    }

    fn settings_summary(&self) -> String {
        format!(
            "⚡ CRF: {} | Preset: {} | Reporte: {}",
            self.settings.crf,
            self.settings.preset,
            if self.settings.gen_report { "Sí" } else { "No" }
        )
    }

    // ---- nombres mostrados ----
    fn file_name_display(p: &Option<PathBuf>, fallback: &str, prefix: &str) -> String {
        match p {
            Some(path) => {
                let name = path
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                if prefix.is_empty() {
                    name
                } else {
                    format!("{prefix}{name}")
                }
            }
            None => fallback.to_string(),
        }
    }

    // ---- selectores ----
    fn pick_visual(&mut self) {
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("Imágenes", &["jpg", "jpeg", "png", "webp"])
            .add_filter("Videos", &["mp4", "mov", "mkv", "avi", "webm", "m4v"])
            .pick_file()
        {
            self.is_visual_video = ffmpeg::is_video_ext(&p);
            self.visual_path = Some(p);
        }
    }

    fn pick_audio(&mut self) {
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("Audio", &["mp3", "wav", "m4a", "opus", "flac", "aac", "ogg"])
            .pick_file()
        {
            self.audio_path = Some(p);
        }
    }

    fn pick_folder(&mut self) {
        if let Some(p) = rfd::FileDialog::new().pick_folder() {
            self.videos_folder = Some(p);
        }
    }

    fn pick_multi_cut_folder(&mut self) {
        if let Some(p) = rfd::FileDialog::new().pick_folder() {
            self.multi_cut_folder = Some(p);
        }
    }

    fn pick_video_for_cut(&mut self) {
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("Videos", &["mp4", "mov", "mkv", "avi", "webm", "m4v"])
            .pick_file()
        {
            self.video_for_cut = Some(p);
        }
    }

    fn pick_rename_folder(&mut self) {
        if let Some(p) = rfd::FileDialog::new().pick_folder() {
            self.rename_folder = Some(p);
        }
    }

    // ---- handlers de acción ----
    fn combine_clicked(&mut self) {
        let (visual, audio) = match (self.visual_path.clone(), self.audio_path.clone()) {
            (Some(v), Some(a)) => (v, a),
            _ => {
                self.show_toast("Please select visual and audio first!", true);
                return;
            }
        };
        if !visual.exists() || !audio.exists() {
            self.show_toast("Alguno de los archivos seleccionados ya no existe.", true);
            return;
        }
        if !self.require_ffmpeg() {
            return;
        }
        let ffmpeg = self.settings.ffmpeg_path.clone();
        let ffprobe = ffmpeg::get_ffprobe_path(&ffmpeg);
        let is_video = self.is_visual_video;
        let visual_s = visual.to_string_lossy().to_string();
        let audio_s = audio.to_string_lossy().to_string();
        self.start_job(move |tx| {
            jobs::combine(tx, &ffmpeg, &ffprobe, &visual_s, &audio_s, is_video);
        });
    }

    fn compress_clicked(&mut self) {
        let folder = match self.videos_folder.clone() {
            Some(f) if f.is_dir() => f,
            _ => {
                self.show_toast("Please select a folder first!", true);
                return;
            }
        };
        if !self.require_ffmpeg() {
            return;
        }
        let ffmpeg = self.settings.ffmpeg_path.clone();
        let ffprobe = ffmpeg::get_ffprobe_path(&ffmpeg);
        let crf = self.settings.crf;
        let preset = self.settings.preset.clone();
        let gen_report = self.settings.gen_report;
        let folder_s = folder.to_string_lossy().to_string();
        self.start_job(move |tx| {
            jobs::compress(tx, &ffmpeg, &ffprobe, &folder_s, crf, &preset, gen_report);
        });
    }

    fn cut_clicked(&mut self) {
        let src = match self.video_for_cut.clone() {
            Some(s) => s,
            None => {
                self.show_toast("Please enter all details!", true);
                return;
            }
        };
        let start_val = self.start_time.trim().to_string();
        let end_val = self.end_time.trim().to_string();
        if start_val.is_empty() || end_val.is_empty() {
            self.show_toast("Please enter all details!", true);
            return;
        }
        if !src.exists() {
            self.show_toast("El video seleccionado ya no existe.", true);
            return;
        }
        if !time::is_valid_time(&start_val) || !time::is_valid_time(&end_val) {
            self.show_toast("Formato de tiempo inválido. Usa HH:MM:SS o MM:SS", true);
            return;
        }
        let (start, end) = match (time::time_to_seconds(&start_val), time::time_to_seconds(&end_val)) {
            (Some(s), Some(e)) if e > s => (s, e),
            _ => {
                self.show_toast("El tiempo final debe ser mayor al inicial", true);
                return;
            }
        };
        if !self.require_ffmpeg() {
            return;
        }
        let ffmpeg = self.settings.ffmpeg_path.clone();
        let crf = self.settings.crf;
        let preset = self.settings.preset.clone();
        let src_s = src.to_string_lossy().to_string();
        self.start_job(move |tx| {
            jobs::cut(tx, &ffmpeg, &src_s, start, end, crf, &preset);
        });
    }

    fn multicut_clicked(&mut self) {
        let folder = match self.multi_cut_folder.clone() {
            Some(f) if f.is_dir() => f,
            _ => {
                self.show_toast("Please select a folder and enter instructions!", true);
                return;
            }
        };
        let txt = self.multi_cut_input.clone();
        if txt.trim().is_empty() {
            self.show_toast("Please select a folder and enter instructions!", true);
            return;
        }
        if !self.require_ffmpeg() {
            return;
        }
        let ffmpeg = self.settings.ffmpeg_path.clone();
        let crf = self.settings.crf;
        let preset = self.settings.preset.clone();
        let folder_s = folder.to_string_lossy().to_string();
        self.start_job(move |tx| {
            jobs::multicut(tx, &ffmpeg, &folder_s, &txt, crf, &preset);
        });
    }

    fn random_rename_clicked(&mut self) {
        let folder = match self.rename_folder.clone() {
            Some(f) if f.is_dir() => f,
            _ => {
                self.show_toast("Please select a folder first!", true);
                return;
            }
        };
        let length: usize = match self.rename_length.trim().parse() {
            Ok(n) if (1..=200).contains(&n) => n,
            _ => {
                self.show_toast("Error: la longitud debe ser un número entero positivo", true);
                return;
            }
        };
        let folder_s = folder.to_string_lossy().to_string();
        let mut renamed = 0usize;
        let mut errors: Vec<String> = Vec::new();
        let mut reserved: HashSet<String> = HashSet::new();
        for (filename, old_path) in self.list_files(&folder_s) {
            let ext = Path::new(&filename)
                .extension()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let new_name = if ext.is_empty() {
                random_name(length)
            } else {
                format!("{}.{}", random_name(length), ext)
            };
            let new_path = jobs::unique_path(&folder_s, &new_name, &mut reserved);
            match std::fs::rename(&old_path, &new_path) {
                Ok(()) => renamed += 1,
                Err(e) => errors.push(format!("{filename}: {e}")),
            }
        }
        self.finish_rename(renamed, errors);
    }

    fn find_replace_clicked(&mut self) {
        let folder = match self.rename_folder.clone() {
            Some(f) if f.is_dir() => f,
            _ => {
                self.show_toast("Please select a folder and enter text to find!", true);
                return;
            }
        };
        let find_text = self.find_text.clone();
        if find_text.trim().is_empty() {
            self.show_toast("Please select a folder and enter text to find!", true);
            return;
        }
        let replace_text = self.replace_text.clone();
        if replace_text.contains('/') || replace_text.contains('\\') {
            self.show_toast("El texto de reemplazo no puede contener / ni \\", true);
            return;
        }
        let folder_s = folder.to_string_lossy().to_string();
        let mut renamed = 0usize;
        let mut errors: Vec<String> = Vec::new();
        let mut reserved: HashSet<String> = HashSet::new();
        for (filename, old_path) in self.list_files(&folder_s) {
            let p = Path::new(&filename);
            let name = p
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let ext = p
                .extension()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            if !name.contains(&find_text) {
                continue;
            }
            let new_name = name.replace(&find_text, &replace_text).trim().to_string();
            if new_name.is_empty() {
                errors.push(format!("{filename}: el nombre resultante quedaría vacío"));
                continue;
            }
            let new_filename = if ext.is_empty() {
                new_name
            } else {
                format!("{new_name}.{ext}")
            };
            if new_filename == filename {
                continue;
            }
            let new_path = jobs::unique_path(&folder_s, &new_filename, &mut reserved);
            match std::fs::rename(&old_path, &new_path) {
                Ok(()) => renamed += 1,
                Err(e) => errors.push(format!("{filename}: {e}")),
            }
        }
        self.finish_rename(renamed, errors);
    }

    fn finish_rename(&mut self, renamed: usize, errors: Vec<String>) {
        if !errors.is_empty() {
            self.error = Some(format!("Errores al renombrar\n{}", errors.join("\n")));
        }
        if renamed > 0 {
            self.show_toast(format!("Renamed {renamed} files!"), false);
        } else if errors.is_empty() {
            self.show_toast("No files were renamed.", true);
        }
    }

    fn list_files(&self, folder: &str) -> Vec<(String, String)> {
        let mut out = Vec::new();
        if let Ok(rd) = std::fs::read_dir(folder) {
            for entry in rd.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }
                let path = entry.path();
                if path.is_file() {
                    out.push((name, path.to_string_lossy().to_string()));
                }
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    // ---- UI ----
    fn ui_tab(&mut self, ui: &mut egui::Ui) {
        let accent = self.accent();
        let black = egui::Color32::BLACK;
        let gray = self.gray();

        match self.tab {
            Tab::Combine => {
                ui.heading("Combine Visual with Audio");
                ui.separator();
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("Select Visual (Image/Video)").color(black))
                            .fill(egui::Color32::from_rgb(171, 71, 188)),
                    )
                    .clicked()
                {
                    self.pick_visual();
                }
                ui.colored_label(gray, Self::file_name_display(&self.visual_path, "No visual selected", ""));
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("Select Audio").color(black))
                            .fill(egui::Color32::from_rgb(103, 58, 183)),
                    )
                    .clicked()
                {
                    self.pick_audio();
                }
                ui.colored_label(gray, Self::file_name_display(&self.audio_path, "No audio selected", ""));
                ui.add_space(6.0);
                let enabled = self.visual_path.is_some() && self.audio_path.is_some() && !self.busy;
                if ui
                    .add_enabled(
                        enabled,
                        egui::Button::new(egui::RichText::new("Combine").color(black)).fill(accent),
                    )
                    .clicked()
                {
                    self.combine_clicked();
                }
            }
            Tab::Compress => {
                ui.heading("Compress Videos");
                ui.separator();
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("Select Folder").color(black))
                            .fill(egui::Color32::from_rgb(63, 81, 181)),
                    )
                    .clicked()
                {
                    self.pick_folder();
                }
                ui.colored_label(gray, Self::file_name_display(&self.videos_folder, "No folder selected", "Folder: "));
                ui.colored_label(accent, self.settings_summary());
                ui.add_space(6.0);
                let enabled = self.videos_folder.is_some() && !self.busy;
                if ui
                    .add_enabled(
                        enabled,
                        egui::Button::new(egui::RichText::new("Compress").color(black)).fill(accent),
                    )
                    .clicked()
                {
                    self.compress_clicked();
                }
            }
            Tab::Cut => {
                ui.heading("Cut Video");
                ui.separator();
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("Select Video").color(black))
                            .fill(egui::Color32::from_rgb(255, 167, 38)),
                    )
                    .clicked()
                {
                    self.pick_video_for_cut();
                }
                ui.colored_label(gray, Self::file_name_display(&self.video_for_cut, "No video selected", "Video: "));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Start Time");
                    ui.add(egui::TextEdit::singleline(&mut self.start_time).hint_text("HH:MM:SS").desired_width(110.0));
                });
                ui.horizontal(|ui| {
                    ui.label("End Time");
                    ui.add(egui::TextEdit::singleline(&mut self.end_time).hint_text("HH:MM:SS").desired_width(110.0));
                });
                ui.add_space(6.0);
                let enabled = self.video_for_cut.is_some() && !self.busy;
                if ui
                    .add_enabled(
                        enabled,
                        egui::Button::new(egui::RichText::new("Cut Video").color(black)).fill(accent),
                    )
                    .clicked()
                {
                    self.cut_clicked();
                }
            }
            Tab::MultiCut => {
                ui.heading("Multi-Cut");
                ui.separator();
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("Select Videos Folder").color(black))
                            .fill(egui::Color32::from_rgb(0, 188, 212)),
                    )
                    .clicked()
                {
                    self.pick_multi_cut_folder();
                }
                ui.colored_label(gray, Self::file_name_display(&self.multi_cut_folder, "No folder selected", "Folder: "));
                ui.add_space(4.0);
                ui.label("Instructions (ID START - END [ROT])");
                ui.scope(|ui| {
                    ui.visuals_mut().extreme_bg_color = egui::Color32::from_rgb(36, 38, 46);
                    ui.add(
                        egui::TextEdit::multiline(&mut self.multi_cut_input)
                            .hint_text("4632 00:04 - 00:35\n2739 r90")
                            .desired_rows(6)
                            .desired_width(f32::INFINITY),
                    );
                });
                ui.add_space(6.0);
                let enabled = self.multi_cut_folder.is_some() && !self.busy;
                if ui
                    .add_enabled(
                        enabled,
                        egui::Button::new(egui::RichText::new("Process All Cuts").color(black)).fill(accent),
                    )
                    .clicked()
                {
                    self.multicut_clicked();
                }
            }
            Tab::Rename => {
                ui.heading("File Renamer");
                ui.separator();
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("Select Folder").color(black))
                            .fill(egui::Color32::from_rgb(96, 125, 139)),
                    )
                    .clicked()
                {
                    self.pick_rename_folder();
                }
                ui.colored_label(gray, Self::file_name_display(&self.rename_folder, "No folder selected", "Folder: "));
                ui.add_space(6.0);
                ui.label(egui::RichText::new("1. Randomize File Names").size(16.0).color(egui::Color32::from_gray(200)));
                ui.horizontal(|ui| {
                    ui.label("Length");
                    ui.add(egui::TextEdit::singleline(&mut self.rename_length).desired_width(70.0));
                    if ui
                        .add(egui::Button::new(egui::RichText::new("Randomize All").color(black)).fill(accent))
                        .clicked()
                    {
                        self.random_rename_clicked();
                    }
                });
                ui.add_space(6.0);
                ui.label(egui::RichText::new("2. Find & Replace").size(16.0).color(egui::Color32::from_gray(200)));
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.find_text).hint_text("text to remove").desired_width(140.0));
                    ui.add(egui::TextEdit::singleline(&mut self.replace_text).hint_text("new text").desired_width(140.0));
                });
                ui.add_space(4.0);
                if ui
                    .add(egui::Button::new(egui::RichText::new("Process Text").color(black)).fill(accent))
                    .clicked()
                {
                    self.find_replace_clicked();
                }
            }
        }
    }

    fn center_window_once(&mut self, ctx: &egui::Context) {
        if !self.first_frame {
            return;
        }
        let (monitor, outer) = ctx.input(|i| {
            let vp = i.viewport();
            (vp.monitor_size, vp.outer_rect)
        });
        if let (Some(monitor), Some(outer)) = (monitor, outer) {
            let size = outer.size();
            let pos = egui::pos2(
                ((monitor.x - size.x) / 2.0).max(0.0),
                ((monitor.y - size.y) / 2.0).max(0.0),
            );
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
            self.first_frame = false;
        }
    }

    fn persist_selections(&mut self) {
        let tab = match self.tab {
            Tab::Combine => 0u32,
            Tab::Compress => 1,
            Tab::Cut => 2,
            Tab::MultiCut => 3,
            Tab::Rename => 4,
        };
        let visual = path_str(&self.visual_path);
        let audio = path_str(&self.audio_path);
        let vcut = path_str(&self.video_for_cut);
        let vids = path_str(&self.videos_folder);
        let mcut = path_str(&self.multi_cut_folder);
        let ren = path_str(&self.rename_folder);
        if self.settings.last_tab != tab
            || self.settings.last_visual != visual
            || self.settings.last_audio != audio
            || self.settings.last_video_for_cut != vcut
            || self.settings.last_videos_folder != vids
            || self.settings.last_multi_cut_folder != mcut
            || self.settings.last_rename_folder != ren
        {
            self.settings.last_tab = tab;
            self.settings.last_visual = visual;
            self.settings.last_audio = audio;
            self.settings.last_video_for_cut = vcut;
            self.settings.last_videos_folder = vids;
            self.settings.last_multi_cut_folder = mcut;
            self.settings.last_rename_folder = ren;
            settings::save(&self.settings);
        }
    }

    fn open_settings(&mut self) {
        self.s_ffmpeg_path = self.settings.ffmpeg_path.clone();
        self.s_crf = self.settings.crf;
        self.s_preset = self.settings.preset.clone();
        self.s_gen_report = self.settings.gen_report;
        self.s_accent = self.settings.accent.clone();
        self.show_settings = true;
    }

    fn settings_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_settings;
        let mut save = false;
        let mut clear = false;
        let mut cancel = false;
        let red = self.red();

        egui::Window::new("Ajustes")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_size([360.0, 470.0])
            .show(ctx, |ui| {
                ui.label(egui::RichText::new("Ruta FFmpeg").strong());
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.s_ffmpeg_path).desired_width(210.0));
                    if ui.button("Seleccionar").clicked() {
                        if let Some(p) = rfd::FileDialog::new().pick_file() {
                            self.s_ffmpeg_path = p.to_string_lossy().to_string();
                        }
                    }
                });
                ui.add_space(8.0);
                ui.label(egui::RichText::new("Compresión").strong());
                ui.horizontal(|ui| {
                    ui.label("CRF (Calidad):");
                    ui.add(egui::Slider::new(&mut self.s_crf, 18..=30).show_value(true));
                });
                egui::ComboBox::from_label("Preset")
                    .selected_text(self.s_preset.clone())
                    .show_ui(ui, |ui| {
                        for p in settings::PRESETS {
                            ui.selectable_value(&mut self.s_preset, (*p).to_string(), *p);
                        }
                    });
                ui.checkbox(&mut self.s_gen_report, "Generar reporte de compresión");
                ui.add_space(8.0);
                ui.label(egui::RichText::new("Apariencia").strong());
                egui::ComboBox::from_label("Color de acento")
                    .selected_text(settings::accent_label(&self.s_accent))
                    .show_ui(ui, |ui| {
                        for (k, label) in settings::ACCENTS {
                            ui.selectable_value(&mut self.s_accent, (*k).to_string(), *label);
                        }
                    });
                ui.add_space(8.0);
                if ui
                    .button(egui::RichText::new("Borrar configuración guardada").color(red))
                    .clicked()
                {
                    clear = true;
                }
                ui.horizontal(|ui| {
                    if ui.button("Guardar").clicked() {
                        save = true;
                    }
                    if ui.button("Cancelar").clicked() {
                        cancel = true;
                    }
                });
            });

        if clear {
            self.s_ffmpeg_path = "ffmpeg".to_string();
            self.s_crf = 25;
            self.s_preset = "medium".to_string();
            self.s_gen_report = true;
            self.s_accent = "amber400".to_string();
            save = true;
        }
        if save {
            self.settings.ffmpeg_path = self.s_ffmpeg_path.trim().to_string();
            if self.settings.ffmpeg_path.is_empty() {
                self.settings.ffmpeg_path = "ffmpeg".to_string();
            }
            self.settings.crf = self.s_crf.clamp(18, 30);
            self.settings.preset = self.s_preset.clone();
            self.settings.gen_report = self.s_gen_report;
            self.settings.accent = self.s_accent.clone();
            self.settings.sanitize();
            settings::save(&self.settings);
            open = false;
        }
        if cancel {
            open = false;
        }
        self.show_settings = open;
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.center_window_once(ctx);
        self.drain_events(ctx);
        self.apply_theme(ctx);
        self.toast_expire();

        egui::TopBottomPanel::top("tabs")
            .frame(egui::Frame::none().inner_margin(egui::Margin::symmetric(10.0, 6.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().button_padding = egui::vec2(8.0, 6.0);
                    if ui
                        .selectable_label(self.tab == Tab::Combine, egui::RichText::new("Combine").size(13.0))
                        .clicked()
                    {
                        self.tab = Tab::Combine;
                    }
                    if ui
                        .selectable_label(self.tab == Tab::Compress, egui::RichText::new("Compress").size(13.0))
                        .clicked()
                    {
                        self.tab = Tab::Compress;
                    }
                    if ui
                        .selectable_label(self.tab == Tab::Cut, egui::RichText::new("Cut").size(13.0))
                        .clicked()
                    {
                        self.tab = Tab::Cut;
                    }
                    if ui
                        .selectable_label(self.tab == Tab::MultiCut, egui::RichText::new("Multi-Cut").size(13.0))
                        .clicked()
                    {
                        self.tab = Tab::MultiCut;
                    }
                    if ui
                        .selectable_label(self.tab == Tab::Rename, egui::RichText::new("Rename").size(13.0))
                        .clicked()
                    {
                        self.tab = Tab::Rename;
                    }
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::none().inner_margin(egui::Margin::same(18.0)))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.ui_tab(ui);
                });
            });

        egui::TopBottomPanel::bottom("status")
            .frame(
                egui::Frame::none()
                    .fill(egui::Color32::from_rgb(20, 21, 26))
                    .inner_margin(egui::Margin::symmetric(16.0, 8.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if self.busy {
                        ui.add(egui::Spinner::new().size(14.0));
                        ui.add_space(6.0);
                    }
                    if let Some(p) = self.progress {
                        ui.add(egui::ProgressBar::new(p).desired_width(110.0));
                        ui.add_space(6.0);
                    }
                    if !self.status.is_empty() {
                        let txt = self.status.clone();
                        ui.label(egui::RichText::new(txt).color(self.gray()));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("⚙").on_hover_text("Ajustes").clicked() {
                            self.open_settings();
                        }
                    });
                });
            });

        if self.show_settings {
            self.settings_window(ctx);
        }

        if let Some(err) = self.error.clone() {
            let mut open = true;
            egui::Window::new("Error")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .default_size([430.0, 230.0])
                .show(ctx, |ui| {
                    ui.label(egui::RichText::new("Error").color(self.red()).strong());
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.monospace(err.as_str());
                    });
                });
            if !open {
                self.error = None;
            }
        }

        self.persist_selections();

        if let Some((msg, is_err)) = self.toast.clone() {
            let color = if is_err { self.red() } else { self.accent() };
            egui::Window::new("toast")
                .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -10.0])
                .title_bar(false)
                .resizable(false)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label(egui::RichText::new(msg).color(color));
                });
        }
    }
}

fn random_name(len: usize) -> String {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9E37_79B9_7F4A_7C15);
    seed ^= (std::process::id() as u64) << 32;
    let mut out = String::with_capacity(len);
    for _ in 0..len {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let idx = ((seed >> 33) as usize) % ALPHABET.len();
        out.push(ALPHABET[idx] as char);
    }
    out
}

fn path_str(p: &Option<PathBuf>) -> String {
    p.as_ref().map(|x| x.to_string_lossy().to_string()).unwrap_or_default()
}

fn existing_path(s: &str) -> Option<PathBuf> {
    if s.is_empty() {
        None
    } else {
        let p = PathBuf::from(s);
        if p.exists() { Some(p) } else { None }
    }
}

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "plex".to_owned(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/IBMPlexSans-Regular.ttf")).into(),
    );
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "plex".to_owned());
    ctx.set_fonts(fonts);
}

fn load_icon() -> egui::IconData {
    let bytes = include_bytes!("../assets/icon.png");
    if let Ok(img) = image::load_from_memory(bytes) {
        let img = img.to_rgba8();
        let (w, h) = img.dimensions();
        egui::IconData {
            rgba: img.into_raw(),
            width: w,
            height: h,
        }
    } else {
        egui::IconData {
            rgba: vec![0, 0, 0, 0],
            width: 1,
            height: 1,
        }
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([440.0, 550.0])
            .with_min_inner_size([400.0, 500.0])
            .with_icon(load_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "Conc",
        options,
        Box::new(|cc| {
            setup_fonts(&cc.egui_ctx);
            Ok(Box::new(App::default()))
        }),
    )
}
