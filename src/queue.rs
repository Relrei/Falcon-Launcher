//! レンダリングの一覧。
//! 開いている Blender のシーンでチェックを入れるとランチャーに登録され、消すのはランチャーか開いているシーンで。
//! オンオフのスイッチで描くかどうかを決める。順番は上から下へ。中断した後も続けられるように記録を残す。
//! 監視と調整(一時停止と再開・CPU のスレッド数・優先度・事前のメモリ上限)は残す。
//!
//! 連携: アドオン(addon/falcon_launcher_link.py)が <届け先>/<id>/ にシーンの写し(scene.blend)と entry.tsv を置く。
//!   チェックを外すとアドオンがそのフォルダを消し、ランチャーは行を外す。ランチャーで消すとフォルダを消し、アドオンがチェックを外す。
//! 続き: 出力のフォルダは行ごとに決まっていて、描く時は上書きしない設定にするので、描いてある枚は飛ばして続きから描く。

use super::*;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write as _};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, PartialEq)]
pub enum JobState {
    Wait,
    Running,
    Paused,
    /// 途中で止まった(続きから描ける)
    Partial,
    Done,
    Failed,
}

impl JobState {
    pub fn text(self) -> &'static str {
        match self {
            JobState::Wait => "待ち",
            JobState::Running => "描いています",
            JobState::Paused => "一時停止",
            JobState::Partial => "途中",
            JobState::Done => "済み",
            JobState::Failed => "失敗",
        }
    }
    fn key(self) -> &'static str {
        match self {
            JobState::Wait => "wait",
            JobState::Running | JobState::Paused | JobState::Partial => "partial",
            JobState::Done => "done",
            JobState::Failed => "failed",
        }
    }
    fn from_key(s: &str) -> Self {
        match s {
            "done" => JobState::Done,
            "failed" => JobState::Failed,
            "partial" => JobState::Partial,
            _ => JobState::Wait,
        }
    }
}

#[derive(Clone)]
pub struct Job {
    pub id: u64,
    /// 描く .blend(アドオンから来た物はシーンの写し)と、元の場所
    pub blend: PathBuf,
    pub source: PathBuf,
    pub build: String,
    pub build_title: String,
    pub scene: String,
    /// None ならファイルの範囲のまま
    pub range: Option<(i32, i32, i32)>,
    pub camera: String,
    pub keep_output: bool,
    /// 描くかどうかのスイッチ
    pub on: bool,
    pub state: JobState,
    pub progress: String,
    /// 出力のフォルダ(1 度決めたら変えない。「最初から」で空にする)
    pub out: PathBuf,
    pub started: i64,
    pub ended: i64,
    /// アドオンの届けたフォルダの名前(手で足した物は空)
    pub inbox: String,
    /// entry.tsv の時刻(アドオンが範囲などを変えたら読み直す)
    pub inbox_mtime: u64,
    /// このシーンだけの割り当て(無い物は一覧の全体の設定のまま)
    pub lim: Limits,
    /// 写しの描き方(アドオンが写しを書いた時の値。手で足した物は空)
    pub scene_rs: SceneRender,
    /// このシーンだけ描き方を変える物(None はシーンのまま)。
    /// アドオンから来た物は Blender で決める(entry.tsv の over_*)。手で足した物はランチャーで決める
    pub rs: RenderOver,
}

/// 写しの描き方(entry.tsv の engine・samples・denoise・denoiser)
#[derive(Clone, Default, PartialEq)]
pub struct SceneRender {
    /// render.engine(CYCLES・NEURAL_RENDERING・BLENDER_EEVEE_NEXT など。分からない時は空)
    pub engine: String,
    pub samples: Option<u32>,
    pub denoise: Option<bool>,
    pub denoiser: String,
}

impl SceneRender {
    pub fn is_eevee(&self) -> bool {
        self.engine.contains("EEVEE")
    }
    /// 予約.tsv の 19 列目(例「e=CYCLES,s=128,d=1,dn=OPTIX」、無ければ「-」)
    fn key(&self) -> String {
        let mut v = vec![];
        if !self.engine.is_empty() { v.push(format!("e={}", self.engine)); }
        if let Some(x) = self.samples { v.push(format!("s={x}")); }
        if let Some(x) = self.denoise { v.push(format!("d={}", x as u8)); }
        if !self.denoiser.is_empty() { v.push(format!("dn={}", self.denoiser)); }
        if v.is_empty() { "-".into() } else { v.join(",") }
    }
    fn from_key(k: &str) -> Self {
        let mut r = SceneRender::default();
        for kv in k.split(',') {
            match kv.split_once('=') {
                Some(("e", x)) => r.engine = x.into(),
                Some(("s", x)) => r.samples = x.parse().ok(),
                Some(("d", x)) => r.denoise = Some(x == "1"),
                Some(("dn", x)) => r.denoiser = x.into(),
                _ => {}
            }
        }
        r
    }
}

/// シーンごとの描き方の上書き。None は「シーンのまま」
#[derive(Clone, Default, PartialEq)]
pub struct RenderOver {
    pub samples: Option<u32>,
    pub denoise: Option<bool>,
    /// OPENIMAGEDENOISE・OPTIX・DLSS
    pub denoiser: Option<String>,
}

/// 選べるデノイザー(値と出す名前)。DLSS は Falcon の版だけ
pub fn denoisers(falcon: bool) -> Vec<(&'static str, &'static str)> {
    let mut v = vec![("OPENIMAGEDENOISE", "OpenImageDenoise"), ("OPTIX", "OptiX")];
    if falcon {
        v.push(("DLSS", "DLSS"));
    }
    v
}

pub fn denoiser_name(k: &str) -> &str {
    match k {
        "OPENIMAGEDENOISE" => "OpenImageDenoise",
        "OPTIX" => "OptiX",
        "DLSS" => "DLSS",
        "AUTO" => "自動",
        x => x,
    }
}

