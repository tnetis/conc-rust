use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const PRESETS: &[&str] = &[
    "ultrafast",
    "superfast",
    "veryfast",
    "faster",
    "fast",
    "medium",
    "slow",
    "slower",
];

pub const ACCENTS: &[(&str, &str)] = &[
    ("amber400", "Ámbar (Original)"),
    ("blue400", "Azul"),
    ("green400", "Verde"),
    ("red400", "Rojo"),
    ("purple400", "Morado"),
    ("pink400", "Rosa"),
    ("teal400", "Verde Azulado"),
    ("orange400", "Naranja"),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub ffmpeg_path: String,
    pub crf: i32,
    pub preset: String,
    pub gen_report: bool,
    pub accent: String,
    pub last_tab: u32,
    pub last_visual: String,
    pub last_audio: String,
    pub last_video_for_cut: String,
    pub last_videos_folder: String,
    pub last_multi_cut_folder: String,
    pub last_rename_folder: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ffmpeg_path: "ffmpeg".to_string(),
            crf: 25,
            preset: "medium".to_string(),
            gen_report: true,
            accent: "amber400".to_string(),
            last_tab: 0,
            last_visual: String::new(),
            last_audio: String::new(),
            last_video_for_cut: String::new(),
            last_videos_folder: String::new(),
            last_multi_cut_folder: String::new(),
            last_rename_folder: String::new(),
        }
    }
}

impl Settings {
    /// Valida los valores cargados (equivalente a la validación de la app Python).
    pub fn sanitize(&mut self) {
        if self.ffmpeg_path.trim().is_empty() {
            self.ffmpeg_path = "ffmpeg".to_string();
        }
        self.crf = self.crf.clamp(18, 30);
        if !PRESETS.contains(&self.preset.as_str()) {
            self.preset = "medium".to_string();
        }
        if !ACCENTS.iter().any(|(k, _)| *k == self.accent.as_str()) {
            self.accent = "amber400".to_string();
        }
    }
}

pub fn accent_label(key: &str) -> String {
    ACCENTS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, label)| label.to_string())
        .unwrap_or_else(|| "Ámbar (Original)".to_string())
}

pub fn accent_color(key: &str) -> eframe::egui::Color32 {
    match key {
        "blue400" => eframe::egui::Color32::from_rgb(66, 165, 245),
        "green400" => eframe::egui::Color32::from_rgb(102, 187, 106),
        "red400" => eframe::egui::Color32::from_rgb(239, 83, 80),
        "purple400" => eframe::egui::Color32::from_rgb(171, 71, 188),
        "pink400" => eframe::egui::Color32::from_rgb(236, 64, 122),
        "teal400" => eframe::egui::Color32::from_rgb(38, 166, 154),
        "orange400" => eframe::egui::Color32::from_rgb(255, 167, 38),
        _ => eframe::egui::Color32::from_rgb(255, 193, 7), // amber400
    }
}

fn config_path() -> PathBuf {
    if let Some(proj) = directories::ProjectDirs::from("com", "conc", "concat") {
        return proj.config_dir().join("settings.json");
    }
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("settings.json")
}

pub fn load() -> Settings {
    let path = config_path();
    if let Ok(data) = std::fs::read_to_string(&path) {
        if let Ok(mut s) = serde_json::from_str::<Settings>(&data) {
            s.sanitize();
            return s;
        }
    }
    Settings::default()
}

pub fn save(s: &Settings) {
    let path = config_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string_pretty(s) {
        let _ = std::fs::write(&path, json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_preset_and_accent() {
        let mut s = Settings {
            preset: "bogus".to_string(),
            accent: "nope".to_string(),
            crf: 99,
            ..Default::default()
        };
        s.sanitize();
        assert_eq!(s.preset, "medium");
        assert_eq!(s.accent, "amber400");
        assert_eq!(s.crf, 30);
    }

    #[test]
    fn sanitize_keeps_valid() {
        let mut s = Settings {
            preset: "veryfast".to_string(),
            accent: "teal400".to_string(),
            crf: 20,
            ..Default::default()
        };
        s.sanitize();
        assert_eq!(s.preset, "veryfast");
        assert_eq!(s.accent, "teal400");
        assert_eq!(s.crf, 20);
    }
}
