use std::{
    collections::HashSet,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, Sender},
    },
    thread,
};

use crate::app::Mode;

pub struct Job {
    pub id: u64,
    pub url: String,
    pub mode: Mode,
    pub quality: String,
    pub audio_id: String,
    pub outdir: PathBuf,
    pub cookies: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AudioFormat {
    pub id: String,
    pub codec: String,
    pub abr: f64,
    pub ext: String,
}

#[derive(Clone, Debug)]
pub enum Msg {
    Started {
        id: u64,
    },
    Title {
        id: u64,
        title: String,
    },
    Progress {
        id: u64,
        pct: f64,
        speed: String,
        eta: String,
    },
    Done {
        id: u64,
        path: String,
    },
    Failed {
        id: u64,
        msg: String,
    },
    Formats {
        seq: u64,
        entries: Vec<AudioFormat>,
    },
    FormatsFailed {
        seq: u64,
        msg: String,
    },
}

type Current = Arc<Mutex<Option<(u64, Child)>>>;
type Cancelled = Arc<Mutex<HashSet<u64>>>;

pub struct Worker {
    tx: Sender<Job>,
    msg_tx: Sender<Msg>,
    rx: Receiver<Msg>,
    current: Current,
    cancelled: Cancelled,
}

impl Worker {
    pub fn new() -> Self {
        let (job_tx, job_rx) = mpsc::channel();
        let (msg_tx, msg_rx) = mpsc::channel();
        let current: Current = Arc::new(Mutex::new(None));
        let cancelled: Cancelled = Arc::new(Mutex::new(HashSet::new()));
        let worker_current = current.clone();
        let worker_cancelled = cancelled.clone();
        let worker_tx = msg_tx.clone();
        thread::spawn(move || worker_loop(job_rx, worker_tx, worker_current, worker_cancelled));
        Self {
            tx: job_tx,
            msg_tx,
            rx: msg_rx,
            current,
            cancelled,
        }
    }

    pub fn enqueue(&self, job: Job) {
        let _ = self.tx.send(job);
    }

    pub fn probe(&self, url: String, cookies: Option<PathBuf>, seq: u64) {
        let tx = self.msg_tx.clone();
        thread::spawn(move || match probe_formats(&url, cookies.as_deref()) {
            Ok(entries) => {
                let _ = tx.send(Msg::Formats { seq, entries });
            }
            Err(msg) => {
                let _ = tx.send(Msg::FormatsFailed { seq, msg });
            }
        });
    }

    pub fn poll(&self) -> Vec<Msg> {
        let mut messages = Vec::new();
        while let Ok(msg) = self.rx.try_recv() {
            messages.push(msg);
        }
        messages
    }

    pub fn cancel(&self, id: u64) {
        self.cancelled.lock().unwrap().insert(id);
        let mut guard = self.current.lock().unwrap();
        if let Some((current_id, child)) = guard.as_mut()
            && *current_id == id
        {
            let _ = child.kill();
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        if let Some((_, child)) = self.current.lock().unwrap().as_mut() {
            let _ = child.kill();
        }
    }
}

fn worker_loop(rx: Receiver<Job>, tx: Sender<Msg>, current: Current, cancelled: Cancelled) {
    while let Ok(job) = rx.recv() {
        if cancelled.lock().unwrap().remove(&job.id) {
            continue;
        }
        let id = job.id;
        if let Err(msg) = run(job, &tx, &current, &cancelled) {
            let _ = tx.send(Msg::Failed { id, msg });
        }
        if let Ok(mut guard) = current.lock() {
            guard.take();
        }
        cancelled.lock().unwrap().remove(&id);
    }
}

fn run(job: Job, tx: &Sender<Msg>, current: &Current, cancelled: &Cancelled) -> Result<(), String> {
    let mut child = Command::new("yt-dlp")
        .args(build_args(&job))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("无法启动 yt-dlp: {err}"))?;