impl RenderOver {
    pub fn is_empty(&self) -> bool {
        *self == RenderOver::default()
    }
    /// 予約.tsv の 18 列目(例「s=64,d=1,dn=DLSS」、無ければ「-」)
    fn key(&self) -> String {
        let mut v = vec![];
        if let Some(x) = self.samples { v.push(format!("s={x}")); }
        if let Some(x) = self.denoise { v.push(format!("d={}", x as u8)); }
        if let Some(x) = &self.denoiser { v.push(format!("dn={x}")); }
        if v.is_empty() { "-".into() } else { v.join(",") }
    }
    fn from_key(k: &str) -> Self {
        let mut r = RenderOver::default();
        for kv in k.split(',') {
            match kv.split_once('=') {
                Some(("s", x)) => r.samples = x.parse().ok(),
                Some(("d", x)) => r.denoise = Some(x == "1"),
                Some(("dn", x)) if !x.is_empty() => r.denoiser = Some(x.into()),
                _ => {}
            }
        }
        r
    }
    /// 行に出す短い説明(上書きした物だけ)
    pub fn text(&self) -> String {
        let mut v = vec![];
        if let Some(x) = self.samples { v.push(format!("{x} サンプル")); }
        match (self.denoise, &self.denoiser) {
            (Some(false), _) => v.push("デノイズ無し".into()),
            (_, Some(d)) => v.push(denoiser_name(d).to_string()),
            (Some(true), None) => v.push("デノイズ".into()),
            _ => {}
        }
        v.join(" · ")
    }
    /// 描く時に --python-expr へ足す行(エンジンを見て、Cycles 系なら scene.cycles、EEVEE ならサンプル数だけ)
    fn python(&self) -> String {
        if self.is_empty() {
            return String::new();
        }
        let mut cy = String::new();
        if let Some(n) = self.samples {
            cy.push_str(&format!(" c.samples={n}\n"));
        }
        if let Some(d) = self.denoise {
            cy.push_str(&format!(" c.use_denoising={}\n", if d { "True" } else { "False" }));
        }
        if let Some(d) = &self.denoiser {
            // 選べない時(機器の設定が無いなど)は、シーンのデノイザーのまま描き、ログに残す
            let d: String = d.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
            cy.push_str(&format!(" try: c.denoiser='{d}'\n except Exception as x: print('Falcon Launcher: デノイザー {d} を選べませんでした:', x)\n"));
        }
        let mut s = format!("s=bpy.context.scene\ne=s.render.engine\nif e in ('CYCLES','NEURAL_RENDERING'):\n c=s.cycles\n{cy}");
        if let Some(n) = self.samples {
            s.push_str(&format!("elif 'EEVEE' in e:\n s.eevee.taa_render_samples={n}\n"));
        }
        s.push_str("print('Falcon Launcher: 描き方', e, getattr(s.cycles,'samples','') if hasattr(s,'cycles') else '', getattr(getattr(s,'cycles',None),'denoiser',''))\n");
        s
    }
}

/// シーンごとの割り当て。None は「全体と同じ」
#[derive(Clone, Copy, Default, PartialEq)]
pub struct Limits {
    pub threads: Option<u32>,
    pub nice: Option<i32>,
    pub ram_gb: Option<f32>,
    pub vram_mib: Option<u32>,
}

impl Limits {
    pub fn is_empty(&self) -> bool {
        *self == Limits::default()
    }
    /// 予約.tsv の 17 列目(例「t=6,n=10,r=8,v=6000」、無ければ「-」)
    fn key(&self) -> String {
        let mut v = vec![];
        if let Some(x) = self.threads { v.push(format!("t={x}")); }
        if let Some(x) = self.nice { v.push(format!("n={x}")); }
        if let Some(x) = self.ram_gb { v.push(format!("r={x}")); }
        if let Some(x) = self.vram_mib { v.push(format!("v={x}")); }
        if v.is_empty() { "-".into() } else { v.join(",") }
    }
    fn from_key(k: &str) -> Self {
        let mut l = Limits::default();
        for kv in k.split(',') {
            match kv.split_once('=') {
                Some(("t", x)) => l.threads = x.parse().ok(),
                Some(("n", x)) => l.nice = x.parse().ok(),
                Some(("r", x)) => l.ram_gb = x.parse().ok(),
                Some(("v", x)) => l.vram_mib = x.parse().ok(),
                _ => {}
            }
        }
        l
    }
    /// 行に出す短い説明(個別の物だけ)
    pub fn text(&self) -> String {
        let mut v = vec![];
        if let Some(x) = self.threads { v.push(if x == 0 { "CPU 全部".to_string() } else { format!("CPU {x} 本") }); }
        if let Some(x) = self.ram_gb { v.push(if x <= 0.0 { "RAM 上限なし".to_string() } else { format!("RAM {x} GB") }); }
        if let Some(x) = self.vram_mib { v.push(if x == 0 { "VRAM 上限なし".to_string() } else { format!("VRAM {x} MiB") }); }
        if let Some(x) = self.nice { v.push(format!("nice {x}")); }
        v.join(" · ")
    }
}

