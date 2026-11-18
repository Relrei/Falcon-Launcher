//! 外枠(2026-11-16)。
//! 並べ方: 左に細いアイコンの列(頁と版)、上に戻る・進むと今いる所、
//! 右に「使う版・最近開いたファイル・お知らせ」の列、真ん中に版ごとの頁(タブ・探す欄・行の一覧)。
//! 色・ロゴ・絵・コードはどれも自前。

use super::*;
use super::page::{bold, day_of, elide};
use queue::JobState;

mod render_panel;
mod setup_page;

pub(super) const RAIL_W: f32 = 68.0;
pub(super) const TOP_H: f32 = 50.0;
const RIGHT_W: f32 = 300.0;
/// 外側(左の列と上の帯)。中身の面(BG)より一段暗い
pub(super) const OUTER: Color32 = Color32::from_rgb(0x16, 0x18, 0x1c);
/// お知らせの絵(Falcon で描いたプリズムの虹)
const NEWS_IMG: egui::ImageSource<'static> = egui::include_image!("../assets/bg.jpg");

const I_HOME: &str = "\u{f015}";
const I_COMPASS: &str = "\u{f14e}";
const I_GEAR: &str = "\u{f013}";
const I_HISTORY: &str = "\u{f1da}";
const I_TASKS: &str = "\u{f0ae}";
const I_SORT: &str = "\u{f0dc}";
const I_CODE: &str = "\u{f121}";
const I_UP: &str = "\u{f077}";
const I_DOWNLOAD: &str = "\u{f019}";

/// 戻る・進むで覚える所
pub(super) type NavState = (View, String, StartTab);

/// 丸みのある普通のボタン
fn soft_button(text: impl Into<String>) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.into()).font(bold(13.5)).color(TEXT)).fill(CARD).stroke(Stroke::new(1.0, LINE)).corner_radius(CornerRadius::same(10)).min_size(Vec2::new(0.0, 36.0))
}

/// 色の付いた主なボタン
fn accent_button(text: impl Into<String>) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.into()).font(bold(13.5)).color(ON_ACCENT)).fill(ACCENT).stroke(Stroke::NONE).corner_radius(CornerRadius::same(10)).min_size(Vec2::new(0.0, 36.0))
}

/// 行の中の小さな「開く」(薄い色に縁)
fn row_button(text: impl Into<String>) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.into()).font(bold(13.0)).color(ACCENT_TEXT)).fill(ACCENT.gamma_multiply(0.12)).stroke(Stroke::new(1.0, ACCENT.gamma_multiply(0.7))).corner_radius(CornerRadius::same(9)).min_size(Vec2::new(0.0, 32.0))
}

/// 探す欄(虫めがね付きの横長の箱)
fn search_box(ui: &mut egui::Ui, text: &mut String, hint: &str, w: f32) -> egui::Response {
    egui::Frame::new().fill(CARD).stroke(Stroke::new(1.0, LINE)).corner_radius(CornerRadius::same(10)).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
        ui.set_width(w - 24.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{I_SEARCH} ")).size(13.0).color(FAINT));
            ui.add(egui::TextEdit::singleline(text).hint_text(hint).frame(egui::Frame::NONE).margin(egui::Margin::symmetric(0, 3)).desired_width(f32::INFINITY))
        })
        .inner
    })
    .inner
}

/// 版の印(Falcon は色の箱に立方体、公式は橙に B)
fn paint_build_icon(ui: &egui::Ui, r: Rect, b: &Build, radius: u8) {
    let falcon = !b.key.starts_with("official:");
    let p = ui.painter();
    p.rect_filled(r, CornerRadius::same(radius), CARD_HI);
    p.text(r.center(), Align2::CENTER_CENTER, if falcon { I_CUBE } else { "B" }, if falcon { FontId::proportional(r.height() * 0.45) } else { bold(r.height() * 0.45) }, if falcon { ACCENT } else { ORANGE });
}

/// 見出し(右に「すべて」などの字のボタン)。押されたら true
fn heading(ui: &mut egui::Ui, title: &str, n: usize, more: &str) -> bool {
    let mut hit = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).font(bold(17.0)).color(TEXT));
        if n > 0 {
            ui.label(RichText::new(n.to_string()).size(13.0).color(FAINT));
        }
        if !more.is_empty() {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                hit = ui.add(egui::Button::new(RichText::new(more).size(12.5).color(MUTED)).frame(false)).clicked();
            });
        }
    });
    hit
}

/// 月の見出し(今月・先月・2026 年 9 月)
fn month_label(ts: i64) -> (i64, String) {
    let ym = |t: i64| {
        let (y, m, _) = civil((t + tz_offset()).div_euclid(86400));
        (y, m)
    };
    let (y, m) = ym(ts);
    let (ny, nm) = ym(now());
    let key = y * 12 + m;
    let label = if key == ny * 12 + nm {
        "今月".to_string()
    } else if key == ny * 12 + nm - 1 {
        "先月".to_string()
    } else {
        format!("{y} 年 {m} 月")
    };
    (key, label)
}

/// .blend の行で押された物
enum BlendHit {
    Open(PathBuf),
    Queue(PathBuf),
    Folder(PathBuf),
}

impl App {
    // ---------- 戻る・進む ----------

    pub(super) fn track_history(&mut self) {
        let now = (self.view, self.sel.clone(), self.start_tab);
        match &self.nav_last {
            Some(last) if *last != now => {
                if !self.nav_jump {
                    self.nav_back.push(last.clone());
                    self.nav_fwd.clear();
                    if self.nav_back.len() > 100 {
                        self.nav_back.remove(0);
                    }
                }
                self.nav_jump = false;
                self.nav_last = Some(now);
            }
            None => self.nav_last = Some(now),
            _ => self.nav_jump = false,
        }
    }

    fn restore(&mut self, s: NavState) {
        self.view = s.0;
        self.sel = s.1;
        self.start_tab = s.2;
        self.album_open = None;
        self.pages.clear();
        self.nav_jump = true;
    }

    fn go_back(&mut self) {
        if let Some(s) = self.nav_back.pop() {
            if let Some(cur) = self.nav_last.clone() {
                self.nav_fwd.push(cur);
            }
            self.restore(s);
        }
    }

    fn go_fwd(&mut self) {
        if let Some(s) = self.nav_fwd.pop() {
            if let Some(cur) = self.nav_last.clone() {
                self.nav_back.push(cur);
            }
            self.restore(s);
        }
    }

    fn goto(&mut self, view: View, ctx: &egui::Context) {
        self.view = view;
        self.album_open = None;
        self.pages.clear();
        if view == View::Prep {
            self.prep_checks = prep::check_all(&self.prep_paths);
        }
        if view == View::Trees && self.trees.is_empty() && self.trees_rx.is_none() {
            self.start_trees(ctx);
        }
    }

    /// 版の頁を開く(tab が None なら今のタブのまま)
    pub(super) fn open_version(&mut self, key: &str, tab: Option<StartTab>) {
        self.sel = key.to_string();
        self.page = key.to_string();
        if let Some(t) = tab {
            self.start_tab = t;
        }
        self.album_open = None;
        self.pages.clear();
        self.view = View::Start;
    }

    // ---------- 上の帯 ----------

    fn here(&self) -> (&'static str, String) {
        match self.view {
            View::Dash => (I_HOME, "ホーム".into()),
            View::Start => (I_CUBE, self.cur_build().map(|b| b.title).unwrap_or_default()),
            View::Home => (I_COMPASS, "ビルドを探す・建てる".into()),
            View::Renders => (I_PICTURE, "レンダリング結果".into()),
            View::Queue => (I_TASKS, "レンダリングの一覧".into()),
            View::Trees => (I_CODE, "木と枝".into()),
            View::Log => (I_LOG, "ログ".into()),
            View::Settings => (I_GEAR, "設定".into()),
            View::Prep => (I_DOWNLOAD, "建てる準備".into()),
        }
    }

