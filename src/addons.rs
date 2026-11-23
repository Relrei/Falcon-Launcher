//! アドオンの頁。どの Blender の版に、どのアドオンが、どの版で入っているかを見る。
//! 縦にアドオン、横に Blender の版。入っている升に、そのアドオンの版を出す。升を押すとフォルダを開く。
//! 読む所(読むだけ):
//! - Blender の設定: ~/.config/blender/<版>/scripts/addons/*(bl_info)と extensions/<置き場>/*(blender_manifest.toml)
//! - ビルドに同梱: <ビルド>/bin/<版>/scripts/addons_core/*(公式の Blender に無い物だけ。Falcon の足した物)
use super::*;
use super::page::{bold, elide};
use std::fs;

pub struct Cell {
    pub version: String,
    pub path: PathBuf,
    /// 名前に .disabled が付いている(手で止めた物)
    pub disabled: bool,
}

pub struct Row {
    pub id: String,
    pub name: String,
    /// 「アドオン」「拡張」「同梱」
    pub kind: &'static str,
    pub cells: HashMap<String, Cell>,
}

pub struct Column {
    pub key: String,
    pub title: String,
    /// その版で動くビルド
    pub sub: String,
}

#[derive(Default)]
pub struct Table {
    pub cols: Vec<Column>,
    pub rows: Vec<Row>,
}

pub struct Addons {
    pub user: Table,
    pub bundled: Table,
    pub scanned: i64,
}

/// 「5.0」→ (5, 0)。版の形でなければ None
fn mm(s: &str) -> Option<(u32, u32)> {
    let (a, b) = s.split_once('.')?;
    Some((a.parse().ok()?, b.parse().ok()?))
}

/// ファイルの頭(64KB まで)
fn head(p: &Path) -> String {
    use std::io::Read;
    let mut s = String::new();
    if let Ok(f) = fs::File::open(p) {
        let _ = f.take(65536).read_to_string(&mut s);
    }
    s
}

fn quoted(s: &str) -> Option<String> {
    let q = s.find(['"', '\''])?;
    let c = s[q..].chars().next()?;
    let rest = &s[q + 1..];
    Some(rest[..rest.find(c)?].to_string())
}

/// bl_info の name と version((1, 2, 0) → 1.2.0)
fn bl_info(py: &Path) -> (String, String) {
    let text = head(py);
    let Some(at) = text.find("bl_info") else { return (String::new(), String::new()) };
    let body = &text[at..];
    let body = &body[..body.find("\n}").map(|i| i + 2).unwrap_or(body.len())];
    let mut name = String::new();
    let mut ver = String::new();
    for l in body.lines() {
        let t = l.trim_start();
        if name.is_empty() && (t.starts_with("\"name\"") || t.starts_with("'name'")) {
            name = t.split_once(':').and_then(|x| quoted(x.1)).unwrap_or_default();
        }
        if ver.is_empty() && (t.starts_with("\"version\"") || t.starts_with("'version'")) {
            if let Some(inner) = t.split_once('(').and_then(|x| x.1.split_once(')')).map(|x| x.0) {
                ver = inner.split(',').map(str::trim).filter(|x| !x.is_empty()).collect::<Vec<_>>().join(".");
            }
        }
    }
    (name, ver)
}

/// blender_manifest.toml の name と version
fn manifest(p: &Path) -> (String, String) {
    let text = head(p);
    let get = |k: &str| text.lines().map(str::trim).find(|l| l.split_once('=').is_some_and(|x| x.0.trim() == k)).and_then(|l| quoted(l.split_once('=')?.1)).unwrap_or_default();
    (get("name"), get("version"))
}

/// scripts/addons(や addons_core)の中のアドオン: (id, 名前, 版, 場所, 止め)
fn legacy(dir: &Path) -> Vec<(String, String, String, PathBuf, bool)> {
    let mut out = vec![];
    for e in fs::read_dir(dir).into_iter().flatten().flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        if n.starts_with('.') || n.starts_with("__") {
            continue;
        }
        let p = e.path();
        let (py, id) = if p.is_dir() {
            (p.join("__init__.py"), n.clone())
        } else if let Some(stem) = n.strip_suffix(".py") {
            (p.clone(), stem.to_string())
        } else {
            continue;
        };
        if !py.is_file() {
            continue;
        }
        let disabled = id.contains(".disabled");
        let id = id.split(".disabled").next().unwrap_or(&id).to_string();
        let (name, ver) = bl_info(&py);
        out.push((id, name, ver, p, disabled));
    }
    out
}

