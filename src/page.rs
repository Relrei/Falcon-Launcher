//! ページ(2026-11-09 から: 版ごとの頁)。
//! - 探す: 左に絞り込み、右にプロジェクトの行
//! - プロジェクトのページ: 頭に名前と「起動する」、タブに 説明・ギャラリー・更新履歴・機能、右に細かい札
//! - 機能: falcon_features.py の機能ごとの表(版は機能ごと)。試験のビルドと古い版は同じ表の切り替え
//! - 更新履歴: 版とコミットとビルドの記録を折りたたみの線で
//! - ギャラリー: 今まで取った絵・動画と、説明つきの絵
use super::*;
use egui::text::{LayoutJob, TextFormat, TextWrapping};

const CHANGES_PER_PAGE: usize = 20;
const ROWS_PER_PAGE: usize = 20;

pub(super) fn bold(size: f32) -> FontId {
    FontId::new(size, egui::FontFamily::Name("bold".into()))
}

pub(super) fn elide(ui: &egui::Ui, text: &str, font: FontId, color: Color32, max_w: f32) -> Arc<egui::Galley> {
    let mut job = LayoutJob::single_section(text.to_string(), TextFormat { font_id: font, color, ..Default::default() });
    job.wrap = TextWrapping { max_width: max_w.max(10.0), max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
    ui.painter().layout_job(job)
}

/// 丸い小さな札。絵を描くだけで場所を返す
pub(super) fn paint_chip(ui: &egui::Ui, pos: Pos2, text: &str, color: Color32, max_w: f32) -> Rect {
    let g = elide(ui, text, FontId::proportional(12.0), color, max_w - 16.0);
    let r = Rect::from_min_size(pos, g.size() + Vec2::new(16.0, 7.0));
    ui.painter().rect_filled(r, CornerRadius::same(11), CARD_HI);
    ui.painter().rect_stroke(r, CornerRadius::same(11), Stroke::new(1.0, LINE), StrokeKind::Inside);
    ui.painter().galley(r.min + Vec2::new(8.0, 3.5), g, color);
    r
}

fn tag(ui: &mut egui::Ui, text: &str, color: Color32) {
    let g = ui.painter().layout_no_wrap(text.to_string(), FontId::proportional(12.0), color);
    let (r, _) = ui.allocate_exact_size(g.size() + Vec2::new(16.0, 7.0), Sense::hover());
    ui.painter().rect_filled(r, CornerRadius::same(11), CARD_HI);
    ui.painter().rect_stroke(r, CornerRadius::same(11), Stroke::new(1.0, LINE), StrokeKind::Inside);
    ui.painter().galley(r.min + Vec2::new(8.0, 3.5), g, color);
}

fn side_card(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new().fill(CARD).corner_radius(CornerRadius::same(14)).inner_margin(egui::Margin::same(16)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(title).font(bold(16.0)).color(TEXT));
        ui.add_space(6.0);
        add(ui);
    });
    ui.add_space(12.0);
}

fn sub_title(ui: &mut egui::Ui, text: &str) {
    ui.add_space(2.0);
    ui.label(RichText::new(text).font(bold(13.0)).color(MUTED));
}


fn green_button(text: String) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text).font(bold(15.0)).color(ON_ACCENT)).fill(ACCENT).stroke(Stroke::NONE).corner_radius(CornerRadius::same(12))
}

fn plain_item(text: String) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text).size(13.0).color(TEXT)).frame(false)
}

/// ⋮ の小さなメニュー
fn kebab(ui: &mut egui::Ui, rect: Rect, salt: &str, round: bool, add: impl FnOnce(&mut egui::Ui)) {
    let mut child = ui.new_child(egui::UiBuilder::new().id_salt(("kebab", salt)).max_rect(rect).layout(egui::Layout::left_to_right(egui::Align::Center)));
    child.spacing_mut().button_padding = egui::vec2(if round { 19.0 } else { 12.0 }, if round { 13.0 } else { 7.0 });
    let b = egui::Button::new(RichText::new(I_MORE).size(if round { 16.0 } else { 14.0 }).color(if round { TEXT } else { MUTED })).fill(if round { CARD } else { Color32::TRANSPARENT }).stroke(Stroke::NONE).corner_radius(CornerRadius::same(if round { 22 } else { 6 }));
    egui::containers::menu::MenuButton::from_button(b).ui(&mut child, add);
}

/// 機能の札の字と色: 動 = 起動できる・ビ = ビルド中・失 = 失敗・未 = 未ビルド・古 = 古い版
fn state_badge(b: &Build) -> (&'static str, Color32, String) {
    if b.building.is_some() {
        ("ビ", ORANGE, "ビルド中".into())
    } else if b.kind == Kind::Old && b.key != "v5" && !b.key.starts_with("official:") {
        ("古", MUTED, "古い版".into())
    } else if b.ok == Some(false) {
        ("失", ERR, format!("ビルドに失敗 {} 件", b.fails))
    } else if b.has_bin {
        ("動", OK, "起動できる".into())
    } else {
        ("未", FAINT, "まだビルドしていない".into())
    }
}

/// falcon_features.py の kind の字と札の言葉と色
fn feature_kind(kind: &str) -> (&'static str, &'static str, Color32) {
    match kind {
        "engine" => ("E", "エンジン", OK),
        "switch" => ("S", "専用の切り替え", LINK),
        "toggle" => ("T", "入り切り", ORANGE),
        "restart" => ("R", "再起動で反映", WARN),
        "auto" => ("A", "自動", MUTED),
        _ => ("-", "本家と同じ", FAINT),
    }
}

/// 日付だけ(「今日」「11-05」)
pub(super) fn day_of(ts: i64) -> String {
    when(ts).split(' ').next().unwrap_or("").to_string()
}

fn paint_icon(ui: &mut egui::Ui, icon: Rect, b: &Build) {
    let p = ui.painter().clone();
    let radius = (icon.width() * 0.19) as u8;
    match thumb_path(b) {
        Some(path) => {
            egui::Image::new(format!("file://{}", path.display())).corner_radius(CornerRadius::same(radius)).paint_at(ui, icon);
        }
        None if b.kind == Kind::Main => {
            p.rect_filled(icon, CornerRadius::same(radius), CARD_HI);
            p.text(icon.center(), Align2::CENTER_CENTER, I_CUBE, FontId::proportional(icon.width() * 0.46), ACCENT);
        }
        None => {
            p.rect_filled(icon, CornerRadius::same(radius), hue_color(&b.key, 0.25));
            let mono: String = b.key.trim_start_matches("old:").chars().filter(|c| c.is_alphanumeric()).take(2).collect::<String>().to_uppercase();
            p.text(icon.center(), Align2::CENTER_CENTER, mono, bold(icon.width() * 0.37), MUTED);
        }
    }
}

/// 頁送り: < 1 2 … 14 >。全部で 1 頁なら出さない。右から左の並びの中で呼ぶ
fn pager(ui: &mut egui::Ui, page: &mut usize, pages: usize) {
    if pages <= 1 {
        return;
    }
    let cur = (*page).min(pages - 1);
    let mut show: Vec<Option<usize>> = vec![];
    for i in 0..pages {
        if i == 0 || i + 1 == pages || i.abs_diff(cur) <= 1 {
            show.push(Some(i));
        } else if show.last().is_some_and(|x| x.is_some()) {
            show.push(None);
        }
    }
    ui.spacing_mut().item_spacing.x = 4.0;
    if cur + 1 < pages && ui.add(egui::Button::new(RichText::new(I_RIGHT).size(11.0).color(MUTED)).frame(false)).clicked() {
        *page = cur + 1;
    }
    for s in show.iter().rev() {
        match s {
            None => {
                ui.label(RichText::new("…").color(FAINT));
            }
            Some(i) => {
                let on = *i == cur;
                let b = egui::Button::new(RichText::new((i + 1).to_string()).font(bold(13.0)).color(if on { ACCENT_TEXT } else { MUTED })).fill(if on { ACCENT.gamma_multiply(0.22) } else { Color32::TRANSPARENT }).stroke(Stroke::NONE).corner_radius(CornerRadius::same(14)).min_size(Vec2::splat(28.0));
                if ui.add(b).clicked() {
                    *page = *i;
                }
            }
        }
    }
    if cur > 0 && ui.add(egui::Button::new(RichText::new(I_LEFT).size(11.0).color(MUTED)).frame(false)).clicked() {
        *page = cur - 1;
    }
}