impl Job {
    pub fn name(&self) -> String {
        let p = if self.source.as_os_str().is_empty() { &self.blend } else { &self.source };
        p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
    }
    pub fn log(&self) -> PathBuf {
        config_dir().join("予約").join(format!("{}.log", self.id))
    }
    pub fn range_text(&self) -> String {
        match self.range {
            Some((a, b, _)) if a == b => format!("{a} の 1 枚"),
            Some((a, b, s)) if s > 1 => format!("{a}–{b} · {s} おき"),
            Some((a, b, _)) => format!("{a}–{b}"),
            None => "ファイルの範囲".into(),
        }
    }
    pub fn count(&self) -> Option<i32> {
        self.range.map(|(a, b, s)| (b - a) / s.max(1) + 1)
    }
    /// 描き終えた枚(出力のフォルダの絵の数)
    pub fn done_frames(&self) -> usize {
        if self.out.as_os_str().is_empty() {
            return 0;
        }
        std::fs::read_dir(&self.out).map(|r| r.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "png" || x == "exr" || x == "jpg")).count()).unwrap_or(0)
    }
    /// 1 枚の秒(描いている間だけ。今回描いた絵のファイルの時刻の間隔から。始めの 1 枚は起動・組み立ての時間が入るので数えない)。
    /// Some(None) は描いているがまだ 2 枚に満たない時
    pub fn per_frame_secs(&self) -> Option<Option<f64>> {
        if self.state != JobState::Running || self.started <= 0 || self.out.as_os_str().is_empty() {
            return None;
        }
        let mut times: Vec<i64> = std::fs::read_dir(&self.out)
            .map(|r| {
                r.flatten()
                    .filter(|e| e.path().extension().is_some_and(|x| x == "png" || x == "exr" || x == "jpg"))
                    .filter_map(|e| e.metadata().ok()?.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_millis() as i64))
                    .filter(|&t| t >= self.started * 1000)
                    .collect()
            })
            .unwrap_or_default();
        if times.len() < 2 {
            return Some(None);
        }
        times.sort_unstable();
        Some(Some((times[times.len() - 1] - times[0]) as f64 / (times.len() - 1) as f64 / 1000.0))
    }
    /// 残りの秒(描いた速さから計算する)。Some(None) はまだ計っている時
    pub fn remaining_secs(&self) -> Option<Option<i64>> {
        let total = self.count()? as i64;
        let per = self.per_frame_secs()?;
        let left = (total - self.done_frames() as i64).max(0);
        Some(per.map(|p| (p * left as f64).round() as i64))
    }
    /// 「約 8 分」「約 1 時間 5 分」「1 分未満」
    pub fn rough(r: i64) -> String {
        if r < 60 {
            "1 分未満".to_string()
        } else if r < 3600 {
            format!("約 {} 分", (r + 30) / 60)
        } else {
            format!("約 {} 時間 {} 分", r / 3600, (r % 3600 + 30) / 60)
        }
    }
    /// 「残り 約 8 分 · 今日 14:20 ごろ」(描いていない時は None)
    pub fn remaining_text(&self) -> Option<String> {
        let Some(r) = self.remaining_secs()? else {
            return Some("残りの時間を計っています".into());
        };
        Some(format!("残り {} · {} ごろ", Self::rough(r), when(now() + r)))
    }
    pub fn runnable(&self) -> bool {
        self.on && matches!(self.state, JobState::Wait | JobState::Partial)
    }
}

/// 調整(使う人が選んだ物)と、全体のスイッチ
#[derive(Clone)]
pub struct Settings {
    /// 0 なら全部
    pub threads: u32,
    /// 0..19(大きいほど後回し)
    pub nice: i32,
    /// RAM の上限 GB(0 なら無し)。systemd のスコープで本当に絞る
    pub ram_gb: f32,
    /// VRAM の上限 MiB(0 なら無し)。外から絞れないので、近ければ始めず、超えたら止める
    pub vram_mib: u32,
    /// 進めるかどうか(入っている間は、オンの行を上から描いていく)
    pub run: bool,
}

impl Default for Settings {
    fn default() -> Self {
        // VRAM は全体で 9000MiB 以内の決まり(pc.md)
        Self { threads: 0, nice: 10, ram_gb: 0.0, vram_mib: 9000, run: false }
    }
}

fn settings_file() -> PathBuf {
    config_dir().join("予約の設定.tsv")
}

impl Settings {
    /// 全体の設定に、そのシーンだけの割り当てをかぶせた物
    pub fn with(&self, l: &Limits) -> Settings {
        Settings {
            threads: l.threads.unwrap_or(self.threads),
            nice: l.nice.unwrap_or(self.nice),
            ram_gb: l.ram_gb.unwrap_or(self.ram_gb),
            vram_mib: l.vram_mib.unwrap_or(self.vram_mib),
            run: self.run,
        }
    }
}

fn load_settings() -> Settings {
    let mut s = Settings::default();
    for l in std::fs::read_to_string(settings_file()).unwrap_or_default().lines() {
        let Some((k, v)) = l.split_once('\t') else { continue };
        match k {
            "threads" => s.threads = v.parse().unwrap_or(0),
            "nice" => s.nice = v.parse().unwrap_or(10),
            "ram_gb" => s.ram_gb = v.parse().unwrap_or(0.0),
            "vram_mib" => s.vram_mib = v.parse().unwrap_or(9000),
            // 起動した時は、前に進めていても止めておく(勝手に GPU を使い始めないように)
            _ => {}
        }
    }
    s
}

pub fn save_settings(s: &Settings) {
    let _ = std::fs::create_dir_all(config_dir());
    let _ = std::fs::write(settings_file(), format!("threads\t{}\nnice\t{}\nram_gb\t{}\nvram_mib\t{}\n", s.threads, s.nice, s.ram_gb, s.vram_mib));
}

/// 届け先(アドオンが読む連携.tsv にも書く)
pub fn inbox_dir() -> PathBuf {
    let cfg = config_dir().join("連携.tsv");
    if let Some(p) = std::fs::read_to_string(&cfg).ok().and_then(|t| t.lines().find_map(|l| l.strip_prefix("inbox\t").map(PathBuf::from))) {
        return p;
    }
    let p = crate::places::data_dir().join("届いた");
    let _ = std::fs::create_dir_all(config_dir());
    let _ = std::fs::write(&cfg, format!("inbox\t{}\n", p.display()));
    p
}

/// Blender のアドオン(ランチャーに入れて持つ。入れる時に書き出す)
pub const ADDON_PY: &str = include_str!("../addon/falcon_launcher_link.py");

fn file() -> PathBuf {
    config_dir().join("予約.tsv")
}