    pub(super) fn top_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let pill = |ui: &mut egui::Ui, dot: Color32, text: String| -> egui::Response {
            let mut job = egui::text::LayoutJob::default();
            job.append("● ", 0.0, egui::TextFormat { font_id: FontId::proportional(10.0), color: dot, valign: egui::Align::Center, ..Default::default() });
            job.append(&text, 0.0, egui::TextFormat { font_id: FontId::proportional(12.5), color: MUTED, valign: egui::Align::Center, ..Default::default() });
            ui.add(egui::Button::new(job).fill(Color32::TRANSPARENT).stroke(Stroke::new(1.0, LINE)).corner_radius(CornerRadius::same(15)).min_size(Vec2::new(0.0, 30.0)))
        };
        ui.horizontal_centered(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let r = egui::Button::new(RichText::new(I_REFRESH).size(13.0).color(MUTED)).fill(Color32::TRANSPARENT).stroke(Stroke::new(1.0, LINE)).corner_radius(CornerRadius::same(15)).min_size(Vec2::splat(30.0));
                if ui.add(r).on_hover_text("見回す(ビルド・絵・ファイルを読み直す)").clicked() {
                    self.refresh_all(ctx);
                }
                // 起動中の Blender
                let n = self.running.len();
                let text = if n == 0 { "起動中の Blender なし".to_string() } else { format!("Blender {n} 本起動中") };
                pill(ui, if n == 0 { FAINT } else { OK }, text);
                // 描いている物
                if let Some(j) = self.queue.snapshot().into_iter().find(|j| matches!(j.state, JobState::Running | JobState::Paused)) {
                    let c = if j.state == JobState::Paused { WARN } else { ACCENT };
                    if pill(ui, c, format!("{} を描いています · {} 枚", j.name(), j.done_frames())).on_hover_text("レンダリングの一覧を開く").clicked() {
                        self.goto(View::Queue, ctx);
                    }
                }
                // ビルド中
                let building: Vec<(String, String)> = self.builds.iter().filter_map(|b| b.building.as_ref().map(|p| (b.key.clone(), format!("{} をビルド中 {} / {}", b.title, p.done, p.total)))).collect();
                for (k, t) in building {
                    if pill(ui, WARN, t).on_hover_text("ログを開く").clicked() {
                        self.apply(vec![Action::ShowLog(k)]);
                    }
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    ui.add_space(6.0);
                    let (ir, _) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::hover());
                    ui.painter().rect_filled(ir, CornerRadius::same(7), ACCENT);
                    ui.painter().text(ir.center(), Align2::CENTER_CENTER, I_CUBE, FontId::proportional(14.0), ON_ACCENT);
                    ui.label(RichText::new("Falcon Launcher").font(bold(18.0)).color(TEXT));
                    ui.add_space(8.0);
                    for back in [true, false] {
                        let can = if back { !self.nav_back.is_empty() } else { !self.nav_fwd.is_empty() };
                        let b = egui::Button::new(RichText::new(if back { I_LEFT } else { I_RIGHT }).size(10.5).color(if can { TEXT } else { FAINT })).fill(Color32::TRANSPARENT).stroke(Stroke::new(1.0, LINE)).corner_radius(CornerRadius::same(14)).min_size(Vec2::splat(28.0));
                        if ui.add_enabled(can, b).on_hover_text(if back { "戻る" } else { "進む" }).clicked() {
                            if back {
                                self.go_back();
                            } else {
                                self.go_fwd();
                            }
                        }
                    }
                    ui.add_space(6.0);
                    let (icon, text) = self.here();
                    ui.label(RichText::new(format!("{icon} ")).size(14.0).color(MUTED));
                    ui.label(RichText::new(text).font(bold(14.5)).color(TEXT));
                    if !self.message.is_empty() {
                        ui.add_space(10.0);
                        ui.add(egui::Label::new(RichText::new(&self.message).size(12.0).color(FAINT)).truncate()).on_hover_text(&self.message);
                    }
                });
            });
        });
    }

    // ---------- 左の細い列 ----------

    fn rail_button(ui: &mut egui::Ui, icon: &str, on: bool, dot: bool) -> egui::Response {
        let (r, resp) = ui.allocate_exact_size(Vec2::splat(46.0), Sense::click());
        let p = ui.painter();
        if on {
            p.circle_filled(r.center(), 23.0, ACCENT.gamma_multiply(0.18));
        } else if resp.hovered() {
            p.circle_filled(r.center(), 23.0, CARD_HI);
        }
        p.text(r.center(), Align2::CENTER_CENTER, icon, FontId::proportional(18.0), if on { ACCENT_TEXT } else { MUTED });
        if dot {
            p.circle_filled(r.right_top() + Vec2::new(-9.0, 9.0), 4.0, ACCENT);
        }
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    }

    pub(super) fn rail(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let mut open = None;
        let mut go = None;
        ui.vertical_centered(|ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.add_space(10.0);
            let items = [(View::Dash, I_HOME, "ホーム"), (View::Home, I_COMPASS, "ビルドを探す・建てる"), (View::Renders, I_PICTURE, "レンダリング結果"), (View::Queue, I_TASKS, "レンダリングの一覧"), (View::Trees, I_CODE, "木と枝"), (View::Prep, I_DOWNLOAD, "建てる準備(要る道具と SDK)")];
            for (view, icon, tip) in items {
                let dot = view == View::Queue && self.queue.is_running();
                if Self::rail_button(ui, icon, self.view == view, dot).on_hover_text(tip).clicked() {
                    go = Some(view);
                }
            }
            ui.add_space(4.0);
            let (lr, _) = ui.allocate_exact_size(Vec2::new(32.0, 1.0), Sense::hover());
            ui.painter().rect_filled(lr, CornerRadius::ZERO, LINE);
            ui.add_space(4.0);
            // 版の列
            for (_, list) in self.left_groups() {
                for b in list {
                    let (r, resp) = ui.allocate_exact_size(Vec2::splat(46.0), Sense::click());
                    let on = self.view == View::Start && self.sel == b.key;
                    if on {
                        ui.painter().circle_filled(r.center(), 23.0, ACCENT.gamma_multiply(0.18));
                    } else if resp.hovered() {
                        ui.painter().circle_filled(r.center(), 23.0, CARD_HI);
                    }
                    paint_build_icon(ui, Rect::from_center_size(r.center(), Vec2::splat(32.0)), &b, 8);
                    if self.is_running(&b) {
                        ui.painter().circle_filled(r.right_bottom() - Vec2::new(9.0, 9.0), 4.5, OK);
                    }
                    let (_, state) = self.state_of(&b);
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(format!("{}  ·  {state}", b.title)).clicked() {
                        open = Some(b.key.clone());
                    }
                }
            }
        });
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
            ui.add_space(12.0);
            // 設定
            if Self::rail_button(ui, I_GEAR, self.view == View::Settings, false).on_hover_text("設定").clicked() {
                go = Some(View::Settings);
            }
            if Self::rail_button(ui, I_LOG, self.view == View::Log, false).on_hover_text("ビルドのログ").clicked() {
                go = Some(View::Log);
            }
        });
        if let Some(k) = open {
            self.open_version(&k, None);
        }
        if let Some(v) = go {
            self.goto(v, ctx);
        }
    }

    // ---------- 真ん中と右の列 ----------

    pub(super) fn main_area(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let full = ui.max_rect();
        let p = ui.painter();
        let cr = CornerRadius { nw: 16, ne: 0, sw: 0, se: 0 };
        p.rect_filled(full, cr, BG);
        p.rect_stroke(full, cr, Stroke::new(1.0, LINE), StrokeKind::Inside);
        let right = Rect::from_min_max(Pos2::new(full.right() - RIGHT_W, full.top()), full.max);
        let content = Rect::from_min_max(full.min, Pos2::new(right.left(), full.bottom()));
        p.rect_filled(Rect::from_min_size(right.min, Vec2::new(1.0, right.height())), CornerRadius::ZERO, LINE);
        let mut cui = ui.new_child(egui::UiBuilder::new().id_salt("content").max_rect(content.shrink(1.0)).layout(egui::Layout::top_down(egui::Align::Min)));
        cui.set_clip_rect(content.shrink(1.0));
        let mut actions = vec![];
        match self.view {
            View::Dash => self.dash_page(&mut cui, ctx),
            View::Start => self.version_page(&mut cui, ctx),
            View::Home => {
                Self::page_scroll(&mut cui, "discover", |ui, w| {
                    self.home_list(ui, w, &mut actions);
                });
            }
            View::Renders => {
                Self::page_scroll(&mut cui, "renders", |ui, w| {
                    ui.label(RichText::new("レンダリング結果").font(bold(24.0)).color(TEXT));
                    ui.add_space(12.0);
                    self.renders_grid(ui, None, w);
                });
            }
            View::Queue => {
                Self::page_scroll(&mut cui, "queue", |ui, w| {
                    ui.label(RichText::new("レンダリングの一覧").font(bold(24.0)).color(TEXT));
                    ui.add_space(12.0);
                    self.queue_page(ui, w, ctx);
                });
            }
            View::Trees => {
                egui::Frame::new().inner_margin(egui::Margin::symmetric(24, 6)).show(&mut cui, |ui| self.trees_view(ui, ctx));
            }
            View::Settings => {
                Self::page_scroll(&mut cui, "settings", |ui, w| self.settings_page(ui, w, ctx));
            }
            View::Prep => {
                Self::page_scroll(&mut cui, "prep", |ui, w| self.prep_page(ui, w, ctx));
            }
            View::Log => {
                egui::Frame::new().inner_margin(egui::Margin::symmetric(24, 10)).show(&mut cui, |ui| self.log_view(ui));
            }
        }
        self.apply(actions);
        let inner = Rect::from_min_max(right.min + Vec2::new(16.0, 0.0), right.max - Vec2::new(14.0, 0.0));
        let mut rui = ui.new_child(egui::UiBuilder::new().id_salt("right").max_rect(inner).layout(egui::Layout::top_down(egui::Align::Min)));
        rui.set_clip_rect(right);
        self.right_panel(&mut rui, ctx);
    }

    /// 真ん中の頁(縦に流れる)。w は中身の幅
    fn page_scroll(ui: &mut egui::Ui, salt: impl std::hash::Hash + std::fmt::Debug, add: impl FnOnce(&mut egui::Ui, f32)) {
        egui::ScrollArea::vertical().id_salt(salt).auto_shrink(false).show(ui, |ui| {
            egui::Frame::new().inner_margin(egui::Margin { left: 24, right: 24, top: 22, bottom: 40 }).show(ui, |ui| {
                let w = ui.available_width();
                ui.set_width(w);
                add(ui, w);
            });
        });
    }

    fn right_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.view == View::Home {
            // ビルドを探す時は、右の列を絞り込みにする
            egui::ScrollArea::vertical().id_salt("right-filters").auto_shrink(false).show(ui, |ui| {
                ui.add_space(16.0);
                self.home_filters(ui);
            });
            return;
        }
        if self.view == View::Queue {
            // レンダリングの一覧では、右の列を選んだシーンの描き方にする
            self.render_panel(ui);
            return;
        }
        let cur = self.cur_build();
        let mut pick = None;
        let mut open = None;
        let mut go_tab = None;
        egui::ScrollArea::vertical().id_salt("right").auto_shrink(false).show(ui, |ui| {
            // 縦の送り棒の分を空ける
            let w = ui.available_width() - 14.0;
            ui.set_max_width(w);
            ui.add_space(16.0);
            // 使う版
            ui.label(RichText::new("使う版").font(bold(15.0)).color(TEXT));
            ui.add_space(6.0);
            if let Some(b) = &cur {
                let (r, resp) = ui.allocate_exact_size(Vec2::new(w, 56.0), Sense::click());
                let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                let p = ui.painter();
                p.rect_filled(r, CornerRadius::same(12), if resp.hovered() { CARD_HI } else { CARD });
                p.rect_stroke(r, CornerRadius::same(12), Stroke::new(1.0, LINE), StrokeKind::Inside);
                paint_build_icon(ui, Rect::from_min_size(r.min + Vec2::new(10.0, 10.0), Vec2::splat(36.0)), b, 9);
                let x = r.left() + 56.0;
                let p = ui.painter();
                p.galley(Pos2::new(x, r.top() + 9.0), elide(ui, &b.title, bold(14.0), TEXT, r.right() - x - 34.0), TEXT);
                let (dot, state) = self.state_of(b);
                p.circle_filled(Pos2::new(x + 3.5, r.top() + 38.0), 3.5, dot);
                p.text(Pos2::new(x + 12.0, r.top() + 38.0), Align2::LEFT_CENTER, state, FontId::proportional(11.5), MUTED);
                p.text(Pos2::new(r.right() - 18.0, r.center().y), Align2::CENTER_CENTER, I_DOWN, FontId::proportional(11.0), MUTED);
                let groups = self.start_groups();
                egui::Popup::menu(&resp).show(|ui| {
                    ui.set_min_width(260.0);
                    egui::ScrollArea::vertical().max_height(460.0).show(ui, |ui| {
                        for (head, list) in &groups {
                            ui.label(RichText::new(*head).size(11.0).color(FAINT));
                            for x in list {
                                if ui.selectable_label(x.key == b.key, &x.title).clicked() {
                                    pick = Some(x.key.clone());
                                    ui.close();
                                }
                            }
                            ui.add_space(4.0);
                        }
                    });
                });
                ui.add_space(4.0);
                ui.label(RichText::new(".blend を開く時と、レンダリングの一覧へ足す時に使います").size(11.0).color(FAINT));
            }
            Self::right_line(ui, w);
            // 最近開いたファイル
            ui.horizontal(|ui| {
                ui.label(RichText::new("最近開いたファイル").font(bold(15.0)).color(TEXT));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(egui::Button::new(RichText::new("すべて").size(12.0).color(ACCENT_TEXT)).frame(false)).clicked() {
                        go_tab = Some(StartTab::Blend);
                    }
                });
            });
            ui.add_space(4.0);
            if self.blends.is_none() {
                self.blends = Some(Arc::new(start::recent_blends()));
            }
            let recent = self.blends.clone().unwrap_or_default();
            if recent.is_empty() {
                ui.label(RichText::new("まだありません").size(12.0).color(FAINT));
            }
            for f in recent.iter().take(6) {
                let (r, resp) = ui.allocate_exact_size(Vec2::new(w, 42.0), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(r, CornerRadius::same(8), CARD);
                }
                let tr = Rect::from_min_size(r.left_top() + Vec2::new(4.0, 5.0), Vec2::new(32.0, 32.0));
                match self.blend_thumb_of(&f.path) {
                    Some(t) => egui::Image::new(format!("file://{}", t.display())).corner_radius(CornerRadius::same(6)).paint_at(ui, tr),
                    None => {
                        ui.painter().rect_filled(tr, CornerRadius::same(6), hue_color(&f.path.display().to_string(), 0.2));
                    }
                }
                let x = tr.right() + 10.0;
                let name = f.path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                let p = ui.painter();
                p.galley(Pos2::new(x, r.top() + 5.0), elide(ui, &name, FontId::proportional(12.5), TEXT, r.right() - x - 4.0), TEXT);
                p.text(Pos2::new(x, r.top() + 30.0), Align2::LEFT_CENTER, ago(f.ts), FontId::proportional(10.5), FAINT);
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(format!("{}\n押すと使う版で開きます", f.path.display())).clicked() {
                    open = Some(f.path.clone());
                }
            }
            Self::right_line(ui, w);
            // お知らせ: 使う版の新しい変更
            ui.label(RichText::new("お知らせ").font(bold(15.0)).color(TEXT));
            ui.add_space(8.0);
            let ih = (w * 0.5).round();
            let (ir, iresp) = ui.allocate_exact_size(Vec2::new(w, ih), Sense::click());
            egui::Image::new(NEWS_IMG).uv(Rect::from_min_max(Pos2::new(0.18, 0.25), Pos2::new(0.82, 0.95))).corner_radius(CornerRadius::same(12)).paint_at(ui, ir);
            if let Some(b) = &cur {
                let list = self.changes.get(&b.key).cloned();
                ui.add_space(8.0);
                match list.as_ref().and_then(|l| l.first()) {
                    Some(c) => {
                        ui.add(egui::Label::new(RichText::new(&c.title).font(bold(14.0)).color(TEXT)).wrap());
                        ui.add_space(2.0);
                        for c2 in list.iter().flat_map(|l| l.iter().skip(1).take(2)) {
                            ui.add(egui::Label::new(RichText::new(format!("· {}", c2.title)).size(12.0).color(MUTED)).wrap());
                        }
                        ui.add_space(4.0);
                        ui.label(RichText::new(format!("{}  ·  {}", b.branch, day_of(c.ts))).size(11.5).color(FAINT));
                    }
                    None => {
                        ui.label(RichText::new(if b.repo.as_os_str().is_empty() { "この版には変更履歴がありません" } else { "変更履歴を読んでいます…" }).size(12.0).color(FAINT));
                    }
                }
                if iresp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    go_tab = Some(StartTab::Changes);
                }
                ui.add_space(6.0);
                if ui.add(egui::Button::new(RichText::new("更新履歴を見る").size(12.5).color(ACCENT_TEXT)).frame(false)).clicked() {
                    go_tab = Some(StartTab::Changes);
                }
                // 建てた物より先の変更
                if let Some(n) = self.aheads.get(&b.key).copied().flatten().filter(|n| *n > 0 && b.has_bin) {
                    ui.add_space(6.0);
                    let can = matches!(b.kind, Kind::Main | Kind::Test);
                    let t = RichText::new(format!("{I_BUILD}  枝が建てた物より {n} 本先です{}", if can { " · 建て直す" } else { "" })).size(12.5).color(WARN);
                    if ui.add(egui::Button::new(t).frame(false)).clicked() && can {
                        self.confirm_build = Some(b.key.clone());
                    }
                }
            }
            Self::right_line(ui, w);
            if let Some((avail, size)) = self.disk {
                // 名前は場所.tsv の disk を書いた時だけ(既定の建て先の名前「build」は、ディスクの名前に見えて紛らわしい)
                let name = crate::places::places().disk_label.clone();
                ui.label(RichText::new(format!("{name}空き {} / {}", gb(avail), gb(size))).size(11.0).color(FAINT));
            }
            if !self.main_branch_now.is_empty() {
                let c = if self.main_branch_now == scan::MAIN_BRANCH { FAINT } else { WARN };
                ui.label(RichText::new(format!("本体の木: {}", self.main_branch_now)).size(11.0).color(c));
            }
            ui.label(RichText::new(format!("最後の見回り {}  ·  Falcon Launcher 0.1.0", when(self.scanned_at))).size(11.0).color(FAINT));
            ui.add_space(16.0);
        });
        if let Some(k) = pick {
            self.sel = k.clone();
            if self.view == View::Start {
                self.page = k;
                self.pages.clear();
            }
        }
        if let Some(p) = open {
            self.open_blend(&p);
        }
        if let Some(t) = go_tab {
            if let Some(b) = &cur {
                self.open_version(&b.key, Some(t));
            }
        }
        let _ = ctx;
    }

    fn right_line(ui: &mut egui::Ui, w: f32) {
        ui.add_space(14.0);
        let (r, _) = ui.allocate_exact_size(Vec2::new(w, 1.0), Sense::hover());
        ui.painter().rect_filled(r, CornerRadius::ZERO, LINE);
        ui.add_space(14.0);
    }

    // ---------- ホーム ----------

    fn dash_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.gallery.is_empty() && self.gallery_rx.is_none() {
            self.start_gallery(ctx);
        }
        if self.blends.is_none() {
            self.blends = Some(Arc::new(start::recent_blends()));
        }
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        if let Some(p) = dropped.into_iter().find(|p| p.extension().is_some_and(|e| e == "blend")) {
            self.path_paste = p.display().to_string();
        }
        let mut hits = vec![];
        let mut actions = vec![];
        let mut open_ver = None;
        let mut go = None;
        Self::page_scroll(ui, "dash", |ui, w| {
            self.update_banner(ui, ctx);
            // 新しく起動する所
            if let Some(b) = self.cur_build() {
                self.launch_block(ui, &b, &mut actions);
            }
            ui.add_space(30.0);
            // 続きから
            heading(ui, "続きから", 0, "");
            ui.add_space(8.0);
            let recent = self.blends.clone().unwrap_or_default();
            if recent.is_empty() {
                ui.label(RichText::new("最近開いた .blend がありません。").color(FAINT));
            }
            for f in recent.iter().take(5) {
                if let Some(h) = self.blend_row(ui, f, w) {
                    hits.push(h);
                }
                ui.add_space(8.0);
            }
            ui.add_space(18.0);
            // 版のカード
            if heading(ui, "版", 0, "ビルドを探す") {
                go = Some(View::Home);
            }
            ui.add_space(8.0);
            let list: Vec<Build> = self.left_groups().into_iter().flat_map(|(_, v)| v).collect();
            if list.is_empty() {
                ui.label(RichText::new(format!("まだ版がありません。左の {I_DOWNLOAD}(建てる準備)で要る物をそろえて Falcon Engine を建てると出ます。公式の Blender は ~/Applications などに置くと自動で出ます")).size(12.5).color(FAINT));
            }
            let gap = 12.0;
            let cols = ((w + gap) / (300.0 + gap)).floor().max(1.0) as usize;
            let cw = (w - gap * (cols as f32 - 1.0)) / cols as f32;
            for row in list.chunks(cols) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for b in row {
                        let (r, resp) = ui.allocate_exact_size(Vec2::new(cw, 76.0), Sense::click());
                        let p = ui.painter();
                        p.rect_filled(r, CornerRadius::same(14), if resp.hovered() { CARD_HI } else { CARD });
                        p.rect_stroke(r, CornerRadius::same(14), Stroke::new(1.0, LINE), StrokeKind::Inside);
                        paint_build_icon(ui, Rect::from_min_size(r.min + Vec2::new(14.0, 14.0), Vec2::splat(48.0)), b, 12);
                        let x = r.left() + 76.0;
                        let p = ui.painter();
                        p.galley(Pos2::new(x, r.top() + 16.0), elide(ui, &b.title, bold(15.0), TEXT, r.right() - x - 110.0), TEXT);
                        let (dot, state) = self.state_of(b);
                        p.circle_filled(Pos2::new(x + 3.5, r.top() + 50.0), 3.5, dot);
                        p.text(Pos2::new(x + 12.0, r.top() + 50.0), Align2::LEFT_CENTER, state, FontId::proportional(12.0), MUTED);
                        let br = Rect::from_min_size(Pos2::new(r.right() - 96.0, r.center().y - 16.0), Vec2::new(82.0, 32.0));
                        if b.launchable() {
                            if place_fill(ui, br, true, row_button(format!("{I_PLAY}  起動"))).clicked() {
                                actions.push(Action::Launch(b.key.clone()));
                            }
                        } else if resp.clicked() {
                            open_ver = Some(b.key.clone());
                        }
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            open_ver = Some(b.key.clone());
                        }
                    }
                });
                ui.add_space(gap);
            }
            ui.add_space(18.0);
            // 最近の絵
            if heading(ui, "最近の絵", 0, "すべて") {
                go = Some(View::Renders);
            }
            ui.add_space(8.0);
            let items: Vec<Media> = self.gallery.iter().flat_map(|a| a.items.iter().filter(|m| m.frames.is_empty()).take(2).cloned()).take(8).collect();
            if items.is_empty() {
                ui.label(RichText::new(if self.gallery_rx.is_some() { "絵を集めています…" } else { "絵はまだありません。" }).color(FAINT));
            }
            let cols = ((w + gap) / (240.0 + gap)).floor().max(1.0) as usize;
            let cw = (w - gap * (cols as f32 - 1.0)) / cols as f32;
            let mut click = None;
            for (ri, row) in items.chunks(cols).enumerate() {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for (ci, m) in row.iter().enumerate() {
                        let (r, resp) = ui.allocate_exact_size(Vec2::new(cw, cw * 9.0 / 16.0), Sense::click());
                        self.paint_thumb(ui, r, m, CornerRadius::same(12), true);
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            click = Some(ri * cols + ci);
                        }
                    }
                });
                ui.add_space(gap);
            }
            if let Some(i) = click {
                self.lightbox = Some((items.clone(), i, PathBuf::new()));
            }
        });
        self.blend_hits(hits);
        self.apply(actions);
        if let Some(k) = open_ver {
            self.open_version(&k, None);
        }
        if let Some(v) = go {
            self.goto(v, ctx);
        }
    }

    // ---------- 版の頁 ----------

    fn version_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(b) = self.cur_build() else {
            ui.label(RichText::new("ビルドが見つかりません").color(FAINT));
            return;
        };
        self.prepare(&b, ctx);
        // 窓へ落とされた .blend は、探す欄へ入れる(「パスで開く」で開ける)
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        if let Some(p) = dropped.into_iter().find(|p| p.extension().is_some_and(|e| e == "blend")) {
            self.blend_query = p.display().to_string();
            self.start_tab = StartTab::Blend;
        }
        if self.start_tab == StartTab::Overview {
            self.start_tab = StartTab::Blend;
        }
        let mut actions = vec![];
        Self::page_scroll(ui, ("ver", b.key.clone()), |ui, w| {
            self.version_header(ui, &b, &mut actions);
            ui.add_space(16.0);
            let (lr, _) = ui.allocate_exact_size(Vec2::new(w, 1.0), Sense::hover());
            ui.painter().rect_filled(lr, CornerRadius::ZERO, LINE);
            ui.add_space(16.0);
            self.version_tabs(ui, &b);
            ui.add_space(14.0);
            self.page = b.key.clone();
            match self.start_tab {
                StartTab::Overview | StartTab::Blend => self.blend_page(ui, w),
                StartTab::Renders => self.renders_grid(ui, Some(&b), w),
                StartTab::Queue => self.queue_page(ui, w, ctx),
                StartTab::Changes => self.changes_tab(ui, &b, w, &mut actions),
                StartTab::Features if b.kind == Kind::Ext => self.scenes_tab(ui, w, &mut actions),
                StartTab::Features => self.features_tab(ui, w, &mut actions),
                StartTab::Files => self.files_page(ui, &b, w),
                StartTab::About => {
                    let side = 280.0;
                    let main = w - side - 20.0;
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 20.0;
                        ui.allocate_ui_with_layout(Vec2::new(main, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                            ui.set_width(main);
                            ui.spacing_mut().item_spacing.x = 8.0;
                            self.about_tab(ui, &b, main);
                        });
                        ui.allocate_ui_with_layout(Vec2::new(side, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                            ui.set_width(side);
                            ui.spacing_mut().item_spacing.x = 8.0;
                            self.page_side(ui, &b, &mut actions);
                        });
                    });
                }
            }
        });
        self.apply(actions);
    }

    fn version_header(&mut self, ui: &mut egui::Ui, b: &Build, actions: &mut Vec<Action>) {
        ui.horizontal(|ui| {
            let (ir, _) = ui.allocate_exact_size(Vec2::splat(64.0), Sense::hover());
            paint_build_icon(ui, ir, b, 14);
            ui.add_space(10.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.add_space(4.0);
                ui.label(RichText::new(&b.title).font(bold(24.0)).color(TEXT));
                let mut meta = vec![format!("{I_CUBE}  {}", b.category)];
                if b.built_ts > 0 {
                    meta.push(format!("{I_CLOCK}  {} に建てた", when(b.built_ts)));
                }
                if let Some(n) = self.size_of(b) {
                    meta.push(format!("{I_HDD}  {}", gb(n)));
                }
                if !b.hash.is_empty() {
                    meta.push(b.hash[..b.hash.len().min(9)].to_string());
                }
                ui.label(RichText::new(meta.join("    ·    ")).size(13.5).color(MUTED));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                let round = |icon: &str| egui::Button::new(RichText::new(icon).size(15.0).color(TEXT)).fill(CARD).stroke(Stroke::new(1.0, LINE)).corner_radius(CornerRadius::same(22)).min_size(Vec2::splat(44.0));
                let more = ui.add(round(I_MORE));
                egui::Popup::menu(&more).show(|ui| {
                    ui.set_min_width(220.0);
                    if matches!(b.kind, Kind::Main | Kind::Test) && ui.button(format!("{I_BUILD}   {}", if b.has_bin { "建て直す" } else { "建てる" })).clicked() {
                        actions.push(Action::Build(b.key.clone()));
                    }
                    if b.dir.join("build.log").is_file() && ui.button(format!("{I_LOG}   ビルドのログ")).clicked() {
                        actions.push(Action::ShowLog(b.key.clone()));
                    }
                    if ui.button(format!("{I_LICENSE}   説明")).clicked() {
                        self.start_tab = StartTab::About;
                    }
                    if ui.button(format!("{I_FOLDER}   ファイル")).clicked() {
                        self.start_tab = StartTab::Files;
                    }
                });
                if ui.add_enabled(b.dir.is_dir(), round(I_FOLDER)).on_hover_text("ビルド先を開く").clicked() {
                    actions.push(Action::Folder(b.dir.clone()));
                }
                let (text, primary) = if b.building.is_some() {
                    (format!("{I_LOG}   ビルド中・ログ"), false)
                } else if b.launchable() {
                    (format!("{I_PLAY}   起動"), true)
                } else {
                    (format!("{I_BUILD}   建てる"), true)
                };
                let btn = egui::Button::new(RichText::new(text).font(bold(16.0)).color(if primary { ON_ACCENT } else { TEXT })).fill(if primary { ACCENT } else { CARD_HI }).stroke(Stroke::NONE).corner_radius(CornerRadius::same(12)).min_size(Vec2::new(118.0, 44.0));
                if ui.add(btn).clicked() {
                    actions.push(if b.building.is_some() {
                        Action::ShowLog(b.key.clone())
                    } else if b.launchable() {
                        Action::Launch(b.key.clone())
                    } else {
                        Action::Build(b.key.clone())
                    });
                }
            });
        });
    }

    fn version_tabs(&mut self, ui: &mut egui::Ui, b: &Build) {
        let n_blend = self.blends.as_ref().map(|v| v.len()).unwrap_or(0);
        let n_renders: usize = self.gallery.iter().filter(|a| scan::album_matches(a, b)).map(|a| a.items.len()).sum();
        let n_queue = self.queue.snapshot().len();
        let n_changes = self.changes.get(&b.key).map(|v| v.len()).unwrap_or(0);
        let mut tabs = vec![(StartTab::Blend, I_CUBE, ".blend", n_blend), (StartTab::Renders, I_PICTURE, "レンダリング結果", n_renders), (StartTab::Queue, I_TASKS, "一覧", n_queue), (StartTab::Changes, I_HISTORY, "更新履歴", n_changes), (StartTab::About, I_LICENSE, "説明", 0)];
        match b.kind {
            Kind::Main => tabs.push((StartTab::Features, I_CAL, "機能", self.features.len())),
            Kind::Ext => tabs.push((StartTab::Features, I_CAL, "場面", self.scenes.len())),
            _ => {
                if self.start_tab == StartTab::Features {
                    self.start_tab = StartTab::Blend;
                }
            }
        }
        tabs.push((StartTab::Files, I_FOLDER, "ファイル", 0));
        egui::Frame::new().fill(CARD).stroke(Stroke::new(1.0, LINE)).corner_radius(CornerRadius::same(14)).inner_margin(egui::Margin::same(5)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for (tab, icon, text, n) in tabs {
                    let on = tab == self.start_tab;
                    let c = if on { ACCENT_TEXT } else { TEXT };
                    let mut job = egui::text::LayoutJob::default();
                    let f = |font: FontId, color: Color32| egui::TextFormat { font_id: font, color, valign: egui::Align::Center, ..Default::default() };
                    job.append(&format!("{icon}   "), 0.0, f(FontId::proportional(13.0), if on { ACCENT_TEXT } else { MUTED }));
                    job.append(text, 0.0, f(bold(13.5), c));
                    if n > 0 {
                        job.append(&format!("  {n}"), 0.0, f(FontId::proportional(12.0), if on { ACCENT_TEXT.gamma_multiply(0.8) } else { FAINT }));
                    }
                    let resp = ui.add(egui::Button::new(job).fill(if on { ACCENT.gamma_multiply(0.16) } else { Color32::TRANSPARENT }).stroke(Stroke::NONE).corner_radius(CornerRadius::same(10)).min_size(Vec2::new(0.0, 36.0)));
                    if resp.clicked() {
                        self.start_tab = tab;
                        self.album_open = None;
                        self.pages.clear();
                    }
                }
            });
        });
    }

    // ---------- .blend ----------

    fn blend_page(&mut self, ui: &mut egui::Ui, w: f32) {
        let list = self.blends.clone().unwrap_or_default();
        let typed = PathBuf::from(self.blend_query.trim().trim_matches('\'').trim_matches('"'));
        let is_path = typed.is_file();
        let mut reload = false;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let n = list.len();
            search_box(ui, &mut self.blend_query, &format!("{n} 個の中から探す · .blend のパスを貼ると開けます"), (w - 270.0).max(200.0));
            if ui.add(soft_button(format!("{I_REFRESH}  読み直す"))).clicked() {
                reload = true;
            }
            if ui.add_enabled(is_path, accent_button(format!("{I_PLAY}  パスで開く"))).on_hover_text("探す欄に貼ったパスの .blend を、使う版で開きます").clicked() {
                self.open_blend(&typed);
            }
        });
        if reload {
            self.blends = Some(Arc::new(start::recent_blends()));
        }
        ui.add_space(10.0);
        // 絞り込みの札(保存した版)
        let mut vers: Vec<String> = vec![];
        for f in list.iter() {
            let v = if f.compressed { "圧縮".to_string() } else if f.version.is_empty() { "不明".into() } else { format!("Blender {}", f.version) };
            if !vers.contains(&v) {
                vers.push(v);
            }
        }
        vers.sort();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.label(RichText::new(I_FILTER).size(13.0).color(MUTED));
            for v in std::iter::once("すべて".to_string()).chain(vers) {
                let on = if v == "すべて" { self.blend_ver.is_none() } else { self.blend_ver.as_deref() == Some(v.as_str()) };
                let b = egui::Button::new(RichText::new(&v).size(12.5).color(if on { ACCENT_TEXT } else { MUTED })).fill(if on { ACCENT.gamma_multiply(0.14) } else { Color32::TRANSPARENT }).stroke(Stroke::new(1.0, if on { ACCENT.gamma_multiply(0.7) } else { LINE })).corner_radius(CornerRadius::same(10)).min_size(Vec2::new(0.0, 28.0));
                if ui.add(b).clicked() {
                    self.blend_ver = if v == "すべて" { None } else { Some(v) };
                }
            }
        });
        ui.add_space(10.0);
        let q = if is_path { String::new() } else { self.blend_query.trim().to_lowercase() };
        let shown: Vec<BlendFile> = list
            .iter()
            .filter(|f| q.is_empty() || f.path.display().to_string().to_lowercase().contains(&q))
            .filter(|f| {
                let v = if f.compressed { "圧縮".to_string() } else if f.version.is_empty() { "不明".into() } else { format!("Blender {}", f.version) };
                self.blend_ver.as_ref().is_none_or(|x| *x == v)
            })
            .cloned()
            .collect();
        if shown.is_empty() {
            ui.label(RichText::new(if list.is_empty() { "最近開いた .blend がありません。" } else { "当てはまる物がありません。" }).color(FAINT));
        }
        let mut hits = vec![];
        for f in &shown {
            if let Some(h) = self.blend_row(ui, f, w) {
                hits.push(h);
            }
            ui.add_space(8.0);
        }
        self.blend_hits(hits);
    }

    /// .blend の 1 行(絵・名前・場所・保存した版・開く・…)
    fn blend_row(&mut self, ui: &mut egui::Ui, f: &BlendFile, w: f32) -> Option<BlendHit> {
        let (r, resp) = ui.allocate_exact_size(Vec2::new(w, 70.0), Sense::click());
        if !ui.is_rect_visible(r) {
            return None;
        }
        let mut hit = None;
        let p = ui.painter().clone();
        p.rect_filled(r, CornerRadius::same(14), if resp.hovered() { CARD_HI } else { CARD });
        p.rect_stroke(r, CornerRadius::same(14), Stroke::new(1.0, LINE), StrokeKind::Inside);
        let tr = Rect::from_min_size(r.min + Vec2::new(12.0, 11.0), Vec2::splat(48.0));
        match self.blend_thumb_of(&f.path) {
            Some(t) => {
                p.rect_filled(tr, CornerRadius::same(10), BG);
                egui::Image::new(format!("file://{}", t.display())).corner_radius(CornerRadius::same(10)).paint_at(ui, tr);
            }
            None => {
                p.rect_filled(tr, CornerRadius::same(10), hue_color(&f.path.display().to_string(), 0.2));
                p.text(tr.center(), Align2::CENTER_CENTER, I_CUBE, FontId::proportional(18.0), TEXT.gamma_multiply(0.5));
            }
        }
        let x = tr.right() + 14.0;
        let name = f.path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let name_w = (w * 0.42).max(160.0);
        let g = elide(ui, &name, bold(14.5), TEXT, name_w);
        let nw = g.size().x;
        p.galley(Pos2::new(x, r.top() + 14.0), g, TEXT);
        let dir = f.path.parent().and_then(|d| d.file_name()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        p.galley(Pos2::new(x + nw + 10.0, r.top() + 16.0), elide(ui, &format!("{I_FOLDER}  {dir}"), FontId::proportional(12.5), MUTED, (w * 0.5 - nw).max(40.0)), MUTED);
        p.text(Pos2::new(x, r.top() + 50.0), Align2::LEFT_CENTER, format!("{}  ·  {:.0} MB", ago(f.ts), f.bytes as f64 / 1e6), FontId::proportional(12.0), FAINT);
        // 真ん中: 保存した版
        let ver = if f.compressed { "圧縮して保存".to_string() } else if f.version.is_empty() { "版は不明".into() } else { format!("Blender {} で保存", f.version) };
        p.text(Pos2::new(r.left() + w * 0.62, r.center().y), Align2::CENTER_CENTER, format!("{I_CUBE}  {ver}"), FontId::proportional(13.0), MUTED);
        // 右: 開く・…
        let mr = Rect::from_min_size(Pos2::new(r.right() - 44.0, r.center().y - 16.0), Vec2::new(32.0, 32.0));
        let br = Rect::from_min_size(Pos2::new(mr.left() - 92.0, r.center().y - 16.0), Vec2::new(86.0, 32.0));
        if place_fill(ui, br, true, row_button(format!("{I_PLAY}  開く"))).on_hover_text("使う版で開く").clicked() {
            hit = Some(BlendHit::Open(f.path.clone()));
        }
        let more = place_fill(ui, mr, true, egui::Button::new(RichText::new(I_MORE).size(14.0).color(MUTED)).frame(false));
        egui::Popup::menu(&more).show(|ui| {
            ui.set_min_width(220.0);
            if ui.button(format!("{I_PLAY}   使う版で開く")).clicked() {
                hit = Some(BlendHit::Open(f.path.clone()));
            }
            if ui.button(format!("{I_TASKS}   レンダリングの一覧に足す")).clicked() {
                hit = Some(BlendHit::Queue(f.path.clone()));
            }
            if ui.button(format!("{I_FOLDER}   フォルダを開く")).clicked() {
                hit = Some(BlendHit::Folder(f.path.clone()));
            }
        });
        if hit.is_none() && resp.on_hover_text(f.path.display().to_string()).double_clicked() {
            hit = Some(BlendHit::Open(f.path.clone()));
        }
        hit
    }

    fn blend_hits(&mut self, hits: Vec<BlendHit>) {
        for h in hits {
            match h {
                BlendHit::Open(p) => self.open_blend(&p),
                BlendHit::Queue(p) => {
                    if let Some(b) = self.cur_build() {
                        self.queue.add(p.clone(), &b, None);
                        self.message = format!("{} をレンダリングの一覧に足しました({})", p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(), b.title);
                    }
                }
                BlendHit::Folder(p) => {
                    if let Some(d) = p.parent() {
                        let _ = Command::new("xdg-open").arg(d).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
                    }
                }
            }
        }
    }

    // ---------- レンダリング結果 ----------

    fn renders_grid(&mut self, ui: &mut egui::Ui, only: Option<&Build>, w: f32) {
        let gallery = self.gallery.clone();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            search_box(ui, &mut self.gallery_query, "組の名前・説明で探す", w);
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let sort = ui.add(soft_button(format!("{I_SORT}  {}  {I_DOWN}", if self.render_old_first { "古い順" } else { "新しい順" })));
            egui::Popup::menu(&sort).show(|ui| {
                if ui.selectable_label(!self.render_old_first, "新しい順").clicked() {
                    self.render_old_first = false;
                }
                if ui.selectable_label(self.render_old_first, "古い順").clicked() {
                    self.render_old_first = true;
                }
            });
            let group = ui.add(soft_button(format!("{I_GRID}  {}  {I_DOWN}", if self.render_by_album { "組ごと" } else { "月ごと" })));
            egui::Popup::menu(&group).show(|ui| {
                if ui.selectable_label(!self.render_by_album, "月ごと").clicked() {
                    self.render_by_album = false;
                }
                if ui.selectable_label(self.render_by_album, "組ごと").clicked() {
                    self.render_by_album = true;
                }
            });
        });
        ui.add_space(14.0);
        if gallery.is_empty() {
            ui.label(RichText::new(if self.gallery_rx.is_some() { "絵と動画を集めています…" } else { "絵も動画もまだありません。" }).color(FAINT));
            return;
        }
        let q = self.gallery_query.to_lowercase();
        let mut albums: Vec<&Album> = gallery.iter().filter(|a| only.is_none_or(|b| scan::album_matches(a, b))).filter(|a| q.is_empty() || a.title.to_lowercase().contains(&q) || a.note.to_lowercase().contains(&q)).collect();
        albums.sort_by_key(|a| a.ts);
        if !self.render_old_first {
            albums.reverse();
        }
        // 組を見出しにまとめる(並びは保つ)
        let mut groups: Vec<(String, String, Vec<(String, Media)>)> = vec![];
        for a in albums {
            let (key, label) = if self.render_by_album { (a.dir.display().to_string(), a.title.clone()) } else {
                let (k, l) = month_label(a.ts);
                (k.to_string(), l)
            };
            let items = a.items.iter().map(|m| (a.title.clone(), m.clone()));
            match groups.iter_mut().find(|g| g.0 == key) {
                Some(g) => g.2.extend(items),
                None => groups.push((key, label, items.collect())),
            }
        }
        if groups.is_empty() {
            ui.label(RichText::new("当てはまる物がありません。").color(FAINT));
            return;
        }
        let gap = 12.0;
        let cols = ((w + gap) / (280.0 + gap)).floor().max(1.0) as usize;
        let cw = (w - gap * (cols as f32 - 1.0)) / cols as f32;
        let th = cw * 9.0 / 16.0;
        let mut click = None;
        for (key, label, items) in &groups {
            let closed = self.render_closed.contains(key);
            let (hr, hresp) = ui.allocate_exact_size(Vec2::new(w, 30.0), Sense::click());
            let p = ui.painter();
            p.text(Pos2::new(hr.left() + 8.0, hr.center().y), Align2::CENTER_CENTER, if closed { I_DOWN } else { I_UP }, FontId::proportional(11.0), MUTED);
            let g = elide(ui, label, bold(15.0), TEXT, w - 120.0);
            let lw = g.size().x;
            p.galley(Pos2::new(hr.left() + 24.0, hr.center().y - g.size().y / 2.0), g, TEXT);
            let cnt = items.len().to_string();
            let cr = Rect::from_min_size(Pos2::new(hr.left() + 32.0 + lw, hr.center().y - 10.0), Vec2::new(10.0 + 7.0 * cnt.len() as f32, 20.0));
            p.rect_filled(cr, CornerRadius::same(10), CARD_HI);
            p.text(cr.center(), Align2::CENTER_CENTER, cnt, FontId::proportional(11.5), MUTED);
            if hresp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && !self.render_closed.remove(key) {
                self.render_closed.insert(key.clone());
            }
            let (lr, _) = ui.allocate_exact_size(Vec2::new(w, 1.0), Sense::hover());
            ui.painter().rect_filled(lr, CornerRadius::ZERO, LINE);
            ui.add_space(12.0);
            if !closed {
                for (ri, row) in items.chunks(cols).enumerate() {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = gap;
                        for (ci, (title, m)) in row.iter().enumerate() {
                            let (r, resp) = ui.allocate_exact_size(Vec2::new(cw, th), Sense::click());
                            if ui.is_rect_visible(r) {
                                self.paint_thumb(ui, r, m, CornerRadius::same(12), true);
                                if resp.hovered() {
                                    ui.painter().rect_stroke(r, CornerRadius::same(12), Stroke::new(2.0, ACCENT.gamma_multiply(0.8)), StrokeKind::Inside);
                                }
                            }
                            let tip = if m.caption.is_empty() { title.clone() } else { format!("{title}\n{}", m.caption) };
                            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tip).clicked() {
                                click = Some((items.iter().map(|x| x.1.clone()).collect::<Vec<_>>(), ri * cols + ci));
                            }
                        }
                    });
                    ui.add_space(gap);
                }
            }
            ui.add_space(10.0);
        }
        if let Some((list, i)) = click {
            self.lightbox = Some((list, i, PathBuf::new()));
        }
    }

    // ---------- 設定 ----------

    fn settings_card(ui: &mut egui::Ui, w: f32, title: &str, note: &str, add: impl FnOnce(&mut egui::Ui, f32)) {
        egui::Frame::new().fill(CARD.gamma_multiply(0.6)).stroke(Stroke::new(1.0, LINE)).corner_radius(CornerRadius::same(14)).inner_margin(egui::Margin::same(18)).show(ui, |ui| {
            let iw = w - 36.0;
            ui.set_width(iw);
            ui.label(RichText::new(title).font(bold(16.0)).color(TEXT));
            if !note.is_empty() {
                ui.label(RichText::new(note).size(12.0).color(FAINT));
            }
            ui.add_space(10.0);
            add(ui, iw);
        });
        ui.add_space(14.0);
    }

    /// 名前・値の 1 行(左に名前、右に値)
    fn setting_row(ui: &mut egui::Ui, w: f32, name: &str, tip: &str, add: impl FnOnce(&mut egui::Ui)) {
        ui.horizontal(|ui| {
            ui.set_min_height(34.0);
            // 名前は左寄せ
            ui.allocate_ui_with_layout(Vec2::new(220.0, 30.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.set_min_width(220.0);
                let r = ui.label(RichText::new(name).size(13.5).color(MUTED));
                if !tip.is_empty() {
                    r.on_hover_text(tip);
                }
            });
            ui.allocate_ui_with_layout(Vec2::new(w - 230.0, 30.0), egui::Layout::left_to_right(egui::Align::Center), add);
        });
    }

    fn settings_page(&mut self, ui: &mut egui::Ui, w: f32, ctx: &egui::Context) {
        ui.label(RichText::new("設定").font(bold(24.0)).color(TEXT));
        ui.add_space(14.0);
        // レンダリングの一覧の既定
        let mut s = self.queue.settings();
        let before = s.clone();
        Self::settings_card(ui, w, "レンダリングの一覧", "描く時の重さの決まり。一覧の頁の調整と同じ値です(次の 1 本から効きます)", |ui, iw| {
            let ncpu = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) as u32;
            Self::setting_row(ui, iw, "CPU のスレッド", "0 は全部", |ui| {
                ui.add(egui::DragValue::new(&mut s.threads).range(0..=ncpu).custom_formatter(|v, _| if v == 0.0 { "全部".into() } else { format!("{v:.0} 本") }));
                ui.label(RichText::new(format!("この PC は {ncpu} 本")).size(12.0).color(FAINT));
            });
            Self::setting_row(ui, iw, "優先度(nice)", "0〜19。大きいほど後回しにして、ほかの作業を重くしない", |ui| {
                ui.add(egui::DragValue::new(&mut s.nice).range(0..=19).custom_formatter(|v, _| format!("nice {v:.0}")));
            });
            Self::setting_row(ui, iw, "RAM の上限", "超えると描いている物だけが止められる(systemd のスコープ)", |ui| {
                ui.add(egui::DragValue::new(&mut s.ram_gb).range(0.0..=1024.0).speed(0.5).custom_formatter(|v, _| if v == 0.0 { "無し".into() } else { format!("{v:.0} GB") }));
            });
            Self::setting_row(ui, iw, "VRAM の上限", "全体の VRAM がこれに近い時は次を始めず、超えたら止める", |ui| {
                ui.add(egui::DragValue::new(&mut s.vram_mib).range(0..=81920).speed(64.0).custom_formatter(|v, _| if v == 0.0 { "無し".into() } else { format!("{v:.0} MiB") }));
            });
        });
        if s.threads != before.threads || s.nice != before.nice || s.ram_gb != before.ram_gb || s.vram_mib != before.vram_mib {
            if s.nice != before.nice && self.queue.is_running() {
                if let Err(e) = self.queue.renice(s.nice) {
                    self.message = e;
                }
            }
            self.queue.set_settings(s);
        }
        // Blender との連携
        let mut install = false;
        Self::settings_card(ui, w, "Blender との連携", "シーンの「ランチャーに登録」を使うためのアドオン(Falcon Launcher Link)", |ui, iw| {
            Self::setting_row(ui, iw, "アドオン", "ランチャーに入っています(falcon_launcher_link.py)", |ui| {
                if ui.add(soft_button(format!("{I_EXT}  アドオンを入れる"))).on_hover_text("~/.config/blender/<版>/scripts/addons へ写します").clicked() {
                    install = true;
                }
                ui.label(RichText::new("入れたあと、Blender の設定 > アドオンで有効にします").size(12.0).color(FAINT));
            });
            Self::setting_row(ui, iw, "届け先", "アドオンがシーンの写しを置く所(~/.config/falcon-launcher/連携.tsv)", |ui| {
                Self::path_cell(ui, &queue::inbox_dir());
            });
        });
        if install {
            let v = queue::install_addon();
            self.message = if v.is_empty() { "アドオンを写せませんでした".into() } else { format!("アドオンを Blender {} へ写しました。設定 > アドオンで「Falcon Launcher Link」を有効にしてください", v.join(" · ")) };
        }
        // 置き場所
        Self::settings_card(ui, w, "置き場所", "押すとファイルの画面で開きます", |ui, iw| {
            let pl = crate::places::places();
            let places: [(&str, PathBuf); 8] = [
                ("レンダリングの出力", PathBuf::from(&pl.render)),
                ("ギャラリー(手で置く絵)", PathBuf::from(&pl.gallery)),
                ("連番を動画にした物", PathBuf::from(&pl.seq)),
                ("Falcon Engine のソース", PathBuf::from(&pl.source)),
                ("Falcon Engine の建て先", PathBuf::from(&pl.falcon_build)),
                ("説明の文(版ごと)", PathBuf::from(&pl.about)),
                ("ランチャーの設定(場所.tsv で置き場を変えられます)", config_dir()),
                ("小さな絵の控え", PathBuf::from(format!("{}/.cache/falcon-launcher", std::env::var("HOME").unwrap_or_default()))),
            ];
            for (name, path) in places {
                Self::setting_row(ui, iw, name, "", |ui| Self::path_cell(ui, &path));
            }
        });
        // 公式の Blender
        let mut rescan = false;
        let search = crate::places::places().blender_search.iter().map(|s| format!("{s}・")).collect::<String>();
        Self::settings_card(ui, w, "公式の Blender", &format!("{search}~/Applications・~/Downloads・~/.local/opt・/opt の blender-* と、/usr/bin・snap・Steam・flatpak から自動で探します"), |ui, iw| {
            let list: Vec<Build> = self.builds.iter().filter(|b| b.key.starts_with("official:")).cloned().collect();
            if list.is_empty() {
                ui.label(RichText::new("見つかっていません").size(12.5).color(FAINT));
            }
            for b in &list {
                let bin = scan::blender_bin(b).unwrap_or_default();
                Self::setting_row(ui, iw, &b.title, "", |ui| Self::path_cell(ui, &bin));
            }
            ui.add_space(4.0);
            if ui.add(soft_button(format!("{I_REFRESH}  探し直す"))).clicked() {
                rescan = true;
            }
        });
        if rescan {
            self.refresh_all(ctx);
        }
        // ランチャーについて
        Self::settings_card(ui, w, "ランチャーについて", "", |ui, iw| {
            Self::setting_row(ui, iw, "版", "", |ui| {
                ui.label(RichText::new("Falcon Launcher 0.1.0 · Linux").size(13.5).color(TEXT));
            });
            Self::setting_row(ui, iw, "ソース", "", |ui| Self::path_cell(ui, Path::new(env!("CARGO_MANIFEST_DIR"))));
            Self::setting_row(ui, iw, "ビルドの決まり", "", |ui| {
                ui.label(RichText::new("nice -n 10 ninja -j5(在席中の画面を重くしないため)").size(13.0).color(MUTED));
            });
        });
    }

    // ---------- 建てる準備 ----------

    fn prep_page(&mut self, ui: &mut egui::Ui, w: f32, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("建てる準備").font(bold(24.0)).color(TEXT));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(soft_button(format!("{I_REFRESH}  確かめ直す"))).clicked() {
                    self.prep_checks = prep::check_all(&self.prep_paths);
                }
            });
        });
        let (pretty, _) = prep::distro();
        ui.label(RichText::new(format!("Falcon Engine をこの PC で建てるのに要る物です。{pretty} · Windows は後で足します")).size(12.5).color(FAINT));
        ui.add_space(6.0);
        let need: Vec<&prep::Check> = self.prep_checks.iter().filter(|c| !c.optional).collect();
        let ok = need.iter().filter(|c| c.state == prep::State::Ok).count();
        let (txt, col) = if ok == need.len() { ("揃っています".to_string(), OK) } else { (format!("要る物 {} のうち {} が揃っています", need.len(), ok), WARN) };
        ui.label(RichText::new(txt).font(bold(14.0)).color(col));
        ui.label(RichText::new("NVIDIA の物(CUDA・OptiX・DLSS)は、GitHub の README の場所から取り、下の枠へ落とすか、置いた場所を入れてください。パッケージの命令は sudo が要るので、写して端末で打ってください").size(12.0).color(FAINT));
        ui.add_space(14.0);
        self.setup_card(ui, w, ctx);
        self.prep_drop_card(ui, w, ctx);

        let mut groups: Vec<&'static str> = vec![];
        for c in &self.prep_checks {
            if !groups.contains(&c.group) {
                groups.push(c.group);
            }
        }
        let checks = self.prep_checks.clone();
        let mut set: Option<(String, String)> = None;
        for g in groups {
            let list: Vec<&prep::Check> = checks.iter().filter(|c| c.group == g).collect();
            Self::settings_card(ui, w, g, "", |ui, iw| {
                for (i, c) in list.iter().enumerate() {
                    if i > 0 {
                        ui.add_space(6.0);
                        let (lr, _) = ui.allocate_exact_size(Vec2::new(iw, 1.0), Sense::hover());
                        ui.painter().rect_filled(lr, CornerRadius::ZERO, LINE);
                        ui.add_space(6.0);
                    }
                    let (icon, col, word) = match c.state {
                        prep::State::Ok => ("\u{f00c}", OK, "あり"),
                        prep::State::Warn => ("\u{f071}", WARN, "要確認"),
                        prep::State::Missing if c.optional => ("\u{f111}", FAINT, "無し(無くても建てられる)"),
                        prep::State::Missing => ("\u{f00d}", ERR, "無し"),
                    };
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(icon).size(14.0).color(col));
                        ui.label(RichText::new(c.name).font(bold(14.0)).color(TEXT));
                        ui.label(RichText::new(word).size(12.0).color(col));
                        ui.label(RichText::new(format!("·  {}", c.need)).size(12.0).color(FAINT));
                    });
                    if !c.found.is_empty() {
                        ui.add(egui::Label::new(RichText::new(&c.found).family(egui::FontFamily::Monospace).size(12.0).color(MUTED)).truncate());
                    }
                    if c.state == prep::State::Ok && !c.has_path {
                        continue;
                    }
                    ui.horizontal_wrapped(|ui| {
                        if let Some(cmd) = prep::package_command(&c.how) {
                            ui.add(egui::Label::new(RichText::new(&cmd).family(egui::FontFamily::Monospace).size(12.5).color(TEXT).background_color(CARD_HI)));
                            if ui.add(soft_button("写す")).on_hover_text("命令を写す(端末に貼って打つ)").clicked() {
                                ctx.copy_text(cmd.clone());
                                self.message = format!("写しました: {cmd}");
                            }
                        }
                        if let prep::How::Manual { url } = &c.how {
                            if c.state != prep::State::Ok && ui.add(soft_button(format!("{I_EXT}  取りに行く頁を開く"))).on_hover_text(*url).clicked() {
                                let _ = Command::new("xdg-open").arg(url).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
                            }
                        }
                    });
                    if c.has_path {
                        let cur = self.prep_paths.get(c.key).cloned().unwrap_or_default();
                        let hint = match c.key {
                            "build_dir" => prep::default_build_dir(),
                            "cuda" => "例: /opt/cuda(空なら PATH と /opt/cuda・/usr/local/cuda を探す)".into(),
                            "optix" => "例: ~/sdk/NVIDIA-OptiX-SDK-9.0.0-linux64-x86_64".into(),
                            _ => "置いたフォルダ".into(),
                        };
                        let edit = self.prep_edit.entry(c.key.to_string()).or_insert(cur.clone());
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("置いた場所").size(12.5).color(MUTED));
                            ui.add(egui::TextEdit::singleline(edit).hint_text(hint).desired_width((iw - 260.0).max(200.0)).font(egui::TextStyle::Monospace));
                            let changed = edit.trim() != cur.trim();
                            if ui.add_enabled(changed, soft_button("決める")).clicked() {
                                set = Some((c.key.to_string(), edit.trim().to_string()));
                            }
                        });
                    }
                }
            });
        }
        if let Some((k, v)) = set {
            let v = if let Some(rest) = v.strip_prefix("~/") { format!("{}/{rest}", std::env::var("HOME").unwrap_or_default()) } else { v };
            self.prep_edit.insert(k.clone(), v.clone());
            self.prep_paths.insert(k, v);
            prep::save_paths(&self.prep_paths);
            self.prep_checks = prep::check_all(&self.prep_paths);
            self.message = "置いた場所を決めて、確かめ直しました".into();
        }
        // 公式の Blender は準備なしで起動できる
        ui.label(RichText::new("公式の Blender は、この準備が無くても左の列から起動できます(設定 > 公式の Blender)").size(12.0).color(FAINT));
    }

    /// 落とす置き場(取った物をドラッグ・アンド・ドロップで入れる)
    fn prep_drop_card(&mut self, ui: &mut egui::Ui, w: f32, ctx: &egui::Context) {
        // 受け取りの結果
        if let Some(rx) = &self.prep_rx {
            if let Ok(r) = rx.try_recv() {
                self.prep_rx = None;
                match r {
                    Ok(prep::Taken::Set(k, p)) => {
                        let v = p.display().to_string();
                        self.prep_paths.insert(k.to_string(), v.clone());
                        self.prep_edit.insert(k.to_string(), v.clone());
                        prep::save_paths(&self.prep_paths);
                        self.prep_checks = prep::check_all(&self.prep_paths);
                        self.message = format!("{} として入れました: {v}", prep::key_name(k));
                    }
                    Ok(prep::Taken::Command(cmd, note)) => self.prep_cmd = Some((cmd, note)),
                    Err(e) => self.message = e,
                }
            } else {
                ctx.request_repaint_after(Duration::from_millis(200));
            }
        }
        let mut take: Option<PathBuf> = None;
        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
        if let Some(p) = ctx.input(|i| i.raw.dropped_files.first().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty())) {
            take = Some(p);
        }
        let busy = self.prep_rx.is_some();
        Self::settings_card(ui, w, "ここに落として入れる", "GitHub の README の場所から取った物(フォルダ・.zip・.tar.*・OptiX の .sh・CUDA の .run)を、この枠へ落とします。何の SDK かは中身で見分けます", |ui, iw| {
            let (r, _) = ui.allocate_exact_size(Vec2::new(iw, 96.0), Sense::hover());
            let col = if hovering { ACCENT } else { LINE };
            ui.painter().rect_filled(r, CornerRadius::same(12), if hovering { ACCENT.gamma_multiply(0.08) } else { Color32::TRANSPARENT });
            // 点線の縁
            let mut x = r.left() + 6.0;
            while x < r.right() - 6.0 {
                for y in [r.top(), r.bottom()] {
                    ui.painter().line_segment([Pos2::new(x, y), Pos2::new((x + 8.0).min(r.right() - 6.0), y)], Stroke::new(1.5, col));
                }
                x += 14.0;
            }
            let mut y = r.top() + 6.0;
            while y < r.bottom() - 6.0 {
                for x in [r.left(), r.right()] {
                    ui.painter().line_segment([Pos2::new(x, y), Pos2::new(x, (y + 8.0).min(r.bottom() - 6.0))], Stroke::new(1.5, col));
                }
                y += 14.0;
            }
            let text = if busy { "受け取っています(展開中)…".to_string() } else if hovering { "離すと入れます".to_string() } else { format!("{I_DOWNLOAD}  ここへ落とす") };
            ui.painter().text(r.center(), Align2::CENTER_CENTER, text, FontId::proportional(15.0), if hovering { ACCENT_TEXT } else { MUTED });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("またはパスを貼る").size(12.5).color(MUTED));
                ui.add(egui::TextEdit::singleline(&mut self.prep_paste).hint_text("落とせない時(Wayland など)はここへ").desired_width((iw - 240.0).max(200.0)).font(egui::TextStyle::Monospace));
                if ui.add_enabled(!busy && !self.prep_paste.trim().is_empty(), soft_button("入れる")).clicked() {
                    let p = self.prep_paste.trim().trim_matches('\'').trim_matches('"').trim_start_matches("file://").to_string();
                    take = Some(PathBuf::from(p));
                    self.prep_paste.clear();
                }
            });
            if let Some((cmd, note)) = self.prep_cmd.clone() {
                ui.add_space(8.0);
                ui.label(RichText::new(note).size(12.5).color(WARN));
                ui.horizontal_wrapped(|ui| {
                    ui.add(egui::Label::new(RichText::new(&cmd).family(egui::FontFamily::Monospace).size(12.0).color(TEXT).background_color(CARD_HI)));
                    if ui.add(soft_button("写す")).clicked() {
                        ctx.copy_text(cmd.clone());
                        self.message = "命令を写しました".into();
                    }
                    if ui.add(soft_button("閉じる")).clicked() {
                        self.prep_cmd = None;
                    }
                });
            }
        });
        if let (Some(p), false) = (take, busy) {
            self.prep_cmd = None;
            let (tx, rx) = channel();
            self.prep_rx = Some(rx);
            self.message = format!("受け取っています: {}", p.display());
            let c = ctx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(prep::take(&p));
                c.request_repaint();
            });
        }
    }

    /// パスと「開く」
    fn path_cell(ui: &mut egui::Ui, path: &Path) {
        let exists = path.exists();
        let r = ui.add(egui::Button::new(RichText::new(format!("{I_FOLDER}  開く")).size(12.5).color(if exists { TEXT } else { FAINT })).fill(CARD).stroke(Stroke::new(1.0, LINE)).corner_radius(CornerRadius::same(8)).min_size(Vec2::new(0.0, 28.0)));
        if exists && r.clicked() {
            let target = if path.is_file() { path.parent().unwrap_or(path) } else { path };
            let _ = Command::new("xdg-open").arg(target).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
        }
        ui.add(egui::Label::new(RichText::new(path.display().to_string()).family(egui::FontFamily::Monospace).size(12.0).color(if exists { MUTED } else { FAINT })).truncate());
        if !exists {
            ui.label(RichText::new("まだ無い").size(11.5).color(FAINT));
        }
    }

    // ---------- ホームの起動の塊 ----------

    fn launch_block(&mut self, ui: &mut egui::Ui, b: &Build, actions: &mut Vec<Action>) {
        // 札: 枝と分け方
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let mut chips = vec![];
            if !b.branch.is_empty() {
                chips.push((b.branch.clone(), TEXT, CARD_HI));
            }
            chips.push((b.category.to_string(), MUTED, Color32::TRANSPARENT));
            for (t, c, fill) in chips {
                egui::Frame::new().fill(fill).corner_radius(CornerRadius::same(8)).inner_margin(egui::Margin::symmetric(10, 5)).show(ui, |ui| {
                    ui.label(RichText::new(t).size(12.5).color(c));
                });
            }
        });
        ui.add_space(12.0);
        // 印と大きな名前
        ui.horizontal(|ui| {
            let (ir, _) = ui.allocate_exact_size(Vec2::splat(60.0), Sense::hover());
            paint_build_icon(ui, ir, b, 14);
            ui.add_space(10.0);
            ui.label(RichText::new(&b.title).font(bold(46.0)).color(TEXT));
        });
        ui.add_space(8.0);
        let mut stats = vec![];
        if !b.what.is_empty() {
            stats.push(b.what.clone());
        }
        if b.built_ts > 0 {
            stats.push(format!("{} に建てた", when(b.built_ts)));
        }
        if let Some(n) = self.size_of(b) {
            stats.push(gb(n));
        }
        let (_, state) = self.state_of(b);
        stats.push(state);
        ui.label(RichText::new(stats.join("  ·  ")).size(14.5).color(MUTED));
        ui.add_space(18.0);
        // 起動(⋯)・版・フォルダ
        let h = 56.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            let (text, primary) = if b.building.is_some() {
                (format!("{I_LOG}   ビルド中・ログ"), false)
            } else if b.launchable() {
                (format!("{I_PLAY}   起動"), true)
            } else {
                (format!("{I_BUILD}   建てる"), true)
            };
            let fg = if primary { ON_ACCENT } else { TEXT };
            egui::Frame::new().fill(if primary { ACCENT } else { CARD_HI }).corner_radius(CornerRadius::same(12)).show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.horizontal(|ui| {
                    let main = ui.add(egui::Button::new(RichText::new(&text).font(bold(17.0)).color(fg)).fill(Color32::TRANSPARENT).stroke(Stroke::NONE).min_size(Vec2::new(150.0, h)));
                    if main.on_hover_text("ファイルを開かずに、新しく起動します").clicked() {
                        actions.push(if b.building.is_some() {
                            Action::ShowLog(b.key.clone())
                        } else if b.launchable() {
                            Action::Launch(b.key.clone())
                        } else {
                            Action::Build(b.key.clone())
                        });
                    }
                    let more = ui.add(egui::Button::new(RichText::new("⋯").font(bold(18.0)).color(fg)).fill(Color32::TRANSPARENT).stroke(Stroke::NONE).min_size(Vec2::new(46.0, h)));
                    egui::Popup::menu(&more).show(|ui| {
                        ui.set_min_width(220.0);
                        if ui.button(format!("{I_CUBE}   版の頁を開く")).clicked() {
                            self.open_version(&b.key, Some(StartTab::Blend));
                        }
                        if matches!(b.kind, Kind::Main | Kind::Test) && ui.button(format!("{I_BUILD}   {}", if b.has_bin { "建て直す" } else { "建てる" })).clicked() {
                            actions.push(Action::Build(b.key.clone()));
                        }
                        if b.dir.join("build.log").is_file() && ui.button(format!("{I_LOG}   ビルドのログ")).clicked() {
                            actions.push(Action::ShowLog(b.key.clone()));
                        }
                        if ui.button(format!("{I_LICENSE}   説明")).clicked() {
                            self.open_version(&b.key, Some(StartTab::About));
                        }
                    });
                });
            });
            // 版(押すと選べる)
            let label = if b.hash.is_empty() { b.title.clone() } else { format!("{}  ·  {}", b.title, &b.hash[..b.hash.len().min(9)]) };
            let (r, resp) = ui.allocate_exact_size(Vec2::new(300.0, h), Sense::click());
            let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
            let p = ui.painter();
            p.rect_filled(r, CornerRadius::same(12), if resp.hovered() { CARD_HI } else { CARD });
            p.rect_stroke(r, CornerRadius::same(12), Stroke::new(1.0, LINE), StrokeKind::Inside);
            p.text(Pos2::new(r.left() + 16.0, r.top() + 15.0), Align2::LEFT_CENTER, "版", FontId::proportional(11.0), FAINT);
            p.galley(Pos2::new(r.left() + 16.0, r.top() + 24.0), elide(ui, &label, bold(14.5), TEXT, r.width() - 52.0), TEXT);
            p.text(Pos2::new(r.right() - 18.0, r.center().y), Align2::CENTER_CENTER, I_DOWN, FontId::proportional(11.0), MUTED);
            let groups = self.start_groups();
            egui::Popup::menu(&resp).show(|ui| {
                ui.set_min_width(300.0);
                egui::ScrollArea::vertical().max_height(460.0).show(ui, |ui| {
                    for (head, list) in &groups {
                        ui.label(RichText::new(*head).size(11.0).color(FAINT));
                        for x in list {
                            let t = if x.hash.is_empty() { x.title.clone() } else { format!("{}  ·  {}", x.title, &x.hash[..x.hash.len().min(9)]) };
                            if ui.selectable_label(x.key == b.key, t).clicked() {
                                self.sel = x.key.clone();
                                ui.close();
                            }
                        }
                        ui.add_space(4.0);
                    }
                });
            });
            let fb = egui::Button::new(RichText::new(I_FOLDER).size(16.0).color(TEXT)).fill(CARD).stroke(Stroke::new(1.0, LINE)).corner_radius(CornerRadius::same(12)).min_size(Vec2::splat(h));
            if ui.add_enabled(b.dir.is_dir(), fb).on_hover_text("ビルド先を開く").clicked() {
                actions.push(Action::Folder(b.dir.clone()));
            }
        });
        ui.add_space(12.0);
        // .blend のパスを貼って開く
        let typed = PathBuf::from(self.path_paste.trim().trim_matches('\'').trim_matches('"'));
        let mut go = false;
        egui::Frame::new().fill(CARD).stroke(Stroke::new(1.0, LINE)).corner_radius(CornerRadius::same(12)).inner_margin(egui::Margin { left: 14, right: 8, top: 8, bottom: 8 }).show(ui, |ui| {
            ui.set_width(340.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{I_CUBE} ")).size(14.0).color(FAINT));
                let edit = ui.add(egui::TextEdit::singleline(&mut self.path_paste).hint_text(".blend のパスを貼る・落とす").frame(egui::Frame::NONE).desired_width(220.0));
                if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    go = true;
                }
                let ob = egui::Button::new(RichText::new("開く").size(13.0).color(TEXT)).fill(CARD_HI).corner_radius(CornerRadius::same(8)).min_size(Vec2::new(56.0, 32.0));
                if ui.add_enabled(typed.is_file(), ob).on_hover_text("使う版で開きます").clicked() {
                    go = true;
                }
            });
        });
        if go {
            if typed.is_file() {
                self.open_blend(&typed);
                self.path_paste.clear();
            } else if !self.path_paste.trim().is_empty() {
                self.message = format!("{} がありません", typed.display());
            }
        }
    }
}