impl App {
    pub(super) fn start_gallery(&mut self, ctx: &egui::Context) {
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(scan::gallery());
            ctx.request_repaint();
        });
        self.gallery_rx = Some(rx);
    }

    pub(super) fn ask_changes(&mut self, b: &Build, ctx: &egui::Context) {
        if self.changes_asked.insert(b.key.clone()) {
            let (tx, b, ctx) = (self.changes_tx.clone(), b.clone(), ctx.clone());
            std::thread::spawn(move || {
                let v = scan::changes(&b);
                let _ = tx.send((b.key.clone(), v));
                ctx.request_repaint();
            });
        }
    }

    pub(super) fn ask_features(&mut self, ctx: &egui::Context) {
        if self.features.is_empty() && self.features_rx.is_none() {
            let (tx, rx) = channel();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(scan::features());
                ctx.request_repaint();
            });
            self.features_rx = Some(rx);
        }
    }

    /// 小さな絵。まだなら作るよう頼んで None
    pub(super) fn thumb(&mut self, path: &Path, video: bool) -> Option<PathBuf> {
        if let Some(t) = self.thumb_done.lock().ok().and_then(|d| d.get(path).cloned()) {
            return t;
        }
        if self.thumb_asked.insert(path.to_path_buf()) {
            let _ = self.thumb_tx.send((path.to_path_buf(), video));
        }
        None
    }

    /// 大きさ: クラスターは表示器ではなく場面の合計
    pub(super) fn size_of(&self, b: &Build) -> Option<u64> {
        if b.kind == Kind::Ext {
            return Some(self.scenes.iter().map(|s| s.bytes).sum());
        }
        self.sizes.get(&b.key).map(|(_, n)| *n)
    }

    fn page_no(&self, name: &'static str) -> usize {
        self.pages.get(name).copied().unwrap_or(0)
    }

    // ---------- 上の帯 ----------

    // ---------- 探す ----------

    fn filter_row(ui: &mut egui::Ui, text: &str, count: usize, on: bool) -> bool {
        let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), Sense::click());
        let p = ui.painter();
        if on {
            p.rect_filled(r, CornerRadius::same(8), ACCENT.gamma_multiply(0.18));
        } else if resp.hovered() {
            p.rect_filled(r, CornerRadius::same(8), CARD_HI);
        }
        p.text(r.left_center() + Vec2::new(10.0, 0.0), Align2::LEFT_CENTER, text, FontId::proportional(13.5), if on { ACCENT_TEXT } else { MUTED });
        p.text(r.right_center() - Vec2::new(10.0, 0.0), Align2::RIGHT_CENTER, count.to_string(), FontId::proportional(12.0), FAINT);
        resp.clicked()
    }

    pub(super) fn home_filters(&mut self, ui: &mut egui::Ui) {
        let count = |f: &dyn Fn(&Build) -> bool| self.builds.iter().filter(|b| f(b)).count();
        let kinds: [(Tab, &str, usize); 5] = [
            (Tab::All, "すべて", self.builds.len()),
            (Tab::Project, "プロジェクト", count(&|b| matches!(b.kind, Kind::Main | Kind::Ext))),
            (Tab::Test, "試験のビルド", count(&|b| b.kind == Kind::Test)),
            (Tab::Old, "古い版", count(&|b| b.kind == Kind::Old)),
            (Tab::Fav, "お気に入り", count(&|b| self.favorites.contains(&b.key))),
        ];
        let cats: Vec<(&'static str, usize)> = scan::CATEGORIES.iter().map(|c| (*c, count(&|b| b.category == *c))).filter(|x| x.1 > 0).collect();
        let states = [
            (StateFilter::All, self.builds.len()),
            (StateFilter::Ready, count(&|b| b.launchable())),
            (StateFilter::Building, count(&|b| b.building.is_some())),
            (StateFilter::NotBuilt, count(&|b| !b.has_bin && b.building.is_none())),
            (StateFilter::Failed, count(&|b| b.ok == Some(false))),
        ];
        side_card(ui, "種類", |ui| {
            for (t, s, n) in kinds {
                if Self::filter_row(ui, s, n, self.tab == t) {
                    self.tab = t;
                }
            }
        });
        side_card(ui, "分け方", |ui| {
            for (c, n) in cats {
                let on = self.category == Some(c);
                if Self::filter_row(ui, c, n, on) {
                    self.category = if on { None } else { Some(c) };
                }
            }
        });
        side_card(ui, "状態", |ui| {
            for (s, n) in states {
                if Self::filter_row(ui, s.text(), n, self.state_filter == s) {
                    self.state_filter = s;
                }
            }
        });
    }

    pub(super) fn home_list(&mut self, ui: &mut egui::Ui, w: f32, actions: &mut Vec<Action>) {
        let tiles_before = self.look.build_tiles;
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.query).hint_text(format!("{I_SEARCH}  名前・枝・説明で探す")).desired_width(w - 252.0).margin(egui::Margin::symmetric(12, 8)));
            let text = |s: Sort| if s == Sort::New { "並び: 新しい順" } else { "並び: 名前順" };
            egui::ComboBox::from_id_salt("sort").selected_text(text(self.sort)).width(140.0).show_ui(ui, |ui| {
                for s in [Sort::New, Sort::Name] {
                    ui.selectable_value(&mut self.sort, s, text(s));
                }
            });
            // 行の一覧とカードの並び
            for (tiles, icon, tip) in [(false, I_LIST, "行の一覧"), (true, I_TILES, "カードの並び")] {
                let on = self.look.build_tiles == tiles;
                let b = egui::Button::new(RichText::new(icon).size(14.0).color(if on { ACCENT_TEXT } else { MUTED })).fill(if on { ACCENT.gamma_multiply(0.16) } else { CARD }).stroke(Stroke::new(1.0, if on { ACCENT.gamma_multiply(0.6) } else { LINE })).corner_radius(CornerRadius::same(9)).min_size(Vec2::splat(34.0));
                if ui.add(b).on_hover_text(tip).clicked() {
                    self.look.build_tiles = tiles;
                }
            }
        });
        if self.look.build_tiles != tiles_before {
            self.look.save();
        }
        ui.add_space(10.0);
        let tab = self.tab;
        let favs = self.favorites.clone();
        let list = self.filtered(|b| match tab {
            Tab::All => true,
            Tab::Project => matches!(b.kind, Kind::Main | Kind::Ext),
            Tab::Test => b.kind == Kind::Test,
            Tab::Old => b.kind == Kind::Old,
            Tab::Fav => favs.contains(&b.key),
        });
        if list.is_empty() {
            ui.add_space(20.0);
            ui.label(RichText::new("当てはまる物がありません。").color(FAINT));
        }
        if self.look.build_tiles {
            let gap = 12.0;
            let cols = ((w + gap) / (250.0 + gap)).floor().max(1.0) as usize;
            let cw = (w - gap * (cols as f32 - 1.0)) / cols as f32;
            for row in list.chunks(cols) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for b in row {
                        self.home_tile(ui, b, cw, actions);
                    }
                });
                ui.add_space(gap);
            }
            return;
        }
        for b in &list {
            self.home_row(ui, b, w, actions);
            ui.add_space(10.0);
        }
    }

    /// カードの並びの 1 枚(絵・名前・説明・札・大きさと更新・起動する)
    fn home_tile(&mut self, ui: &mut egui::Ui, b: &Build, w: f32, actions: &mut Vec<Action>) {
        let (r, resp) = ui.allocate_exact_size(Vec2::new(w, 232.0), Sense::click());
        if !ui.is_rect_visible(r) {
            return;
        }
        let p = ui.painter().clone();
        p.rect_filled(r, CornerRadius::same(14), if resp.hovered() { CARD_HI } else { CARD });
        if resp.clicked() {
            actions.push(Action::Open(b.key.clone()));
        }
        let pad = 14.0;
        let x = r.left() + pad;
        let text_w = w - pad * 2.0;
        paint_icon(ui, Rect::from_min_size(r.min + Vec2::splat(pad), Vec2::splat(64.0)), b);
        // 絵の右に大きさと更新
        let rx = r.right() - pad;
        if let Some(n) = self.size_of(b) {
            p.text(Pos2::new(rx, r.top() + pad + 14.0), Align2::RIGHT_CENTER, format!("{I_HDD}  {}", gb(n)), bold(13.0), TEXT);
        }
        if let Some(t) = self.updated.get(&b.key) {
            p.text(Pos2::new(rx, r.top() + pad + 38.0), Align2::RIGHT_CENTER, format!("{I_CLOCK}  更新 {}", ago(*t)), FontId::proportional(12.0), FAINT);
        }
        let ty = r.top() + pad + 64.0 + 10.0;
        p.galley(Pos2::new(x, ty), elide(ui, &b.title, bold(17.0), TEXT, text_w), TEXT);
        let what = if b.what.is_empty() { "説明はまだありません" } else { b.what.as_str() };
        p.galley(Pos2::new(x, ty + 28.0), elide(ui, what, FontId::proportional(12.5), MUTED, text_w), MUTED);
        let mut cx = x;
        let (_, color, state) = state_badge(b);
        for (t, c) in [(b.category.to_string(), MUTED), (state, color)] {
            let room = r.right() - pad - cx;
            if room < 40.0 {
                break;
            }
            cx = paint_chip(ui, Pos2::new(cx, ty + 52.0), &t, c, room.min(160.0)).right() + 6.0;
        }
        let br = Rect::from_min_size(Pos2::new(x, r.bottom() - pad - 34.0), Vec2::new(text_w, 34.0));
        if b.building.is_some() {
            if place_fill(ui, br, true, button(RichText::new(format!("{I_LOG}  ログ")).color(TEXT), false)).clicked() {
                actions.push(Action::ShowLog(b.key.clone()));
            }
        } else if b.launchable() {
            if place_fill(ui, br, true, green_button(format!("{I_PLAY}  起動する"))).clicked() {
                actions.push(Action::Launch(b.key.clone()));
            }
        } else if place_fill(ui, br, true, button(RichText::new(format!("{I_BUILD}  ビルドする")).color(TEXT), false)).clicked() {
            actions.push(Action::Build(b.key.clone()));
        }
    }

    fn home_row(&mut self, ui: &mut egui::Ui, b: &Build, w: f32, actions: &mut Vec<Action>) {
        let (r, resp) = ui.allocate_exact_size(Vec2::new(w, 124.0), Sense::click());
        if !ui.is_rect_visible(r) {
            return;
        }
        let p = ui.painter().clone();
        p.rect_filled(r, CornerRadius::same(14), if resp.hovered() { CARD_HI } else { CARD });
        if resp.clicked() {
            actions.push(Action::Open(b.key.clone()));
        }
        paint_icon(ui, Rect::from_min_size(r.min + Vec2::new(14.0, 14.0), Vec2::splat(96.0)), b);
        let x = r.left() + 126.0;
        let right_w = 170.0;
        let text_w = w - 126.0 - right_w - 16.0;
        let tg = elide(ui, &b.title, bold(20.0), TEXT, text_w);
        let tw = tg.size().x;
        p.galley(Pos2::new(x, r.top() + 13.0), tg, TEXT);
        if tw + 80.0 < text_w {
            p.text(Pos2::new(x + tw + 8.0, r.top() + 26.0), Align2::LEFT_CENTER, "by Relrei", FontId::proportional(13.0), FAINT);
        }
        let what = if b.what.is_empty() { "説明はまだありません" } else { b.what.as_str() };
        p.galley(Pos2::new(x, r.top() + 44.0), elide(ui, what, FontId::proportional(13.5), MUTED, text_w), MUTED);
        let mut cx = x;
        let y = r.top() + 80.0;
        let (_, color, state) = state_badge(b);
        for (t, c) in [(b.category.to_string(), MUTED), (state, color)] {
            cx = paint_chip(ui, Pos2::new(cx, y), &t, c, 180.0).right() + 6.0;
        }
        // 右の数字
        let rx = r.right() - 16.0;
        let line = |yy: f32, t: String, font: FontId, c: Color32| {
            p.text(Pos2::new(rx, yy), Align2::RIGHT_CENTER, t, font, c);
        };
        if let Some(n) = self.size_of(b) {
            line(r.top() + 24.0, format!("{I_HDD}  {}", gb(n)), bold(14.0), TEXT);
        }
        if let Some(t) = self.updated.get(&b.key) {
            line(r.top() + 48.0, format!("{I_CLOCK}  更新 {}", ago(*t)), FontId::proportional(12.5), FAINT);
        }
        let br = Rect::from_min_size(Pos2::new(rx - 120.0, r.bottom() - 46.0), Vec2::new(120.0, 34.0));
        if b.building.is_some() {
            if place_fill(ui, br, true, button(RichText::new(format!("{I_LOG}  ログ")).color(TEXT), false)).clicked() {
                actions.push(Action::ShowLog(b.key.clone()));
            }
        } else if b.launchable() {
            if place_fill(ui, br, true, green_button(format!("{I_PLAY}  起動する"))).clicked() {
                actions.push(Action::Launch(b.key.clone()));
            }
        } else if place_fill(ui, br, true, button(RichText::new(format!("{I_BUILD}  ビルドする")).color(TEXT), false)).clicked() {
            actions.push(Action::Build(b.key.clone()));
        }
    }

    // ---------- プロジェクトのページ ----------

    pub(super) fn page_side(&mut self, ui: &mut egui::Ui, b: &Build, actions: &mut Vec<Action>) {
        match b.kind {
            Kind::Main => side_card(ui, "対応", |ui| {
                sub_title(ui, "基盤");
                ui.horizontal_wrapped(|ui| {
                    for t in ["Blender 5.3b", "OIDN 2.5", "DLSS SDK", "Falcor", "Vulkan", "CUDA", "OptiX", "OpenPBR"] {
                        tag(ui, t, MUTED);
                    }
                });
                ui.add_space(6.0);
                sub_title(ui, "対象");
                ui.horizontal_wrapped(|ui| {
                    tag(ui, &format!("{I_LINUX} Linux"), ORANGE);
                    tag(ui, "NVIDIA GPU", ACCENT_TEXT);
                });
                ui.add_space(6.0);
                sub_title(ui, "動かす所");
                ui.horizontal_wrapped(|ui| tag(ui, &format!("{I_DESKTOP} この PC"), MUTED));
            }),
            Kind::Ext => side_card(ui, "対応", |ui| {
                sub_title(ui, "基盤");
                ui.horizontal_wrapped(|ui| {
                    for t in ["Vulkan 1.3", "meshoptimizer", "Blender のシェーダー"] {
                        tag(ui, t, MUTED);
                    }
                });
                ui.add_space(6.0);
                sub_title(ui, "動かす所");
                ui.horizontal_wrapped(|ui| tag(ui, &format!("{I_DESKTOP} Blender の外"), ORANGE));
            }),
            _ => {}
        }
        side_card(ui, "リンク", |ui| {
            let link = |ui: &mut egui::Ui, icon: &str, text: &str, enabled: bool| -> bool { ui.add_enabled(enabled, egui::Button::new(RichText::new(format!("{icon}  {text}  {I_EXT}")).size(13.0).color(MUTED)).frame(false)).clicked() };
            if link(ui, I_FOLDER, "ビルド先を開く", b.dir.is_dir()) {
                actions.push(Action::Folder(b.dir.clone()));
            }
            if link(ui, I_FOLDER, "ソースを開く", b.repo.is_dir()) {
                actions.push(Action::Folder(b.repo.clone()));
            }
            if b.dir.join("build.log").is_file() && link(ui, I_LOG, "ビルドのログ", true) {
                actions.push(Action::ShowLog(b.key.clone()));
            }
        });
        side_card(ui, "タグ", |ui| {
            ui.horizontal_wrapped(|ui| {
                tag(ui, b.category, MUTED);
                let (_, color, text) = state_badge(b);
                tag(ui, &text, color);
            });
        });
        side_card(ui, "作り手", |ui| {
            ui.horizontal(|ui| {
                let (c, _) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::hover());
                ui.painter().circle_filled(c.center(), 17.0, CARD_HI);
                ui.painter().text(c.center(), Align2::CENTER_CENTER, I_USER, FontId::proportional(14.0), MUTED);
                ui.label(RichText::new("Relrei").font(bold(14.0)).color(TEXT));
            });
        });
        side_card(ui, "詳細", |ui| {
            let mut row = |icon: &str, k: &str, v: String| {
                if !v.is_empty() {
                    ui.add(egui::Label::new(RichText::new(format!("{icon}  {k} {v}")).size(13.0).color(MUTED)).wrap());
                    ui.add_space(2.0);
                }
            };
            if b.kind == Kind::Ext {
                row(I_LICENSE, "ライセンス", "GPL-2.0-or-later".into());
            }
            row(I_CLOCK, "更新", self.updated.get(&b.key).map(|t| ago(*t)).unwrap_or_default());
            row(I_CAL, "ビルドした日", if b.built_ts > 0 { ago(b.built_ts) } else { String::new() });
            row(I_HDD, "大きさ", self.size_of(b).map(gb).unwrap_or_default());
            row(I_BRANCH, "枝", b.branch.clone());
            row(I_COMMIT_SMALL, "hash", b.hash.clone());
            row(I_FOLDER, "", if b.dir.as_os_str().is_empty() { b.exec.clone() } else { b.dir.display().to_string() });
        });
    }

    // ---------- 説明 ----------

    fn about_text(&mut self, b: &Build) -> String {
        if let Some(t) = self.about.get(&b.key) {
            return t.clone();
        }
        let key = b.key.trim_start_matches("old:");
        let mine = format!("{}/{key}.md", crate::places::places().about);
        let text = std::fs::read_to_string(&mine).or_else(|_| std::fs::read_to_string(format!("{}/integration/{key}/README.md", crate::places::places().project))).unwrap_or_else(|_| {
            let what = if b.what.is_empty() { "説明はまだありません。".to_string() } else { b.what.clone() };
            format!("{what}\n\n## 書き足す場所\n\nこの説明は `{mine}` に書くと、ここに出ます。`![説明](/絶対パス/絵.png)` で絵を入れられます。")
        });
        self.about.insert(b.key.clone(), text.clone());
        text
    }

    pub(super) fn about_tab(&mut self, ui: &mut egui::Ui, b: &Build, w: f32) {
        let text = self.about_text(b);
        egui::Frame::new().fill(CARD).corner_radius(CornerRadius::same(14)).inner_margin(egui::Margin::same(20)).show(ui, |ui| {
            ui.set_width(w - 40.0);
            for line in text.lines() {
                let l = line.trim_end();
                let clean: String = l.chars().filter(|c| *c != '`' && *c != '*').collect();
                if let Some(t) = clean.strip_prefix("# ") {
                    ui.label(RichText::new(t).font(bold(24.0)).color(TEXT));
                    ui.add_space(4.0);
                } else if let Some(t) = clean.strip_prefix("## ").or_else(|| clean.strip_prefix("### ")) {
                    ui.add_space(14.0);
                    ui.label(RichText::new(t).font(bold(20.0)).color(TEXT));
                    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
                    ui.painter().rect_filled(r, CornerRadius::ZERO, LINE);
                    ui.add_space(4.0);
                } else if let Some(t) = clean.strip_prefix("- ") {
                    ui.horizontal_top(|ui| {
                        ui.add_space(6.0);
                        ui.label(RichText::new("•").size(14.5).color(MUTED));
                        ui.add(egui::Label::new(RichText::new(t).size(14.5).color(MUTED)).wrap());
                    });
                } else if l.starts_with("![") {
                    if let Some((cap, rest)) = l[2..].split_once("](") {
                        let path = rest.trim_end_matches(')');
                        ui.add(egui::Image::new(format!("file://{path}")).max_width(ui.available_width()).corner_radius(CornerRadius::same(8)));
                        if !cap.is_empty() {
                            ui.label(RichText::new(cap).size(12.5).color(FAINT));
                        }
                        ui.add_space(6.0);
                    }
                } else if clean.trim().is_empty() {
                    ui.add_space(6.0);
                } else {
                    ui.add(egui::Label::new(RichText::new(clean).size(14.5).color(MUTED)).wrap());
                }
            }
        });
    }

    // ---------- 機能の表 ----------

    fn table_frame(ui: &mut egui::Ui, w: f32, add: impl FnOnce(&mut egui::Ui)) {
        egui::Frame::new().fill(CARD).corner_radius(CornerRadius::same(14)).show(ui, |ui| {
            ui.set_width(w);
            ui.spacing_mut().item_spacing.y = 0.0;
            add(ui);
        });
    }

    fn table_head(ui: &mut egui::Ui, w: f32, cols: &[(f32, &str)]) {
        let (head, _) = ui.allocate_exact_size(Vec2::new(w, 44.0), Sense::hover());
        for (x, s) in cols {
            ui.painter().text(Pos2::new(head.left() + x, head.center().y), Align2::LEFT_CENTER, *s, bold(13.5), TEXT);
        }
    }

    fn table_row(ui: &mut egui::Ui, w: f32, h: f32, click: bool) -> (Rect, egui::Response) {
        let (r, resp) = ui.allocate_exact_size(Vec2::new(w, h), if click { Sense::click() } else { Sense::hover() });
        ui.painter().rect_filled(Rect::from_min_size(r.min, Vec2::new(w, 1.0)), CornerRadius::ZERO, LINE);
        if click && resp.hovered() {
            ui.painter().rect_filled(r.shrink(1.0), CornerRadius::same(4), CARD_HI);
        }
        (r, resp)
    }

    fn round_letter(ui: &egui::Ui, center: Pos2, letter: &str, color: Color32) {
        let p = ui.painter();
        p.circle_filled(center, 15.0, color.gamma_multiply(0.16));
        p.circle_stroke(center, 15.0, Stroke::new(1.0, color.gamma_multiply(0.45)));
        p.text(center, Align2::CENTER_CENTER, letter, bold(13.0), color);
    }

    fn green_play(ui: &mut egui::Ui, rect: Rect, tip: &str) -> bool {
        place_fill(ui, rect, true, egui::Button::new(RichText::new(I_PLAY).size(14.0).color(ACCENT_TEXT)).frame(false)).on_hover_text(tip).clicked()
    }

    pub(super) fn features_tab(&mut self, ui: &mut egui::Ui, w: f32, actions: &mut Vec<Action>) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(I_FILTER).size(13.0).color(MUTED));
            let text = |t: FeatTab| match t {
                FeatTab::Features => "本体の機能",
                FeatTab::Tests => "試験のビルド",
                FeatTab::Old => "古い版",
            };
            let before = self.feat_tab;
            egui::ComboBox::from_id_salt("feat").selected_text(text(self.feat_tab)).width(120.0).show_ui(ui, |ui| {
                for t in [FeatTab::Features, FeatTab::Tests, FeatTab::Old] {
                    ui.selectable_value(&mut self.feat_tab, t, text(t));
                }
            });
            if before != self.feat_tab {
                self.pages.remove("rows");
            }
            if self.feat_tab != FeatTab::Features {
                ui.add(egui::TextEdit::singleline(&mut self.query).hint_text(format!("{I_SEARCH}  探す")).desired_width(170.0).margin(egui::Margin::symmetric(10, 6)));
                egui::ComboBox::from_id_salt("state").selected_text(self.state_filter.text()).width(110.0).show_ui(ui, |ui| {
                    for s in [StateFilter::All, StateFilter::Ready, StateFilter::Building, StateFilter::NotBuilt, StateFilter::Failed] {
                        ui.selectable_value(&mut self.state_filter, s, s.text());
                    }
                });
                egui::ComboBox::from_id_salt("cat").selected_text(self.category.unwrap_or("すべての分け方")).width(110.0).show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.category, None, "すべての分け方");
                    for c in scan::CATEGORIES {
                        ui.selectable_value(&mut self.category, Some(c), c);
                    }
                });
            }
        });
        ui.add_space(10.0);
        match self.feat_tab {
            FeatTab::Features => self.feature_table(ui, w, actions),
            FeatTab::Tests => {
                let list = self.filtered(|b| b.kind == Kind::Test);
                self.build_table(ui, list, w, actions);
            }
            FeatTab::Old => {
                let list = self.filtered(|b| b.kind == Kind::Old);
                self.build_table(ui, list, w, actions);
            }
        }
    }

    /// 本体の機能(falcon_features.py)の表。▶ でその機能だけ有効にして開く
    fn feature_table(&mut self, ui: &mut egui::Ui, w: f32, actions: &mut Vec<Action>) {
        let feats = self.features.clone();
        if feats.is_empty() {
            ui.label(RichText::new(if self.features_rx.is_some() { "読んでいます…" } else { "機能の一覧を読めませんでした。" }).color(FAINT));
            return;
        }
        let host_ok = self.builds.iter().any(|b| b.key == "host" && b.launchable());
        let (x_ver, x_kind) = (w - 320.0, w - 240.0);
        let test_keys: HashSet<String> = self.builds.iter().filter(|b| b.kind == Kind::Test).map(|b| b.key.clone()).collect();
        Self::table_frame(ui, w, |ui| {
            Self::table_head(ui, w, &[(62.0, "機能"), (x_ver, "版"), (x_kind, "種類")]);
            let none = Feature { id: "none".into(), label: "機能なし".into(), version: String::new(), kind: String::new(), units: vec![], description: "本家の Cycles と比べる時に、機能を全部切って開く".into() };
            for f in feats.iter().chain(std::iter::once(&none)) {
                let first_unit = f.units.iter().find(|u| test_keys.contains(*u)).cloned();
                let (r, resp) = Self::table_row(ui, w, 64.0, first_unit.is_some());
                if resp.clicked() {
                    if let Some(u) = &first_unit {
                        actions.push(Action::Open(u.clone()));
                    }
                }
                let (letter, kind_text, color) = feature_kind(&f.kind);
                Self::round_letter(ui, Pos2::new(r.left() + 30.0, r.center().y), letter, color);
                let tw = x_ver - 62.0 - 12.0;
                ui.painter().galley(r.min + Vec2::new(62.0, 12.0), elide(ui, &f.label, bold(14.5), TEXT, tw), TEXT);
                ui.painter().galley(r.min + Vec2::new(62.0, 35.0), elide(ui, &f.description, FontId::proportional(12.5), MUTED, tw), MUTED);
                if !f.version.is_empty() {
                    paint_chip(ui, Pos2::new(r.left() + x_ver, r.center().y - 11.0), &format!("v{}", f.version), MUTED, 74.0);
                }
                paint_chip(ui, Pos2::new(r.left() + x_kind, r.center().y - 11.0), kind_text, MUTED, 150.0);
                let play = Rect::from_min_size(Pos2::new(r.right() - 76.0, r.center().y - 16.0), Vec2::new(34.0, 32.0));
                if host_ok && Self::green_play(ui, play, &format!("{} だけ有効で起動する", f.label)) {
                    actions.push(Action::LaunchFeature(f.id.clone(), f.label.clone()));
                }
                let more = Rect::from_min_size(Pos2::new(r.right() - 40.0, r.center().y - 16.0), Vec2::new(34.0, 32.0));
                kebab(ui, more, &f.id, false, |ui| {
                    if ui.add_enabled(host_ok, plain_item(format!("{I_PLAY}  これだけ有効で起動する"))).clicked() {
                        actions.push(Action::LaunchFeature(f.id.clone(), f.label.clone()));
                    }
                    for u in f.units.iter().filter(|u| test_keys.contains(*u)) {
                        if ui.add(plain_item(format!("{I_RIGHT}  試験 {u} のページ"))).clicked() {
                            actions.push(Action::Open(u.clone()));
                        }
                    }
                });
            }
        });
        ui.add_space(8.0);
        ui.label(RichText::new("頭の「起動する」は全機能有効で開きます。行の ▶ は、その機能だけ有効で開きます。").size(12.5).color(FAINT));
    }

    /// 試験のビルドと古い版の表
    fn build_table(&mut self, ui: &mut egui::Ui, list: Vec<Build>, w: f32, actions: &mut Vec<Action>) {
        if list.is_empty() {
            ui.label(RichText::new("当てはまる物がありません。").color(FAINT));
            return;
        }
        let pages = list.len().div_ceil(ROWS_PER_PAGE);
        let mut page = self.page_no("rows").min(pages - 1);
        let (x_cat, x_date, x_size) = (w - 400.0, w - 300.0, w - 196.0);
        Self::table_frame(ui, w, |ui| {
            Self::table_head(ui, w, &[(62.0, "ビルド"), (x_cat, "分け方"), (x_date, "ビルドした日"), (x_size, "大きさ")]);
            for b in list.iter().skip(page * ROWS_PER_PAGE).take(ROWS_PER_PAGE) {
                let (r, resp) = Self::table_row(ui, w, 64.0, true);
                if resp.clicked() {
                    actions.push(Action::Open(b.key.clone()));
                }
                let (letter, color, tip) = state_badge(b);
                let c = Pos2::new(r.left() + 30.0, r.center().y);
                Self::round_letter(ui, c, letter, color);
                ui.interact(Rect::from_center_size(c, Vec2::splat(30.0)), ui.scope_id().with(("badge", &b.key)), Sense::hover()).on_hover_text(tip);
                let tw = x_cat - 62.0 - 12.0;
                ui.painter().galley(r.min + Vec2::new(62.0, 12.0), elide(ui, &b.title, bold(14.5), TEXT, tw), TEXT);
                let what = if b.what.is_empty() { "説明はまだありません" } else { b.what.as_str() };
                ui.painter().galley(r.min + Vec2::new(62.0, 35.0), elide(ui, what, FontId::proportional(12.5), MUTED, tw), MUTED);
                paint_chip(ui, Pos2::new(r.left() + x_cat, r.center().y - 11.0), b.category, MUTED, 92.0);
                let built = if b.built_ts > 0 { when(b.built_ts) } else { "-".to_string() };
                ui.painter().text(Pos2::new(r.left() + x_date, r.center().y), Align2::LEFT_CENTER, built, FontId::proportional(12.5), MUTED);
                let size = self.sizes.get(&b.key).map(|(_, n)| gb(*n)).unwrap_or_else(|| "-".into());
                ui.painter().text(Pos2::new(r.left() + x_size, r.center().y), Align2::LEFT_CENTER, size, FontId::proportional(12.5), MUTED);
                let act = Rect::from_min_size(Pos2::new(r.right() - 76.0, r.center().y - 16.0), Vec2::new(34.0, 32.0));
                if b.building.is_some() {
                    if place_fill(ui, act, true, egui::Button::new(RichText::new(I_LOG).size(14.0).color(WARN)).frame(false)).on_hover_text("ログ").clicked() {
                        actions.push(Action::ShowLog(b.key.clone()));
                    }
                } else if b.launchable() {
                    if Self::green_play(ui, act, "起動する") {
                        actions.push(Action::Launch(b.key.clone()));
                    }
                } else if b.kind == Kind::Test && place_fill(ui, act, true, egui::Button::new(RichText::new(I_BUILD).size(14.0).color(MUTED)).frame(false)).on_hover_text("ビルドする").clicked() {
                    actions.push(Action::Build(b.key.clone()));
                }
                let more = Rect::from_min_size(Pos2::new(r.right() - 40.0, r.center().y - 16.0), Vec2::new(34.0, 32.0));
                let fav = self.favorites.contains(&b.key);
                kebab(ui, more, &b.key, false, |ui| {
                    if ui.add(plain_item(format!("{}  {}", if fav { I_HEART } else { I_HEART_O }, if fav { "お気に入りから外す" } else { "お気に入りに足す" }))).clicked() {
                        actions.push(Action::Fav(b.key.clone()));
                    }
                    if ui.add_enabled(b.dir.is_dir(), plain_item(format!("{I_FOLDER}  ビルド先を開く"))).clicked() {
                        actions.push(Action::Folder(b.dir.clone()));
                    }
                    if ui.add_enabled(b.dir.join("build.log").is_file(), plain_item(format!("{I_LOG}  ビルドのログ"))).clicked() {
                        actions.push(Action::ShowLog(b.key.clone()));
                    }
                    if b.kind == Kind::Test && ui.add_enabled(b.building.is_none(), plain_item(format!("{I_BUILD}  {}", if b.has_bin { "ビルドし直す" } else { "ビルドする" }))).clicked() {
                        actions.push(Action::Build(b.key.clone()));
                    }
                });
            }
        });
        ui.add_space(8.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| pager(ui, &mut page, pages));
        self.pages.insert("rows", page);
    }

    /// クラスターの場面の表
    pub(super) fn scenes_tab(&mut self, ui: &mut egui::Ui, w: f32, actions: &mut Vec<Action>) {
        let scenes = self.scenes.clone();
        if scenes.is_empty() {
            ui.label(RichText::new("場面がありません。").color(FAINT));
            return;
        }
        let pages = scenes.len().div_ceil(ROWS_PER_PAGE);
        let mut page = self.page_no("rows").min(pages - 1);
        let (x_kind, x_date, x_size) = (w - 400.0, w - 290.0, w - 186.0);
        Self::table_frame(ui, w, |ui| {
            Self::table_head(ui, w, &[(62.0, "場面"), (x_kind, "描き方"), (x_date, "書き出し"), (x_size, "大きさ")]);
            for s in scenes.iter().skip(page * ROWS_PER_PAGE).take(ROWS_PER_PAGE) {
                let (r, _) = Self::table_row(ui, w, 56.0, false);
                let (letter, color) = if s.material && s.look { ("G", OK) } else if s.material { ("M", LINK) } else { ("S", FAINT) };
                Self::round_letter(ui, Pos2::new(r.left() + 30.0, r.center().y), letter, color);
                ui.painter().galley(Pos2::new(r.left() + 62.0, r.center().y - 10.0), elide(ui, &s.name, bold(14.5), TEXT, x_kind - 74.0), TEXT);
                let kind = if s.material { "材質・GI" } else { "形だけ" };
                paint_chip(ui, Pos2::new(r.left() + x_kind, r.center().y - 11.0), kind, MUTED, 100.0);
                ui.painter().text(Pos2::new(r.left() + x_date, r.center().y), Align2::LEFT_CENTER, when(s.ts), FontId::proportional(12.5), MUTED);
                ui.painter().text(Pos2::new(r.left() + x_size, r.center().y), Align2::LEFT_CENTER, gb(s.bytes), FontId::proportional(12.5), MUTED);
                let act = Rect::from_min_size(Pos2::new(r.right() - 50.0, r.center().y - 16.0), Vec2::new(34.0, 32.0));
                if Self::green_play(ui, act, "窓で開いて歩き回る(Esc で閉じる)") {
                    actions.push(Action::LaunchScene(s.clone()));
                }
            }
        });
        ui.add_space(8.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| pager(ui, &mut page, pages));
        self.pages.insert("rows", page);
    }

    // ---------- 更新履歴 ----------

    pub(super) fn changes_tab(&mut self, ui: &mut egui::Ui, b: &Build, w: f32, actions: &mut Vec<Action>) {
        let Some(all) = self.changes.get(&b.key).cloned() else {
            ui.label(RichText::new("読んでいます…").color(FAINT));
            return;
        };
        let list: Vec<&Change> = all
            .iter()
            .filter(|c| match self.change_filter {
                ChangeFilter::All => true,
                ChangeFilter::Merge => c.kind == ChangeKind::Merge,
                ChangeFilter::Commit => c.kind == ChangeKind::Commit,
                ChangeFilter::Built => c.kind == ChangeKind::Built,
                ChangeFilter::Version => c.kind == ChangeKind::Version,
            })
            .collect();
        let pages = list.len().div_ceil(CHANGES_PER_PAGE).max(1);
        let mut page = self.page_no("changes").min(pages - 1);
        ui.horizontal(|ui| {
            ui.label(RichText::new(I_FILTER).size(13.0).color(MUTED));
            let before = self.change_filter;
            egui::ComboBox::from_id_salt("chg").selected_text(self.change_filter.text()).width(130.0).show_ui(ui, |ui| {
                for f in [ChangeFilter::All, ChangeFilter::Merge, ChangeFilter::Commit, ChangeFilter::Built, ChangeFilter::Version] {
                    ui.selectable_value(&mut self.change_filter, f, f.text());
                }
            });
            if before != self.change_filter {
                page = 0;
            }
            ui.label(RichText::new(format!("{} 件", list.len())).size(12.5).color(FAINT));
            if ui.add(button(RichText::new("すべて畳む").size(12.5).color(TEXT), false)).clicked() {
                self.open_changes.clear();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| pager(ui, &mut page, pages));
        });
        ui.add_space(10.0);
        if list.is_empty() {
            ui.label(RichText::new("当てはまる更新はありません。").color(FAINT));
            self.pages.insert("changes", page);
            return;
        }
        egui::Frame::new().fill(CARD).corner_radius(CornerRadius::same(14)).inner_margin(egui::Margin::symmetric(16, 12)).show(ui, |ui| {
            ui.set_width(w - 32.0);
            let shown: Vec<&&Change> = list.iter().skip(page * CHANGES_PER_PAGE).take(CHANGES_PER_PAGE).collect();
            let n = shown.len();
            for (i, c) in shown.into_iter().enumerate() {
                self.change_row(ui, b, c, i + 1 == n, actions);
            }
        });
        ui.add_space(8.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| pager(ui, &mut page, pages));
        self.pages.insert("changes", page);
    }

    fn change_row(&mut self, ui: &mut egui::Ui, b: &Build, c: &Change, last: bool, actions: &mut Vec<Action>) {
        let w = ui.available_width();
        let open = self.open_changes.contains(&c.id);
        let (r, resp) = ui.allocate_exact_size(Vec2::new(w, 38.0), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(r, CornerRadius::same(6), CARD_HI.gamma_multiply(0.6));
        }
        if resp.clicked() {
            actions.push(Action::Toggle(c.id.clone(), c.full.clone()));
        }
        let painter = ui.painter().clone();
        let dot = Pos2::new(r.left() + 10.0, r.center().y);
        let dot_color = match c.kind {
            ChangeKind::Built if !c.ok => ERR,
            ChangeKind::Built => ACCENT,
            ChangeKind::Version => LINK,
            ChangeKind::Merge | ChangeKind::Commit => ORANGE,
        };
        // 右の「起動する」(今ビルド済みの物の行だけ)
        let current = c.kind == ChangeKind::Built && c.ok && b.launchable();
        let right_w = if current { 110.0 } else { 0.0 };
        if current {
            let br = Rect::from_min_size(Pos2::new(r.right() - right_w, r.top() + 4.0), Vec2::new(right_w, 30.0));
            if place_fill(ui, br, true, egui::Button::new(RichText::new(format!("{I_PLAY}  起動する")).font(bold(14.0)).color(ACCENT_TEXT)).frame(false)).clicked() {
                actions.push(Action::Launch(b.key.clone()));
            }
        }
        // 開閉の印と「題 by 誰 on いつ」
        painter.text(Pos2::new(r.left() + 30.0, r.center().y), Align2::CENTER_CENTER, if open { I_DOWN } else { I_RIGHT }, FontId::proportional(10.0), FAINT);
        let tx = r.left() + 42.0;
        let who = painter.layout_no_wrap(c.who.clone(), FontId::proportional(13.5), LINK);
        let date = painter.layout_no_wrap(format!("· {}", when(c.ts)), FontId::proportional(13.5), MUTED);
        let by = painter.layout_no_wrap("by".into(), FontId::proportional(13.5), MUTED);
        let tag_g = painter.layout_no_wrap(c.tag.clone(), FontId::monospace(11.5), FAINT);
        let tail = by.size().x + who.size().x + date.size().x + tag_g.size().x + 40.0;
        let title_max = (w - 42.0 - right_w - tail - 12.0).max(120.0);
        let tg = elide(ui, &c.title, bold(16.0), TEXT, title_max);
        let tw = tg.size().x;
        painter.galley(Pos2::new(tx, r.center().y - tg.size().y / 2.0), tg, TEXT);
        let mut mx = tx + tw + 10.0;
        for (g, gap) in [(by, 5.0), (who, 5.0), (date, 12.0), (tag_g, 0.0)] {
            let gs = g.size();
            painter.galley(Pos2::new(mx, r.center().y - gs.y / 2.0), g, TEXT);
            mx += gs.x + gap;
        }
        // 開いた中身
        let mut bottom = r.bottom();
        if open {
            let detail = if c.full.is_empty() { None } else { self.details.get(&c.full).cloned() };
            ui.horizontal_top(|ui| {
                ui.add_space(42.0);
                ui.vertical(|ui| {
                    ui.set_width(w - 58.0);
                    let bullet = |ui: &mut egui::Ui, t: &str| {
                        ui.horizontal_top(|ui| {
                            ui.add_space(4.0);
                            ui.label(RichText::new("•").size(14.0).color(MUTED));
                            ui.add(egui::Label::new(RichText::new(t).size(14.0).color(MUTED)).wrap());
                        });
                    };
                    for l in &c.body {
                        ui.add(egui::Label::new(RichText::new(l).size(14.0).color(MUTED)).wrap());
                    }
                    if let Some(d) = &detail {
                        for l in &d.body {
                            match l.strip_prefix("- ").or_else(|| l.strip_prefix("・")) {
                                Some(t) => bullet(ui, t),
                                None => {
                                    ui.add(egui::Label::new(RichText::new(l).size(14.0).color(MUTED)).wrap());
                                }
                            }
                        }
                        if !d.merged.is_empty() {
                            ui.add_space(4.0);
                            ui.label(RichText::new(format!("入ったコミット {} 本", d.merged.len())).size(12.5).color(FAINT));
                            for (h, s) in &d.merged {
                                bullet(ui, &format!("{s}  ({h})"));
                            }
                        }
                        if !d.stat.is_empty() {
                            ui.add_space(4.0);
                            ui.label(RichText::new(&d.stat).size(12.0).color(FAINT));
                        }
                    }
                    ui.add_space(8.0);
                });
            });
            bottom = ui.cursor().min.y;
        }
        // 線: 開いている所は実線、畳んでいる所は点線(次の点まで)
        if !last {
            let (a, z) = (dot.y + 7.0, bottom + 12.0);
            let color = ORANGE.gamma_multiply(0.85);
            if open {
                painter.line_segment([Pos2::new(dot.x, a), Pos2::new(dot.x, z)], Stroke::new(2.0, color));
            } else {
                let mut y = a;
                while y < z {
                    painter.line_segment([Pos2::new(dot.x, y), Pos2::new(dot.x, (y + 4.0).min(z))], Stroke::new(2.0, color));
                    y += 8.0;
                }
            }
        }
        painter.circle_filled(dot, 7.0, dot_color);
    }

    // ---------- ギャラリー ----------

    /// 小さな絵を枠に描く。引き伸ばさない。
    /// cover = true: 枠いっぱいに切り出す(比べの絵は左上の 1 枚)。false: 全部を枠の中に収める
    /// 絵の小さな絵の URI。ask なら、まだ無い時に作るよう頼む
    pub(super) fn thumb_uri(&mut self, m: &Media, ask: bool) -> Option<String> {
        let pending = !m.frames.is_empty() && !m.path.is_file();
        let (src, video) = if pending { (m.frames[0].clone(), false) } else { (m.path.clone(), m.video) };
        let t = if ask { self.thumb(&src, video) } else { self.thumb_done.lock().ok().and_then(|d| d.get(&src).cloned()).flatten() };
        t.map(|t| format!("file://{}", t.display()))
    }

    /// 画面のすぐ外の絵を、前もって読んでおく(小さな絵を作り、控えに載せる。描かない)
    pub(super) fn prefetch_thumb(&mut self, ctx: &egui::Context, m: &Media, size: Vec2) {
        if let Some(uri) = self.thumb_uri(m, true) {
            let _ = egui::Image::new(uri).load_for_size(ctx, size);
        }
    }

    /// ライブラリの絵のうち、遠くなった物(この前の回に残すと決めなかった物)を控えから外す。
    /// ライブラリを描いていない時は全部外す(App::ui から)
    pub(super) fn sweep_gallery_tex(&mut self, ctx: &egui::Context) {
        let frame = ctx.cumulative_frame_nr();
        self.gallery_drawn = frame;
        let old: Vec<String> = self.gallery_tex.iter().filter(|(_, f)| **f + 1 < frame).map(|(u, _)| u.clone()).collect();
        for u in old {
            ctx.forget_image(&u);
            self.gallery_tex.remove(&u);
        }
    }

    pub(super) fn paint_thumb(&mut self, ui: &mut egui::Ui, r: Rect, m: &Media, cr: CornerRadius, cover: bool) {
        let painter = ui.painter().clone();
        painter.rect_filled(r, cr, BG);
        let pending = !m.frames.is_empty() && !m.path.is_file();
        let loaded = self.thumb_uri(m, true).and_then(|uri| {
            let img = egui::Image::new(uri);
            let size = img.load_for_size(ui.ctx(), r.size()).ok().and_then(|p| p.size())?;
            Some((img, size))
        });
        match loaded {
            Some((img, size)) if cover => {
                let (a, ra) = (size.x / size.y.max(1.0), r.width() / r.height());
                let uv = if a > ra {
                    let w = ra / a;
                    let x = if a > 2.4 { 0.0 } else { (1.0 - w) / 2.0 };
                    Rect::from_min_max(Pos2::new(x, 0.0), Pos2::new(x + w, 1.0))
                } else {
                    let h = a / ra;
                    let y = if a < 1.2 { 0.0 } else { (1.0 - h) / 2.0 };
                    Rect::from_min_max(Pos2::new(0.0, y), Pos2::new(1.0, y + h))
                };
                img.uv(uv).corner_radius(cr).paint_at(ui, r);
            }
            Some((img, size)) => {
                let k = (r.width() / size.x.max(1.0)).min(r.height() / size.y.max(1.0));
                img.paint_at(ui, Rect::from_center_size(r.center(), size * k));
            }
            None => {
                painter.text(r.center(), Align2::CENTER_CENTER, "…", FontId::proportional(16.0), FAINT);
            }
        }
        if pending {
            let failed = self.seq_done.lock().ok().and_then(|d| d.get(&m.path).copied()) == Some(false);
            let text = if failed { "動画にできませんでした" } else { "動画にしています…" };
            let g = painter.layout_no_wrap(text.into(), FontId::proportional(11.5), TEXT);
            let br = Rect::from_min_size(Pos2::new(r.left() + 8.0, r.bottom() - g.size().y - 14.0), g.size() + Vec2::new(12.0, 6.0));
            painter.rect_filled(br, CornerRadius::same(4), Color32::from_black_alpha(190));
            painter.galley(br.min + Vec2::new(6.0, 3.0), g, TEXT);
        }
        if m.video {
            painter.circle_filled(r.center(), 19.0, Color32::from_black_alpha(170));
            painter.text(r.center() + Vec2::new(2.0, 0.0), Align2::CENTER_CENTER, I_PLAY, FontId::proportional(15.0), Color32::WHITE);
        }
    }

    /// 拡大表示を閉じた時・絵を替えた時の片付け(動画を止め、読んだ絵を控えから外す)
    fn lightbox_release(&mut self, ctx: &egui::Context, keep: Option<&str>) {
        if self.lightbox_uri.as_deref() != keep {
            if let Some(u) = self.lightbox_uri.take() {
                ctx.forget_image(&u);
            }
        }
    }

    pub(super) fn lightbox_modal(&mut self, ctx: &egui::Context) {
        let Some((items, mut i, dir)) = self.lightbox.clone() else {
            self.player = None;
            self.lightbox_release(ctx, None);
            return;
        };
        if items.is_empty() {
            self.lightbox = None;
            return;
        }
        let n = items.len();
        if ctx.input(|s| s.key_pressed(egui::Key::ArrowRight)) {
            i = (i + 1) % n;
        }
        if ctx.input(|s| s.key_pressed(egui::Key::ArrowLeft)) {
            i = (i + n - 1) % n;
        }
        let m = items[i.min(n - 1)].clone();
        // 絵が替わったら、拡大と移動を元に戻す
        if self.lb_item.as_ref() != Some(&m.path) {
            self.lb_item = Some(m.path.clone());
            self.lb_zoom = 0.0;
            self.lb_pan = Vec2::ZERO;
            self.lb_scrub = None;
        }
        let screen = ctx.content_rect().size();
        let ready = m.path.is_file();
        let is_video = m.video && ready;
        // 動画は下に再生バーと操作の列を置くので、その分だけ低く
        let (mw, mh) = ((screen.x - 120.0).max(300.0), (screen.y - if is_video { 270.0 } else { 220.0 }).max(200.0));
        // 「フォルダ」はライブラリから開くと空なので、その絵の置き場にする
        let orig_dir = dir.clone();
        let dir = if dir.as_os_str().is_empty() { m.path.parent().map(Path::to_path_buf).unwrap_or_default() } else { dir };
        // 動画は ffmpeg でほどいて見せる(egui は動画を読めず、赤い三角になっていた)
        if is_video {
            if self.player.as_ref().is_none_or(|p| p.path != m.path) {
                self.player = None;
                match video::Player::open(&m.path, Vec2::new(mw, mh)) {
                    Ok(p) => self.player = Some(p),
                    Err(e) => self.message = e,
                }
            }
            self.lightbox_release(ctx, None);
        } else {
            self.player = None;
            let uri = format!("file://{}", m.path.display());
            self.lightbox_release(ctx, Some(&uri));
            if ready {
                self.lightbox_uri = Some(uri);
            }
        }
        // 動画のキー(Blender の再生に寄せる): スペース 再生・止める / , . 1 コマ / [ ] 倍速
        if let Some(p) = self.player.as_ref().filter(|_| is_video) {
            // ボタンへ届く前に受け取る(スペースが押したボタンにも効いて、2 回切り替わらないように)
            let (space, back, fwd, slower, faster) = ctx.input_mut(|s| {
                let none = egui::Modifiers::NONE;
                (s.consume_key(none, egui::Key::Space), s.consume_key(none, egui::Key::Comma), s.consume_key(none, egui::Key::Period), s.consume_key(none, egui::Key::OpenBracket), s.consume_key(none, egui::Key::CloseBracket))
            });
            if space {
                p.toggle();
            }
            if back {
                p.step(-1);
            }
            if fwd {
                p.step(1);
            }
            let cur = video::SPEEDS.iter().position(|s| (*s - p.speed()).abs() < 0.01).unwrap_or(2);
            if slower && cur > 0 {
                p.set_speed(video::SPEEDS[cur - 1]);
            }
            if faster && cur + 1 < video::SPEEDS.len() {
                p.set_speed(video::SPEEDS[cur + 1]);
            }
        }
        // 見せる物の元の大きさ(拡大の基準)とテクスチャ
        let (content, tex_id): (Option<Vec2>, Option<egui::TextureId>) = if !ready {
            (None, None)
        } else if is_video {
            match self.player.as_mut() {
                Some(p) => {
                    let src = Vec2::new(p.src[0] as f32, p.src[1] as f32);
                    (Some(src), p.texture(ctx).map(|t| t.id()))
                }
                None => (None, None),
            }
        } else {
            match egui::Image::new(format!("file://{}", m.path.display())).load_for_size(ctx, Vec2::new(mw, mh)) {
                Ok(egui::load::TexturePoll::Ready { texture }) => (Some(texture.size), Some(texture.id)),
                _ => (None, None),
            }
        };
        let fit = content.map(|c| (mw / c.x).min(mh / c.y)).unwrap_or(1.0);
        let mut close = false;
        let mut outside = None;
        let mut toggle = false;
        let vinfo = self.player.as_ref().filter(|_| is_video).map(|p| (p.paused(), p.position(), p.duration, p.fps, p.speed(), p.error()));
        let resp = egui::Modal::new(egui::Id::unique("falcon-launcher-lightbox")).show(ctx, |ui| {
            // ---- 絵の枠: ホイールで拡大(マウスの所を中心に)・ドラッグで動かす・ダブルクリックか Home で全体 ----
            let (area, aresp) = ui.allocate_exact_size(Vec2::new(mw, mh), Sense::click_and_drag());
            let painter = ui.painter_at(area);
            painter.rect_filled(area, CornerRadius::same(6), Color32::from_black_alpha(90));
            let mut scale = if self.lb_zoom > 0.0 { self.lb_zoom } else { fit };
            if content.is_some() {
                if aresp.hovered() {
                    let dy = ui.input(|s| s.smooth_scroll_delta.y);
                    if dy != 0.0 {
                        // ホイール 1 回で 1.25 倍(Blender と同じくらい。1 回 ≒ 112 点)
                        let new = (scale * (dy * 0.002).exp()).clamp(fit.min(1.0) * 0.1, 32.0);
                        if let Some(at) = aresp.hover_pos() {
                            // マウスの下の点がずれないように
                            let rel = at - (area.center() + self.lb_pan);
                            self.lb_pan = at - area.center() - rel * (new / scale);
                        }
                        scale = new;
                        self.lb_zoom = new;
                    }
                }
                if aresp.dragged() {
                    if self.lb_zoom == 0.0 {
                        self.lb_zoom = scale;
                    }
                    self.lb_pan += aresp.drag_delta();
                }
                let (home, one) = ui.input(|s| (s.key_pressed(egui::Key::Home) || s.key_pressed(egui::Key::F), s.key_pressed(egui::Key::Num1)));
                if aresp.double_clicked() || home {
                    self.lb_zoom = 0.0;
                    self.lb_pan = Vec2::ZERO;
                    scale = fit;
                }
                if one {
                    // 等倍(元の 1 画素を画面の 1 画素に)
                    scale = 1.0 / ui.ctx().pixels_per_point();
                    self.lb_zoom = scale;
                }
                if self.lb_zoom == 0.0 {
                    self.lb_pan = Vec2::ZERO;
                }
            }
            if let Some(p) = self.player.as_ref().filter(|_| is_video) {
                p.set_zoomed(scale > fit * 1.05);
            }
            if !ready {
                let text = if m.frames.is_empty() { "ファイルが見つかりません" } else { "連番を動画にしています…(できたら見られます)" };
                painter.text(area.center(), Align2::CENTER_CENTER, text, FontId::proportional(15.0), MUTED);
            } else if let Some(e) = vinfo.as_ref().and_then(|v| v.5.clone()) {
                painter.text(area.center(), Align2::CENTER_CENTER, e, FontId::proportional(14.0), ERR);
            } else if let (Some(c), Some(id)) = (content, tex_id) {
                let r = Rect::from_center_size(area.center() + self.lb_pan, c * scale);
                painter.image(id, r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                // 左上に倍率(元の画素に対して)
                let pct = scale * ui.ctx().pixels_per_point() * 100.0;
                let label = if self.lb_zoom == 0.0 { format!("全体 · {pct:.0}%") } else { format!("{pct:.0}%") };
                let g = painter.layout_no_wrap(label, FontId::proportional(11.5), TEXT);
                let br = Rect::from_min_size(area.min + Vec2::new(8.0, 8.0), g.size() + Vec2::new(12.0, 6.0));
                painter.rect_filled(br, CornerRadius::same(4), Color32::from_black_alpha(170));
                painter.galley(br.min + Vec2::new(6.0, 3.0), g, TEXT);
                let tip = "ホイールで拡大・縮小 / ドラッグで動かす / ダブルクリックか Home で全体 / 1 で等倍";
                aresp.on_hover_text_at_pointer(tip);
            } else {
                painter.text(area.center(), Align2::CENTER_CENTER, if is_video { "動画を読んでいます…" } else { "絵を読んでいます…" }, FontId::proportional(14.0), MUTED);
            }
            // ---- 動画: 再生バーと操作の列 ----
            if let Some((paused, pos, duration, fps, speed, _)) = vinfo {
                ui.add_space(8.0);
                let shown = self.lb_scrub.unwrap_or(pos);
                let (br, bresp) = ui.allocate_exact_size(Vec2::new(mw, 22.0), Sense::click_and_drag());
                let p = ui.painter();
                let track = Rect::from_center_size(br.center(), Vec2::new(br.width() - 16.0, 6.0));
                let frac = if duration > 0.0 { (shown / duration).clamp(0.0, 1.0) as f32 } else { 0.0 };
                p.rect_filled(track, CornerRadius::same(3), CARD_HI);
                p.rect_filled(Rect::from_min_max(track.min, Pos2::new(track.left() + track.width() * frac, track.bottom())), CornerRadius::same(3), ACCENT);
                let hx = track.left() + track.width() * frac;
                p.circle_filled(Pos2::new(hx, track.center().y), if bresp.hovered() || bresp.dragged() { 8.0 } else { 6.5 }, TEXT);
                let at_t = |x: f32| ((x - track.left()) / track.width()).clamp(0.0, 1.0) as f64 * duration;
                if let Some(pp) = bresp.hover_pos() {
                    let t = at_t(pp.x);
                    bresp.clone().on_hover_text_at_pointer(format!("{}  ·  コマ {}", clock(t), (t * fps as f64).round() as i64 + 1));
                }
                if duration > 0.0 {
                    if let Some(pp) = bresp.interact_pointer_pos().filter(|_| bresp.is_pointer_button_down_on()) {
                        let t = at_t(pp.x);
                        // ドラッグ中は 0.08 秒おきに飛ぶ(ffmpeg を起こし直すので間引く)
                        if self.lb_scrub_at.elapsed() > Duration::from_millis(80) {
                            if let Some(pl) = &self.player {
                                pl.seek(t);
                            }
                            self.lb_scrub_at = Instant::now();
                        }
                        self.lb_scrub = Some(t);
                    }
                    if bresp.drag_stopped() || bresp.clicked() {
                        // 押して離すのが 1 回の描き直しの間に起きた時は、離した所で飛ぶ
                        let t = self.lb_scrub.or_else(|| bresp.interact_pointer_pos().map(|pp| at_t(pp.x)));
                        if let (Some(pl), Some(t)) = (&self.player, t) {
                            pl.seek(t);
                        }
                        self.lb_scrub = None;
                    }
                }
                ui.add_space(4.0);
                // 各枠は決まった幅:
                // ボタンの字や時刻・コマ番号の桁が変わっても、右の物がずれないように
                let mono = FontId::monospace(12.5);
                let frames_total = (duration * fps as f64).round() as i64;
                let time_text = format!("{} / {}", clock(shown), clock(duration));
                let frame_text = format!("コマ {} / {frames_total}", (shown * fps as f64).round() as i64 + 1);
                // 一番長くなる時(全体と同じ桁)の幅
                let widest = |t: String| ui.painter().layout_no_wrap(t, mono.clone(), TEXT).size().x + 4.0;
                let time_w = widest(format!("{} / {}", clock(duration), clock(duration)));
                let frame_w = widest(format!("コマ {frames_total} / {frames_total}"));
                let fixed = |ui: &mut egui::Ui, w: f32, add: &mut dyn FnMut(&mut egui::Ui)| {
                    ui.allocate_ui_with_layout(Vec2::new(w, 32.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.set_min_width(w);
                        add(ui);
                    });
                };
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let small = |t: &str| button(RichText::new(t).color(TEXT), false).min_size(Vec2::new(36.0, 32.0));
                    if ui.add(small("\u{f048}")).on_hover_text("1 コマ戻る( , )").clicked() {
                        if let Some(pl) = &self.player {
                            pl.step(-1);
                        }
                    }
                    let play = button(RichText::new(if paused { format!("{I_PLAY}  再生") } else { "❚❚  止める".to_string() }).color(TEXT), false).min_size(Vec2::new(96.0, 32.0));
                    if ui.add(play).on_hover_text("スペースのキーでも切り替えられます").clicked() {
                        toggle = true;
                    }
                    if ui.add(small("\u{f051}")).on_hover_text("1 コマ進む( . )").clicked() {
                        if let Some(pl) = &self.player {
                            pl.step(1);
                        }
                    }
                    ui.add_space(8.0);
                    fixed(ui, time_w, &mut |ui| {
                        ui.label(RichText::new(&time_text).font(mono.clone()).color(TEXT));
                    });
                    fixed(ui, frame_w, &mut |ui| {
                        ui.label(RichText::new(&frame_text).font(mono.clone()).color(MUTED));
                    });
                    ui.add_space(8.0);
                    // 倍速
                    let text = |s: f32| if s == 1.0 { "等速 1×".to_string() } else { format!("{s}×") };
                    egui::ComboBox::from_id_salt("lightbox-speed").selected_text(text(speed)).width(84.0).show_ui(ui, |ui| {
                        for s in video::SPEEDS {
                            if ui.selectable_label((s - speed).abs() < 0.01, text(s)).clicked() {
                                if let Some(pl) = &self.player {
                                    pl.set_speed(s);
                                }
                            }
                        }
                    })
                    .response
                    .on_hover_text("[ と ] のキーでも変えられます");
                    ui.label(RichText::new(format!("{fps:.0} fps · 繰り返し · 音なし")).size(12.0).color(FAINT));
                });
            }
            ui.add_space(6.0);
            if !m.caption.is_empty() {
                ui.add(egui::Label::new(RichText::new(&m.caption).size(14.0).color(TEXT)).wrap());
            }
            ui.label(RichText::new(m.path.display().to_string()).family(egui::FontFamily::Monospace).size(11.5).color(FAINT));
            ui.horizontal(|ui| {
                if ui.add_enabled(n > 1, button(RichText::new(I_LEFT).color(TEXT), false)).on_hover_text("前の物(←)").clicked() {
                    i = (i + n - 1) % n;
                }
                ui.label(RichText::new(format!("{} / {n}", i + 1)).size(12.5).color(MUTED));
                if ui.add_enabled(n > 1, button(RichText::new(I_RIGHT).color(TEXT), false)).on_hover_text("次の物(→)").clicked() {
                    i = (i + 1) % n;
                }
                ui.add_space(12.0);
                if ui.add(button(RichText::new(format!("{I_EXT}  外で開く")).color(TEXT), false)).clicked() {
                    outside = Some(m.path.clone());
                }
                if ui.add(button(RichText::new(format!("{I_FOLDER}  フォルダ")).color(TEXT), false)).clicked() {
                    outside = Some(dir.clone());
                }
                if ui.add(button(RichText::new("閉じる").color(TEXT), false)).clicked() {
                    close = true;
                }
            });
        });
        if toggle {
            if let Some(p) = &self.player {
                p.toggle();
            }
        }
        if let Some(p) = outside {
            // 外で開く時は、ランチャーの中の再生を止める
            if let Some(pl) = &self.player {
                pl.pause();
            }
            self.apply(vec![Action::Play(p)]);
        }
        self.lightbox = if close || resp.should_close() { None } else { Some((items, i, orig_dir)) };
        if self.lightbox.is_none() {
            self.player = None;
            self.lb_item = None;
            self.lightbox_release(ctx, None);
        }
    }
}

/// 「1:05.3」の形の時刻
fn clock(t: f64) -> String {
    let t = t.max(0.0);
    format!("{}:{:04.1}", (t / 60.0).floor() as u64, t % 60.0)
}
