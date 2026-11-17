//! 建てる準備の頁の「取って建てる・更新」の枠と、その確かめの画面・見回り(中身は setup.rs)。
//! 新しい版の検知と、ランチャーから Falcon Engine を取って建てるところまでを、ランチャーの中で済ませるため。

use super::*;

/// 置き場の版を見る間合い(GitHub へ 1 回 ls-remote するだけ)
const REMOTE_EVERY: Duration = Duration::from_secs(3 * 3600);

impl App {
    pub(crate) fn poll_setup(&mut self, ctx: &egui::Context) {
        // 置き場の版: 起動した時と 3 時間ごと
        if self.remote_rx.is_none() && self.remote.checked.is_none_or(|t| t.elapsed() > REMOTE_EVERY) {
            self.check_remote(ctx);
        }
        if let Some(rx) = &self.remote_rx {
            if let Ok(r) = rx.try_recv() {
                self.remote_rx = None;
                self.remote.checked = Some(Instant::now());
                match r {
                    Ok(tags) => {
                        self.remote.latest = tags.into_iter().next();
                        self.remote.error = None;
                    }
                    Err(e) => self.remote.error = Some(e),
                }
            }
        }
        // 走っている物(1 秒ごと)と手元の様子(5 秒ごと)
        let every = if self.setup_running { 1 } else { 5 };
        if self.setup_last.elapsed() > Duration::from_secs(every) {
            self.setup_last = Instant::now();
            let now = setup::running_pid().is_some();
            if now || self.setup_status.stage.is_empty() {
                self.setup_status = setup::status();
            }
            if self.setup_running && !now {
                // 終わった
                self.setup_status = setup::status();
                self.builds = scan::builds();
                self.prep_checks = prep::check_all(&self.prep_paths);
                self.message = match setup::last_rc() {
                    Some(0) => "Falcon Engine を建て終わりました。ホームの版の一覧から起動できます".into(),
                    Some(143) => "取って建てるを止めました。もう一度押すと続きから建てます".into(),
                    Some(rc) => format!("取って建てるが途中で止まりました(rc {rc})。建てる準備の頁に記録の終わりを出しています"),
                    None => "取って建てるが終わりました(結果が分かりません)".into(),
                };
            }
            self.setup_running = now;
            self.setup_info = setup::info();
        }
    }

