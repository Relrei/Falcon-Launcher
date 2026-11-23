//! 拡大表示の動画の再生。
//! egui は動画を読めないので、ffmpeg で 1 コマずつ RGBA にほどき、テクスチャへ差し替えて見せる。
//! 音は出さない(描いた動画は音が無い)。終わりまで来たら頭から繰り返す。
//! 再生バーと倍速: 時刻へ飛ぶ(ffmpeg を -ss で起こし直す)・倍速・1 コマ送り。
//! 拡大した時は、元の大きさでほどき直す(ぼやけないように)。
use eframe::egui;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// 画面へ送るコマの速さの上限(書き換えに合わせる)。倍速の時は、元のコマを間引いてこの速さに収める
const MAX_SHOW_FPS: f32 = 60.0;
/// 元の大きさでほどく時の上限
const MAX_DECODE: (f32, f32) = (3840.0, 2160.0);

pub const SPEEDS: [f32; 6] = [0.25, 0.5, 1.0, 1.5, 2.0, 4.0];

struct Shared {
    /// 最後にほどいたコマ(RGBA と大きさ)
    frame: Mutex<Option<(Vec<u8>, [usize; 2])>>,
    /// ほどいたコマの通し番号(新しいコマが来たかを見る)
    count: AtomicU64,
    /// 今のコマの時刻(ミリ秒)
    pos_ms: AtomicU64,
    paused: AtomicBool,
    stop: AtomicBool,
    /// 飛ぶ先の時刻(秒)
    seek: Mutex<Option<f64>>,
    speed: Mutex<f32>,
    /// ほどく大きさ(変わったら起こし直す)
    size: Mutex<[usize; 2]>,
    error: Mutex<Option<String>>,
    child: Mutex<Option<Child>>,
}

pub struct Player {
    pub path: PathBuf,
    /// 元の大きさ・速さ・長さ(秒)
    pub src: [u32; 2],
    pub fps: f32,
    pub duration: f64,
    /// 全体に合わせた時にほどく大きさ
    fit: [usize; 2],
    shared: Arc<Shared>,
    tex: Option<egui::TextureHandle>,
    shown: u64,
}

/// ffprobe で幅・高さ・速さ・長さを聞く
fn probe(path: &Path) -> Option<(u32, u32, f32, f64)> {
    let o = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height,r_frame_rate,nb_frames:format=duration", "-of", "default=nw=1"])
        .arg(path)
        .stdin(Stdio::null())
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&o.stdout);
    let get = |k: &str| text.lines().find_map(|l| l.strip_prefix(&format!("{k}=")).map(str::trim).map(str::to_string));
    let w: u32 = get("width")?.parse().ok()?;
    let h: u32 = get("height")?.parse().ok()?;
    let fps = get("r_frame_rate")
        .and_then(|r| {
            let (a, b) = r.split_once('/').unwrap_or((&r, "1"));
            let (a, b): (f32, f32) = (a.parse().ok()?, b.parse().ok()?);
            (b > 0.0 && a > 0.0).then(|| a / b)
        })
        .unwrap_or(24.0)
        .clamp(1.0, 240.0);
    let frames: f64 = get("nb_frames").and_then(|n| n.parse().ok()).unwrap_or(0.0);
    let duration = get("duration").and_then(|d| d.parse::<f64>().ok()).filter(|d| *d > 0.0).unwrap_or(frames / fps as f64);
    Some((w, h, fps, duration))
}

/// (w, h) を box の中に収めた偶数の大きさ(大きくはしない)
fn fit_in(w: u32, h: u32, bw: f32, bh: f32) -> [usize; 2] {
    let k = (bw / w as f32).min(bh / h as f32).min(1.0);
    [((w as f32 * k) as usize / 2 * 2).max(2), ((h as f32 * k) as usize / 2 * 2).max(2)]
}

