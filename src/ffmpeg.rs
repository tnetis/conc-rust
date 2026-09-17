use regex::Regex;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::OnceLock;

pub const VIDEO_EXTS: &[&str] = &["mp4", "mov", "mkv", "avi", "webm", "m4v"];
pub const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "webp"];
pub const AUDIO_EXTS: &[&str] = &["mp3", "wav", "m4a", "opus", "flac", "aac", "ogg"];

pub const ROTATION_FILTERS: &[(&str, &str)] = &[
    ("r90", "transpose=1"),
    ("r-90", "transpose=2"),
    ("r180", "transpose=1,transpose=1"),
];

pub fn rotation_filter(rot: &str) -> Option<&'static str> {
    ROTATION_FILTERS
        .iter()
        .find(|(k, _)| *k == rot)
        .map(|(_, v)| *v)
}

pub fn is_video_ext(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            let el = e.to_lowercase();
            VIDEO_EXTS.iter().any(|v| *v == el.as_str())
        })
        .unwrap_or(false)
}

pub fn is_audio_ext(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            let el = e.to_lowercase();
            AUDIO_EXTS.iter().any(|v| *v == el.as_str())
        })
        .unwrap_or(false)
}

pub fn is_image_ext(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            let el = e.to_lowercase();
            IMAGE_EXTS.iter().any(|v| *v == el.as_str())
        })
        .unwrap_or(false)
}

/// Ruta de ffprobe: si hay un ffprobe junto al ffmpeg configurado, se usa ese.
pub fn get_ffprobe_path(ffmpeg_path: &str) -> String {
    let ext = if cfg!(windows) { ".exe" } else { "" };
    if let Some(dir) = Path::new(ffmpeg_path).parent() {
        if !dir.as_os_str().is_empty() {
            let candidate = dir.join(format!("ffprobe{ext}"));
            if candidate.exists() {
                return candidate.to_string_lossy().to_string();
            }
        }
    }
    "ffprobe".to_string()
}

pub fn ffmpeg_available(ffmpeg: &str) -> bool {
    Command::new(ffmpeg)
        .arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Corte: usa -ss (entrada) + -t (duración). SIN -avoid_negative_ts (bug corregido).
pub fn build_cut_cmd(
    ffmpeg: &str,
    src: &str,
    out: &str,
    start: Option<f64>,
    end: Option<f64>,
    rotation: Option<&str>,
    crf: i32,
    preset: &str,
) -> Vec<String> {
    let mut cmd: Vec<String> = vec![ffmpeg.to_string(), "-y".into(), "-hide_banner".into()];
    if let Some(s) = start {
        cmd.push("-ss".into());
        cmd.push(format!("{s:.3}"));
    }
    cmd.push("-i".into());
    cmd.push(src.to_string());
    if let Some(e) = end {
        let dur = e - start.unwrap_or(0.0);
        cmd.push("-t".into());
        cmd.push(format!("{dur:.3}"));
    }
    if let Some(rot) = rotation {
        if let Some(vf) = rotation_filter(rot) {
            cmd.push("-vf".into());
            cmd.push(vf.to_string());
            cmd.push("-c:v".into());
            cmd.push("libx264".into());
            cmd.push("-crf".into());
            cmd.push(crf.to_string());
            cmd.push("-preset".into());
            cmd.push(preset.to_string());
            cmd.push("-pix_fmt".into());
            cmd.push("yuv420p".into());
            cmd.push("-c:a".into());
            cmd.push("copy".into());
        }
    } else {
        cmd.push("-c".into());
        cmd.push("copy".into());
    }
    cmd.push(out.to_string());
    cmd
}

pub fn build_compress_cmd(
    ffmpeg: &str,
    input: &str,
    output: &str,
    crf: i32,
    preset: &str,
) -> Vec<String> {
    vec![
        ffmpeg.to_string(),
        "-y".into(),
        "-hide_banner".into(),
        "-i".into(),
        input.to_string(),
        "-c:v".into(),
        "libx264".into(),
        "-crf".into(),
        crf.to_string(),
        "-preset".into(),
        preset.to_string(),
        "-c:a".into(),
        "aac".into(),
        "-fps_mode".into(),
        "passthrough".into(),
        "-movflags".into(),
        "+faststart".into(),
        output.to_string(),
    ]
}

pub fn build_combine_cmd(
    ffmpeg: &str,
    visual: &str,
    audio: &str,
    out: &str,
    is_video: bool,
    audio_duration: f64,
) -> Vec<String> {
    let mut cmd: Vec<String> = vec![ffmpeg.to_string(), "-y".into(), "-hide_banner".into()];
    if is_video {
        cmd.push("-stream_loop".into());
        cmd.push("-1".into());
    } else {
        cmd.push("-loop".into());
        cmd.push("1".into());
        cmd.push("-framerate".into());
        cmd.push("1".into());
    }
    cmd.push("-i".into());
    cmd.push(visual.to_string());
    cmd.push("-i".into());
    cmd.push(audio.to_string());
    cmd.push("-map".into());
    cmd.push("0:v:0".into());
    cmd.push("-map".into());
    cmd.push("1:a:0".into());
    if is_video {
        cmd.push("-c:v".into());
        cmd.push("libx264".into());
        cmd.push("-preset".into());
        cmd.push("veryfast".into());
        cmd.push("-crf".into());
        cmd.push("26".into());
    } else {
        cmd.push("-c:v".into());
        cmd.push("libx264".into());
        cmd.push("-tune".into());
        cmd.push("stillimage".into());
        cmd.push("-preset".into());
        cmd.push("veryfast".into());
        cmd.push("-crf".into());
        cmd.push("26".into());
        cmd.push("-r".into());
        cmd.push("1".into());
    }
    cmd.push("-vf".into());
    cmd.push(
        "scale=1920:1080:force_original_aspect_ratio=decrease,pad=1920:1080:(ow-iw)/2:(oh-ih)/2:color=black"
            .into(),
    );
    cmd.push("-pix_fmt".into());
    cmd.push("yuv420p".into());
    cmd.push("-c:a".into());
    cmd.push("aac".into());
    cmd.push("-shortest".into());
    cmd.push("-t".into());
    cmd.push(format!("{audio_duration:.3}"));
    cmd.push("-movflags".into());
    cmd.push("+faststart".into());
    cmd.push(out.to_string());
    cmd
}

/// Duración en segundos usando ffprobe (JSON).
pub fn get_duration(ffprobe: &str, path: &str) -> Option<f64> {
    if !Path::new(path).exists() {
        return None;
    }
    let out = Command::new(ffprobe)
        .args(["-v", "quiet", "-print_format", "json", "-show_format", path])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    v["format"]["duration"].as_str()?.parse::<f64>().ok()
}

fn progress_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"time=(\d+):(\d+):(\d+(?:\.\d+)?)").unwrap())
}