pub fn load() -> Vec<Job> {
    let Ok(text) = std::fs::read_to_string(file()) else { return vec![] };
    text.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            if f.len() < 16 {
                return None;
            }
            let range = (f[6] != "-").then(|| {
                let v: Vec<i32> = f[6].split(',').filter_map(|x| x.parse().ok()).collect();
                (v.first().copied().unwrap_or(1), v.get(1).copied().unwrap_or(1), v.get(2).copied().unwrap_or(1))
            });
            Some(Job {
                id: f[0].parse().ok()?,
                blend: PathBuf::from(f[1]),
                source: PathBuf::from(f[2]),
                build: f[3].into(),
                build_title: f[4].into(),
                scene: f[5].into(),
                range,
                camera: f[7].into(),
                keep_output: f[8] == "1",
                state: JobState::from_key(f[9]),
                progress: String::new(),
                out: PathBuf::from(f[10]),
                started: f[11].parse().unwrap_or(0),
                ended: f[12].parse().unwrap_or(0),
                inbox: f[13].into(),
                on: f[14] == "1",
                inbox_mtime: f[15].parse().unwrap_or(0),
                lim: f.get(16).map(|k| Limits::from_key(k)).unwrap_or_default(),
                rs: f.get(17).map(|k| RenderOver::from_key(k)).unwrap_or_default(),
                scene_rs: f.get(18).map(|k| SceneRender::from_key(k)).unwrap_or_default(),
            })
        })
        .collect()
}

pub fn save(jobs: &[Job]) {
    let _ = std::fs::create_dir_all(config_dir());
    let text: String = jobs
        .iter()
        .map(|j| {
            let range = j.range.map(|(a, b, s)| format!("{a},{b},{s}")).unwrap_or_else(|| "-".into());
            format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                j.id,
                j.blend.display(),
                j.source.display(),
                j.build,
                j.build_title,
                j.scene,
                range,
                j.camera,
                if j.keep_output { 1 } else { 0 },
                j.state.key(),
                j.out.display(),
                j.started,
                j.ended,
                j.inbox,
                if j.on { 1 } else { 0 },
                j.inbox_mtime,
                j.lim.key(),
                j.rs.key(),
                j.scene_rs.key()
            )
        })
        .collect();
    let _ = std::fs::write(file(), text);
}

/// 出力の置き場所: <レンダリングの出力(場所.tsv の render)>/<日付>/<時刻> <名前> <シーン> [<key>](初めて描く時に 1 度だけ決める)
fn out_dir(j: &Job) -> PathBuf {
    let l = now() + tz_offset();
    let (y, m, d) = civil(l.div_euclid(86400));
    let hm = format!("{:02}{:02}", l.rem_euclid(86400) / 3600, l.rem_euclid(3600) / 60);
    let stem = Path::new(&j.name()).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let scene = if j.scene.is_empty() { String::new() } else { format!(" {}", j.scene) };
    PathBuf::from(scan::render_dir()).join(format!("{y}-{m:02}-{d:02}")).join(scan::safe_key(&format!("{hm} {stem}{scene} [{}]", j.build)))
}

// ---------- 監視 ----------

#[derive(Clone, Default)]
pub struct Sample {
    pub cpu: f32,
    pub ram_used: u64,
    pub ram_total: u64,
    pub gpu: f32,
    pub vram_used: u32,
    pub vram_total: u32,
    /// 描いている物(プロセスの組)
    pub job_cpu: f32,
    pub job_rss: u64,
    pub job_vram: u32,
}

pub struct Monitor {
    pub hist: VecDeque<Sample>,
}

fn read_cpu() -> (u64, u64) {
    let s = std::fs::read_to_string("/proc/stat").unwrap_or_default();
    let v: Vec<u64> = s.lines().next().unwrap_or("").split_whitespace().skip(1).filter_map(|x| x.parse().ok()).collect();
    let idle = v.get(3).copied().unwrap_or(0) + v.get(4).copied().unwrap_or(0);
    (v.iter().sum(), idle)
}

fn read_mem() -> (u64, u64) {
    let s = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let get = |k: &str| s.lines().find_map(|l| l.strip_prefix(k)).and_then(|r| r.split_whitespace().next()).and_then(|x| x.parse::<u64>().ok()).unwrap_or(0) * 1024;
    let total = get("MemTotal:");
    (total.saturating_sub(get("MemAvailable:")), total)
}

/// プロセスの組(pgid)の CPU 時間の合計(jiffies)と RSS
fn group_stat(pgid: u32) -> (u64, u64, Vec<u32>) {
    let (mut t, mut rss, mut pids) = (0, 0, vec![]);
    for e in std::fs::read_dir("/proc").into_iter().flatten().flatten() {
        let name = e.file_name();
        let Ok(pid) = name.to_string_lossy().parse::<u32>() else { continue };
        let Ok(s) = std::fs::read_to_string(e.path().join("stat")) else { continue };
        let Some(rest) = s.rsplit_once(')').map(|x| x.1) else { continue };
        let f: Vec<&str> = rest.split_whitespace().collect();
        // f[2] が pgrp、f[11] utime、f[12] stime、f[21] rss(ページ)
        if f.get(2).and_then(|x| x.parse::<u32>().ok()) == Some(pgid) {
            t += f.get(11).and_then(|x| x.parse::<u64>().ok()).unwrap_or(0) + f.get(12).and_then(|x| x.parse::<u64>().ok()).unwrap_or(0);
            rss += f.get(21).and_then(|x| x.parse::<u64>().ok()).unwrap_or(0) * 4096;
            pids.push(pid);
        }
    }
    (t, rss, pids)
}

fn nvidia() -> (f32, u32, u32, HashMap<u32, u32>) {
    let o = Command::new("nvidia-smi").args(["--query-gpu=utilization.gpu,memory.used,memory.total", "--format=csv,noheader,nounits"]).stdin(Stdio::null()).stderr(Stdio::null()).output();
    let line = o.map(|o| String::from_utf8_lossy(&o.stdout).lines().next().unwrap_or("").to_string()).unwrap_or_default();
    let v: Vec<f32> = line.split(',').filter_map(|x| x.trim().parse().ok()).collect();
    let apps = Command::new("nvidia-smi").args(["--query-compute-apps=pid,used_memory", "--format=csv,noheader,nounits"]).stdin(Stdio::null()).stderr(Stdio::null()).output();
    let mut per = HashMap::new();
    for l in apps.map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default().lines() {
        let f: Vec<&str> = l.split(',').map(str::trim).collect();
        if let (Some(p), Some(m)) = (f.first().and_then(|x| x.parse().ok()), f.get(1).and_then(|x| x.parse().ok())) {
            per.insert(p, m);
        }
    }
    (v.first().copied().unwrap_or(0.0), v.get(1).copied().unwrap_or(0.0) as u32, v.get(2).copied().unwrap_or(0.0) as u32, per)
}