/// extensions/<置き場>/<id>/blender_manifest.toml
fn extensions(dir: &Path) -> Vec<(String, String, String, PathBuf, bool)> {
    let mut out = vec![];
    for repo in fs::read_dir(dir).into_iter().flatten().flatten() {
        if repo.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        for e in fs::read_dir(repo.path()).into_iter().flatten().flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            let m = e.path().join("blender_manifest.toml");
            if n.starts_with('.') || !m.is_file() {
                continue;
            }
            let (name, ver) = manifest(&m);
            out.push((n, name, ver, e.path(), false));
        }
    }
    out
}

/// ビルドの中の版のフォルダ(<ビルド>/bin/5.3・<ビルド>/4.5・/usr/share/blender/4.5)
pub fn version_dir(b: &Build) -> Option<(String, PathBuf)> {
    let mut roots = vec![b.dir.join("bin"), b.dir.clone()];
    if b.dir.ends_with("bin") {
        if let Some(p) = b.dir.parent() {
            roots.push(p.join("share/blender"));
        }
    }
    let mut found: Vec<(String, PathBuf)> = roots
        .iter()
        .flat_map(|r| fs::read_dir(r).into_iter().flatten().flatten())
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            (mm(&n).is_some() && e.path().join("scripts").is_dir()).then(|| (n, e.path()))
        })
        .collect();
    found.sort_by_key(|x| mm(&x.0));
    found.pop()
}

fn add(rows: &mut Vec<Row>, kind: &'static str, col: &str, items: Vec<(String, String, String, PathBuf, bool)>) {
    for (id, name, version, path, disabled) in items {
        let row = match rows.iter_mut().position(|r| r.id == id && r.kind == kind) {
            Some(i) => &mut rows[i],
            None => {
                rows.push(Row { id: id.clone(), name: String::new(), kind, cells: HashMap::new() });
                rows.last_mut().unwrap()
            }
        };
        if row.name.is_empty() && !name.is_empty() {
            row.name = name;
        }
        // 止めた写しより、生きている方を出す
        if row.cells.get(col).is_none_or(|c| c.disabled && !disabled) {
            row.cells.insert(col.to_string(), Cell { version, path, disabled });
        }
    }
}

pub fn scan(builds: &[Build]) -> Addons {
    let home = std::env::var("HOME").unwrap_or_default();
    let conf = PathBuf::from(format!("{home}/.config/blender"));
    // ビルド → 版
    let bver: Vec<(&Build, String, PathBuf)> = builds.iter().filter_map(|b| version_dir(b).map(|(v, p)| (b, v, p))).collect();
    // Blender の設定に入れた物
    let mut vers: Vec<String> = fs::read_dir(&conf).into_iter().flatten().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| mm(n).is_some()).collect();
    vers.sort_by_key(|v| mm(v));
    let mut user = Table::default();
    for v in &vers {
        let users: Vec<&str> = bver.iter().filter(|x| &x.1 == v).map(|x| x.0.title.as_str()).collect();
        user.cols.push(Column { key: v.clone(), title: format!("Blender {v}"), sub: users.join("・") });
        let d = conf.join(v);
        add(&mut user.rows, "アドオン", v, legacy(&d.join("scripts/addons")));
        add(&mut user.rows, "拡張", v, extensions(&d.join("extensions")));
    }
    // ビルドに同梱(公式に無い物だけ)
    let mut official: HashMap<String, HashSet<String>> = HashMap::new();
    for (b, v, p) in &bver {
        if b.key.starts_with("official:") {
            official.entry(v.clone()).or_default().extend(legacy(&p.join("scripts/addons_core")).into_iter().map(|x| x.0));
        }
    }
    let any_official: HashSet<String> = official.values().flatten().cloned().collect();
    let mut bundled = Table::default();
    for (b, v, p) in &bver {
        if b.key.starts_with("official:") {
            continue;
        }
        let base = official.get(v).unwrap_or(&any_official);
        let items: Vec<_> = legacy(&p.join("scripts/addons_core"))
            .into_iter()
            .filter(|x| if base.is_empty() { x.0.contains("falcon") || head(&x.3.join("__init__.py")).to_lowercase().contains("falcon") } else { !base.contains(&x.0) })
            .collect();
        if items.is_empty() {
            continue;
        }
        bundled.cols.push(Column { key: b.key.clone(), title: b.title.clone(), sub: format!("Blender {v} の同梱") });
        add(&mut bundled.rows, "同梱", &b.key, items);
    }
    for t in [&mut user, &mut bundled] {
        t.rows.sort_by_key(|r| (r.kind, if r.name.is_empty() { r.id.to_lowercase() } else { r.name.to_lowercase() }));
    }
    Addons { user, bundled, scanned: now() }
}