fn run(sh: Arc<Shared>, path: PathBuf, fps: f32) {
    let mut start = 0.0f64;
    // 起こし直した直後は、止めていても 1 コマだけ見せる
    let mut preview = true;
    'outer: loop {
        if sh.stop.load(Ordering::SeqCst) {
            break;
        }
        let speed = *sh.speed.lock().unwrap();
        let [w, h] = *sh.size.lock().unwrap();
        // 元の 1 秒からいくつコマを取るか(倍速で画面へ送る速さが上限を超えないように間引く)
        let rate = fps.min(MAX_SHOW_FPS / speed).max(1.0);
        let spawned = Command::new("nice")
            .args(["-n", "10", "ffmpeg", "-nostdin", "-loglevel", "error", "-ss", &format!("{start:.3}"), "-i"])
            .arg(&path)
            .args(["-vf", &format!("fps={rate},scale={w}:{h}:flags=bilinear"), "-f", "rawvideo", "-pix_fmt", "rgba", "-"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        let mut child = match spawned {
            Ok(c) => c,
            Err(e) => {
                *sh.error.lock().unwrap() = Some(format!("ffmpeg を起こせませんでした: {e}"));
                return;
            }
        };
        let Some(mut out) = child.stdout.take() else { return };
        if let Some(mut old) = sh.child.lock().unwrap().replace(child) {
            let _ = old.kill();
            let _ = old.wait();
        }
        let mut buf = vec![0u8; w * h * 4];
        let step = Duration::from_secs_f64(1.0 / (rate * speed) as f64);
        let mut n = 0u64;
        let mut next = Instant::now();
        let restart = |sh: &Shared| {
            if let Some(mut c) = sh.child.lock().unwrap().take() {
                let _ = c.kill();
                let _ = c.wait();
            }
        };
        loop {
            if sh.stop.load(Ordering::SeqCst) {
                break 'outer;
            }
            // 今見せているコマの時刻(起こし直す時はここから。次のコマからにすると、止めていても 1 コマ進んでしまう)
            let shown_t = start + n.saturating_sub(1) as f64 / rate as f64;
            let now_t = start + n as f64 / rate as f64;
            // 飛ぶ・倍速や大きさが変わった → 起こし直す
            if let Some(t) = sh.seek.lock().unwrap().take() {
                start = t.max(0.0);
                preview = true;
                restart(&sh);
                continue 'outer;
            }
            if *sh.speed.lock().unwrap() != speed || *sh.size.lock().unwrap() != [w, h] {
                start = shown_t;
                preview = true;
                restart(&sh);
                continue 'outer;
            }
            if sh.paused.load(Ordering::SeqCst) && !preview {
                std::thread::sleep(Duration::from_millis(15));
                next = Instant::now();
                continue;
            }
            if out.read_exact(&mut buf).is_err() {
                if sh.stop.load(Ordering::SeqCst) {
                    break 'outer;
                }
                if n == 0 && start <= 0.0 {
                    *sh.error.lock().unwrap() = Some("動画をほどけませんでした(ffmpeg)".into());
                    break 'outer;
                }
                // 終わりまで来た → 頭から
                start = 0.0;
                restart(&sh);
                continue 'outer;
            }
            sh.pos_ms.store((now_t * 1000.0) as u64, Ordering::SeqCst);
            *sh.frame.lock().unwrap() = Some((buf.clone(), [w, h]));
            sh.count.fetch_add(1, Ordering::SeqCst);
            n += 1;
            if preview {
                preview = false;
                next = Instant::now() + step;
                continue;
            }
            // 元の速さ × 倍速で進める(ほどくのが遅れた時は、遅れを持ち越さない)
            next += step;
            let now = Instant::now();
            if next > now {
                std::thread::sleep(next - now);
            } else if now - next > step * 4 {
                next = now;
            }
        }
    }
    if let Some(mut c) = sh.child.lock().unwrap().take() {
        let _ = c.kill();
        let _ = c.wait();
    }
}

impl Player {
    /// view(見せる枠)に合わせた大きさでほどき始める
    pub fn open(path: &Path, view: egui::Vec2) -> Result<Self, String> {
        let (w0, h0, fps, duration) = probe(path).ok_or_else(|| format!("動画の大きさを読めませんでした(ffprobe): {}", path.display()))?;
        let fit = fit_in(w0, h0, view.x, view.y);
        let shared = Arc::new(Shared {
            frame: Mutex::new(None),
            count: AtomicU64::new(0),
            pos_ms: AtomicU64::new(0),
            paused: AtomicBool::new(false),
            stop: AtomicBool::new(false),
            seek: Mutex::new(None),
            speed: Mutex::new(1.0),
            size: Mutex::new(fit),
            error: Mutex::new(None),
            child: Mutex::new(None),
        });
        let (sh, p) = (shared.clone(), path.to_path_buf());
        std::thread::spawn(move || run(sh, p, fps));
        Ok(Player { path: path.to_path_buf(), src: [w0, h0], fps, duration, fit, shared, tex: None, shown: 0 })
    }

    pub fn paused(&self) -> bool {
        self.shared.paused.load(Ordering::SeqCst)
    }

    pub fn toggle(&self) {
        self.shared.paused.fetch_xor(true, Ordering::SeqCst);
    }

    pub fn pause(&self) {
        self.shared.paused.store(true, Ordering::SeqCst);
    }

    pub fn error(&self) -> Option<String> {
        self.shared.error.lock().ok().and_then(|e| e.clone())
    }

    /// 今の時刻(秒)
    pub fn position(&self) -> f64 {
        self.shared.pos_ms.load(Ordering::SeqCst) as f64 / 1000.0
    }

    pub fn seek(&self, t: f64) {
        let t = if self.duration > 0.0 { t.clamp(0.0, (self.duration - 0.5 / self.fps as f64).max(0.0)) } else { t.max(0.0) };
        *self.shared.seek.lock().unwrap() = Some(t);
    }

    /// 1 コマ送る・戻す(止めて)
    pub fn step(&self, frames: i32) {
        self.pause();
        self.seek(self.position() + frames as f64 / self.fps as f64);
    }

    pub fn speed(&self) -> f32 {
        *self.shared.speed.lock().unwrap()
    }

    pub fn set_speed(&self, s: f32) {
        *self.shared.speed.lock().unwrap() = s.clamp(0.1, 8.0);
    }

    /// 拡大しているか(元の大きさでほどくか、枠に合わせた大きさでほどくか)
    pub fn set_zoomed(&self, zoomed: bool) {
        let want = if zoomed { fit_in(self.src[0], self.src[1], MAX_DECODE.0, MAX_DECODE.1) } else { self.fit };
        let mut s = self.shared.size.lock().unwrap();
        if *s != want {
            *s = want;
        }
    }

    /// 新しいコマが来ていればテクスチャへ差し替え、見せるテクスチャを返す
    pub fn texture(&mut self, ctx: &egui::Context) -> Option<egui::TextureHandle> {
        let count = self.shared.count.load(Ordering::SeqCst);
        if count != self.shown {
            if let Some((buf, size)) = self.shared.frame.lock().ok().and_then(|f| f.clone()) {
                let img = egui::ColorImage::from_rgba_unmultiplied(size, &buf);
                match &mut self.tex {
                    Some(t) => t.set(img, egui::TextureOptions::LINEAR),
                    None => self.tex = Some(ctx.load_texture("falcon-launcher-video", img, egui::TextureOptions::LINEAR)),
                }
                self.shown = count;
            }
        }
        // 止めていても、飛んだ先のコマを受け取るために少しだけ見回る
        let wait = if self.paused() { 0.1 } else { 1.0 / (self.fps.min(MAX_SHOW_FPS) * 2.0) };
        ctx.request_repaint_after(Duration::from_secs_f32(wait));
        self.tex.clone()
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        if let Some(mut c) = self.shared.child.lock().ok().and_then(|mut c| c.take()) {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}