    let stdout = child.stdout.take().ok_or("yt-dlp stdout 不可用")?;
    let stderr = child.stderr.take();
    let errors = Arc::new(Mutex::new(Vec::<String>::new()));
    let stderr_reader = stderr.map(|stderr| {
        let errors = errors.clone();
        thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                errors.lock().unwrap().push(line);
            }
        })
    });

    *current.lock().unwrap() = Some((job.id, child));
    let was_cancelled = cancelled.lock().unwrap().contains(&job.id);
    if was_cancelled && let Some((_, child)) = current.lock().unwrap().as_mut() {
        let _ = child.kill();
    }
    let _ = tx.send(Msg::Started { id: job.id });

    let mut last_title = String::new();
    let mut out_path = None;
    for line in BufReader::new(stdout).lines() {
        let Ok(line) = line else { break };
        if let Some(title) = marker_value(&line, "bili-title:") {
            let title = title.trim().to_string();
            if !title.is_empty() && title != last_title {
                last_title = title.clone();
                let _ = tx.send(Msg::Title { id: job.id, title });
            }
        } else if let Some(payload) = marker_value(&line, "bili-dl:") {
            if let Some((pct, speed, eta)) = parse_progress(payload) {
                let _ = tx.send(Msg::Progress {
                    id: job.id,
                    pct,
                    speed,
                    eta,
                });
            }
        } else if let Some(path) = marker_value(&line, "bili-out:") {
            out_path = Some(path.trim().to_string());
        }
    }

    let mut child = current
        .lock()
        .unwrap()
        .take()
        .map(|(_, child)| child)
        .ok_or("下载进程丢失")?;
    let status = child
        .wait()
        .map_err(|err| format!("等待 yt-dlp 失败: {err}"))?;
    if let Some(handle) = stderr_reader {
        let _ = handle.join();
    }

    if status.success() {
        let path = out_path.unwrap_or_else(|| job.outdir.display().to_string());
        let _ = tx.send(Msg::Done { id: job.id, path });
        Ok(())
    } else {
        let errors = errors.lock().unwrap();
        let msg = errors
            .iter()
            .rev()
            .find(|line| line.contains("ERROR"))
            .or_else(|| errors.last())
            .cloned()
            .unwrap_or_else(|| format!("yt-dlp 退出: {status}"));
        Err(msg)
    }
}

fn probe_formats(url: &str, cookies: Option<&Path>) -> Result<Vec<AudioFormat>, String> {
    let mut command = Command::new("yt-dlp");
    command.args([
        "--no-colors",
        "--no-playlist",
        "--skip-download",
        "--print",
        "%(formats.:.{format_id,acodec,vcodec,abr,ext})j",
    ]);
    if let Some(cookies) = cookies {
        command.arg("--cookies").arg(cookies);
    }
    command.arg(url);
    let output = command
        .output()
        .map_err(|err| format!("无法启动 yt-dlp: {err}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let msg = stderr
            .lines()
            .rev()
            .find(|line| line.contains("ERROR"))
            .or_else(|| stderr.lines().last())
            .unwrap_or("获取音频格式失败")
            .trim()
            .to_string();
        return Err(msg);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json = stdout
        .lines()
        .rev()
        .find(|line| line.trim_start().starts_with('['))
        .ok_or("yt-dlp 未返回格式列表")?;
    parse_audio_formats(json)
}

fn parse_audio_formats(json: &str) -> Result<Vec<AudioFormat>, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|err| format!("解析格式列表失败: {err}"))?;
    let formats = value.as_array().ok_or("格式列表不是数组")?;
    let mut entries = Vec::new();
    for format in formats {
        let vcodec = format["vcodec"].as_str().unwrap_or("none");
        let acodec = format["acodec"].as_str().unwrap_or("none");
        if vcodec != "none" || acodec == "none" {
            continue;
        }
        let id = format["format_id"].as_str().unwrap_or("").to_string();
        if id.is_empty() {
            continue;
        }
        entries.push(AudioFormat {
            id,
            codec: acodec.to_string(),
            abr: format["abr"].as_f64().unwrap_or(0.0),
            ext: format["ext"].as_str().unwrap_or("").to_string(),
        });
    }
    entries.sort_by(|a, b| b.abr.total_cmp(&a.abr));
    Ok(entries)
}

fn build_args(job: &Job) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "--no-colors".into(),
        "--newline".into(),
        "--no-playlist".into(),
        "--no-simulate".into(),
        "--progress".into(),
        "--print".into(),
        "before_dl:bili-title:%(title)s".into(),
        "--progress-template".into(),
        "download:bili-dl:%(progress.downloaded_bytes)s|%(progress.total_bytes)s|%(progress.total_bytes_estimate)s|%(progress.speed)s|%(progress.eta)s|%(progress._percent_str)s".into(),
        "--print".into(),
        "after_move:bili-out:%(filepath)s".into(),
        "-P".into(),
        job.outdir.to_string_lossy().into_owned(),
        "-o".into(),
        "%(title).100s [%(id)s].%(ext)s".into(),
    ];
    if let Some(cookies) = &job.cookies {
        args.push("--cookies".into());
        args.push(cookies.to_string_lossy().into_owned());
    }

    match job.mode {
        Mode::Video => {
            let video = if job.quality == "best" {
                "bv*".to_string()
            } else {
                format!("bv*[height<={}]", job.quality)
            };
            let expression = if !job.audio_id.is_empty() {
                format!("{video}+{}", job.audio_id)
            } else if job.quality == "best" {
                "bv*+ba/b".to_string()
            } else {
                format!("{video}+ba/b[height<={}]/b", job.quality)
            };
            args.push("-f".into());
            args.push(expression);
            args.push("--merge-output-format".into());
            args.push("mp4".into());
        }
        Mode::Audio => {
            let expression = if job.audio_id.is_empty() {
                "ba/b".to_string()
            } else {
                job.audio_id.clone()
            };
            args.push("-f".into());
            args.push(expression);
            args.push("-x".into());
            args.push("--audio-format".into());
            args.push("mp3".into());
            args.push("--audio-quality".into());
            args.push("0".into());
        }
    }

    args.push(job.url.clone());
    args
}