// ---------- 係 ----------

pub struct Runner {
    pub jobs: Arc<Mutex<Vec<Job>>>,
    pub settings: Arc<Mutex<Settings>>,
    pub monitor: Arc<Mutex<Monitor>>,
    running: Arc<AtomicBool>,
    /// 今の 1 本を止める(続きは残す)
    halt: Arc<AtomicBool>,
    pid: Arc<Mutex<Option<u32>>>,
    /// 今描いている行の id
    current: Arc<Mutex<Option<u64>>>,
    watched: Arc<Mutex<Instant>>,
    last_inbox: Instant,
    pub notice: Arc<Mutex<String>>,
}

impl Runner {
    pub fn new(ctx: &egui::Context) -> Self {
        let r = Self {
            jobs: Arc::new(Mutex::new(load())),
            settings: Arc::new(Mutex::new(load_settings())),
            monitor: Arc::new(Mutex::new(Monitor { hist: VecDeque::new() })),
            running: Arc::new(AtomicBool::new(false)),
            halt: Arc::new(AtomicBool::new(false)),
            pid: Arc::new(Mutex::new(None)),
            current: Arc::new(Mutex::new(None)),
            watched: Arc::new(Mutex::new(Instant::now() - Duration::from_secs(60))),
            last_inbox: Instant::now() - Duration::from_secs(60),
            notice: Arc::new(Mutex::new(String::new())),
        };
        r.start_monitor(ctx.clone());
        r
    }

    pub fn snapshot(&self) -> Vec<Job> {
        self.jobs.lock().map(|j| j.clone()).unwrap_or_default()
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn watch(&self) {
        if let Ok(mut w) = self.watched.lock() {
            *w = Instant::now();
        }
    }

    pub fn settings(&self) -> Settings {
        self.settings.lock().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn set_settings(&self, s: Settings) {
        save_settings(&s);
        if let Ok(mut x) = self.settings.lock() {
            *x = s;
        }
    }

    /// 手で一覧に足す(ランチャーの .blend から。元のファイルをそのまま描く)
    pub fn add(&self, blend: PathBuf, b: &Build, range: Option<(i32, i32, i32)>) {
        self.edit(|jobs| {
            let id = next_id(jobs);
            jobs.push(Job { id, blend: blend.clone(), source: blend, build: b.key.clone(), build_title: b.title.clone(), scene: String::new(), range, camera: String::new(), keep_output: false, on: true, state: JobState::Wait, progress: String::new(), out: PathBuf::new(), started: 0, ended: 0, inbox: String::new(), inbox_mtime: 0, lim: Limits::default(), scene_rs: SceneRender::default(), rs: RenderOver::default() });
        });
    }

    pub fn edit(&self, f: impl FnOnce(&mut Vec<Job>)) {
        if let Ok(mut jobs) = self.jobs.lock() {
            f(&mut jobs);
            save(&jobs);
        }
    }

    /// アドオンとの同期(2 秒おき): 新しい登録を足し、変わった物を読み直し、チェックを外された物を外す
    pub fn sync_inbox(&mut self, builds: &[Build], fallback: Option<&Build>) -> (usize, usize) {
        if self.last_inbox.elapsed() < Duration::from_secs(2) {
            return (0, 0);
        }
        self.last_inbox = Instant::now();
        let root = inbox_dir();
        let mut seen = HashSet::new();
        let (mut added, mut removed) = (0, 0);
        let entries: Vec<(String, PathBuf, u64)> = std::fs::read_dir(&root)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let f = e.path().join("entry.tsv");
                let mt = std::fs::metadata(&f).ok()?.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
                (!name.starts_with('.')).then_some((name, e.path(), mt))
            })
            .collect();
        let cur = self.current.lock().ok().and_then(|c| *c);
        let mut stop_current = false;
        self.edit(|jobs| {
            for (name, dir, mt) in &entries {
                seen.insert(name.clone());
                let text = std::fs::read_to_string(dir.join("entry.tsv")).unwrap_or_default();
                let get = |k: &str| text.lines().find_map(|l| l.strip_prefix(&format!("{k}\t"))).unwrap_or("").to_string();
                let num = |k: &str, d: i32| get(k).parse().unwrap_or(d);
                let range = Some((num("start", 1), num("end", 1), num("step", 1)));
                let scene_rs = SceneRender {
                    engine: get("engine"),
                    samples: get("samples").parse().ok(),
                    denoise: match get("denoise").as_str() {
                        "1" => Some(true),
                        "0" => Some(false),
                        _ => None,
                    },
                    denoiser: get("denoiser"),
                };
                // Blender のアドオンで決めた、ランチャーで描く時だけの描き方
                let rs = RenderOver {
                    samples: get("over_samples").parse().ok(),
                    denoise: match get("over_denoise").as_str() {
                        "1" => Some(true),
                        "0" => Some(false),
                        _ => None,
                    },
                    denoiser: Some(get("over_denoiser")).filter(|d| !d.is_empty()),
                };
                if let Some(j) = jobs.iter_mut().find(|j| &j.inbox == name) {
                    // 描き方はいつもアドオンの値(ランチャーでは変えない)
                    j.rs = rs.clone();
                    if j.inbox_mtime != *mt {
                        // アドオンで範囲などを変えた・写しを取り直した
                        j.range = range;
                        j.camera = get("camera");
                        j.keep_output = get("keep_output") == "1";
                        j.scene = get("scene");
                        j.scene_rs = scene_rs;
                        j.inbox_mtime = *mt;
                    }
                    continue;
                }
                let binary = std::fs::canonicalize(get("binary")).ok();
                let b = builds.iter().find(|b| scan::blender_bin(b).and_then(|p| std::fs::canonicalize(p).ok()).is_some_and(|p| Some(p) == binary)).or(fallback);
                let Some(b) = b else { continue };
                let id = next_id(jobs);
                jobs.push(Job {
                    id,
                    blend: dir.join("scene.blend"),
                    source: PathBuf::from(get("source")),
                    build: b.key.clone(),
                    build_title: b.title.clone(),
                    scene: get("scene"),
                    range,
                    camera: get("camera"),
                    keep_output: get("keep_output") == "1",
                    on: true,
                    state: JobState::Wait,
                    progress: String::new(),
                    out: PathBuf::new(),
                    started: 0,
                    ended: 0,
                    inbox: name.clone(),
                    inbox_mtime: *mt,
                        lim: Limits::default(),
                    scene_rs,
                    rs,
                });
                added += 1;
            }
            // アドオンでチェックを外された物(フォルダが無くなった物)を外す
            let before = jobs.len();
            jobs.retain(|j| {
                let keep = j.inbox.is_empty() || seen.contains(&j.inbox);
                if !keep && Some(j.id) == cur {
                    stop_current = true;
                }
                keep
            });
            removed = before - jobs.len();
        });
        if stop_current {
            self.halt_current();
        }
        (added, removed)
    }

