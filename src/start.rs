//! ホーム。
//! 並べ方: 左に版の一覧、全面の背景、大きな題と起動、下に枠つきのタブ。
//! 絵・色・コードはどれも自前(背景は Falcon で描いたプリズムの虹)。
//! ほしい物: ホーム・レンダリング結果・レンダリング予約・.blend 一覧・起動・ビルドや公式の版を選んで起動・
//! フォルダ・ログ・ファイル構造(押したら飛べる)・詳細(今の「詳細」の頁)。

use super::*;
use super::page::{bold, elide};
use queue::JobState;

/// 表の地(面より一段明るい)
const TABLE: Color32 = Color32::from_rgb(0x1d, 0x1f, 0x23);


#[derive(Clone, Copy, PartialEq)]
pub enum StartTab {
    Overview,
    About,
    Changes,
    Features,
    Blend,
    Renders,
    Queue,
    Files,
}

/// 最近開いた .blend(Blender の recent-files.txt から)
#[derive(Clone)]
pub struct BlendFile {
    pub path: PathBuf,
    pub ts: i64,
    pub bytes: u64,
    /// 保存した Blender の版(「5.3」など)。圧縮されていて読めない時は空
    pub version: String,
    pub compressed: bool,
}

/// 頭の数バイトから、保存した Blender の版と、圧縮かどうか
fn blend_header(path: &Path) -> (String, bool) {
    use std::io::Read;
    let mut b = [0u8; 17];
    let n = std::fs::File::open(path).and_then(|mut f| f.read(&mut b)).unwrap_or(0);
    if n >= 4 && (b[..4] == [0x28, 0xb5, 0x2f, 0xfd] || b[..2] == [0x1f, 0x8b]) {
        return (String::new(), true);
    }
    if n < 12 || &b[..7] != b"BLENDER" {
        return (String::new(), false);
    }
    // 古い形「BLENDER-v405」と、新しい形「BLENDER17-01v0500」
    let digits = if b[7].is_ascii_digit() && n >= 17 { &b[13..17] } else { &b[9..12] };
    let v: u32 = std::str::from_utf8(digits).ok().and_then(|s| s.parse().ok()).unwrap_or(0);
    (if v > 0 { format!("{}.{}", v / 100, v % 100) } else { String::new() }, false)
}

pub fn recent_blends() -> Vec<BlendFile> {
    let home = std::env::var("HOME").unwrap_or_default();
    let mut seen = HashSet::new();
    let mut v = vec![];
    let Ok(rd) = std::fs::read_dir(format!("{home}/.config/blender")) else { return v };
    for e in rd.flatten() {
        let Ok(text) = std::fs::read_to_string(e.path().join("config/recent-files.txt")) else { continue };
        for line in text.lines() {
            let p = PathBuf::from(line.trim());
            if p.as_os_str().is_empty() || !seen.insert(p.clone()) {
                continue;
            }
            let Ok(md) = std::fs::metadata(&p) else { continue };
            let ts = md.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0);
            let (version, compressed) = blend_header(&p);
            v.push(BlendFile { path: p, ts, bytes: md.len(), version, compressed });
        }
    }
    v.sort_by(|a, b| b.ts.cmp(&a.ts));
    v
}

/// .blend の中の小さな絵を、Blender の blender-thumbnailer で取り出す(圧縮された物も読める)
pub fn blend_thumb(thumbnailer: &Path, path: &Path) -> Option<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_default();
    let dir = PathBuf::from(format!("{home}/.cache/falcon-launcher/blend"));
    let _ = std::fs::create_dir_all(&dir);
    let md = std::fs::metadata(path).ok()?;
    let mt = md.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    let key = format!("{}|{mt}", path.display()).bytes().fold(1469598103934665603u64, |a, b| (a ^ b as u64).wrapping_mul(1099511628211));
    let out = dir.join(format!("{key:016x}.png"));
    if out.is_file() {
        return Some(out);
    }
    let ok = Command::new(thumbnailer).arg(path).arg(&out).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().map(|s| s.success()).unwrap_or(false);
    (ok && out.is_file()).then_some(out)
}


fn open_path(p: &Path) {
    let _ = Command::new("xdg-open").arg(p).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
}

impl App {
    /// ホームで選んでいるビルド(無ければ本線)
    pub(super) fn cur_build(&self) -> Option<Build> {
        self.builds.iter().find(|b| b.key == self.sel).or_else(|| self.builds.iter().find(|b| b.kind == Kind::Main)).cloned()
    }

    /// 左の一覧の組: (見出し, ビルド)
    pub(super) fn start_groups(&self) -> Vec<(&'static str, Vec<Build>)> {
        let fav = |v: &mut Vec<Build>| v.sort_by_key(|b| (!self.favorites.contains(&b.key), -b.built_ts));
        let mut falcon: Vec<Build> = self.builds.iter().filter(|b| matches!(b.kind, Kind::Main | Kind::Ext) || b.key == "v5").cloned().collect();
        falcon.sort_by_key(|b| (b.key != "v5", b.kind != Kind::Main, b.kind == Kind::Ext));
        let mut tests: Vec<Build> = self.builds.iter().filter(|b| b.kind == Kind::Test && (b.has_bin || b.building.is_some())).cloned().collect();
        fav(&mut tests);
        let official: Vec<Build> = self.builds.iter().filter(|b| b.key.starts_with("official:")).cloned().collect();
        let mut old: Vec<Build> = self.builds.iter().filter(|b| b.kind == Kind::Old && b.key != "v5" && !b.key.starts_with("official:")).cloned().collect();
        fav(&mut old);
        [("Falcon", falcon), ("試験のビルド", tests), ("公式の Blender", official), ("古い版", old)].into_iter().filter(|(_, v)| !v.is_empty()).collect()
    }

    /// 左の列に出す物: Falcon(v5・v4 の本線)と公式の Blender
    pub(super) fn left_groups(&self) -> Vec<(&'static str, Vec<Build>)> {
        // 前の版(prev:<版>。更新の時に残した物)は今の版の後ろに
        let mut falcon: Vec<Build> = self.builds.iter().filter(|b| b.kind == Kind::Main || b.key == "v5" || b.key.starts_with("prev:")).cloned().collect();
        falcon.sort_by_key(|b| (b.key != "v5", b.key.starts_with("prev:")));
        let official: Vec<Build> = self.builds.iter().filter(|b| b.key.starts_with("official:")).cloned().collect();
        [("Falcon", falcon), ("公式の Blender", official)].into_iter().filter(|(_, v)| !v.is_empty()).collect()
    }