fn parse_progress(payload: &str) -> Option<(f64, String, String)> {
    let parts: Vec<&str> = payload.split('|').collect();
    if parts.len() < 5 {
        return None;
    }
    let downloaded = parse_num(parts[0]);
    let total = parse_num(parts[1]).or_else(|| parse_num(parts[2]));
    let speed = parse_num(parts[3]).map(format_speed).unwrap_or_default();
    let eta = parse_num(parts[4]).map(format_eta).unwrap_or_default();
    let pct = match (downloaded, total) {
        (Some(done), Some(total)) if total > 0.0 => (done / total).clamp(0.0, 1.0),
        _ => parts
            .get(5)
            .and_then(|text| parse_percent(text))
            .unwrap_or(0.0),
    };
    Some((pct, speed, eta))
}

fn parse_num(text: &str) -> Option<f64> {
    text.trim().parse::<f64>().ok()
}

fn marker_value<'a>(line: &'a str, marker: &str) -> Option<&'a str> {
    line.find(marker).map(|idx| &line[idx + marker.len()..])
}

fn parse_percent(text: &str) -> Option<f64> {
    let value = text.trim().trim_end_matches('%').parse::<f64>().ok()?;
    Some((value / 100.0).clamp(0.0, 1.0))
}

fn format_speed(bytes_per_second: f64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    if bytes_per_second >= GIB {
        format!("{:.2} GiB/s", bytes_per_second / GIB)
    } else if bytes_per_second >= MIB {
        format!("{:.1} MiB/s", bytes_per_second / MIB)
    } else if bytes_per_second >= KIB {
        format!("{:.0} KiB/s", bytes_per_second / KIB)
    } else {
        format!("{:.0} B/s", bytes_per_second)
    }
}

fn format_eta(seconds: f64) -> String {
    let seconds = seconds.max(0.0) as u64;
    if seconds >= 3600 {
        format!(
            "{:02}:{:02}:{:02}",
            seconds / 3600,
            seconds % 3600 / 60,
            seconds % 60
        )
    } else {
        format!("{:02}:{:02}", seconds / 60, seconds % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(mode: Mode, quality: &str, audio_id: &str) -> Job {
        Job {
            id: 1,
            url: "https://www.bilibili.com/video/BV1xx".to_string(),
            mode,
            quality: quality.to_string(),
            audio_id: audio_id.to_string(),
            outdir: PathBuf::from("/tmp"),
            cookies: None,
        }
    }

    #[test]
    fn video_args_use_quality_and_audio_id() {
        let args = build_args(&job(Mode::Video, "720", "30280"));
        let format = args
            .windows(2)
            .find(|pair| pair[0] == "-f")
            .map(|pair| pair[1].as_str());
        assert_eq!(format, Some("bv*[height<=720]+30280"));
        assert!(args.iter().any(|arg| arg == "--merge-output-format"));
    }

    #[test]
    fn video_args_auto_audio_and_best_quality() {
        let args = build_args(&job(Mode::Video, "best", ""));
        let format = args
            .windows(2)
            .find(|pair| pair[0] == "-f")
            .map(|pair| pair[1].as_str());
        assert_eq!(format, Some("bv*+ba/b"));
    }

    #[test]
    fn audio_args_use_requested_id() {
        let args = build_args(&job(Mode::Audio, "1080", "30216"));
        let format = args
            .windows(2)
            .find(|pair| pair[0] == "-f")
            .map(|pair| pair[1].as_str());
        assert_eq!(format, Some("30216"));
        assert!(args.iter().any(|arg| arg == "-x"));
    }

    #[test]
    fn parses_audio_formats_only_and_sorts_by_bitrate() {
        let json = r#"[
            {"format_id":"30216","acodec":"mp4a.40.5","vcodec":"none","abr":43.9,"ext":"m4a"},
            {"format_id":"30016","acodec":"none","vcodec":"avc1.64001E","abr":0,"ext":"mp4"},
            {"format_id":"30280","acodec":"mp4a.40.2","vcodec":"none","abr":203.7,"ext":"m4a"}
        ]"#;
        let entries = parse_audio_formats(json).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, "30280");
        assert_eq!(entries[1].id, "30216");
    }

    #[test]
    fn progress_from_byte_counts() {
        let (pct, speed, eta) = parse_progress("524288|1048576|NA|1048576|61| 50.0%").unwrap();
        assert!((pct - 0.5).abs() < 1e-9);
        assert_eq!(speed, "1.0 MiB/s");
        assert_eq!(eta, "01:01");
    }

    #[test]
    fn progress_falls_back_to_percent() {
        let (pct, speed, eta) = parse_progress("NA|NA|NA|NA|NA| 7.5%").unwrap();
        assert!((pct - 0.075).abs() < 1e-9);
        assert!(speed.is_empty());
        assert!(eta.is_empty());
    }

    #[test]
    fn short_progress_payload_is_rejected() {
        assert!(parse_progress("1|2").is_none());
    }
}