    /// 一覧から消す。アドオンの写しのフォルダも消す(アドオンはそれを見てチェックを外す)。出力の絵は残す
    pub fn remove(&self, id: u64) {
        if self.current.lock().ok().and_then(|c| *c) == Some(id) {
            self.halt_current();
        }
        let mut gone = None;
        self.edit(|js| {
            if let Some(i) = js.iter().position(|j| j.id == id) {
                let j = js.remove(i);
                if !j.inbox.is_empty() {
                    gone = Some(j.inbox);
                }
            }
        });
        if let Some(name) = gone {
            let d = inbox_dir().join(&name);
            if d.join("entry.tsv").is_file() {
                let _ = std::fs::remove_dir_all(d);
            }
        }
    }

    fn signal(&self, sig: &str) -> bool {
        match self.pid.lock().ok().and_then(|p| *p) {
            Some(pid) => Command::new("kill").arg(format!("-{sig}")).arg("--").arg(format!("-{pid}")).status().is_ok_and(|s| s.success()),
            None => false,
        }
    }

    pub fn pause(&self) {
        if self.signal("STOP") {
            self.set_current_state(JobState::Paused);
        }
    }

    pub fn resume(&self) {
        if self.signal("CONT") {
            self.set_current_state(JobState::Running);
        }
    }

    fn set_current_state(&self, s: JobState) {
        let cur = self.current.lock().ok().and_then(|c| *c);
        self.edit(|js| {
            if let Some(j) = js.iter_mut().find(|j| Some(j.id) == cur) {
                j.state = s;
            }
        });
    }

    /// 描いている最中の優先度を変える(下げる方だけ。上げるのは管理者の権限が要る)
    pub fn renice(&self, nice: i32) -> Result<(), String> {
        let Some(pid) = self.pid.lock().ok().and_then(|p| *p) else { return Ok(()) };
        let o = Command::new("renice").args(["-n", &nice.to_string(), "-g", &pid.to_string()]).output().map_err(|e| e.to_string())?;
        if o.status.success() {
            Ok(())
        } else {
            Err("優先度を上げる(数を小さくする)には管理者の権限が要るので、次の 1 本から効きます".into())
        }
    }

    /// 今の 1 本を止める(「途中」として残り、次に進める時は続きから)
    pub fn halt_current(&self) {
        self.halt.store(true, Ordering::SeqCst);
        self.signal("CONT");
        self.signal("TERM");
    }

    /// 進めるスイッチ。入れると、オンの行を上から描く。切ると今の 1 本も止める(続きは残る)
    pub fn set_run(&self, run: bool, builds: Vec<Build>, ctx: egui::Context) {
        if let Ok(mut s) = self.settings.lock() {
            s.run = run;
        }
        if run {
            self.kick(builds, ctx);
        } else {
            self.halt_current();
        }
    }

    /// 進めるスイッチが入っていて、描ける行があり、まだ動いていなければ動かす
    pub fn kick(&self, builds: Vec<Build>, ctx: egui::Context) {
        let run = self.settings.lock().map(|s| s.run).unwrap_or(false);
        let any = self.jobs.lock().map(|js| js.iter().any(Job::runnable)).unwrap_or(false);
        if !run || !any || self.running.swap(true, Ordering::SeqCst) {
            return;
        }
        let (jobs, running, halt, pid, settings, monitor, notice, current) = (self.jobs.clone(), self.running.clone(), self.halt.clone(), self.pid.clone(), self.settings.clone(), self.monitor.clone(), self.notice.clone(), self.current.clone());
        std::thread::spawn(move || {
            loop {
                if !settings.lock().map(|s| s.run).unwrap_or(false) {
                    break;
                }
                // VRAM の上限: ほかの物で既に上限に近い時は、始めずに待つ
                let next_lim = jobs.lock().ok().and_then(|js| js.iter().find(|j| j.runnable()).map(|j| j.lim)).unwrap_or_default();
                let lim = settings.lock().map(|s| s.with(&next_lim).vram_mib).unwrap_or(0);
                let used = monitor.lock().ok().and_then(|m| m.hist.back().map(|s| s.vram_used)).unwrap_or(0);
                if lim > 0 && used + 512 > lim {
                    if let Ok(mut n) = notice.lock() {
                        *n = format!("VRAM が {used}MiB 使われていて上限 {lim}MiB に近いので、次を始めずに待っています");
                    }
                    ctx.request_repaint();
                    std::thread::sleep(Duration::from_secs(3));
                    continue;
                }
                // 上から、オンで描ける物
                let next = jobs.lock().ok().and_then(|mut js| {
                    let j = js.iter_mut().find(|j| j.runnable())?;
                    j.state = JobState::Running;
                    j.started = now();
                    j.ended = 0;
                    if j.out.as_os_str().is_empty() {
                        j.out = out_dir(j);
                    }
                    j.progress = "起こしています".into();
                    let c = j.clone();
                    save(&js);
                    Some(c)
                });
                let Some(job) = next else { break };
                if let Ok(mut n) = notice.lock() {
                    n.clear();
                }
                if let Ok(mut c) = current.lock() {
                    *c = Some(job.id);
                }
                halt.store(false, Ordering::SeqCst);
                ctx.request_repaint();
                let s = settings.lock().map(|s| s.with(&job.lim)).unwrap_or_default();
                let (state, last) = run_one(&job, &builds, &s, &pid, &jobs, &ctx);
                let state = if halt.load(Ordering::SeqCst) && state != JobState::Done { JobState::Partial } else { state };
                if let Ok(mut c) = current.lock() {
                    *c = None;
                }
                if let Ok(mut js) = jobs.lock() {
                    if let Some(j) = js.iter_mut().find(|j| j.id == job.id) {
                        j.state = state;
                        j.ended = now();
                        j.progress = last;
                    }
                    save(&js);
                }
                ctx.request_repaint();
                // 失敗が続いて空回りしないよう、少し間をあける
                if state == JobState::Failed {
                    std::thread::sleep(Duration::from_secs(1));
                }
            }
            running.store(false, Ordering::SeqCst);
            ctx.request_repaint();
        });
    }

