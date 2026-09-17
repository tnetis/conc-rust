use crate::ffmpeg;
use crate::time;
use std::collections::HashSet;
use std::io::Read;
use std::path::Path;
use std::sync::mpsc::Sender;
use std::time::Instant;

/// Eventos que un hilo de trabajo envía a la UI.
pub enum UiEvent {
    Status(String),
    Progress(f32),
    Toast(String),
    Error(String),
    Finished,
}

/// Ruta que no exista en disco ni esté reservada por otro trabajo pendiente.
pub fn unique_path(folder: &str, filename: &str, reserved: &mut HashSet<String>) -> String {
    let mut candidate = Path::new(folder).join(filename);
    let stem = Path::new(filename)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = Path::new(filename)
        .extension()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let mut i = 1;
    while candidate.exists() || reserved.contains(&candidate.to_string_lossy().to_string()) {
        let name = if ext.is_empty() {
            format!("{stem}_{i}")
        } else {
            format!("{stem}_{i}.{ext}")
        };
        candidate = Path::new(folder).join(name);
        i += 1;
    }
    let s = candidate.to_string_lossy().to_string();
    reserved.insert(s.clone());
    s
}

/// Ejecuta un comando ffmpeg, reportando progreso por el canal.
pub fn run_one(
    tx: &Sender<UiEvent>,
    cmd: &[String],
    prefix: &str,
    counter: &str,
    total_seconds: Option<f64>,
) -> Result<(), String> {
    let initial = if counter.is_empty() {
        format!("{prefix}...")
    } else {
        format!("{prefix} {counter}...")
    };
    let _ = tx.send(UiEvent::Status(initial));

    let mut child = ffmpeg::spawn_ffmpeg(cmd).map_err(|e| {
        format!(
            "No se pudo ejecutar '{}': {e}",
            cmd.first().map(|s| s.as_str()).unwrap_or("ffmpeg")
        )
    })?;

    let mut full_log: Vec<String> = Vec::new();
    let mut last_update = Instant::now();

    if let Some(mut stderr) = child.stderr.take() {
        let mut pending: Vec<u8> = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            let n = match stderr.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(_) => break,
            };
            pending.extend_from_slice(&buf[..n]);
            let mut i = 0usize;
            while i < pending.len() {
                if pending[i] == b'\r' || pending[i] == b'\n' {
                    let line = String::from_utf8_lossy(&pending[..i]).to_string();
                    pending.drain(..=i);
                    if !line.is_empty() {
                        handle_line(tx, &line, prefix, counter, total_seconds, &mut full_log, &mut last_update);
                    }
                    i = 0;
                } else {
                    i += 1;
                }
            }
        }
        if !pending.is_empty() {
            let line = String::from_utf8_lossy(&pending).to_string();
            handle_line(tx, &line, prefix, counter, total_seconds, &mut full_log, &mut last_update);
        }
    }

    let status = child.wait().map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        let start = full_log.len().saturating_sub(60);
        let tail = full_log[start..].join("\n");
        if tail.is_empty() {
            Err(format!("FFmpeg terminó con código {:?}", status.code()))
        } else {
            Err(tail)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_line(
    tx: &Sender<UiEvent>,
    line: &str,
    prefix: &str,
    counter: &str,
    total_seconds: Option<f64>,
    full_log: &mut Vec<String>,
    last_update: &mut Instant,
) {
    full_log.push(line.to_string());
    let lower = line.to_lowercase();
    if lower.contains("time=") && (lower.contains("frame=") || lower.contains("size=")) {
        if last_update.elapsed().as_secs_f32() < 0.3 {
            return;
        }
        if let Some(cur) = ffmpeg::parse_progress_time(&lower) {
            let base = if counter.is_empty() {
                format!("{prefix}: {}", time::fmt_seconds(cur))
            } else {
                format!("{prefix} {counter}: {}", time::fmt_seconds(cur))
            };
            let mut txt = base;
            if let Some(t) = total_seconds {
                if t > 0.0 {
                    let pct = (cur / t * 100.0).min(99.9);
                    txt.push_str(&format!(" ({pct:.0}%)"));
                    let _ = tx.send(UiEvent::Progress((pct / 100.0) as f32));
                }
            }
            let _ = tx.send(UiEvent::Status(txt));
            *last_update = Instant::now();
        }
    }
}

fn now_string() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn combine(
    tx: &Sender<UiEvent>,
    ffmpeg: &str,
    ffprobe: &str,
    visual: &str,
    audio: &str,
    is_video: bool,
) {
    let dur = match ffmpeg::get_duration(ffprobe, audio) {
        Some(d) if d > 0.0 => d,
        _ => {
            let _ = tx.send(UiEvent::Error(
                "No se pudo leer la duración del audio seleccionado.".to_string(),
            ));
            return;
        }
    };
    let folder = Path::new(audio)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string());
    let stem = Path::new(audio)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let out_name = format!("{stem}.mp4");
    let mut reserved = HashSet::new();
    let out = unique_path(&folder, &out_name, &mut reserved);
    let cmd = ffmpeg::build_combine_cmd(ffmpeg, visual, audio, &out, is_video, dur);
    let name = Path::new(&out)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| out.clone());
    match run_one(tx, &cmd, "Combining Media", "", Some(dur)) {
        Ok(()) => {
            let _ = tx.send(UiEvent::Toast(format!("Video created: {name}")));
        }
        Err(e) => {
            let _ = tx.send(UiEvent::Error(format!("Error FFmpeg: Combining Media\n{e}")));
        }
    }
}

