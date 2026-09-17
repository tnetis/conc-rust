use regex::Regex;
use std::collections::HashSet;
use std::path::Path;
use std::sync::OnceLock;

#[derive(Debug, Clone)]
pub struct Instruction {
    pub id: String,
    pub start: Option<String>,
    pub end: Option<String>,
    pub rotation: Option<String>,
    pub has_action: bool,
    pub matched: bool,
}

fn multicut_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)(\d+)(?:\s+(\d{1,2}:\d{2}(?::\d{2})?(?:\.\d+)?)\s*-\s*(\d{1,2}:\d{2}(?::\d{2})?(?:\.\d+)?))?(?:\s*(r\s*-?\s*90|r\s*180))?",
        )
        .unwrap()
    })
}

/// Parsea las instrucciones multi-cut (mismo patrón que la app Python).
/// Devuelve (instrucciones, ids con tiempo inválido fin<=inicio).
pub fn parse_multi_cut_lines(txt: &str) -> (Vec<Instruction>, Vec<String>) {
    let re = multicut_re();
    let mut jobs = Vec::new();
    let mut invalid = Vec::new();

    for raw in txt.lines() {
        let mut line = raw.trim();
        if line.is_empty() {
            continue;
        }
        // quitar prefijo de log "[12:34:56] ..."
        if let Some(pos) = line.rfind("] ") {
            line = &line[pos + 2..];
        }
        // quitar prefijo "Cut 2: ..."
        if let Some(pos) = line.find(": ") {
            line = &line[pos + 2..];
        }

        let caps = match re.captures(line) {
            Some(c) => c,
            None => continue,
        };
        let id = match caps.get(1) {
            Some(m) => m.as_str().to_string(),
            None => continue,
        };
        if id.is_empty() {
            continue;
        }
        let start = caps.get(2).map(|m| m.as_str().to_string());
        let end = caps.get(3).map(|m| m.as_str().to_string());
        let rotation = caps.get(4).map(|m| {
            let s: String = m.as_str().chars().filter(|c| !c.is_whitespace()).collect();
            s.to_lowercase()
        });

        if let (Some(s), Some(e)) = (&start, &end) {
            if let (Some(ss), Some(ee)) =
                (crate::time::time_to_seconds(s), crate::time::time_to_seconds(e))
            {
                if ee <= ss {
                    invalid.push(id.clone());
                    continue;
                }
            }
        }

        let has_action = start.is_some() || end.is_some() || rotation.is_some();
        jobs.push(Instruction {
            id,
            start,
            end,
            rotation,
            has_action,
            matched: false,
        });
    }

    (jobs, invalid)
}

/// Últimos 4 dígitos del último bloque numérico del nombre (sin extensión).
pub fn last4_digits(name_without_ext: &str) -> Option<String> {
    let digit_re = Regex::new(r"\d+").unwrap();
    let last = digit_re.find_iter(name_without_ext).map(|m| m.as_str()).last()?;
    let n = last.len();
    let start = n.saturating_sub(4);
    Some(last[start..].to_string())
}

/// Devuelve los vídeos pendientes de comprimir (lógica de la app Python).
pub fn list_videos_to_compress(folder: &str) -> Vec<String> {
    let mut files: Vec<String> = match std::fs::read_dir(folder) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|name| Path::new(&format!("{folder}/{name}")).is_file())
            .collect(),
        Err(_) => Vec::new(),
    };
    files.sort();

    let stems: HashSet<String> = files
        .iter()
        .map(|f| {
            Path::new(f)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase()
        })
        .collect();

    let mut pending = Vec::new();
    for f in &files {
        let lower = f.to_lowercase();
        let is_video = crate::ffmpeg::VIDEO_EXTS.iter().any(|e| lower.ends_with(e));
        if !is_video {
            continue;
        }
        let stem = Path::new(f)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let stem_l = stem.to_lowercase();

        // es una salida comprimida previa (rNombre.mp4) si existe Nombre.*
        if stem_l.starts_with('r') && stem_l.len() > 1 && stems.contains(&stem_l[1..]) {
            continue;
        }
        // ya tiene su versión comprimida
        let compressed = format!("r{stem_l}");
        if stems.contains(compressed.as_str()) {
            continue;
        }
        pending.push(f.clone());
    }
    pending
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_lines() {
        let (jobs, invalid) = parse_multi_cut_lines("4632 00:04 - 00:35\n2739 r90\n1234 00:10 - 00:05\n");
        assert_eq!(jobs.len(), 2);
        assert_eq!(invalid, vec!["1234".to_string()]);

        assert_eq!(jobs[0].id, "4632");
        assert_eq!(jobs[0].start.as_deref(), Some("00:04"));
        assert_eq!(jobs[0].end.as_deref(), Some("00:35"));
        assert_eq!(jobs[0].rotation, None);
        assert!(jobs[0].has_action);

        assert_eq!(jobs[1].id, "2739");
        assert_eq!(jobs[1].rotation.as_deref(), Some("r90"));
        assert!(jobs[1].has_action);
    }

    #[test]
    fn parse_r180_variant() {
        let (jobs, _) = parse_multi_cut_lines("5000 r-90\n6000 r 180\n");
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].rotation.as_deref(), Some("r-90"));
        assert_eq!(jobs[1].rotation.as_deref(), Some("r180"));
    }

    #[test]
    fn last4() {
        assert_eq!(last4_digits("video_12345678"), Some("5678".to_string()));
        assert_eq!(last4_digits("abc12"), Some("12".to_string()));
        assert_eq!(last4_digits("00000042"), Some("42".to_string()));
        assert_eq!(last4_digits("no_digits"), None);
    }

    #[test]
    fn compress_list() {
        let dir = std::env::temp_dir().join(format!(
            "concat_test_{}_{}",
            std::process::id(),
            chrono::Utc::now().timestamp_millis()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let files = [
            "video.mp4",
            "rvideo.mp4",
            "recording.mp4",
            "clip.mov",
            "rclip.mov",
            "audio.mp3",
            "video.srt",
        ];
        for f in files {
            std::fs::write(dir.join(f), b"x").unwrap();
        }
        let list = list_videos_to_compress(dir.to_str().unwrap());
        assert_eq!(list, vec!["recording.mp4".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