    /// 1 秒おきに CPU・メモリ・GPU・VRAM と、描いている物を測る(この頁を見ている時か、描いている時だけ)
    fn start_monitor(&self, ctx: egui::Context) {
        let (monitor, pid, watched, running, settings, jobs, notice, current) = (self.monitor.clone(), self.pid.clone(), self.watched.clone(), self.running.clone(), self.settings.clone(), self.jobs.clone(), self.notice.clone(), self.current.clone());
        std::thread::spawn(move || {
            let mut prev = read_cpu();
            let mut prev_job: Option<(u32, u64)> = None;
            let ncpu = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) as f32;
            loop {
                std::thread::sleep(Duration::from_millis(1000));
                let seen = watched.lock().map(|w| w.elapsed() < Duration::from_secs(5)).unwrap_or(false);
                if !seen && !running.load(Ordering::SeqCst) {
                    continue;
                }
                let cur = read_cpu();
                let (dt, di) = (cur.0.saturating_sub(prev.0), cur.1.saturating_sub(prev.1));
                prev = cur;
                let (ram_used, ram_total) = read_mem();
                let (gpu, vram_used, vram_total, per) = nvidia();
                let mut s = Sample { cpu: if dt > 0 { 100.0 * (1.0 - di as f32 / dt as f32) } else { 0.0 }, ram_used, ram_total, gpu, vram_used, vram_total, ..Default::default() };
                if let Some(p) = pid.lock().ok().and_then(|p| *p) {
                    let (t, rss, pids) = group_stat(p);
                    if let Some((_, pt)) = prev_job.filter(|(pp, _)| *pp == p) {
                        // 全体の何 % か(1 秒あたり 100 jiffies、CPU の本数で割る)
                        s.job_cpu = t.saturating_sub(pt) as f32 / ncpu;
                    }
                    prev_job = Some((p, t));
                    s.job_rss = rss;
                    s.job_vram = pids.iter().filter_map(|x| per.get(x)).sum();
                    // VRAM の上限を超えたら止める(途中として残る)
                    let cur_lim = current.lock().ok().and_then(|c| *c).and_then(|c| jobs.lock().ok().and_then(|js| js.iter().find(|j| j.id == c).map(|j| j.lim))).unwrap_or_default();
                    let lim = settings.lock().map(|s| s.with(&cur_lim).vram_mib).unwrap_or(0);
                    if lim > 0 && s.vram_used > lim {
                        let _ = Command::new("kill").arg("-CONT").arg("--").arg(format!("-{p}")).status();
                        let _ = Command::new("kill").arg("-TERM").arg("--").arg(format!("-{p}")).status();
                        let c = current.lock().ok().and_then(|c| *c);
                        if let Ok(mut js) = jobs.lock() {
                            if let Some(j) = js.iter_mut().find(|j| Some(j.id) == c) {
                                j.progress = format!("VRAM が全体で {}MiB になり、上限 {lim}MiB を超えたので止めました", s.vram_used);
                            }
                        }
                        if let Ok(mut n) = notice.lock() {
                            *n = format!("VRAM が上限 {lim}MiB を超えたので、描いていた物を止めました(スイッチを切りました)");
                        }
                        if let Ok(mut st) = settings.lock() {
                            st.run = false;
                        }
                    }
                } else {
                    prev_job = None;
                }
                if let Ok(mut m) = monitor.lock() {
                    m.hist.push_back(s);
                    while m.hist.len() > 120 {
                        m.hist.pop_front();
                    }
                }
                ctx.request_repaint();
            }
        });
    }
}

fn next_id(jobs: &[Job]) -> u64 {
    jobs.iter().map(|j| j.id).max().unwrap_or(0).max(now() as u64 * 100) + 1
}