pub fn compress(
    tx: &Sender<UiEvent>,
    ffmpeg: &str,
    ffprobe: &str,
    folder: &str,
    crf: i32,
    preset: &str,
    gen_report: bool,
) {
    let files = crate::multicut::list_videos_to_compress(folder);
    if files.is_empty() {
        let _ = tx.send(UiEvent::Toast(
            "No new videos found (already compressed?)".to_string(),
        ));
        return;
    }
    let total = files.len();
    let mut report = vec![
        format!("REPORTE DE COMPRESIÓN - {}", now_string()),
        format!("CRF: {crf} | Preset: {preset}"),
        "-".repeat(50),
    ];
    let mut reserved = HashSet::new();
    for (i, filename) in files.iter().enumerate() {
        let in_path = format!("{folder}/{filename}");
        let stem = Path::new(filename)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let out_path = unique_path(folder, &format!("r{stem}.mp4"), &mut reserved);
        let cmd = ffmpeg::build_compress_cmd(ffmpeg, &in_path, &out_path, crf, preset);
        let counter = format!("({}/{total})", i + 1);
        let dur = ffmpeg::get_duration(ffprobe, &in_path);
        match run_one(tx, &cmd, "Compressing", &counter, dur) {
            Ok(()) => {
                let _ = tx.send(UiEvent::Toast(format!("Compressed: {filename}")));
            }
            Err(e) => {
                let _ = tx.send(UiEvent::Error(format!(
                    "Error FFmpeg: Compressing {filename}\n{e}"
                )));
            }
        }
        if gen_report {
            let dur_orig = ffmpeg::get_duration(ffprobe, &in_path);
            let dur_comp = ffmpeg::get_duration(ffprobe, &out_path);
            let size_orig = std::fs::metadata(&in_path).ok().map(|m| m.len());
            let size_comp = std::fs::metadata(&out_path).ok().map(|m| m.len());
            let status = match (dur_orig, dur_comp) {
                (Some(a), Some(b)) if (a - b).abs() <= 0.6 => "OK".to_string(),
                (Some(a), Some(b)) => format!("ALERTA (Dif: {:.2}s)", (a - b).abs()),
                _ => "ERROR".to_string(),
            };
            let orig_str = dur_orig
                .map(|d| format!("{d:.1}s"))
                .unwrap_or_else(|| "N/A".to_string());
            let comp_str = dur_comp
                .map(|d| format!("{d:.1}s"))
                .unwrap_or_else(|| "N/A".to_string());
            let size_line = match (size_orig, size_comp) {
                (Some(o), Some(c)) => {
                    let omb = o as f64 / (1024.0 * 1024.0);
                    let cmb = c as f64 / (1024.0 * 1024.0);
                    let pct = if o > 0 {
                        (1.0 - c as f64 / o as f64) * 100.0
                    } else {
                        0.0
                    };
                    let word = if pct >= 0.0 { "Reducción" } else { "Aumento" };
                    format!("   TAMAÑO: {omb:.2} MB -> {cmb:.2} MB ({word}: {:.1}%)", pct.abs())
                }
                _ => "   TAMAÑO: N/A".to_string(),
            };
            report.push(format!(
                "ARCHIVO: {filename}\n   RESULTADO: {status} (Orig: {orig_str} | Comp: {comp_str})\n{size_line}\n"
            ));
        }
    }
    if gen_report {
        let path = format!("{folder}/reporte_compresion.txt");
        match std::fs::write(&path, report.join("\n")) {
            Ok(()) => {
                let _ = tx.send(UiEvent::Toast(
                    "All compressions completed! Report generated.".to_string(),
                ));
            }
            Err(e) => {
                let _ = tx.send(UiEvent::Error(format!("No se pudo escribir el reporte\n{e}")));
            }
        }
    } else {
        let _ = tx.send(UiEvent::Toast("All compressions completed!".to_string()));
    }
}