    fn check_remote(&mut self, ctx: &egui::Context) {
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(setup::remote_tags());
            ctx.request_repaint();
        });
        self.remote_rx = Some(rx);
    }

    /// 新しい版がある時の札(ホームの上)
    pub(crate) fn update_banner(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(latest) = self.remote.latest.clone() else { return };
        if self.setup_running || self.setup_info.foreign || !self.setup_info.is_newer(&latest) {
            return;
        }
        // 今の版: 建てた版。まだ建てていなければ、取ったソースの版
        let cur = match (&self.setup_info.built, &self.setup_info.source_tag) {
            (Some(b), _) => b.clone(),
            (None, Some(t)) => format!("ソース {}・まだ建てていません", t.trim_start_matches(setup::TAG_PREFIX)),
            (None, None) => "?".into(),
        };
        egui::Frame::new().fill(ACCENT.gamma_multiply(0.10)).stroke(Stroke::new(1.0, ACCENT.gamma_multiply(0.6))).corner_radius(CornerRadius::same(12)).inner_margin(egui::Margin::symmetric(16, 10)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(I_DOWNLOAD).size(15.0).color(ACCENT_TEXT));
                ui.label(RichText::new(format!("Falcon Engine の新しい版 {} があります(今は {cur})", latest.trim_start_matches(setup::TAG_PREFIX))).font(bold(14.0)).color(TEXT));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(row_button("建てる準備で更新する")).clicked() {
                        self.goto(View::Prep, ctx);
                    }
                });
            });
        });
        ui.add_space(14.0);
    }

    /// 建てる準備の頁の枠
    pub(crate) fn setup_card(&mut self, ui: &mut egui::Ui, w: f32, ctx: &egui::Context) {
        let info = self.setup_info.clone();
        let note = "GitHub から Falcon Engine を取り、Blender の部品を取り、設定して建てます(docs/BUILD.md と同じ手順)。ランチャーを閉じても続きます";
        let mut start_tag: Option<String> = None;
        Self::settings_card(ui, w, "Falcon Engine を取って建てる・更新", note, |ui, iw| {
            // 置き場の版
            ui.horizontal(|ui| {
                ui.label(RichText::new("置き場の一番新しい版").size(13.0).color(MUTED));
                match (&self.remote.latest, &self.remote.error, self.remote_rx.is_some()) {
                    (_, _, true) => ui.label(RichText::new("見ています…").size(13.0).color(FAINT)),
                    (_, Some(e), _) => ui.label(RichText::new(e).size(12.5).color(WARN)),
                    (Some(l), _, _) => ui.label(RichText::new(l.trim_start_matches(setup::TAG_PREFIX)).font(bold(13.5)).color(TEXT)),
                    (None, _, _) => ui.label(RichText::new("版のタグがありません").size(13.0).color(FAINT)),
                };
                ui.label(RichText::new(format!("·  {}", setup::repo())).size(12.0).color(FAINT));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add_enabled(self.remote_rx.is_none(), soft_button(format!("{I_REFRESH}  版を確かめる"))).clicked() {
                        self.check_remote(ctx);
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("建てた版").size(13.0).color(MUTED));
                let built = info.built.clone().unwrap_or_else(|| "まだ".into());
                ui.label(RichText::new(built).font(bold(13.5)).color(TEXT));
                let src = match (&info.source_tag, info.has_source) {
                    (Some(t), _) => format!("·  ソース {}", t.trim_start_matches(setup::TAG_PREFIX)),
                    (None, true) => "·  ソースはタグの上にありません".into(),
                    (None, false) => "·  ソースはまだ取っていません".into(),
                };
                ui.label(RichText::new(src).size(12.0).color(FAINT));
            });
            ui.add_space(8.0);

            // 走っている間 / 前の回の結果
            let st = self.setup_status.clone();
            if self.setup_running {
                ui.label(RichText::new(format!("{I_BUILD}  {}", if st.stage.is_empty() { "始めています" } else { &st.stage })).font(bold(14.0)).color(ACCENT_TEXT));
                // 帯は段 4 の間ずっと出す(出たり消えたりでボタンがずれないように)
                if st.stage.starts_with("4/4") {
                    let p = &st.progress;
                    let (f, t) = if p.total > 0 { (p.done as f32 / p.total as f32, format!("{} / {}  ·  {}", p.done, p.total, p.step)) } else { (0.0, "数えています".into()) };
                    ui.add(egui::ProgressBar::new(f).desired_width(iw).text(RichText::new(t).color(TEXT)));
                }
            } else if setup::last_rc() == Some(143) {
                ui.label(RichText::new(format!("\u{f04d}  前の回は「{}」の所で止めました。もう一度押すと続きから建てます", st.stage)).font(bold(13.5)).color(MUTED));
            } else if let Some(rc) = setup::last_rc().filter(|&rc| rc != 0) {
                ui.label(RichText::new(format!("\u{f071}  前の回は「{}」で失敗しました(rc {rc})。下は記録の終わりです", st.stage)).font(bold(13.5)).color(WARN));
            } else if setup::last_rc() == Some(0) && st.stage.starts_with("終わり") {
                ui.label(RichText::new(format!("\u{f00c}  前の回は終わりまで進みました({})", st.stage.trim_start_matches("終わり"))).size(13.0).color(OK));
            }
            let show_tail = self.setup_running || setup::last_rc().is_some_and(|rc| rc != 0 && rc != 143);
            if show_tail && !st.tail.is_empty() {
                egui::Frame::new().fill(BG).corner_radius(CornerRadius::same(8)).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
                    ui.set_width(iw - 20.0);
                    for l in &st.tail {
                        ui.add(egui::Label::new(RichText::new(l).family(egui::FontFamily::Monospace).size(11.5).color(MUTED)).truncate());
                    }
                });
                ui.add_space(6.0);
            }

            // 押す物
            ui.horizontal_wrapped(|ui| {
                if self.setup_running {
                    if ui.add(soft_button("\u{f04d}  止める")).on_hover_text("台本と、その下の git・ninja をまとめて止めます。次に始めると、建てた所の続きから建てます").clicked() {
                        setup::stop();
                        self.message = "取って建てるを止めました".into();
                    }
                } else if info.foreign {
                    ui.label(RichText::new(format!("{} はランチャーが取ったソースではないので(開発の木など)、ここからは切り替えません", places::places().source)).size(12.5).color(FAINT));
                } else if let Some(latest) = self.remote.latest.clone() {
                    let need_ok = self.prep_checks.iter().filter(|c| !c.optional).all(|c| c.state == prep::State::Ok);
                    let v = latest.trim_start_matches(setup::TAG_PREFIX).to_string();
                    let (label, primary) = if info.is_newer(&latest) {
                        (format!("{I_DOWNLOAD}  {} → {v} に更新して建てる", info.built.clone().or(info.source_tag.clone()).unwrap_or_default().trim_start_matches(setup::TAG_PREFIX)), true)
                    } else if !info.has_source {
                        (format!("{I_DOWNLOAD}  {v} を取って建てる"), true)
                    } else if info.built.is_none() {
                        (format!("{I_BUILD}  {v} を建てる(続きから)"), true)
                    } else {
                        (format!("{I_BUILD}  {v} を建て直す"), false)
                    };
                    let b = if primary { accent_button(label) } else { soft_button(label) };
                    let r = ui.add_enabled(need_ok, b);
                    let r = if need_ok { r } else { r.on_disabled_hover_text("上の要る物が揃ってから始められます") };
                    if r.clicked() {
                        start_tag = Some(latest.clone());
                    }
                    if !info.is_newer(&latest) && info.built.is_some() {
                        ui.label(RichText::new("最新です").size(12.5).color(OK));
                    }
                }
                if setup::log_path().is_file() && ui.add(soft_button(format!("{I_LOG}  記録を開く"))).on_hover_text(setup::log_path().display().to_string()).clicked() {
                    let _ = Command::new("xdg-open").arg(setup::log_path()).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
                }
            });
            ui.add_space(6.0);
            // 並びの数
            ui.horizontal(|ui| {
                ui.label(RichText::new("建てる時の並びの数").size(12.5).color(MUTED));
                let max = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2) as u32;
                let mut j = setup::jobs(&self.prep_paths);
                if ui.add(egui::DragValue::new(&mut j).range(1..=max)).changed() {
                    self.prep_paths.insert("jobs".into(), j.to_string());
                    prep::save_paths(&self.prep_paths);
                }
                ui.label(RichText::new(format!("(nice -n 10 で回します。CPU {max} のうち。多いほど速く、画面が重くなります)")).size(12.0).color(FAINT));
            });
        });
        if let Some(tag) = start_tag {
            match setup::plan(&tag, &self.prep_paths) {
                Ok(p) => self.setup_confirm = Some(p),
                Err(e) => self.message = e,
            }
        }
    }

    pub(crate) fn setup_confirm_modal(&mut self, ctx: &egui::Context) {
        let Some(p) = &self.setup_confirm else { return };
        let mut go = false;
        let mut close = false;
        let resp = egui::Modal::new(egui::Id::unique("falcon-launcher-setup-confirm")).show(ctx, |ui| {
            ui.set_width(620.0);
            let v = p.tag.trim_start_matches(setup::TAG_PREFIX);
            ui.label(RichText::new(if p.fresh { format!("Falcon Engine {v} を取って建てます") } else { format!("Falcon Engine を {v} にして建てます") }).size(17.0).color(TEXT));
            ui.add_space(8.0);
            let mut lines = vec![];
            if p.fresh {
                lines.push(format!("1. ソースを取ります(約 0.4 GB。試験の絵 tests/files は取りません): {}", p.source));
            } else {
                lines.push(format!("1. ソースを {v} に切り替えます: {}(手で変えた所がある時は止まります)", p.source));
            }
            lines.push(if p.fresh { "2. Blender の部品(lib/linux_x64、約 2.4 GB)を projects.blender.org から取ります".into() } else { "2. Blender の部品(lib/linux_x64)を確かめます(版で変わっていれば取ります)".into() });
            lines.push(format!("3. 設定します: CUDA の核 {} · DLSS {}", p.arch, if p.dlss.is_some() { "あり" } else { "なし(Neural Rendering は使えません)" }));
            lines.push(format!("4. 建てます: {} へ。nice -n 10 で並び {}。初めては 1〜2 時間ほど、更新は変わった所だけ", p.build, p.jobs));
            for l in lines {
                ui.add(egui::Label::new(RichText::new(l).size(12.5).color(MUTED)).wrap());
            }
            ui.add_space(6.0);
            ui.add(egui::Label::new(RichText::new("CUDA・OptiX・DLSS は建てる準備で置いた物を使います(ランチャーは NVIDIA の物を取りに行きません)").size(12.0).color(FAINT)).wrap());
            ui.collapsing(RichText::new("走らせる台本").size(12.5).color(MUTED), |ui| {
                egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                    ui.add(egui::Label::new(RichText::new(&p.script).family(egui::FontFamily::Monospace).size(11.0).color(MUTED)).wrap());
                });
            });
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.add(button(RichText::new(format!("{I_BUILD}  始める")).color(ON_ACCENT), true)).clicked() {
                    go = true;
                }
                if ui.add(button(RichText::new("やめる").color(TEXT), false)).clicked() {
                    close = true;
                }
            });
        });
        if go {
            let p = self.setup_confirm.take().unwrap();
            self.message = match setup::start(&p) {
                Ok(()) => format!("取って建てるを始めました(記録: {})", setup::log_path().display()),
                Err(e) => format!("始められませんでした: {e}"),
            };
            self.setup_running = setup::running_pid().is_some();
            self.setup_status = setup::Status::default();
            self.setup_last = Instant::now() - Duration::from_secs(10);
        } else if close || resp.should_close() {
            self.setup_confirm = None;
        }
    }
}