fn run_one(job: &Job, builds: &[Build], s: &Settings, pid: &Arc<Mutex<Option<u32>>>, jobs: &Arc<Mutex<Vec<Job>>>, ctx: &egui::Context) -> (JobState, String) {
    let Some(b) = builds.iter().find(|b| b.key == job.build) else { return (JobState::Failed, "ビルドが見つかりません".into()) };
    let Some(bin) = scan::blender_bin(b) else { return (JobState::Failed, "blender がありません".into()) };
    if !job.blend.is_file() {
        return (JobState::Failed, format!("{} がありません", job.blend.display()));
    }
    let _ = std::fs::create_dir_all(&job.out);
    let _ = std::fs::create_dir_all(config_dir().join("予約"));
    // ログは続きから描くたびに足していく
    let Ok(mut log) = std::fs::OpenOptions::new().create(true).append(true).open(job.log()) else { return (JobState::Failed, "ログを作れません".into()) };
    // 包む順: メモリの上限(systemd のスコープ)→ 優先度(nice)→ blender。どれも exec するので pid は blender のまま
    let mut argv: Vec<String> = vec![];
    if s.ram_gb > 0.0 {
        argv.extend(["systemd-run", "--user", "--scope", "-q", "-p"].map(String::from));
        argv.push(format!("MemoryMax={}M", (s.ram_gb * 1024.0) as u64));
        argv.extend(["-p", "MemorySwapMax=0", "--"].map(String::from));
    }
    if s.nice > 0 {
        argv.extend(["nice".into(), "-n".into(), s.nice.to_string()]);
    }
    argv.push(bin.display().to_string());
    argv.extend(["-b".into(), job.blend.display().to_string()]);
    if !job.scene.is_empty() {
        argv.extend(["-S".into(), job.scene.clone()]);
    }
    // 続きから描くため、描いてある枚は上書きしない
    let mut expr = String::from("import bpy\nr=bpy.context.scene.render\nr.use_overwrite=False\nr.use_placeholder=False\n");
    if !job.camera.is_empty() {
        let cam = job.camera.replace('\\', "\\\\").replace('\'', "\\'");
        expr.push_str(&format!("c=bpy.data.objects.get('{cam}')\nif c: bpy.context.scene.camera=c\n"));
    }
    // シーンごとの描き方(サンプル数・デノイザー)
    expr.push_str(&job.rs.python());
    argv.extend(["--python-expr".into(), expr]);
    if s.threads > 0 {
        argv.extend(["-t".into(), s.threads.to_string()]);
    }
    let scene = if job.scene.is_empty() { Path::new(&job.name()).file_stem().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default() } else { job.scene.clone() };
    if !job.keep_output {
        argv.extend(["-o".into(), job.out.join(format!("{}_####", scan::safe_key(&scene))).display().to_string(), "-F".into(), "PNG".into()]);
    }
    match job.range {
        Some((a, e, _)) if a == e => argv.extend(["-f".into(), a.to_string()]),
        Some((a, e, st)) => argv.extend(["-s".into(), a.to_string(), "-e".into(), e.to_string(), "-j".into(), st.max(1).to_string(), "-a".into()]),
        None => argv.push("-a".into()),
    }
    let _ = writeln!(log, "# {} {}", when(now()), argv.join(" "));
    let mut cmd = Command::new(&argv[0]);
    cmd.args(&argv[1..]);
    for (k, v) in scan::blender_env(b) {
        cmd.env(k, v);
    }
    use std::os::unix::process::CommandExt;
    cmd.process_group(0).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return (JobState::Failed, format!("起こせません: {e}")),
    };
    if let Ok(mut p) = pid.lock() {
        *p = Some(child.id());
    }
    if let Some(err) = child.stderr.take() {
        let path = job.log().with_extension("err.log");
        std::thread::spawn(move || {
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
                for l in BufReader::new(err).lines().map_while(Result::ok) {
                    let _ = writeln!(f, "{l}");
                }
            }
        });
    }
    let total = job.count();
    let mut last = String::new();
    if let Some(out) = child.stdout.take() {
        let mut shown = Instant::now();
        for l in BufReader::new(out).lines().map_while(Result::ok) {
            let _ = writeln!(log, "{l}");
            if l.starts_with("Fra:") || l.contains("Saved:") || l.contains("Error") || l.contains("skipping") {
                last = l.chars().take(160).collect();
                if shown.elapsed() > Duration::from_millis(300) {
                    shown = Instant::now();
                    if let Ok(mut js) = jobs.lock() {
                        if let Some(j) = js.iter_mut().find(|j| j.id == job.id) {
                            j.progress = last.clone();
                        }
                    }
                    ctx.request_repaint();
                }
            }
        }
    }
    let ok = child.wait().map(|s| s.success()).unwrap_or(false);
    if let Ok(mut p) = pid.lock() {
        *p = None;
    }
    let killed = jobs.lock().ok().and_then(|js| js.iter().find(|j| j.id == job.id).map(|j| j.progress.clone())).filter(|p| p.contains("上限"));
    if let Some(k) = killed {
        return (JobState::Partial, k);
    }
    let done = job.done_frames();
    let full = total.is_none_or(|t| done as i32 >= t) || job.keep_output;
    if ok && full {
        (JobState::Done, format!("{done} 枚"))
    } else if ok {
        (JobState::Partial, format!("{done} 枚まで(終わりましたが、範囲の枚数に足りません。ログを見てください)"))
    } else {
        (JobState::Failed, if last.is_empty() { "途中で止まりました(ログを見てください)".into() } else { last })
    }
}

/// アドオンを Blender の設定のアドオンの所へ写す(使う人が押した時だけ)。入れた版の一覧を返す
pub fn install_addon() -> Vec<String> {
    let home = std::env::var("HOME").unwrap_or_default();
    let mut done = vec![];
    for e in std::fs::read_dir(format!("{home}/.config/blender")).into_iter().flatten().flatten() {
        let v = e.file_name().to_string_lossy().into_owned();
        let ok = v.split('.').next().and_then(|m| m.parse::<u32>().ok()).is_some_and(|m| m >= 3) || v == "2.93";
        if !ok {
            continue;
        }
        let dir = e.path().join("scripts/addons");
        if std::fs::create_dir_all(&dir).is_ok() && std::fs::write(dir.join("falcon_launcher_link.py"), ADDON_PY).is_ok() {
            done.push(v);
        }
    }
    done.sort();
    done
}

#[cfg(test)]
mod tests {
    use super::*;
    /// 描く時に足す Python を書き出す(FALCON_RS_OUT に置く。Blender で通るかは手で確かめる)
    #[test]
    fn render_over_python() {
        let r = RenderOver { samples: Some(7), denoise: Some(true), denoiser: Some("DLSS".into()) };
        let k = r.key();
        assert!(RenderOver::from_key(&k) == r);
        assert!(RenderOver::from_key("-").is_empty());
        let sr = SceneRender { engine: "NEURAL_RENDERING".into(), samples: Some(128), denoise: Some(false), denoiser: "OPTIX".into() };
        assert!(SceneRender::from_key(&sr.key()) == sr);
        if let Ok(p) = std::env::var("FALCON_RS_OUT") {
            std::fs::write(p, format!("import bpy\n{}", r.python())).unwrap();
        }
    }
}
