//! Falcon Launcher。ビルドと Blender の版を見やすく管理して起動する、Rust + egui のデスクトップアプリ。
//! 画面はビルドのカード・更新履歴・更新詳細を元にした。ワークスペース・チームの札は入れない(個人用)。
//! 読むだけの物: ビルド先の 建てた.txt・.building・build.log、git の枝と木。
//! 動かす物: 起動(本体・古い版の .desktop の Exec)・フォルダを開く(xdg-open)・試験を建てる(build.sh)。
//! 置き場は places.rs(~/.config/falcon-launcher/場所.tsv。11-17 公開のため決め打ちをやめた)。
mod page;
mod places;
mod prep;
mod queue;
mod scan;
mod setup;
mod shell;
mod start;

use eframe::egui;
use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Vec2};
use scan::{Album, Build, Change, ChangeDetail, ChangeKind, Feature, Kind, Media, Scene, Tree};
use start::{BlendFile, StartTab};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

// ---------- 色(11-16 自前の色へ替えた) ----------
const BG: Color32 = Color32::from_rgb(0x1b, 0x1d, 0x21);
const PANEL: Color32 = Color32::from_rgb(0x1b, 0x1d, 0x21);
const CARD: Color32 = Color32::from_rgb(0x27, 0x29, 0x2e);
const CARD_HI: Color32 = Color32::from_rgb(0x33, 0x36, 0x3d);
const LINE: Color32 = Color32::from_rgb(0x30, 0x33, 0x39);
const TEXT: Color32 = Color32::from_rgb(0xec, 0xee, 0xf0);
const MUTED: Color32 = Color32::from_rgb(0xb0, 0xb6, 0xbf);
const FAINT: Color32 = Color32::from_rgb(0x86, 0x8c, 0x96);
const ACCENT: Color32 = Color32::from_rgb(0x1b, 0xd9, 0x6a);
const ACCENT_TEXT: Color32 = Color32::from_rgb(0x1b, 0xd9, 0x6a);
const ON_ACCENT: Color32 = Color32::from_rgb(0x00, 0x00, 0x00);
const ORANGE: Color32 = Color32::from_rgb(0xff, 0xa3, 0x47);
const LINK: Color32 = Color32::from_rgb(0x6c, 0xb0, 0xf7);
const OK: Color32 = Color32::from_rgb(0x5c, 0xd6, 0x9a);
const WARN: Color32 = Color32::from_rgb(0xf5, 0xb8, 0x4b);
const ERR: Color32 = Color32::from_rgb(0xf8, 0x71, 0x71);

// 絵の字(JetBrainsMono Nerd Font)
const I_GRID: &str = "\u{f00a}";
const I_LOG: &str = "\u{f120}";
const I_PLAY: &str = "\u{f04b}";
const I_FOLDER: &str = "\u{f07b}";
const I_BRANCH: &str = "\u{e725}";
const I_BUILD: &str = "\u{f0ad}";
const I_REFRESH: &str = "\u{f021}";
const I_SEARCH: &str = "\u{f002}";
const I_CUBE: &str = "\u{f1b2}";
const I_MORE: &str = "\u{f142}";
const I_COMMIT_SMALL: &str = "\u{f417}";
const I_CAL: &str = "\u{f073}";
const I_DESKTOP: &str = "\u{f108}";
const I_LINUX: &str = "\u{f17c}";
const I_LICENSE: &str = "\u{f24e}";
const I_USER: &str = "\u{f007}";
const I_LEFT: &str = "\u{f053}";
const I_HEART: &str = "\u{f004}";
const I_HEART_O: &str = "\u{f08a}";
const I_DOWN: &str = "\u{f078}";
const I_RIGHT: &str = "\u{f054}";
const I_FILTER: &str = "\u{f0b0}";
const I_EXT: &str = "\u{f08e}";
const I_HDD: &str = "\u{f0a0}";
const I_CLOCK: &str = "\u{f017}";
const I_PICTURE: &str = "\u{f03e}";
const I_FILE: &str = "\u{f15b}";
/// 割り当て(つまみの並び)
const I_SLIDERS: &str = "\u{f1de}";

const FONTS: [(&str, &str, bool, bool); 5] = [
    ("jbmono", "JetBrainsMonoNerdFont-Regular.ttf", false, true),
    ("inter", "Inter-Regular.ttf", true, false),
    ("plexjp", "IBMPlexSansJP-Regular.ttf", true, true),
    // 絵の字は Propo(字の幅に合わせた形)。無印は絵が字の幅より右へはみ出し、列の真ん中から 3〜5px 右にずれる
    // Noto を絵の字より先に置く(Inter・IBM Plex が無い PC で、英字が等幅の字体で出ないように)
    ("noto", "NotoSansCJK-Regular.ttc", true, true),
    ("nerd", "JetBrainsMonoNerdFontPropo-Regular.ttf", true, false),
];

const BOLD_FONTS: [(&str, &str); 2] = [
    ("inter-bold", "Inter-Bold.ttf"),
    ("plexjp-bold", "IBMPlexSansJP-Bold.ttf"),
];