    pub(super) fn state_of(&self, b: &Build) -> (Color32, String) {
        if self.is_running(b) {
            (OK, "起動中".into())
        } else if let Some(pr) = &b.building {
            (WARN, format!("ビルド中 {} / {}", pr.done, pr.total))
        } else if b.launchable() {
            (OK, "準備完了".into())
        } else if b.ok == Some(false) {
            (ERR, "建てて失敗".into())
        } else {
            (FAINT, "未建て".into())
        }
    }

    fn thumbnailer(&self) -> Option<PathBuf> {
        let mut c = vec![];
        if let Some(b) = self.cur_build() {
            c.push(b.dir.join("bin/blender-thumbnailer"));
        }
        let pl = crate::places::places();
        c.push(PathBuf::from(&pl.falcon_build).join("bin/blender-thumbnailer"));
        if !pl.project.is_empty() {
            c.push(PathBuf::from(format!("{}/build/host/bin/blender-thumbnailer", pl.project)));
        }
        c.into_iter().find(|p| p.is_file())
    }

    pub(super) fn blend_thumb_of(&mut self, path: &Path) -> Option<PathBuf> {
        if let Some(t) = self.blend_thumbs.lock().ok().and_then(|d| d.get(path).cloned()) {
            return t;
        }
        if self.blend_asked.insert(path.to_path_buf()) {
            if let Some(t) = self.thumbnailer() {
                let _ = self.blend_tx.send((t, path.to_path_buf()));
            }
        }
        None
    }

    pub(super) fn open_blend(&mut self, path: &Path) {
        let Some(b) = self.cur_build() else { return };
        let Some(bin) = scan::blender_bin(&b) else {
            self.message = format!("{} には blender がありません", b.title);
            return;
        };
        let mut c = Command::new(bin);
        c.arg(path);
        for (k, v) in scan::blender_env(&b) {
            c.env(k, v);
        }
        let name = path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        self.spawn(c, &format!("{name}({})", b.title));
    }

    fn add_job(&mut self, path: &Path, range: Option<(i32, i32, i32)>) {
        let Some(b) = self.cur_build() else { return };
        self.queue.add(path.to_path_buf(), &b, range);
        self.message = format!("{} をレンダリングの一覧に足しました({})", path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(), b.title);
    }

    // ---------- ホーム ----------

    pub(super) fn prepare(&mut self, b: &Build, ctx: &egui::Context) {
        if self.gallery.is_empty() && self.gallery_rx.is_none() {
            self.start_gallery(ctx);
        }
        self.ask_changes(b, ctx);
        self.ask_features(ctx);
        if self.blends.is_none() {
            self.blends = Some(Arc::new(recent_blends()));
        }
    }

    // ---------- 予約 ----------