pub fn cut(
    tx: &Sender<UiEvent>,
    ffmpeg: &str,
    src: &str,
    start: f64,
    end: f64,
    crf: i32,
    preset: &str,
) {
    let p = Path::new(src);
    let parent = p
        .parent()
        .map(|x| x.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string());
    let stem = p
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = p
        .extension()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let out_name = if ext.is_empty() {
        format!("rf_{stem}")
    } else {
        format!("rf_{stem}.{ext}")
    };
    let mut reserved = HashSet::new();
    let out = unique_path(&parent, &out_name, &mut reserved);
    let cmd = ffmpeg::build_cut_cmd(ffmpeg, src, &out, Some(start), Some(end), None, crf, preset);
    let name = Path::new(&out)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| out.clone());
    match run_one(tx, &cmd, "Video Cutting", "", Some(end - start)) {
        Ok(()) => {
            let _ = tx.send(UiEvent::Toast(format!("Video cut: {name}")));
        }
        Err(e) => {
            let _ = tx.send(UiEvent::Error(format!("Error FFmpeg: Video Cutting\n{e}")));
        }
    }
}

pub fn multicut(
    tx: &Sender<UiEvent>,
    ffmpeg: &str,
    folder: &str,
    txt: &str,
    crf: i32,
    preset: &str,
) {
    let (mut all_lines, invalid) = crate::multicut::parse_multi_cut_lines(txt);
    if !invalid.is_empty() {
        let _ = tx.send(UiEvent::Toast(format!(
            "Tiempos inválidos (fin <= inicio) en: {}",
            invalid.join(", ")
        )));
    }
    if !all_lines.iter().any(|l| l.has_action) {
        let _ = tx.send(UiEvent::Toast(
            "No se reconocieron instrucciones válidas.".to_string(),
        ));
        return;
    }

    let videos: Vec<String> = match std::fs::read_dir(folder) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| {
                let lower = n.to_lowercase();
                let is_video = crate::ffmpeg::VIDEO_EXTS.iter().any(|e| lower.ends_with(e));
                let is_file = Path::new(&format!("{folder}/{n}")).is_file();
                is_video && is_file && !lower.starts_with("c_")
            })
            .collect(),
        Err(_) => Vec::new(),
    };

    struct Job {
        cmd: Vec<String>,
        success: String,
        expected: Option<f64>,
    }
    let mut jobs: Vec<Job> = Vec::new();
    let mut reserved = HashSet::new();

    // Emparejamiento por últimos 4 dígitos del último bloque numérico del nombre.
    for video in &videos {
        let name_without_ext = Path::new(video)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let file_id = match crate::multicut::last4_digits(&name_without_ext) {
            Some(s) => s,
            None => continue,
        };
        let file_id_clean = file_id.trim_start_matches('0');
        let file_id_clean = if file_id_clean.is_empty() { "0" } else { file_id_clean };

        for line in all_lines.iter_mut() {
            if !line.has_action || line.matched {
                continue;
            }
            let job_id_clean = line.id.trim_start_matches('0');
            let job_id_clean = if job_id_clean.is_empty() { "0" } else { job_id_clean };
            if line.id == file_id || job_id_clean == file_id_clean {
                line.matched = true;
                let src = format!("{folder}/{video}");
                let out = unique_path(folder, &format!("c_{video}"), &mut reserved);
                let start = line.start.as_ref().and_then(|s| time::time_to_seconds(s));
                let end = line.end.as_ref().and_then(|e| time::time_to_seconds(e));
                let rot = line.rotation.as_deref();
                let cmd = ffmpeg::build_cut_cmd(ffmpeg, &src, &out, start, end, rot, crf, preset);
                let expected = end.map(|e| e - start.unwrap_or(0.0));
                let name = Path::new(&out)
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| out.clone());
                jobs.push(Job {
                    cmd,
                    success: format!("Procesado: {name}"),
                    expected,
                });
            }
        }
    }

    // Modo secuencial: solo si no hubo coincidencia por ID. Solo líneas con acción.
    if jobs.is_empty() {
        let mut videos_sorted = videos.clone();
        videos_sorted.sort();
        let mut idx = 0usize;
        for line in all_lines.iter_mut() {
            if !line.has_action {
                continue;
            }
            if idx >= videos_sorted.len() {
                break;
            }
            line.matched = true;
            let video = &videos_sorted[idx];
            idx += 1;
            let src = format!("{folder}/{video}");
            let out = unique_path(folder, &format!("c_{video}"), &mut reserved);
            let start = line.start.as_ref().and_then(|s| time::time_to_seconds(s));
            let end = line.end.as_ref().and_then(|e| time::time_to_seconds(e));
            let rot = line.rotation.as_deref();
            let cmd = ffmpeg::build_cut_cmd(ffmpeg, &src, &out, start, end, rot, crf, preset);
            let expected = end.map(|e| e - start.unwrap_or(0.0));
            let name = Path::new(&out)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| out.clone());
            jobs.push(Job {
                cmd,
                success: format!("Procesado: {name}"),
                expected,
            });
        }
    }

    let unmatched: Vec<String> = all_lines
        .iter()
        .filter(|l| l.has_action && !l.matched)
        .map(|l| l.id.clone())
        .collect();

    if jobs.is_empty() {
        let _ = tx.send(UiEvent::Error(format!(
            "No se encontraron videos para procesar. IDs faltantes: {}",
            unmatched.join(", ")
        )));
        return;
    }
    if !unmatched.is_empty() {
        let _ = tx.send(UiEvent::Toast(format!(
            "Aviso: No se procesaron los IDs: {}",
            unmatched.join(", ")
        )));
    }

    let total = jobs.len();
    for (i, job) in jobs.iter().enumerate() {
        let counter = format!("({}/{total})", i + 1);
        match run_one(tx, &job.cmd, "Multi-Cutting", &counter, job.expected) {
            Ok(()) => {
                let _ = tx.send(UiEvent::Toast(job.success.clone()));
            }
            Err(e) => {
                let _ = tx.send(UiEvent::Error(format!("Error FFmpeg: Multi-Cutting\n{e}")));
            }
        }
    }
}