pub fn tz_offset() -> i64 {
    static OFF: OnceLock<i64> = OnceLock::new();
    *OFF.get_or_init(|| {
        let s = Command::new("date").arg("+%z").output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
        let sign = if s.starts_with('-') { -1 } else { 1 };
        let d: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
        let (h, m) = (d.get(0..2).and_then(|x| x.parse::<i64>().ok()).unwrap_or(9), d.get(2..4).and_then(|x| x.parse::<i64>().ok()).unwrap_or(0));
        sign * (h * 3600 + m * 60)
    })
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// 「今日 14:20」「昨日 18:05」「11-06 16:32」
fn when(ts: i64) -> String {
    if ts <= 0 {
        return "-".into();
    }
    let off = tz_offset();
    let (l, n) = (ts + off, now() + off);
    let hm = format!("{:02}:{:02}", l.rem_euclid(86400) / 3600, l.rem_euclid(3600) / 60);
    match n.div_euclid(86400) - l.div_euclid(86400) {
        0 => format!("今日 {hm}"),
        1 => format!("昨日 {hm}"),
        _ => {
            let (_, m, d) = civil(l.div_euclid(86400));
            format!("{m:02}-{d:02} {hm}")
        }
    }
}

/// 「今日」「昨日」「3 日前」「2 か月前」
fn ago(ts: i64) -> String {
    if ts <= 0 {
        return "-".into();
    }
    let off = tz_offset();
    let days = (now() + off).div_euclid(86400) - (ts + off).div_euclid(86400);
    match days {
        ..=0 => "今日".into(),
        1 => "昨日".into(),
        2..=29 => format!("{days} 日前"),
        30..=364 => format!("{} か月前", days / 30),
        _ => format!("{} 年前", days / 365),
    }
}

fn gb(bytes: u64) -> String {
    format!("{:.2} GB", bytes as f64 / 1e9)
}

fn config_dir() -> PathBuf {
    places::config_dir()
}

#[derive(Clone, Copy, PartialEq)]
enum View {
    /// ホーム(続きから・版・最近の絵)
    Dash,
    /// 版の頁
    Start,
    Renders,
    Queue,
    Home,
    Trees,
    Log,
    /// 設定
    Settings,
    /// 建てる準備(要る道具と SDK)
    Prep,
}

#[derive(Clone, Copy, PartialEq)]
enum ChangeFilter {
    All,
    Merge,
    Commit,
    Built,
    Version,
}

impl ChangeFilter {
    fn text(self) -> &'static str {
        match self {
            Self::All => "すべての更新",
            Self::Merge => "統合",
            Self::Commit => "コミット",
            Self::Built => "ビルド",
            Self::Version => "版",
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    All,
    Project,
    Test,
    Old,
    Fav,
}

/// 機能のタブの中の一覧
#[derive(Clone, Copy, PartialEq)]
enum FeatTab {
    Features,
    Tests,
    Old,
}

#[derive(Clone, Copy, PartialEq)]
enum Sort {
    New,
    Name,
}

#[derive(Clone, Copy, PartialEq)]
enum StateFilter {
    All,
    Ready,
    Building,
    NotBuilt,
    Failed,
}

impl StateFilter {
    fn text(self) -> &'static str {
        match self {
            Self::All => "すべての状態",
            Self::Ready => "起動できる",
            Self::Building => "ビルド中",
            Self::NotBuilt => "未建て",
            Self::Failed => "建てて失敗",
        }
    }
}

enum Action {
    Open(String),
    Launch(String),
    Folder(PathBuf),
    Build(String),
    Fav(String),
    ShowLog(String),
    Play(PathBuf),
    Toggle(String, String),
    LaunchFeature(String, String),
    LaunchScene(Scene),
}

struct Heavy {
    updated: HashMap<String, i64>,
    aheads: HashMap<String, Option<u32>>,
    disk: Option<(u64, u64)>,
    main_branch_now: String,
}

struct App {
    view: View,
    /// 戻る・進む
    nav_back: Vec<shell::NavState>,
    nav_fwd: Vec<shell::NavState>,
    nav_last: Option<shell::NavState>,
    nav_jump: bool,
    /// .blend の探す欄と、版の絞り込み
    blend_query: String,
    blend_ver: Option<String>,
    /// レンダリング結果の並び・まとめ方・閉じた見出し
    render_old_first: bool,
    render_by_album: bool,
    render_closed: HashSet<String>,
    /// ホームで選んでいるビルド・タブ・最近の .blend と、その小さな絵の係
    sel: String,
    start_tab: StartTab,
    blends: Option<Arc<Vec<BlendFile>>>,
    blend_tx: Sender<(PathBuf, PathBuf)>,
    blend_thumbs: Arc<Mutex<HashMap<PathBuf, Option<PathBuf>>>>,
    blend_asked: HashSet<PathBuf>,
    /// レンダリング予約と、その足す欄
    queue: queue::Runner,
    queue_input: String,
    /// ホームの「.blend のパスを貼る」欄
    path_paste: String,
    /// 一覧の探す欄・絞り込み(0 すべて 1 オン 2 オフ 3 まだ 4 済み)・選んだ行
    queue_query: String,
    queue_filter: u8,
    queue_sel: HashSet<u64>,
    /// ファイル構造で広げたフォルダと、読んだ中身
    fs_open: HashSet<PathBuf>,
    fs_cache: HashMap<PathBuf, Vec<(PathBuf, bool)>>,
    page: String,
    tab: Tab,
    feat_tab: FeatTab,
    sort: Sort,
    pages: HashMap<&'static str, usize>,
    features: Arc<Vec<Feature>>,
    features_rx: Option<Receiver<Vec<Feature>>>,
    scenes: Vec<Scene>,
    updated: HashMap<String, i64>,
    category: Option<&'static str>,
    query: String,
    state_filter: StateFilter,
    builds: Vec<Build>,
    running: HashSet<PathBuf>,
    last_light: Instant,
    heavy_rx: Option<Receiver<Heavy>>,
    last_heavy: Instant,
    aheads: HashMap<String, Option<u32>>,
    disk: Option<(u64, u64)>,
    main_branch_now: String,
    scanned_at: i64,
    changes: HashMap<String, Arc<Vec<Change>>>,
    changes_asked: HashSet<String>,
    changes_tx: Sender<(String, Vec<Change>)>,
    changes_rx: Receiver<(String, Vec<Change>)>,
    open_changes: HashSet<String>,
    change_filter: ChangeFilter,
    details: HashMap<String, ChangeDetail>,
    about: HashMap<String, String>,
    gallery: Arc<Vec<Album>>,
    gallery_rx: Option<Receiver<Vec<Album>>>,
    gallery_query: String,
    album_open: Option<PathBuf>,
    lightbox: Option<(Vec<Media>, usize, PathBuf)>,
    thumb_tx: Sender<(PathBuf, bool)>,
    thumb_done: Arc<Mutex<HashMap<PathBuf, Option<PathBuf>>>>,
    thumb_asked: HashSet<PathBuf>,
    /// 連番を動画にする係へ渡す口と、済んだ・できなかった物
    seq_tx: Sender<Media>,
    seq_done: Arc<Mutex<HashMap<PathBuf, bool>>>,
    seq_asked: HashSet<PathBuf>,
    favorites: HashSet<String>,
    sizes: HashMap<String, (i64, u64)>,
    size_rx: Option<Receiver<(String, i64, u64)>>,
    trees: Vec<Tree>,
    trees_rx: Option<Receiver<Tree>>,
    tree_filter: String,
    confirm_build: Option<String>,
    /// 建てる準備: 確かめた結果・置いた場所・書きかけの場所
    prep_checks: Vec<prep::Check>,
    prep_paths: HashMap<String, String>,
    prep_edit: HashMap<String, String>,
    /// 落とした物の受け取り(展開は別の糸)・端末で打ってもらう命令・貼ったパス
    prep_rx: Option<Receiver<Result<prep::Taken, String>>>,
    prep_cmd: Option<(String, String)>,
    prep_paste: String,
    /// 取って建てる・更新(setup.rs): 置き場の版の見回り・確かめの画面・走っている物
    remote: setup::Remote,
    remote_rx: Option<Receiver<Result<Vec<String>, String>>>,
    setup_confirm: Option<setup::Plan>,
    setup_status: setup::Status,
    setup_running: bool,
    setup_info: setup::Info,
    setup_last: Instant,
    message: String,
    log_for: String,
    log_text: String,
    log_read: Instant,
}

impl App {
    fn new(ctx: &egui::Context) -> Self {
        let favorites = std::fs::read_to_string(config_dir().join("お気に入り.txt")).unwrap_or_default().lines().map(str::to_string).filter(|s| !s.is_empty()).collect();
        let sizes = std::fs::read_to_string(config_dir().join("大きさ.tsv"))
            .unwrap_or_default()
            .lines()
            .filter_map(|l| {
                let f: Vec<&str> = l.split('\t').collect();
                (f.len() == 3).then(|| (f[0].to_string(), (f[1].parse().unwrap_or(0), f[2].parse().unwrap_or(0))))
            })
            .collect();
        let (changes_tx, changes_rx) = channel();
        let (thumb_tx, thumb_rx) = channel::<(PathBuf, bool)>();
        let thumb_rx = Arc::new(Mutex::new(thumb_rx));
        let thumb_done = Arc::new(Mutex::new(HashMap::new()));
        for _ in 0..2 {
            let (rx, done, ctx) = (thumb_rx.clone(), thumb_done.clone(), ctx.clone());
            std::thread::spawn(move || loop {
                let Ok((path, video)) = rx.lock().map_err(|_| ()).and_then(|r| r.recv().map_err(|_| ())) else { return };
                let t = scan::make_thumb(&path, video);
                if let Ok(mut d) = done.lock() {
                    d.insert(path, t);
                }
                ctx.request_repaint();
            });
        }
        // 連番を動画にする係は 1 本だけ(NVENC を並べない)
        let (seq_tx, seq_rx) = channel::<Media>();
        let seq_done = Arc::new(Mutex::new(HashMap::new()));
        {
            let (done, ctx) = (seq_done.clone(), ctx.clone());
            std::thread::spawn(move || {
                while let Ok(m) = seq_rx.recv() {
                    let ok = scan::encode_seq(&m);
                    if let Ok(mut d) = done.lock() {
                        d.insert(m.path.clone(), ok);
                    }
                    ctx.request_repaint();
                }
            });
        }
        // .blend の小さな絵の係(blender-thumbnailer を 1 本ずつ)
        let (blend_tx, blend_rx) = channel::<(PathBuf, PathBuf)>();
        let blend_thumbs = Arc::new(Mutex::new(HashMap::new()));
        {
            let (done, ctx) = (blend_thumbs.clone(), ctx.clone());
            std::thread::spawn(move || {
                while let Ok((tool, path)) = blend_rx.recv() {
                    let t = start::blend_thumb(&tool, &path);
                    if let Ok(mut d) = done.lock() {
                        d.insert(path, t);
                    }
                    ctx.request_repaint();
                }
            });
        }
        let mut app = Self {
            view: View::Dash,
            nav_back: vec![],
            nav_fwd: vec![],
            nav_last: None,
            nav_jump: false,
            blend_query: String::new(),
            blend_ver: None,
            render_old_first: false,
            render_by_album: false,
            render_closed: HashSet::new(),
            // 今の本線は v5(11-16)。無ければ v4 の本線
            sel: if Path::new(&places::places().falcon_build).join("bin/blender").is_file() || places::places().project.is_empty() { "v5".into() } else { "host".into() },
            start_tab: StartTab::Blend,
            blends: None,
            blend_tx,
            blend_thumbs,
            blend_asked: HashSet::new(),
            queue: queue::Runner::new(ctx),
            queue_input: String::new(),
            path_paste: String::new(),
            queue_query: String::new(),
            queue_filter: 0,
            queue_sel: HashSet::new(),
            fs_open: HashSet::new(),
            fs_cache: HashMap::new(),
            page: "host".into(),
            tab: Tab::All,
            feat_tab: FeatTab::Features,
            sort: Sort::New,
            pages: HashMap::new(),
            features: Arc::new(vec![]),
            features_rx: None,
            scenes: scan::scenes(),
            updated: HashMap::new(),
            category: None,
            query: String::new(),
            state_filter: StateFilter::All,
            builds: scan::builds(),
            running: scan::running_bins(),
            last_light: Instant::now(),
            heavy_rx: None,
            last_heavy: Instant::now(),
            aheads: HashMap::new(),
            disk: None,
            main_branch_now: String::new(),
            scanned_at: now(),
            changes: HashMap::new(),
            changes_asked: HashSet::new(),
            changes_tx,
            changes_rx,
            open_changes: HashSet::new(),
            change_filter: ChangeFilter::All,
            details: HashMap::new(),
            about: HashMap::new(),
            gallery: Arc::new(vec![]),
            gallery_rx: None,
            gallery_query: String::new(),
            album_open: None,
            lightbox: None,
            thumb_tx,
            thumb_done,
            seq_tx,
            seq_done,
            seq_asked: HashSet::new(),
            thumb_asked: HashSet::new(),
            favorites,
            sizes,
            size_rx: None,
            trees: vec![],
            trees_rx: None,
            tree_filter: String::new(),
            confirm_build: None,
            prep_checks: vec![],
            prep_paths: prep::load_paths(),
            prep_edit: HashMap::new(),
            prep_rx: None,
            prep_cmd: None,
            prep_paste: String::new(),
            remote: setup::Remote::default(),
            remote_rx: None,
            setup_confirm: None,
            setup_status: setup::Status::default(),
            setup_running: setup::running_pid().is_some(),
            setup_info: setup::info(),
            setup_last: Instant::now() - Duration::from_secs(10),
            message: String::new(),
            log_for: "host".into(),
            log_text: String::new(),
            log_read: Instant::now() - Duration::from_secs(10),
        };
        app.start_heavy(ctx);
        app.start_sizes(ctx);
        app
    }

    fn start_heavy(&mut self, ctx: &egui::Context) {
        let (tx, rx) = channel();
        let builds = self.builds.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let aheads = builds.iter().filter(|b| b.kind != Kind::Old).map(|b| (b.key.clone(), scan::ahead(&b.branch, &b.hash))).collect();
            // 下の帯の空き: 場所.tsv の disk(無ければ Falcon の建て先)。まだ無い場所は、在る一番近い親で見る
            let mut probe = PathBuf::from(&places::places().disk);
            while !probe.exists() && probe.pop() {}
            let disk = scan::disk(&probe.display().to_string());
            let main_branch_now = Command::new("git").args(["-C", scan::src_dir(), "branch", "--show-current"]).output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
            let updated = scan::updated(&builds);
            let _ = tx.send(Heavy { updated, aheads, disk, main_branch_now });
            ctx.request_repaint();
        });
        self.heavy_rx = Some(rx);
        self.last_heavy = Instant::now();
    }

    /// 建てた時刻が覚えている物と違うビルド先だけ du で測る
    fn start_sizes(&mut self, ctx: &egui::Context) {
        let want: Vec<(String, PathBuf, i64)> = self
            .builds
            .iter()
            .filter(|b| b.has_bin && b.building.is_none() && b.dir.is_dir() && self.sizes.get(&b.key).map(|s| s.0) != Some(b.built_ts))
            .map(|b| (b.key.clone(), b.dir.clone(), b.built_ts))
            .collect();
        if want.is_empty() {
            return;
        }
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            for (key, dir, ts) in want {
                if let Some(n) = scan::dir_size(&dir) {
                    if tx.send((key, ts, n)).is_err() {
                        return;
                    }
                    ctx.request_repaint();
                }
            }
        });
        self.size_rx = Some(rx);
    }

    fn save_sizes(&self) {
        let _ = std::fs::create_dir_all(config_dir());
        let text: String = self.sizes.iter().map(|(k, (t, n))| format!("{k}\t{t}\t{n}\n")).collect();
        let _ = std::fs::write(config_dir().join("大きさ.tsv"), text);
    }

    fn save_favorites(&self) {
        let _ = std::fs::create_dir_all(config_dir());
        let mut v: Vec<&String> = self.favorites.iter().collect();
        v.sort();
        let text: String = v.iter().map(|s| format!("{s}\n")).collect();
        let _ = std::fs::write(config_dir().join("お気に入り.txt"), text);
    }

    fn poll(&mut self, ctx: &egui::Context) {
        self.poll_setup(ctx);
        if let Some(rx) = &self.heavy_rx {
            if let Ok(h) = rx.try_recv() {
                self.updated = h.updated;
                self.aheads = h.aheads;
                self.disk = h.disk;
                self.main_branch_now = h.main_branch_now;
                self.scanned_at = now();
                self.heavy_rx = None;
            }
        }
        if let Some(rx) = &self.features_rx {
            if let Ok(f) = rx.try_recv() {
                self.features = Arc::new(f);
                self.features_rx = None;
            }
        }
        while let Ok((k, v)) = self.changes_rx.try_recv() {
            self.changes.insert(k, Arc::new(v));
        }
        if let Some(rx) = &self.gallery_rx {
            if let Ok(g) = rx.try_recv() {
                // まだ動画になっていない連番を、新しい組から順に係へ渡す
                for m in g.iter().flat_map(|a| a.items.iter()).filter(|m| !m.frames.is_empty() && !m.path.is_file()) {
                    if self.seq_asked.insert(m.path.clone()) {
                        let _ = self.seq_tx.send(m.clone());
                    }
                }
                self.gallery = Arc::new(g);
                self.gallery_rx = None;
            }
        }
        let mut got = false;
        if let Some(rx) = &self.size_rx {
            while let Ok((k, t, n)) = rx.try_recv() {
                self.sizes.insert(k, (t, n));
                got = true;
            }
        }
        if got {
            self.save_sizes();
        }
        if let Some(rx) = &self.trees_rx {
            loop {
                match rx.try_recv() {
                    Ok(t) => self.trees.push(t),
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        self.trees_rx = None;
                        break;
                    }
                }
            }
        }
        // 軽い見回り(3 秒): ビルド先の状態と、動いている blender
        if self.last_light.elapsed() > Duration::from_secs(3) {
            let was_building: HashSet<String> = self.builds.iter().filter(|b| b.building.is_some()).map(|b| b.key.clone()).collect();
            self.builds = scan::builds();
            self.running = scan::running_bins();
            self.last_light = Instant::now();
            let finished = self.builds.iter().any(|b| was_building.contains(&b.key) && b.building.is_none());
            if finished {
                self.start_heavy(ctx);
                self.start_sizes(ctx);
            }
        }
        // 重い見回り(60 秒): 履歴・枝の先・ディスク
        if self.heavy_rx.is_none() && self.last_heavy.elapsed() > Duration::from_secs(60) {
            self.start_heavy(ctx);
        }
    }

    fn refresh_all(&mut self, ctx: &egui::Context) {
        self.builds = scan::builds();
        self.running = scan::running_bins();
        self.last_light = Instant::now();
        self.start_heavy(ctx);
        self.start_sizes(ctx);
        self.changes.clear();
        self.changes_asked.clear();
        self.about.clear();
        self.scenes = scan::scenes();
        self.features = Arc::new(vec![]);
        self.start_gallery(ctx);
        if self.view == View::Trees {
            self.start_trees(ctx);
        }
    }

    fn start_trees(&mut self, ctx: &egui::Context) {
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || scan::trees(tx, ctx));
        self.trees.clear();
        self.trees_rx = Some(rx);
    }

    fn is_running(&self, b: &Build) -> bool {
        !b.dir.as_os_str().is_empty() && self.running.iter().any(|p| p.starts_with(&b.dir))
    }

    /// 探す言葉・状態・分け方で絞って並べる
    fn filtered(&self, keep: impl Fn(&Build) -> bool) -> Vec<Build> {
        let q = self.query.to_lowercase();
        let mut v: Vec<Build> = self
            .builds
            .iter()
            .filter(|b| keep(b))
            .filter(|b| self.category.is_none_or(|c| b.category == c))
            .filter(|b| q.is_empty() || b.title.to_lowercase().contains(&q) || b.branch.to_lowercase().contains(&q) || b.what.to_lowercase().contains(&q))
            .filter(|b| match self.state_filter {
                StateFilter::All => true,
                StateFilter::Ready => b.launchable(),
                StateFilter::Building => b.building.is_some(),
                StateFilter::NotBuilt => !b.has_bin && b.building.is_none(),
                StateFilter::Failed => b.ok == Some(false),
            })
            .cloned()
            .collect();
        match self.sort {
            Sort::Name => v.sort_by(|a, b| a.title.cmp(&b.title)),
            // 本線と外の物 → ビルド中 → 建った物(新しい順)→ 未建て(名前順)
            Sort::New => v.sort_by(|a, b| {
                let rank = |x: &Build| (!matches!(x.kind, Kind::Main | Kind::Ext), x.kind == Kind::Ext, x.building.is_none(), !x.has_bin, -x.built_ts);
                rank(a).cmp(&rank(b)).then_with(|| a.title.cmp(&b.title))
            }),
        }
        v
    }

    fn apply(&mut self, actions: Vec<Action>) {
        for a in actions {
            match a {
                // 詳細の頁はホームへまとめた(11-16)。開くとホームでそのビルドを選び、説明を出す
                Action::Open(k) => {
                    self.pages.clear();
                    self.page = k.clone();
                    self.sel = k;
                    self.start_tab = StartTab::About;
                    self.album_open = None;
                    self.view = View::Start;
                }
                Action::LaunchFeature(id, label) => {
                    let mut c = self.dev_command("host");
                    c.env("FALCON_FEATURE_LAUNCH", &id).env("FALCON_LAUNCH_LABEL", format!("Falcon Engine v4: {label}"));
                    self.spawn(c, &format!("Falcon Engine v4({label}だけ)"));
                }
                Action::LaunchScene(sc) => {
                    let mut c = Command::new("sh");
                    c.arg("-c").arg(scan::scene_command(&sc));
                    self.spawn(c, &format!("クラスターの {}", sc.name));
                }
                Action::Play(p) => {
                    let _ = Command::new("xdg-open").arg(p).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
                }
                Action::Toggle(id, full) => {
                    if !self.open_changes.remove(&id) {
                        if !full.is_empty() && !self.details.contains_key(&full) {
                            let repo = self.builds.iter().find(|b| b.key == self.page).map(|b| b.repo.clone()).unwrap_or_default();
                            let d = scan::change_detail(&repo, &full);
                            self.details.insert(full, d);
                        }
                        self.open_changes.insert(id);
                    }
                }
                Action::Launch(k) => self.launch(&k),
                Action::Folder(d) => {
                    let _ = Command::new("xdg-open").arg(d).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
                }
                Action::Build(k) => self.confirm_build = Some(k),
                Action::Fav(k) => {
                    if !self.favorites.remove(&k) {
                        self.favorites.insert(k);
                    }
                    self.save_favorites();
                }
                Action::ShowLog(k) => {
                    self.log_for = k;
                    self.log_read = Instant::now() - Duration::from_secs(10);
                    self.view = View::Log;
                }
            }
        }
    }

    fn spawn(&mut self, mut cmd: Command, name: &str) {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        self.message = match cmd.spawn() {
            Ok(_) => format!("{name} を起こしました"),
            Err(e) => format!("{name} を起こせませんでした: {e}"),
        };
    }

    /// 本線と試しの枝を起こす命令: 場所.tsv の wrapper(起動の台本)があればそれに単位を渡し、無ければ建て先の bin/blender
    fn dev_command(&self, unit: &str) -> Command {
        let w = &places::places().wrapper;
        if w.is_empty() {
            let dir = self.builds.iter().find(|b| b.key == unit).map(|b| b.dir.clone()).unwrap_or_default();
            Command::new(dir.join("bin/blender"))
        } else {
            let mut c = Command::new(w);
            c.arg(unit);
            c
        }
    }

    fn launch(&mut self, key: &str) {
        let Some(b) = self.builds.iter().find(|b| b.key == key).cloned() else { return };
        let cmd = match b.kind {
            Kind::Main => {
                let mut c = self.dev_command("host");
                c.env("FALCON_FEATURE_LAUNCH", "all").env("FALCON_LAUNCH_LABEL", "Falcon Engine v4");
                c
            }
            Kind::Test => {
                self.dev_command(&b.key)
            }
            Kind::Old => {
                let mut c = Command::new("sh");
                c.arg("-c").arg(&b.exec);
                c
            }
            Kind::Ext => {
                let Some(sc) = self.scenes.first().cloned() else {
                    self.message = "場面がありません".into();
                    return;
                };
                let mut c = Command::new("sh");
                c.arg("-c").arg(scan::scene_command(&sc));
                c
            }
        };
        self.spawn(cmd, &b.title);
    }

    fn start_build(&mut self, key: &str) {
        let out = config_dir().join(format!("建てる-{key}.out"));
        let _ = std::fs::create_dir_all(config_dir());
        let Ok(f) = std::fs::File::create(&out) else { return };
        let f2 = f.try_clone().ok();
        use std::os::unix::process::CommandExt;
        let mut cmd = Command::new(format!("{}/scripts/build.sh", places::places().project));
        cmd.arg(key).process_group(0).stdin(Stdio::null()).stdout(f);
        if let Some(f2) = f2 {
            cmd.stderr(f2);
        }
        self.message = match cmd.spawn() {
            Ok(_) => format!("{key} を建て始めました(台本の出力: {})", out.display()),
            Err(e) => format!("{key} を建て始められませんでした: {e}"),
        };
        self.last_light = Instant::now() - Duration::from_secs(10);
    }
}