    fn tile(ui: &mut egui::Ui, w: f32, title: &str, value: String, sub: String, vals: &[f32], max: f32, color: Color32) {
        egui::Frame::new().fill(CARD.gamma_multiply(0.85)).corner_radius(CornerRadius::same(12)).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
            ui.set_width(w - 24.0);
            ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.add(egui::Label::new(RichText::new(title).size(11.5).color(FAINT)).truncate());
            ui.label(RichText::new(value).font(bold(20.0)).color(TEXT));
            ui.add(egui::Label::new(RichText::new(sub).size(11.0).color(MUTED)).truncate());
            let (r, _) = ui.allocate_exact_size(Vec2::new(w - 24.0, 34.0), Sense::hover());
            let p = ui.painter();
            p.rect_filled(r, CornerRadius::same(4), BG.gamma_multiply(0.5));
            if vals.len() >= 2 {
                let n = 120.0;
                let pts: Vec<Pos2> = vals.iter().enumerate().map(|(i, v)| Pos2::new(r.right() - (vals.len() - 1 - i) as f32 / (n - 1.0) * r.width(), r.bottom() - (v / max.max(1e-3)).clamp(0.0, 1.0) * (r.height() - 2.0) - 1.0)).collect();
                let mut fill = pts.clone();
                fill.push(Pos2::new(pts.last().unwrap().x, r.bottom()));
                fill.push(Pos2::new(pts[0].x, r.bottom()));
                p.add(egui::Shape::convex_polygon(fill, color.gamma_multiply(0.18), Stroke::NONE));
                p.add(egui::Shape::line(pts, Stroke::new(1.5, color)));
            }
            });
        });
    }

    fn monitor_row(&mut self, ui: &mut egui::Ui, w: f32) {
        let hist: Vec<queue::Sample> = self.queue.monitor.lock().map(|m| m.hist.iter().cloned().collect()).unwrap_or_default();
        let last = hist.last().cloned().unwrap_or_default();
        let s = self.queue.settings();
        let tw = (w - 4.0 * 10.0) / 5.0;
        let gib = |b: u64| b as f32 / 1073741824.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            Self::tile(ui, tw, "CPU", format!("{:.0} %", last.cpu), format!("{} 本", std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)), &hist.iter().map(|x| x.cpu).collect::<Vec<_>>(), 100.0, LINK);
            Self::tile(ui, tw, "メモリ", format!("{:.1} GB", gib(last.ram_used)), format!("全体 {:.0} GB{}", gib(last.ram_total), if s.ram_gb > 0.0 { format!(" · 上限 {:.0} GB", s.ram_gb) } else { String::new() }), &hist.iter().map(|x| gib(x.ram_used)).collect::<Vec<_>>(), gib(last.ram_total.max(1)), OK);
            Self::tile(ui, tw, "GPU", format!("{:.0} %", last.gpu), "使用率".into(), &hist.iter().map(|x| x.gpu).collect::<Vec<_>>(), 100.0, ACCENT);
            let vc = if s.vram_mib > 0 && last.vram_used + 512 > s.vram_mib { WARN } else { ORANGE };
            Self::tile(ui, tw, "VRAM", format!("{} MiB", last.vram_used), format!("全体 {} MiB{}", last.vram_total, if s.vram_mib > 0 { format!(" · 上限 {}", s.vram_mib) } else { String::new() }), &hist.iter().map(|x| x.vram_used as f32).collect::<Vec<_>>(), last.vram_total.max(1) as f32, vc);
            let running = self.queue.snapshot().into_iter().find(|j| matches!(j.state, JobState::Running | JobState::Paused));
            let (title, value, sub) = match &running {
                Some(j) => (format!("描いている物 · {}", j.state.text()), format!("{:.0} %", last.job_cpu), format!("RAM {:.1} GB · VRAM {} MiB", gib(last.job_rss), last.job_vram)),
                None => ("描いている物".into(), "-".into(), "今は描いていません".into()),
            };
            Self::tile(ui, tw, &title, value, sub, &hist.iter().map(|x| x.job_cpu).collect::<Vec<_>>(), 100.0, TEXT);
        });
        if hist.is_empty() {
            ui.label(RichText::new("測っています…(この頁を開いている時と、描いている時だけ 1 秒おきに測ります)").size(11.5).color(FAINT));
        }
    }

    /// 入り切りのスイッチ(押されたら true)
    fn toggle(ui: &mut egui::Ui, on: bool) -> bool {
        let (r, resp) = ui.allocate_exact_size(Vec2::new(40.0, 22.0), Sense::click());
        let t = ui.ctx().animate_bool_responsive(resp.id, on);
        let p = ui.painter();
        p.rect_filled(r, CornerRadius::same(11), if on { ACCENT } else { CARD_HI });
        let x = egui::lerp(r.left() + 11.0..=r.right() - 11.0, t);
        p.circle_filled(Pos2::new(x, r.center().y), 8.0, if on { ON_ACCENT } else { MUTED });
        resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
    }

    fn control_row(&mut self, ui: &mut egui::Ui, w: f32, ctx: &egui::Context) {
        let jobs = self.queue.snapshot();
        let cur = jobs.iter().find(|j| matches!(j.state, JobState::Running | JobState::Paused)).cloned();
        // 残り: 描ける物と、今描いている物
        let todo = jobs.iter().filter(|j| j.runnable() || matches!(j.state, JobState::Running | JobState::Paused)).count();
        let mut s = self.queue.settings();
        let before = s.clone();
        egui::Frame::new().fill(CARD.gamma_multiply(0.85)).corner_radius(CornerRadius::same(12)).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
            ui.set_width(w - 24.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(10.0, 8.0);
                // 全体のスイッチ: 入れている間は、オンの行を上から描いていく
                if Self::toggle(ui, s.run) {
                    self.queue.set_run(!s.run, self.builds.clone(), ctx.clone());
                    s.run = !s.run;
                }
                ui.label(RichText::new(if s.run { format!("進めています(残り {todo} 件)") } else { format!("止めています(描く物 {todo} 件)") }).font(bold(13.5)).color(if s.run { ACCENT_TEXT } else { MUTED }));
                match &cur {
                    Some(j) if j.state == JobState::Paused => {
                        if ui.add(button(RichText::new(format!("{I_PLAY}  再開")).color(ON_ACCENT), true).min_size(Vec2::new(90.0, 32.0))).clicked() {
                            self.queue.resume();
                        }
                    }
                    Some(_) => {
                        if ui.add(button(RichText::new("❚❚  一時停止").color(TEXT), false).min_size(Vec2::new(90.0, 32.0))).on_hover_text("描いている物を止めておき、再開で続きから進めます(VRAM は持ったまま)").clicked() {
                            self.queue.pause();
                        }
                    }
                    None => {}
                }
                ui.separator();
                let ncpu = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) as u32;
                ui.label(RichText::new("CPU のスレッド").color(MUTED));
                ui.add(egui::DragValue::new(&mut s.threads).range(0..=ncpu).custom_formatter(|v, _| if v == 0.0 { "全部".into() } else { format!("{v:.0} 本") })).on_hover_text("0 は全部。次の 1 本から効きます");
                ui.label(RichText::new("優先度").color(MUTED));
                ui.add(egui::DragValue::new(&mut s.nice).range(0..=19).custom_formatter(|v, _| format!("nice {v:.0}"))).on_hover_text("0〜19。大きいほど後回しにして、ほかの作業を重くしません");
                ui.label(RichText::new("RAM 上限").color(MUTED));
                ui.add(egui::DragValue::new(&mut s.ram_gb).range(0.0..=1024.0).speed(0.5).custom_formatter(|v, _| if v == 0.0 { "無し".into() } else { format!("{v:.0} GB") })).on_hover_text("超えると、描いている物だけが止められます(systemd のスコープ)。次の 1 本から効きます");
                ui.label(RichText::new("VRAM 上限").color(MUTED));
                ui.add(egui::DragValue::new(&mut s.vram_mib).range(0..=81920).speed(64.0).custom_formatter(|v, _| if v == 0.0 { "無し".into() } else { format!("{v:.0} MiB") })).on_hover_text("全体の VRAM がこれに近い時は次を始めず、超えたら描いている物を止めます(VRAM は外から絞れないため)");
            });
        });
        if s.nice != before.nice && cur.is_some() {
            if let Err(e) = self.queue.renice(s.nice) {
                self.message = e;
            }
        }
        if s.threads != before.threads || s.nice != before.nice || s.ram_gb != before.ram_gb || s.vram_mib != before.vram_mib {
            self.queue.set_settings(s);
        }
        if let Some(n) = self.queue.notice.lock().ok().map(|n| n.clone()).filter(|n| !n.is_empty()) {
            ui.add_space(4.0);
            ui.label(RichText::new(format!("⚠ {n}")).size(12.5).color(WARN));
        }
    }

    /// 選ぶ四角(押されたら true)
    /// 割り当ての 1 行: 入れると個別にする・数・全体の値
    fn lim_row<T: egui::emath::Numeric>(ui: &mut egui::Ui, label: &str, v: &mut Option<T>, global: T, range: std::ops::RangeInclusive<T>, unit: &str, note: &str) {
        let mut on = v.is_some();
        if ui.checkbox(&mut on, label).changed() {
            *v = if on { Some(global) } else { None };
        }
        match v {
            Some(x) => {
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(x).range(range).speed(0.25));
                    ui.label(RichText::new(unit).size(11.5).color(FAINT));
                });
            }
            None => {
                ui.label(RichText::new(note).size(11.5).color(FAINT));
            }
        }
        ui.end_row();
    }

    fn check_box(ui: &mut egui::Ui, r: Rect, on: bool, id: egui::Id) -> bool {
        let resp = ui.interact(r, id, Sense::click());
        let b = Rect::from_center_size(r.center(), Vec2::splat(18.0));
        let p = ui.painter();
        if on {
            p.rect_filled(b, CornerRadius::same(5), ACCENT);
            let c = b.center();
            p.line(vec![c + Vec2::new(-4.5, 0.0), c + Vec2::new(-1.5, 3.5), c + Vec2::new(4.5, -3.5)], Stroke::new(2.0, ON_ACCENT));
        } else {
            p.rect_stroke(b, CornerRadius::same(5), Stroke::new(1.5, if resp.hovered() { MUTED } else { FAINT }), StrokeKind::Inside);
        }
        resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
    }

    /// 一覧の表(選ぶ・順番・シーン・範囲と進み・版・操作)。
    /// レンダリングの順番を決める: 行ごとの ▲▼・順番の数・並べ替え・選んだ物をまとめて動かす
    fn job_table(&mut self, ui: &mut egui::Ui, w: f32, jobs: &[queue::Job], edit: &mut Vec<(u64, &'static str)>) {
        ui.style_mut().interaction.selectable_labels = false;
        let q = self.queue_query.trim().to_lowercase();
        let shown: Vec<(usize, &queue::Job)> = jobs
            .iter()
            .enumerate()
            .filter(|(_, j)| q.is_empty() || j.name().to_lowercase().contains(&q) || j.scene.to_lowercase().contains(&q))
            .filter(|(_, j)| match self.queue_filter {
                1 => j.on,
                2 => !j.on,
                3 => !matches!(j.state, JobState::Done),
                4 => j.state == JobState::Done,
                _ => true,
            })
            .collect();
        let n = jobs.len();
        // 列の幅
        let c_chk = 44.0;
        let c_no = 44.0;
        let c_act = 232.0;
        let c_build = 180.0;
        let rest = w - c_chk - c_no - c_act - c_build - 16.0;
        let c_name = rest * 0.56;
        let c_prog = rest - c_name;
        let x_no = c_chk;
        let x_name = x_no + c_no;
        let x_prog = x_name + c_name;
        let x_build = x_prog + c_prog;
        let x_act = x_build + c_build;
        egui::Frame::new().fill(TABLE).stroke(Stroke::new(1.0, LINE)).corner_radius(CornerRadius::same(14)).show(ui, |ui| {
            ui.set_width(w);
            ui.spacing_mut().item_spacing.y = 0.0;
            // 見出し
            let (hr, _) = ui.allocate_exact_size(Vec2::new(w, 46.0), Sense::hover());
            ui.painter().rect_filled(hr, CornerRadius { nw: 14, ne: 14, sw: 0, se: 0 }, CARD);
            let all = !shown.is_empty() && shown.iter().all(|(_, j)| self.queue_sel.contains(&j.id));
            if Self::check_box(ui, Rect::from_min_size(hr.min, Vec2::new(c_chk, 46.0)), all, ui.scope_id().with("chk-all")) {
                if all {
                    self.queue_sel.clear();
                } else {
                    self.queue_sel.extend(shown.iter().map(|(_, j)| j.id));
                }
            }
            let p = ui.painter();
            for (x, t) in [(x_no, "順"), (x_name, "シーン"), (x_prog, "範囲と進み"), (x_build, "描く版")] {
                p.text(Pos2::new(hr.left() + x + 4.0, hr.center().y), Align2::LEFT_CENTER, t, bold(13.5), MUTED);
            }
            p.text(Pos2::new(hr.right() - 16.0, hr.center().y), Align2::RIGHT_CENTER, "操作", bold(13.5), MUTED);
            if shown.is_empty() {
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    ui.add_space(16.0);
                    ui.label(RichText::new("当てはまる物がありません。").color(FAINT));
                });
                ui.add_space(14.0);
            }
            for (k, (i, j)) in shown.iter().enumerate() {
                let (i, j) = (*i, *j);
                // 行は乗っているかだけを見る(押せる物にすると、左のチェックの箱への押しを横取りする・11-17 録画の下見で見つけた)
                let (r, resp) = ui.allocate_exact_size(Vec2::new(w, 66.0), Sense::hover());
                let running = matches!(j.state, JobState::Running | JobState::Paused);
                let sel = self.queue_sel.contains(&j.id);
                let p = ui.painter().clone();
                p.rect_filled(Rect::from_min_size(r.min, Vec2::new(w, 1.0)), CornerRadius::ZERO, LINE);
                let last = k + 1 == shown.len();
                let cr = if last { CornerRadius { nw: 0, ne: 0, sw: 14, se: 14 } } else { CornerRadius::ZERO };
                if sel {
                    p.rect_filled(r.shrink2(Vec2::new(0.0, 0.5)), cr, ACCENT.gamma_multiply(0.07));
                } else if running {
                    p.rect_filled(r.shrink2(Vec2::new(0.0, 0.5)), cr, CARD.gamma_multiply(0.6));
                } else if resp.hovered() {
                    p.rect_filled(r.shrink2(Vec2::new(0.0, 0.5)), cr, CARD.gamma_multiply(0.35));
                }
                let dim = |c: Color32| if j.on { c } else { c.gamma_multiply(0.45) };
                if Self::check_box(ui, Rect::from_min_size(r.min, Vec2::new(c_chk, r.height())), sel, ui.scope_id().with(("chk", j.id))) && !self.queue_sel.remove(&j.id) {
                    self.queue_sel.insert(j.id);
                }
                // 順番の数(描く順。オフの物も数に入る)
                p.text(Pos2::new(r.left() + x_no + 4.0, r.center().y), Align2::LEFT_CENTER, format!("{}", i + 1), bold(15.0), dim(if running { ACCENT_TEXT } else { MUTED }));
                // シーン: 絵・名前・どこから
                let tr = Rect::from_min_size(Pos2::new(r.left() + x_name, r.top() + 11.0), Vec2::splat(44.0));
                let thumb = if j.source.is_file() { self.blend_thumb_of(&j.source) } else { None };
                p.rect_filled(tr, CornerRadius::same(10), CARD_HI);
                match thumb {
                    Some(t) => egui::Image::new(format!("file://{}", t.display())).corner_radius(CornerRadius::same(10)).tint(if j.on { Color32::WHITE } else { Color32::from_gray(110) }).paint_at(ui, tr),
                    None => {
                        p.text(tr.center(), Align2::CENTER_CENTER, I_CUBE, FontId::proportional(18.0), dim(MUTED));
                    }
                }
                let nx = tr.right() + 12.0;
                let nw = c_name - 44.0 - 20.0;
                let title = if j.scene.is_empty() { j.name() } else { j.scene.clone() };
                p.galley(Pos2::new(nx, r.top() + 13.0), elide(ui, &title, bold(14.5), dim(TEXT), nw), dim(TEXT));
                let from = if j.inbox.is_empty() { format!("{}  ·  手で足した物", j.name()) } else { j.name() };
                if j.lim.is_empty() {
                    p.galley(Pos2::new(nx, r.top() + 36.0), elide(ui, &from, FontId::proportional(12.5), dim(FAINT), nw), dim(FAINT));
                } else {
                    // 個別の割り当てがある時は、それを先に出す
                    let lt = format!("{}  {}", I_SLIDERS, j.lim.text());
                    let g = elide(ui, &lt, FontId::proportional(12.5), dim(ACCENT_TEXT), nw);
                    let gw = g.size().x;
                    p.galley(Pos2::new(nx, r.top() + 36.0), g, dim(ACCENT_TEXT));
                    if nw - gw > 60.0 {
                        p.galley(Pos2::new(nx + gw + 10.0, r.top() + 36.0), elide(ui, &from, FontId::proportional(12.5), dim(FAINT), nw - gw - 10.0), dim(FAINT));
                    }
                }
                // 範囲と進み
                let done = j.done_frames();
                let total = j.count();
                let cnt = match total {
                    Some(t) => format!("{done} / {t} 枚"),
                    None => format!("{done} 枚"),
                };
                let sc = match j.state {
                    JobState::Running => ACCENT_TEXT,
                    JobState::Paused | JobState::Partial => WARN,
                    JobState::Done => TEXT,
                    JobState::Failed => ERR,
                    JobState::Wait => MUTED,
                };
                let px = r.left() + x_prog + 4.0;
                let pw = c_prog - 24.0;
                p.galley(Pos2::new(px, r.top() + 12.0), elide(ui, &format!("{}  ·  {}", j.range_text(), cnt), bold(13.5), dim(TEXT), pw), dim(TEXT));
                let st = if j.progress.is_empty() || !running { j.state.text().to_string() } else { format!("{}  ·  {}", j.state.text(), j.progress) };
                p.galley(Pos2::new(px, r.top() + 33.0), elide(ui, &st, FontId::proportional(12.0), dim(sc), pw), dim(sc));
                let bar = Rect::from_min_size(Pos2::new(px, r.top() + 54.0), Vec2::new(pw, 3.0));
                let frac = total.map(|t| done as f32 / t.max(1) as f32).unwrap_or(if j.state == JobState::Done { 1.0 } else { 0.0 });
                p.rect_filled(bar, CornerRadius::same(2), CARD_HI);
                p.rect_filled(Rect::from_min_size(bar.min, Vec2::new(bar.width() * frac.clamp(0.0, 1.0), 3.0)), CornerRadius::same(2), dim(if j.state == JobState::Done { MUTED } else { sc }));
                // 描く版
                let br = Rect::from_min_size(Pos2::new(r.left() + x_build, r.center().y - 15.0), Vec2::new(c_build - 14.0, 30.0));
                let mut c = ui.new_child(egui::UiBuilder::new().id_salt(("row-build", j.id)).max_rect(br).layout(egui::Layout::left_to_right(egui::Align::Center)));
                let groups = self.start_groups();
                let mut pick = None;
                c.add_enabled_ui(!running, |ui| {
                    egui::ComboBox::from_id_salt(("row-build", j.id)).selected_text(RichText::new(&j.build_title).size(12.5)).width(c_build - 20.0).show_ui(ui, |ui| {
                        for (head, list) in &groups {
                            ui.label(RichText::new(*head).size(11.0).color(FAINT));
                            for b in list.iter().filter(|b| scan::blender_bin(b).is_some()) {
                                if ui.selectable_label(b.key == j.build, &b.title).clicked() {
                                    pick = Some((b.key.clone(), b.title.clone()));
                                }
                            }
                        }
                    });
                });
                if let Some((key, t)) = pick {
                    self.queue.edit(|js| {
                        if let Some(x) = js.iter_mut().find(|x| x.id == j.id) {
                            x.build = key;
                            x.build_title = t;
                        }
                    });
                }
                // 操作: ▲ ▼ ・スイッチ・ごみ箱・…
                let ar = Rect::from_min_max(Pos2::new(r.left() + x_act, r.top()), Pos2::new(r.right() - 10.0, r.bottom()));
                let mut c = ui.new_child(egui::UiBuilder::new().id_salt(("row-act", j.id)).max_rect(ar).layout(egui::Layout::right_to_left(egui::Align::Center)));
                c.spacing_mut().item_spacing.x = 6.0;
                let icon_btn = |t: &str, col: Color32| egui::Button::new(RichText::new(t).size(14.0).color(col)).frame(false).min_size(Vec2::new(28.0, 30.0));
                let more = c.add(icon_btn(I_MORE, MUTED));
                let lb = c.add(icon_btn(I_SLIDERS, if j.lim.is_empty() { MUTED } else { ACCENT_TEXT })).on_hover_text("このシーンだけの割り当て(CPU・メモリ・VRAM・優先度)");
                let mut lim = j.lim;
                egui::Popup::menu(&lb).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
                    ui.set_min_width(330.0);
                    ui.label(RichText::new("このシーンだけの割り当て").strong());
                    ui.label(RichText::new("入れていない物は、一覧の全体の設定のまま。次に描き始める時から効きます").size(11.5).color(FAINT));
                    ui.add_space(6.0);
                    let g = self.queue.settings();
                    egui::Grid::new(("lim-grid", j.id)).num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                        Self::lim_row(ui, "CPU のスレッド", &mut lim.threads, g.threads, 0..=256, "本(0 で全部)", &format!("全体: {}", if g.threads == 0 { "全部".to_string() } else { format!("{} 本", g.threads) }));
                        Self::lim_row(ui, "RAM の上限", &mut lim.ram_gb, g.ram_gb, 0.0..=1024.0, "GB(0 で無し)", &format!("全体: {}", if g.ram_gb <= 0.0 { "無し".to_string() } else { format!("{} GB", g.ram_gb) }));
                        Self::lim_row(ui, "VRAM の上限", &mut lim.vram_mib, g.vram_mib, 0..=200000, "MiB(0 で無し)", &format!("全体: {} MiB", g.vram_mib));
                        Self::lim_row(ui, "優先度(nice)", &mut lim.nice, g.nice, 0..=19, "(大きいほど後回し)", &format!("全体: {}", g.nice));
                    });
                    ui.add_space(6.0);
                    if ui.add_enabled(!lim.is_empty(), egui::Button::new("全部を全体の設定に戻す")).clicked() {
                        lim = queue::Limits::default();
                    }
                });
                if lim != j.lim {
                    self.queue.edit(|js| {
                        if let Some(x) = js.iter_mut().find(|x| x.id == j.id) {
                            x.lim = lim;
                        }
                    });
                }
                egui::Popup::menu(&more).show(|ui| {
                    ui.set_min_width(220.0);
                    if ui.add_enabled(i > 0, egui::Button::new("⤒   一番上へ(次に描く)")).clicked() {
                        edit.push((j.id, "top"));
                    }
                    if ui.add_enabled(i + 1 < n, egui::Button::new("⤓   一番下へ")).clicked() {
                        edit.push((j.id, "bottom"));
                    }
                    ui.separator();
                    if j.out.is_dir() && ui.button(format!("{I_FOLDER}   出力を開く")).clicked() {
                        open_path(&j.out);
                    }
                    if j.log().is_file() && ui.button(format!("{I_LOG}   ログを開く")).clicked() {
                        open_path(&j.log());
                    }
                    if matches!(j.state, JobState::Done | JobState::Failed | JobState::Partial) && ui.button("↺   最初から描き直す(新しいフォルダへ)").clicked() {
                        edit.push((j.id, "restart"));
                    }
                    if j.state == JobState::Failed && ui.button(format!("{I_PLAY}   もう一度(続きから)")).clicked() {
                        edit.push((j.id, "retry"));
                    }
                    if running && ui.button("■   この 1 本を止める(続きは残す)").clicked() {
                        edit.push((j.id, "halt"));
                    }
                });
                let del_tip = if j.inbox.is_empty() { "一覧から消す(描いた絵は残る)" } else { "一覧から消す。Blender のシーンのチェックも外れる(描いた絵は残る)" };
                if c.add(icon_btn("\u{f1f8}", MUTED)).on_hover_text(del_tip).clicked() {
                    edit.push((j.id, "del"));
                }
                if Self::toggle(&mut c, j.on) {
                    edit.push((j.id, "toggle"));
                }
                c.add_space(4.0);
                if c.add_enabled(i + 1 < n, icon_btn("\u{f078}", MUTED)).on_hover_text("1 つ後へ").clicked() {
                    edit.push((j.id, "down"));
                }
                if c.add_enabled(i > 0, icon_btn("\u{f077}", MUTED)).on_hover_text("1 つ前へ").clicked() {
                    edit.push((j.id, "up"));
                }
            }
        });
    }

    /// 並べ替え(1 度だけ並びを変える。そのあとも ▲▼ で直せる)
    fn sort_jobs(&mut self, how: u8) {
        self.queue.edit(|js| {
            // 描いている物は先頭のまま動かさない
            let keep = js.iter().position(|j| matches!(j.state, JobState::Running | JobState::Paused));
            let head: Vec<queue::Job> = keep.map(|k| vec![js.remove(k)]).unwrap_or_default();
            match how {
                0 => js.sort_by_key(|j| (if j.scene.is_empty() { j.name() } else { j.scene.clone() }).to_lowercase()),
                1 => js.sort_by_key(|j| j.count().unwrap_or(i32::MAX) - j.done_frames() as i32),
                2 => js.sort_by_key(|j| std::cmp::Reverse(j.count().unwrap_or(0) - j.done_frames() as i32)),
                3 => js.sort_by_key(|j| j.id),
                4 => js.sort_by_key(|j| (!j.on, j.state == JobState::Done)),
                _ => {}
            }
            let mut v = head;
            v.append(js);
            *js = v;
        });
    }

    pub(super) fn queue_page(&mut self, ui: &mut egui::Ui, w: f32, ctx: &egui::Context) {
        self.queue.watch();
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(button(RichText::new(format!("{I_FOLDER}  届け先")).color(MUTED), false)).on_hover_text(queue::inbox_dir().display().to_string()).clicked() {
                    let d = queue::inbox_dir();
                    let _ = std::fs::create_dir_all(&d);
                    open_path(&d);
                }
                if ui.add(button(RichText::new(format!("{I_EXT}  アドオンを入れる")).color(TEXT), false)).on_hover_text("~/.config/blender/<版>/scripts/addons へ写します。そのあと Blender の設定 > アドオンで「Falcon Launcher Link」を有効にしてください").clicked() {
                    let v = queue::install_addon();
                    self.message = if v.is_empty() { "アドオンを写せませんでした".into() } else { format!("アドオンを Blender {} へ写しました。設定 > アドオンで「Falcon Launcher Link」を有効にしてください", v.join(" · ")) };
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add(egui::Label::new(RichText::new("Blender の「出力」のプロパティで「ランチャーに登録」を入れたシーンが並びます。スイッチの入った物を上の順に描き、止めても続きから描けます").size(12.5).color(FAINT)).truncate());
                });
            });
        });
        ui.add_space(12.0);
        self.monitor_row(ui, w);
        ui.add_space(10.0);
        self.control_row(ui, w, ctx);
        ui.add_space(16.0);
        let jobs = self.queue.snapshot();
        // 探す欄・手で足す
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let p = PathBuf::from(self.queue_input.trim().trim_matches('\'').trim_matches('"'));
            egui::Frame::new().fill(CARD).stroke(Stroke::new(1.0, LINE)).corner_radius(CornerRadius::same(10)).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
                ui.set_width((w - 470.0).max(160.0));
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{I_SEARCH} ")).size(13.0).color(FAINT));
                    ui.add(egui::TextEdit::singleline(&mut self.queue_query).hint_text(format!("{} 件の中から探す", jobs.len())).frame(egui::Frame::NONE).desired_width(f32::INFINITY));
                });
            });
            ui.add(egui::TextEdit::singleline(&mut self.queue_input).hint_text("手で足す: .blend のパスを貼る・落とす").desired_width(300.0).margin(egui::Margin::symmetric(10, 9)));
            if ui.add_enabled(p.is_file(), button(RichText::new(format!("{I_PLAY}  足す")).color(ON_ACCENT), true).min_size(Vec2::new(0.0, 36.0))).on_hover_text("ファイルの範囲を、使う版で描く行を一番下に足します").clicked() {
                self.add_job(&p, None);
                self.queue_input.clear();
            }
        });
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        if let Some(p) = dropped.into_iter().find(|p| p.extension().is_some_and(|e| e == "blend")) {
            self.queue_input = p.display().to_string();
        }
        ui.add_space(10.0);
        // 並べ替え・絞り込み・選んだ物の操作
        let mut sort = None;
        let mut bulk: Option<&'static str> = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let sb = ui.add(button(RichText::new(format!("\u{f0dc}  順番を並べ替える  {I_DOWN}")).color(TEXT), false).min_size(Vec2::new(0.0, 32.0)));
            egui::Popup::menu(&sb).show(|ui| {
                ui.set_min_width(240.0);
                ui.label(RichText::new("描く順を並べ直します(描いている物は先頭のまま)").size(11.5).color(FAINT));
                for (k, t) in [(0u8, "名前順"), (1, "残りの枚数が少ない順"), (2, "残りの枚数が多い順"), (3, "届いた順"), (4, "オンを上・済みを下へ")] {
                    if ui.button(t).clicked() {
                        sort = Some(k);
                    }
                }
            });
            ui.add_space(6.0);
            ui.label(RichText::new(I_FILTER).size(13.0).color(MUTED));
            for (k, t) in [(0u8, "すべて"), (1, "オン"), (2, "オフ"), (3, "まだ"), (4, "済み")] {
                let on = self.queue_filter == k;
                let b = egui::Button::new(RichText::new(t).size(12.5).color(if on { ACCENT_TEXT } else { MUTED })).fill(if on { ACCENT.gamma_multiply(0.14) } else { Color32::TRANSPARENT }).stroke(Stroke::new(1.0, if on { ACCENT.gamma_multiply(0.7) } else { LINE })).corner_radius(CornerRadius::same(10)).min_size(Vec2::new(0.0, 30.0));
                if ui.add(b).clicked() {
                    self.queue_filter = k;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(format!("{} 件 · オン {} 件", jobs.len(), jobs.iter().filter(|j| j.on).count())).size(12.0).color(FAINT));
            });
        });
        // 選んだ物をまとめて動かす帯。選んでいない時も同じ高さを取り、表がずれないようにする
        // (11-17 録画の下見: 1 つ選ぶと帯が出て表が下へずれ、続けて押した 2 つ目が 1 行目の箱に当たって選びが外れていた)
        self.queue_sel.retain(|id| jobs.iter().any(|j| j.id == *id));
        let ns = self.queue_sel.len();
        if !jobs.is_empty() && ns == 0 {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.set_min_height(30.0);
                ui.label(RichText::new("行の左の箱で選ぶと、ここでまとめて動かせます").size(12.5).color(FAINT));
            });
        }
        if ns > 0 {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.set_min_height(30.0);
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.label(RichText::new(format!("選んだ {ns} 件:")).size(12.5).color(TEXT));
                for (k, t) in [("top", "一番上へ"), ("up", "上へ"), ("down", "下へ"), ("bottom", "一番下へ"), ("on", "オン"), ("off", "オフ"), ("del", "消す")] {
                    let col = if k == "del" { ERR } else { TEXT };
                    if ui.add(egui::Button::new(RichText::new(t).size(12.5).color(col)).fill(CARD).stroke(Stroke::new(1.0, LINE)).corner_radius(CornerRadius::same(8)).min_size(Vec2::new(0.0, 30.0))).clicked() {
                        bulk = Some(k);
                    }
                }
                if ui.add(egui::Button::new(RichText::new("選ぶのをやめる").size(12.5).color(MUTED)).frame(false)).clicked() {
                    self.queue_sel.clear();
                }
            });
        }
        ui.add_space(10.0);
        if let Some(k) = sort {
            self.sort_jobs(k);
        }
        if jobs.is_empty() {
            egui::Frame::new().fill(TABLE).stroke(Stroke::new(1.0, LINE)).corner_radius(CornerRadius::same(14)).inner_margin(egui::Margin::same(18)).show(ui, |ui| {
                ui.set_width(w - 36.0);
                ui.label(RichText::new("まだ何もありません。").color(MUTED));
                ui.label(RichText::new("「アドオンを入れる」を押し、Blender の設定でアドオンを有効にして、出力のプロパティの「Falcon Launcher」で「ランチャーに登録」を入れてください。").size(12.5).color(FAINT));
            });
            return;
        }
        let mut edit: Vec<(u64, &'static str)> = vec![];
        let jobs = if sort.is_some() { self.queue.snapshot() } else { jobs };
        self.job_table(ui, w, &jobs, &mut edit);
        // 選んだ物をまとめて
        if let Some(k) = bulk {
            let sel = self.queue_sel.clone();
            match k {
                "del" => {
                    for id in &sel {
                        self.queue.remove(*id);
                    }
                    self.queue_sel.clear();
                }
                _ => self.queue.edit(|js| {
                    let (mut picked, mut rest): (Vec<queue::Job>, Vec<queue::Job>) = js.drain(..).partition(|j| sel.contains(&j.id));
                    match k {
                        "on" | "off" => {
                            for j in picked.iter_mut() {
                                j.on = k == "on";
                            }
                            // 並びは元のまま
                            let order: Vec<u64> = jobs.iter().map(|j| j.id).collect();
                            rest.append(&mut picked);
                            rest.sort_by_key(|j| order.iter().position(|x| *x == j.id).unwrap_or(usize::MAX));
                            *js = rest;
                        }
                        "top" => {
                            picked.append(&mut rest);
                            *js = picked;
                        }
                        "bottom" => {
                            rest.append(&mut picked);
                            *js = rest;
                        }
                        _ => {
                            // 上へ・下へ: 選んだ物をそれぞれ 1 つずつ動かす
                            let order: Vec<u64> = jobs.iter().map(|j| j.id).collect();
                            rest.append(&mut picked);
                            rest.sort_by_key(|j| order.iter().position(|x| *x == j.id).unwrap_or(usize::MAX));
                            let len = rest.len();
                            if k == "up" {
                                for i in 1..len {
                                    if sel.contains(&rest[i].id) && !sel.contains(&rest[i - 1].id) {
                                        rest.swap(i, i - 1);
                                    }
                                }
                            } else {
                                for i in (0..len.saturating_sub(1)).rev() {
                                    if sel.contains(&rest[i].id) && !sel.contains(&rest[i + 1].id) {
                                        rest.swap(i, i + 1);
                                    }
                                }
                            }
                            *js = rest;
                        }
                    }
                }),
            }
            if k == "on" {
                self.queue.kick(self.builds.clone(), ctx.clone());
            }
        }
        let mut kick = false;
        for (id, what) in edit {
            match what {
                "del" => {
                    self.queue.remove(id);
                    self.queue_sel.remove(&id);
                }
                "halt" => self.queue.halt_current(),
                _ => self.queue.edit(|js| {
                    let Some(i) = js.iter().position(|j| j.id == id) else { return };
                    match what {
                        "toggle" => {
                            js[i].on = !js[i].on;
                            kick = js[i].on;
                        }
                        "up" if i > 0 => js.swap(i, i - 1),
                        "down" if i + 1 < js.len() => js.swap(i, i + 1),
                        "top" => {
                            let j = js.remove(i);
                            js.insert(0, j);
                        }
                        "bottom" => {
                            let j = js.remove(i);
                            js.push(j);
                        }
                        "restart" => {
                            // 描いた絵は消さず、次は新しいフォルダへ
                            js[i].out = PathBuf::new();
                            js[i].state = JobState::Wait;
                            js[i].progress.clear();
                            kick = true;
                        }
                        "retry" => {
                            js[i].state = JobState::Partial;
                            kick = true;
                        }
                        _ => {}
                    }
                }),
            }
        }
        if kick {
            self.queue.kick(self.builds.clone(), ctx.clone());
        }
    }

    // ---------- ファイル構造 ----------

    pub(super) fn files_page(&mut self, ui: &mut egui::Ui, b: &Build, w: f32) {
        let mut places: Vec<(&str, &str, PathBuf)> = vec![];
        if !b.repo.as_os_str().is_empty() {
            places.push((I_BRANCH, "ソース", b.repo.clone()));
        }
        if !b.dir.as_os_str().is_empty() {
            places.push((I_HDD, "ビルド先", b.dir.clone()));
        }
        if let Some(bin) = scan::blender_bin(b) {
            places.push((I_PLAY, "本体", bin));
        }
        places.push((I_LOG, "ビルドのログ", b.dir.join("build.log")));
        places.push((I_LICENSE, "説明(詳細の頁)", PathBuf::from(&crate::places::places().about).join(format!("{}.md", b.key))));
        places.push((I_PICTURE, "予約で描いた絵", PathBuf::from(scan::render_dir())));
        places.push((I_PICTURE, "ギャラリー(手で置く絵)", PathBuf::from(&crate::places::places().gallery)));
        if !crate::places::places().project.is_empty() {
            places.push((I_BUILD, "建てる台本", PathBuf::from(format!("{}/scripts", crate::places::places().project))));
        }
        places.push((I_FOLDER, "ランチャーの設定", config_dir()));
        ui.label(RichText::new(format!("{} の場所 · 押すとファイルの画面で開きます。▸ で中を広げます", b.title)).size(12.5).color(FAINT));
        ui.add_space(8.0);
        for (icon, label, path) in places {
            let exists = path.exists();
            self.file_row(ui, w, 0, icon, label, &path, exists, true);
            if exists && path.is_dir() && self.fs_open.contains(&path) {
                self.file_tree(ui, w, &path, 1);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn file_row(&mut self, ui: &mut egui::Ui, w: f32, depth: usize, icon: &str, label: &str, path: &Path, exists: bool, top: bool) {
        let (r, resp) = ui.allocate_exact_size(Vec2::new(w, if top { 40.0 } else { 26.0 }), Sense::click());
        let p = ui.painter();
        if resp.hovered() {
            p.rect_filled(r, CornerRadius::same(8), CARD_HI);
        } else if top {
            p.rect_filled(r, CornerRadius::same(8), CARD.gamma_multiply(0.6));
        }
        let x0 = r.left() + 10.0 + depth as f32 * 18.0;
        let is_dir = path.is_dir();
        let open = self.fs_open.contains(path);
        // ▸ を押すと広げる、それ以外を押すと開く
        let arrow = Rect::from_min_size(Pos2::new(x0, r.top()), Vec2::new(18.0, r.height()));
        if is_dir {
            p.text(arrow.center(), Align2::CENTER_CENTER, if open { I_DOWN } else { I_RIGHT }, FontId::proportional(10.0), MUTED);
        }
        p.text(Pos2::new(x0 + 22.0, r.center().y), Align2::LEFT_CENTER, icon, FontId::proportional(13.0), if exists { MUTED } else { FAINT });
        let tx = x0 + 44.0;
        if top {
            p.text(Pos2::new(tx, r.center().y), Align2::LEFT_CENTER, label, bold(13.5), if exists { TEXT } else { FAINT });
            let pw = r.right() - tx - 150.0;
            p.galley(Pos2::new(tx + 150.0, r.center().y - 8.0), elide(ui, &path.display().to_string(), FontId::monospace(12.0), FAINT, pw), FAINT);
            if !exists {
                p.text(r.right_center() - Vec2::new(10.0, 0.0), Align2::RIGHT_CENTER, "まだ無い", FontId::proportional(11.5), FAINT);
            }
        } else {
            p.galley(Pos2::new(tx, r.center().y - 8.0), elide(ui, label, FontId::proportional(13.0), if is_dir { TEXT } else { MUTED }, r.right() - tx - 90.0), TEXT);
            if let Ok(md) = std::fs::metadata(path) {
                if md.is_file() {
                    p.text(r.right_center() - Vec2::new(10.0, 0.0), Align2::RIGHT_CENTER, format!("{:.1} MB", md.len() as f64 / 1e6), FontId::proportional(11.0), FAINT);
                }
            }
        }
        let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(path.display().to_string());
        if resp.clicked() && exists {
            let on_arrow = resp.interact_pointer_pos().is_some_and(|q| arrow.contains(q));
            if is_dir && on_arrow {
                if !self.fs_open.remove(path) {
                    self.fs_open.insert(path.to_path_buf());
                }
            } else {
                open_path(path);
            }
        }
    }

    fn file_tree(&mut self, ui: &mut egui::Ui, w: f32, dir: &Path, depth: usize) {
        if depth > 8 {
            return;
        }
        let entries = self.fs_cache.entry(dir.to_path_buf()).or_insert_with(|| {
            let mut v: Vec<(PathBuf, bool)> = std::fs::read_dir(dir).map(|r| r.flatten().map(|e| (e.path(), e.file_type().is_ok_and(|t| t.is_dir()))).collect()).unwrap_or_default();
            v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            v
        }).clone();
        let total = entries.len();
        for (path, is_dir) in entries.into_iter().take(200) {
            let name = path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            self.file_row(ui, w, depth, if is_dir { I_FOLDER } else { I_FILE }, &name, &path, true, false);
            if is_dir && self.fs_open.contains(&path) {
                self.file_tree(ui, w, &path, depth + 1);
            }
        }
        if total > 200 {
            ui.label(RichText::new(format!("{}… ほか {} 個(フォルダを開いて見てください)", "    ".repeat(depth + 2), total - 200)).size(11.5).color(FAINT));
        }
    }

}