/// Parea "time=HH:MM:SS.ss" de la salida de ffmpeg a segundos.
pub fn parse_progress_time(line_lower: &str) -> Option<f64> {
    let caps = progress_re().captures(line_lower)?;
    let h: f64 = caps.get(1)?.as_str().parse().ok()?;
    let m: f64 = caps.get(2)?.as_str().parse().ok()?;
    let s: f64 = caps.get(3)?.as_str().parse().ok()?;
    Some(h * 3600.0 + m * 60.0 + s)
}

#[cfg(windows)]
pub fn spawn_ffmpeg(cmd: &[String]) -> std::io::Result<std::process::Child> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    Command::new(cmd.first().map(|s| s.as_str()).unwrap_or("ffmpeg"))
        .args(&cmd[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
}

#[cfg(not(windows))]
pub fn spawn_ffmpeg(cmd: &[String]) -> std::io::Result<std::process::Child> {
    Command::new(cmd.first().map(|s| s.as_str()).unwrap_or("ffmpeg"))
        .args(&cmd[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cut_cmd_no_negative_ts_flag() {
        let cmd = build_cut_cmd("ffmpeg", "a.mp4", "out.mp4", Some(1.0), Some(3.0), None, 25, "medium");
        assert!(!cmd.iter().any(|a| a == "-avoid_negative_ts"));
        assert!(cmd.iter().any(|a| a == "-ss"));
        assert!(cmd.iter().any(|a| a == "-t"));
        assert!(cmd.iter().any(|a| a == "1.000"));
        assert!(cmd.iter().any(|a| a == "2.000"));
        assert!(cmd.iter().any(|a| a == "-c"));
        assert!(cmd.iter().any(|a| a == "copy"));
    }

    #[test]
    fn cut_cmd_with_rotation() {
        let cmd = build_cut_cmd("ffmpeg", "a.mp4", "out.mp4", Some(0.0), Some(2.0), Some("r90"), 25, "medium");
        assert!(cmd.iter().any(|a| a == "transpose=1"));
        assert!(cmd.iter().any(|a| a == "libx264"));
        assert!(cmd.iter().any(|a| a == "-vf"));
    }

    #[test]
    fn rotation_mappings() {
        assert_eq!(rotation_filter("r90"), Some("transpose=1"));
        assert_eq!(rotation_filter("r-90"), Some("transpose=2"));
        assert_eq!(rotation_filter("r180"), Some("transpose=1,transpose=1"));
        assert_eq!(rotation_filter("nope"), None);
    }

    #[test]
    fn combine_image_vs_video() {
        let img = build_combine_cmd("ffmpeg", "a.png", "s.mp3", "o.mp4", false, 10.0);
        assert!(img.iter().any(|a| a == "-loop"));
        assert!(img.iter().any(|a| a == "stillimage"));

        let vid = build_combine_cmd("ffmpeg", "a.mp4", "s.mp3", "o.mp4", true, 10.0);
        assert!(vid.iter().any(|a| a == "-stream_loop"));
        assert!(!vid.iter().any(|a| a == "stillimage"));
        assert!(vid.iter().any(|a| a == "1920:1080"));
    }

    #[test]
    fn compress_cmd_shape() {
        let cmd = build_compress_cmd("ffmpeg", "in.mp4", "out.mp4", 25, "medium");
        assert!(cmd.iter().any(|a| a == "libx264"));
        assert!(cmd.iter().any(|a| a == "25"));
        assert!(cmd.iter().any(|a| a == "medium"));
        assert!(cmd.iter().any(|a| a == "passthrough"));
    }

    #[test]
    fn progress_parse() {
        let line = "frame= 100 fps= 30 size= 512kB time=00:01:02.50 bitrate= 123.4kbits/s";
        assert_eq!(parse_progress_time(&line.to_lowercase()), Some(62.5));
        assert_eq!(parse_progress_time("no time here"), None);
    }
}