// ---------- 描画の部品 ----------

fn small_caps(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(11.0).color(FAINT).extra_letter_spacing(1.2));
}

fn hue_color(key: &str, light: f32) -> Color32 {
    let h = key.bytes().fold(2166136261u32, |a, b| (a ^ b as u32).wrapping_mul(16777619));
    let hue = (h % 360) as f32 / 360.0;
    // 色合いはシンプルに: 色相は使わず、灰色の濃さだけを少し変える
    let _ = hue;
    Color32::from_gray((0x33 as f32 + light * 40.0 + (h % 12) as f32) as u8)
}

fn thumb_path(b: &Build) -> Option<PathBuf> {
    let unit = b.key.trim_start_matches("old:");
    [b.dir.join("サムネ.png"), PathBuf::from(format!("{}/integration/{unit}/サムネ.png", places::places().project))].into_iter().find(|p| p.is_file())
}

/// 決めた場所いっぱいに置く(ボタン用)
fn place_fill(ui: &mut egui::Ui, rect: Rect, enabled: bool, widget: impl egui::Widget) -> egui::Response {
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::left_to_right(egui::Align::Center)));
    if !enabled {
        child.disable();
    }
    child.add_sized(rect.size(), widget)
}

fn button(text: RichText, primary: bool) -> egui::Button<'static> {
    let b = egui::Button::new(text).corner_radius(CornerRadius::same(6));
    if primary { b.fill(ACCENT).stroke(Stroke::NONE) } else { b.fill(CARD_HI).stroke(Stroke::new(1.0, LINE)) }
}