const NAME_W: f32 = 300.0;
const COL_W: f32 = 104.0;
const ROW_H: f32 = 36.0;
const HEAD_H: f32 = 52.0;

impl App {
    pub(super) fn addons_page(&mut self, ui: &mut egui::Ui, w: f32) {
        if self.addons.is_none() {
            self.addons = Some(Arc::new(scan(&self.builds)));
        }
        let data = self.addons.clone().unwrap();
        ui.label(RichText::new("アドオン").font(bold(24.0)).color(TEXT));
        ui.label(RichText::new("どの Blender の版に、どのアドオンが入っているかの表です。升には入っているアドオンの版を出します。升を押すと、そのフォルダを開きます。").size(12.5).color(FAINT));
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.add(egui::TextEdit::singleline(&mut self.addon_query).hint_text(format!("{I_SEARCH}  名前で探す")).desired_width((w - 330.0).max(160.0)).margin(egui::Margin::symmetric(12, 8)));
            ui.checkbox(&mut self.addon_show_disabled, "止めた物も出す");
            if ui.button(format!("{I_REFRESH}  読み直す")).on_hover_text(format!("前に読んだ時: {}", when(data.scanned))).clicked() {
                self.addons = Some(Arc::new(scan(&self.builds)));
            }
        });
        ui.add_space(14.0);
        let mut open = None;
        for (title, note, t, salt) in [
            ("Blender の設定に入れた物", "~/.config/blender/<版>/ の scripts/addons と extensions。同じ版の Blender なら、どのビルドからも使われます", &data.user, "user"),
            ("ビルドに同梱(公式の Blender に無い物)", "ビルドの中の scripts/addons_core のうち、Falcon が足した物", &data.bundled, "bundled"),
        ] {
            let rows: Vec<&Row> = t.rows.iter().filter(|r| self.addon_matches(r)).collect();
            ui.horizontal(|ui| {
                ui.label(RichText::new(title).font(bold(17.0)).color(TEXT));
                ui.label(RichText::new(rows.len().to_string()).size(13.0).color(FAINT));
            });
            ui.label(RichText::new(note).size(12.0).color(FAINT));
            ui.add_space(8.0);
            if t.cols.is_empty() || rows.is_empty() {
                ui.label(RichText::new("当てはまる物がありません。").color(FAINT));
            } else if let Some(p) = Self::addon_table(ui, t, &rows, w, salt) {
                open = Some(p);
            }
            ui.add_space(22.0);
        }
        if let Some(p) = open {
            let _ = Command::new("xdg-open").arg(&p).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
            self.message = format!("フォルダを開きました: {}", p.display());
        }
    }

    fn addon_matches(&self, r: &Row) -> bool {
        if !self.addon_show_disabled && r.cells.values().all(|c| c.disabled) {
            return false;
        }
        let q = self.addon_query.to_lowercase();
        q.is_empty() || r.id.to_lowercase().contains(&q) || r.name.to_lowercase().contains(&q)
    }

    /// 表を描く。押された升のフォルダを返す
    fn addon_table(ui: &mut egui::Ui, t: &Table, rows: &[&Row], w: f32, salt: &str) -> Option<PathBuf> {
        let mut open = None;
        // 同じ名前の行(フォルダ名の違う写し)は、フォルダ名を添えて見分ける
        let mut seen: HashMap<&str, usize> = HashMap::new();
        for r in rows {
            *seen.entry(if r.name.is_empty() { r.id.as_str() } else { r.name.as_str() }).or_default() += 1;
        }
        let total = NAME_W + COL_W * t.cols.len() as f32;
        egui::ScrollArea::horizontal().id_salt(("addons", salt)).auto_shrink([false, true]).show(ui, |ui| {
            let tw = total.max(w);
            let (r, _) = ui.allocate_exact_size(Vec2::new(tw, HEAD_H + ROW_H * rows.len() as f32), Sense::hover());
            let p = ui.painter().clone();
            p.rect_filled(r, CornerRadius::same(12), CARD.gamma_multiply(0.6));
            p.rect_stroke(r, CornerRadius::same(12), Stroke::new(1.0, LINE), StrokeKind::Inside);
            // 見出し
            p.text(Pos2::new(r.left() + 14.0, r.top() + HEAD_H / 2.0), Align2::LEFT_CENTER, "アドオン", bold(13.0), MUTED);
            for (i, c) in t.cols.iter().enumerate() {
                let cx = r.left() + NAME_W + COL_W * i as f32;
                let hr = Rect::from_min_size(Pos2::new(cx, r.top()), Vec2::new(COL_W, HEAD_H));
                p.galley(Pos2::new(cx + 6.0, r.top() + 9.0), elide(ui, &c.title, bold(13.0), TEXT, COL_W - 12.0), TEXT);
                let sub = if c.sub.is_empty() { "ビルドなし" } else { c.sub.as_str() };
                p.galley(Pos2::new(cx + 6.0, r.top() + 29.0), elide(ui, sub, FontId::proportional(11.0), FAINT, COL_W - 12.0), FAINT);
                ui.interact(hr, ui.scope_id().with(("addon-head", salt, i)), Sense::hover()).on_hover_text(format!("{}\n{}", c.title, if c.sub.is_empty() { "この版で動くビルドは見つかっていません" } else { c.sub.as_str() }));
            }
            p.rect_filled(Rect::from_min_size(Pos2::new(r.left(), r.top() + HEAD_H - 1.0), Vec2::new(tw, 1.0)), CornerRadius::ZERO, LINE);
            // 行
            for (ri, row) in rows.iter().enumerate() {
                let y = r.top() + HEAD_H + ROW_H * ri as f32;
                let rr = Rect::from_min_size(Pos2::new(r.left(), y), Vec2::new(tw, ROW_H));
                if !ui.is_rect_visible(rr) {
                    continue;
                }
                if ri % 2 == 1 {
                    p.rect_filled(rr.shrink2(Vec2::new(1.0, 0.0)), CornerRadius::ZERO, CARD.gamma_multiply(0.5));
                }
                let name = if row.name.is_empty() { row.id.as_str() } else { row.name.as_str() };
                let ng = elide(ui, name, bold(13.5), TEXT, NAME_W - 90.0);
                let nw = ng.size().x;
                p.galley(Pos2::new(rr.left() + 14.0, rr.center().y - ng.size().y / 2.0), ng, TEXT);
                let note = if seen.get(name).copied().unwrap_or(0) > 1 { format!("{} · {}", row.kind, row.id) } else { row.kind.to_string() };
                let room = NAME_W - 30.0 - nw;
                if room > 20.0 {
                    p.galley(Pos2::new(rr.left() + 22.0 + nw, rr.center().y - 7.0), elide(ui, &note, FontId::proportional(11.0), FAINT, room), FAINT);
                }
                let nr = Rect::from_min_size(rr.min, Vec2::new(NAME_W, ROW_H));
                ui.interact(nr, ui.scope_id().with(("addon-name", salt, ri)), Sense::hover()).on_hover_text(format!("{name}\nid: {}", row.id));
                for (ci, c) in t.cols.iter().enumerate() {
                    let cr = Rect::from_min_size(Pos2::new(rr.left() + NAME_W + COL_W * ci as f32, y), Vec2::new(COL_W, ROW_H));
                    let Some(cell) = row.cells.get(&c.key) else {
                        p.text(cr.center(), Align2::CENTER_CENTER, "·", FontId::proportional(13.0), LINE);
                        continue;
                    };
                    let resp = ui.interact(cr, ui.scope_id().with(("addon-cell", salt, ri, ci)), Sense::click());
                    let pill = Rect::from_center_size(cr.center(), Vec2::new(COL_W - 14.0, 24.0));
                    let (fill, color) = if cell.disabled { (CARD_HI, FAINT) } else { (ACCENT.gamma_multiply(if resp.hovered() { 0.28 } else { 0.14 }), ACCENT_TEXT) };
                    p.rect_filled(pill, CornerRadius::same(12), fill);
                    let text = match (cell.version.is_empty(), cell.disabled) {
                        (_, true) => "止め".to_string(),
                        (true, false) => "入っている".to_string(),
                        (false, false) => cell.version.clone(),
                    };
                    let g = elide(ui, &text, FontId::proportional(12.0), color, COL_W - 22.0);
                    p.galley(pill.center() - g.size() / 2.0, g, color);
                    let tip = format!("{}\n{}{}\n押すとフォルダを開きます", c.title, cell.path.display(), if cell.disabled { "\n(名前に .disabled が付いていて、止めてあります)" } else { "" });
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tip).clicked() {
                        open = Some(if cell.path.is_dir() { cell.path.clone() } else { cell.path.parent().map(Path::to_path_buf).unwrap_or_default() });
                    }
                }
            }
        });
        open
    }
}