impl App {
    fn trees_view(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.add_space(18.0);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                small_caps(ui, "本体のリポジトリ");
                ui.label(RichText::new("木と枝").size(27.0).color(TEXT));
                ui.label(RichText::new(scan::src_dir()).family(egui::FontFamily::Monospace).size(12.0).color(MUTED));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                if ui.add(button(RichText::new(format!("{I_REFRESH}  調べ直す")).color(ON_ACCENT), true).min_size(Vec2::new(120.0, 34.0))).clicked() {
                    self.start_trees(ctx);
                }
            });
        });
        ui.add_space(12.0);
        let (n, dirty, nobranch, inmain) = (
            self.trees.len(),
            self.trees.iter().filter(|t| t.dirty > 0).count(),
            self.trees.iter().filter(|t| t.branch == "(枝なし)").count(),
            self.trees.iter().filter(|t| t.in_main).count(),
        );
        ui.horizontal(|ui| {
            for (k, v, c) in [("木", n, TEXT), ("未コミットあり", dirty, WARN), ("枝なし", nobranch, MUTED), ("v4 に入っている", inmain, OK)] {
                egui::Frame::new().fill(CARD).stroke(Stroke::new(1.0, LINE)).corner_radius(CornerRadius::same(8)).inner_margin(egui::Margin::symmetric(14, 8)).show(ui, |ui| {
                    ui.label(RichText::new(v.to_string()).size(20.0).color(c));
                    ui.label(RichText::new(k).size(11.5).color(FAINT));
                });
            }
            if self.trees_rx.is_some() {
                ui.label(RichText::new("1 本ずつ調べています…").size(12.0).color(FAINT));
            }
        });
        ui.add_space(12.0);
        ui.add(egui::TextEdit::singleline(&mut self.tree_filter).hint_text(format!("{I_SEARCH}  木・枝で絞る")).desired_width(280.0).margin(egui::Margin::symmetric(10, 6)));
        ui.add_space(10.0);
        let q = self.tree_filter.to_lowercase();
        let mut rows: Vec<Tree> = self.trees.iter().filter(|t| q.is_empty() || t.path.to_lowercase().contains(&q) || t.branch.to_lowercase().contains(&q)).cloned().collect();
        rows.sort_by(|a, b| b.ts.cmp(&a.ts));
        let mut open = None;
        egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            egui::Grid::new("trees").striped(true).num_columns(6).spacing([18.0, 8.0]).show(ui, |ui| {
                for h in ["木", "枝", "最後のコミット", "未コミット", "v4 に", ""] {
                    ui.label(RichText::new(h).size(11.5).color(FAINT));
                }
                ui.end_row();
                for t in &rows {
                    let base = Path::new(scan::src_dir()).parent().map(|p| format!("{}/", p.display())).unwrap_or_default();
                    let short = t.path.trim_start_matches(base.as_str());
                    ui.label(RichText::new(short).family(egui::FontFamily::Monospace).size(12.5).color(TEXT));
                    ui.label(RichText::new(&t.branch).family(egui::FontFamily::Monospace).size(12.0).color(if t.branch == "(枝なし)" { FAINT } else { MUTED }));
                    ui.label(RichText::new(when(t.ts)).size(12.0).color(MUTED));
                    ui.label(RichText::new(if t.dirty > 0 { format!("{} ファイル", t.dirty) } else { "-".into() }).size(12.0).color(if t.dirty > 0 { WARN } else { FAINT }));
                    ui.label(RichText::new(if t.in_main { "入っている" } else { "-" }).size(12.0).color(if t.in_main { OK } else { FAINT }));
                    if ui.add(egui::Button::new(RichText::new(I_FOLDER).color(MUTED)).frame(false)).on_hover_text("フォルダを開く").clicked() {
                        open = Some(t.path.clone());
                    }
                    ui.end_row();
                }
            });
        });
        if let Some(p) = open {
            self.apply(vec![Action::Folder(PathBuf::from(p))]);
        }
    }

    fn log_view(&mut self, ui: &mut egui::Ui) {
        ui.add_space(18.0);
        small_caps(ui, "ビルドのログ");
        ui.label(RichText::new("ログ").size(27.0).color(TEXT));
        ui.add_space(10.0);
        let with_log: Vec<(String, String, PathBuf)> = self.builds.iter().filter(|b| b.dir.join("build.log").is_file()).map(|b| (b.key.clone(), b.title.clone(), b.dir.join("build.log"))).collect();
        let cur_title = with_log.iter().find(|x| x.0 == self.log_for).map(|x| x.1.clone()).unwrap_or_else(|| "選ぶ".into());
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("logfor").selected_text(cur_title).width(260.0).show_ui(ui, |ui| {
                for (k, t, _) in &with_log {
                    if ui.selectable_label(&self.log_for == k, t).clicked() {
                        self.log_for = k.clone();
                        self.log_read = Instant::now() - Duration::from_secs(10);
                    }
                }
            });
            if let Some((_, _, p)) = with_log.iter().find(|x| x.0 == self.log_for) {
                ui.label(RichText::new(p.display().to_string()).family(egui::FontFamily::Monospace).size(11.5).color(FAINT));
            }
        });
        if self.log_read.elapsed() > Duration::from_secs(2) {
            self.log_read = Instant::now();
            self.log_text = with_log
                .iter()
                .find(|x| x.0 == self.log_for)
                .and_then(|(_, _, p)| scan::read_tail(p, 64 * 1024).ok())
                .map(|b| {
                    let t = String::from_utf8_lossy(&b).into_owned();
                    let lines: Vec<&str> = t.lines().collect();
                    lines[lines.len().saturating_sub(400)..].join("\n")
                })
                .unwrap_or_else(|| "このビルド先には build.log がありません".into());
        }
        ui.add_space(10.0);
        egui::Frame::new().fill(BG).stroke(Stroke::new(1.0, LINE)).corner_radius(CornerRadius::same(8)).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
            egui::ScrollArea::both().auto_shrink(false).stick_to_bottom(true).show(ui, |ui| {
                ui.label(RichText::new(&self.log_text).family(egui::FontFamily::Monospace).size(12.0).color(MUTED));
            });
        });
    }

    fn confirm(&mut self, ctx: &egui::Context) {
        let Some(key) = self.confirm_build.clone() else { return };
        let branch = self.builds.iter().find(|b| b.key == key).map(|b| b.branch.clone()).unwrap_or_default();
        let mut go = false;
        let mut close = false;
        let resp = egui::Modal::new(egui::Id::unique("falcon-launcher-confirm")).show(ctx, |ui| {
            ui.set_width(460.0);
            ui.label(RichText::new(if key == "host" { "Falcon Engine v4 を建て直します".to_string() } else { format!("試験 {key} を建てます") }).size(17.0).color(TEXT));
            ui.add_space(8.0);
            let text = if key == "host" {
                format!("本体の木({})の {branch} を、{}/build/host へ差分ビルドします(nice -n 10 ninja -j5)。", scan::src_dir(), places::places().project)
            } else {
                format!("本体の木({})を一時的に {branch} へ切り替えて、{}/build/test-{key} へ差分ビルドします(nice -n 10 ninja -j5)。終われば木は元の枝へ戻ります。\n本体の木に未コミットの変更がある時は、建てずに止まります。", scan::src_dir(), places::places().project)
            };
            ui.add(egui::Label::new(RichText::new(text).size(12.5).color(MUTED)).wrap());
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.add(button(RichText::new(format!("{I_BUILD}  建てる")).color(ON_ACCENT), true)).clicked() {
                    go = true;
                }
                if ui.add(button(RichText::new("やめる").color(TEXT), false)).clicked() {
                    close = true;
                }
            });
        });
        if go {
            self.start_build(&key);
            self.confirm_build = None;
        } else if close || resp.should_close() {
            self.confirm_build = None;
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll(&ctx);
        // アドオンから届いた物を棚へ
        let cur = self.cur_build();
        let (added, removed) = self.queue.sync_inbox(&self.builds, cur.as_ref());
        if added > 0 {
            self.message = format!("Blender から {added} 件がレンダリングの一覧に登録されました");
        }
        if removed > 0 {
            self.message = format!("Blender でチェックを外された {removed} 件を、一覧から外しました");
        }
        // 進めるスイッチが入っていれば、描ける物を描き始める
        if self.queue.settings().run && !self.queue.is_running() {
            self.queue.kick(self.builds.clone(), ctx.clone());
        }
        ctx.request_repaint_after(Duration::from_secs(1));
        // 外枠: 上の帯・左の細い列・真ん中と右の列
        self.track_history();
        egui::Panel::top("top").exact_size(shell::TOP_H).resizable(false).frame(egui::Frame::new().fill(shell::OUTER).inner_margin(egui::Margin::symmetric(10, 0))).show(ui, |ui| self.top_bar(ui, &ctx));
        egui::Panel::left("rail").exact_size(shell::RAIL_W).resizable(false).frame(egui::Frame::new().fill(shell::OUTER)).show(ui, |ui| self.rail(ui, &ctx));
        egui::CentralPanel::default().frame(egui::Frame::new().fill(shell::OUTER)).show(ui, |ui| self.main_area(ui, &ctx));
        self.lightbox_modal(&ctx);
        self.confirm(&ctx);
        self.setup_confirm_modal(&ctx);
    }
}

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let (mut prop, mut mono) = (vec![], vec![]);
    // 字体はファイルの名前で探す(places::find_font: 場所.tsv の fonts → ~/.local/share/falcon-launcher/fonts → ~/.local/share/fonts → /usr/share/fonts)。
    // 無い物は飛ばし、egui の字体(英字だけ)で出す
    for (name, file, p, m) in FONTS {
        let Some(bytes) = places::find_font(file).and_then(|path| std::fs::read(path).ok()) else { continue };
        fonts.font_data.insert(name.into(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
        if p {
            prop.push(name.to_string());
        }
        if m {
            mono.push(name.to_string());
        }
    }
    for (fam, mine) in [(egui::FontFamily::Proportional, prop), (egui::FontFamily::Monospace, mono)] {
        fonts.families.entry(fam).or_default().splice(0..0, mine);
    }
    let mut bold = vec![];
    for (name, file) in BOLD_FONTS {
        let Some(bytes) = places::find_font(file).and_then(|path| std::fs::read(path).ok()) else { continue };
        fonts.font_data.insert(name.into(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
        bold.push(name.to_string());
    }
    let prop_all = fonts.families.get(&egui::FontFamily::Proportional).cloned().unwrap_or_default();
    bold.extend(prop_all);
    fonts.families.insert(egui::FontFamily::Name("bold".into()), bold);
    ctx.set_fonts(fonts);
}

fn setup_style(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = PANEL;
    v.window_stroke = Stroke::new(1.0, LINE);
    v.extreme_bg_color = CARD;
    v.faint_bg_color = Color32::from_rgb(0x14, 0x17, 0x1d);
    v.weak_text_color = Some(MUTED);
    v.hyperlink_color = ACCENT_TEXT;
    v.selection.bg_fill = ACCENT.gamma_multiply(0.45);
    v.selection.stroke = Stroke::new(1.0, ACCENT_TEXT);
    v.window_corner_radius = CornerRadius::same(10);
    let w = &mut v.widgets;
    w.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    for (s, bg, stroke) in [(&mut w.inactive, CARD, LINE), (&mut w.hovered, CARD_HI, Color32::from_rgb(0x3a, 0x40, 0x4b)), (&mut w.active, CARD_HI, ACCENT)] {
        s.bg_fill = bg;
        s.weak_bg_fill = bg;
        s.bg_stroke = Stroke::new(1.0, stroke);
        s.fg_stroke = Stroke::new(1.0, TEXT);
        s.corner_radius = CornerRadius::same(6);
    }
    w.open = w.active;
    ctx.set_visuals(v);
    ctx.all_styles_mut(|st| {
        st.spacing.button_padding = egui::vec2(12.0, 6.0);
        st.spacing.item_spacing = egui::vec2(8.0, 6.0);
        st.spacing.interact_size.y = 30.0;
    });
}

fn main() -> eframe::Result<()> {
    // WAYLAND_DISPLAY の口が無い時(ai-display は X11 へ落とすため偽の名前を入れる)は X11 で開く
    if let Ok(w) = std::env::var("WAYLAND_DISPLAY") {
        let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_default();
        if !Path::new(&dir).join(&w).exists() {
            std::env::remove_var("WAYLAND_DISPLAY");
        }
    }
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("Falcon Launcher").with_app_id("falcon-launcher").with_inner_size([1680.0, 980.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Falcon Launcher",
        opts,
        Box::new(|cc| {
            setup_fonts(&cc.egui_ctx);
            setup_style(&cc.egui_ctx);
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(App::new(&cc.egui_ctx)))
        }),
    )
}
