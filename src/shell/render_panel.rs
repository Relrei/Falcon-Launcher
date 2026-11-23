//! レンダリングの一覧の右の列: 選んだシーンの描き方(サンプル数・デノイズ)と割り当てを、Blender のプロパティのように並べる。
//! 変えた所だけを描く時に上書きする(queue::RenderOver。中身は --python-expr)。空にするとシーンのまま。

use super::*;
use queue::{denoiser_name, denoisers, Job, Limits, RenderOver};

fn engine_name(e: &str) -> &str {
    match e {
        "CYCLES" => "Cycles",
        "NEURAL_RENDERING" => "Cycles-expansion",
        "BLENDER_EEVEE" | "BLENDER_EEVEE_NEXT" => "EEVEE",
        "BLENDER_WORKBENCH" => "Workbench",
        "" => "分かりません",
        x => x,
    }
}

impl App {
    /// 右の列で見ている行。選んだ物が無い時は、描いている物か一番上
    pub(crate) fn focused_job(&self, jobs: &[Job]) -> Option<u64> {
        self.queue_focus
            .filter(|id| jobs.iter().any(|j| j.id == *id))
            .or_else(|| jobs.iter().find(|j| matches!(j.state, JobState::Running | JobState::Paused)).map(|j| j.id))
            .or_else(|| jobs.first().map(|j| j.id))
    }

    /// 一覧の頁の右の列
    pub(super) fn render_panel(&mut self, ui: &mut egui::Ui) {
        let jobs = self.queue.snapshot();
        let focus = self.focused_job(&jobs);
        egui::ScrollArea::vertical().id_salt("right-render").auto_shrink(false).show(ui, |ui| {
            let w = ui.available_width() - 14.0;
            ui.set_max_width(w);
            ui.add_space(16.0);
            ui.label(RichText::new("シーンの描き方").font(bold(15.0)).color(TEXT));
            ui.add_space(4.0);
            let Some(j) = focus.and_then(|id| jobs.iter().find(|j| j.id == id)) else {
                ui.label(RichText::new("一覧にシーンが入ると、ここで描き方を変えられます").size(12.0).color(FAINT));
                return;
            };
            let title = if j.scene.is_empty() { j.name() } else { j.scene.clone() };
            ui.add(egui::Label::new(RichText::new(&title).font(bold(14.0)).color(ACCENT_TEXT)).truncate());
            ui.add(egui::Label::new(RichText::new(format!("{}  ·  {}", j.name(), j.build_title)).size(11.5).color(FAINT)).truncate());
            ui.add_space(2.0);
            ui.label(RichText::new("行の名前を押すと、その行を選びます。変えた所だけ上書きし、次に描き始める時から効きます").size(11.0).color(FAINT));
            let mut rs = j.rs.clone();
            let mut lim = j.lim;
            if j.inbox.is_empty() {
                Self::render_sections(ui, j, &mut rs);
            } else {
                // アドオンから来た物の描き方は Blender で決める(ここは見せるだけ)
                ui.add_enabled_ui(false, |ui| Self::render_sections(ui, j, &mut rs));
                ui.add_space(4.0);
                ui.add(egui::Label::new(RichText::new("サンプル数とデノイザーは、Blender の出力のプロパティの「Falcon Launcher」で決めます(「シーンの描き方のまま」を切る)").size(11.0).color(FAINT)).wrap());
            }
            self.limit_section(ui, j, &mut lim);
            ui.add_space(10.0);
            let own = j.inbox.is_empty();
            let t = if own { "全部をシーン・全体の設定に戻す" } else { "割り当てを全体の設定に戻す" };
            if ui.add_enabled((own && !rs.is_empty()) || !lim.is_empty(), egui::Button::new(t)).clicked() {
                if own {
                    rs = RenderOver::default();
                }
                lim = Limits::default();
            }
            if rs != j.rs || lim != j.lim {
                let id = j.id;
                self.queue.edit(|js| {
                    if let Some(x) = js.iter_mut().find(|x| x.id == id) {
                        x.rs = rs;
                        x.lim = lim;
                    }
                });
            }
            ui.add_space(16.0);
        });
    }

    /// Blender のパネルの見出し(畳める)
    fn panel_head(ui: &mut egui::Ui, id: &str, title: &str, add: impl FnOnce(&mut egui::Ui)) {
        ui.add_space(10.0);
        egui::CollapsingHeader::new(RichText::new(title).font(bold(13.5)).color(TEXT)).id_salt(id).default_open(true).show(ui, |ui| {
            ui.add_space(4.0);
            add(ui);
        });
    }

    /// Blender のプロパティの 1 行: 右寄せの名前と、残りの幅の欄(add は欄の幅を受け取る)
    fn prop_row(ui: &mut egui::Ui, label: &str, col: Color32, add: impl FnOnce(&mut egui::Ui, f32)) {
        const LW: f32 = 78.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.allocate_ui_with_layout(Vec2::new(LW, 24.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.set_min_width(LW);
                ui.label(RichText::new(label).size(12.5).color(col));
            });
            let fw = (ui.available_width() - 2.0).max(60.0);
            add(ui, fw);
        });
        ui.add_space(4.0);
    }

    fn render_sections(ui: &mut egui::Ui, j: &Job, rs: &mut RenderOver) {
        let sc = &j.scene_rs;
        let falcon = !j.build.starts_with("official:");
        let known = !sc.engine.is_empty();
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("エンジン").size(12.0).color(MUTED));
            ui.label(RichText::new(engine_name(&sc.engine)).size(12.0).color(TEXT));
        });
        if !known {
            let why = if j.inbox.is_empty() { "手で足した物は、シーンの値を読んでいません" } else { "アドオンが古い時の登録です(写しを取り直すと読めます)" };
            ui.add(egui::Label::new(RichText::new(why).size(11.0).color(FAINT)).wrap());
        }
        if sc.engine == "BLENDER_WORKBENCH" {
            ui.add_space(6.0);
            ui.label(RichText::new("Workbench は、ここでは変えられません").size(12.0).color(FAINT));
            return;
        }
        // サンプリング
        Self::panel_head(ui, "rp-samples", "サンプリング", |ui| {
            Self::prop_row(ui, "サンプル数", MUTED, |ui, _| {
                let mut on = rs.samples.is_some();
                ui.checkbox(&mut on, "").on_hover_text("入れると、このシーンだけサンプル数を変えます");
                if on != rs.samples.is_some() {
                    rs.samples = on.then(|| sc.samples.unwrap_or(128));
                }
                match &mut rs.samples {
                    Some(n) => {
                        ui.add_sized(Vec2::new(ui.available_width() - 2.0, 22.0), egui::DragValue::new(n).range(1..=1_000_000).speed(1.0));
                    }
                    None => {
                        let t = sc.samples.map(|n| format!("シーンのまま({n})")).unwrap_or("シーンのまま".into());
                        ui.label(RichText::new(t).size(12.0).color(FAINT));
                    }
                }
            });
        });
        if sc.is_eevee() {
            ui.add_space(6.0);
            ui.label(RichText::new("EEVEE にデノイズはありません").size(11.5).color(FAINT));
            return;
        }
        // デノイズ
        Self::panel_head(ui, "rp-denoise", "デノイズ", |ui| {
            let scene_dn = match sc.denoise {
                Some(true) => "シーンのまま(入)",
                Some(false) => "シーンのまま(切)",
                None => "シーンのまま",
            };
            Self::prop_row(ui, "デノイズ", MUTED, |ui, fw| {
                let cur = match rs.denoise {
                    None => scene_dn,
                    Some(true) => "入れる",
                    Some(false) => "切る",
                };
                egui::ComboBox::from_id_salt("rp-dn-on").selected_text(cur).width(fw).show_ui(ui, |ui| {
                    ui.selectable_value(&mut rs.denoise, None, scene_dn);
                    ui.selectable_value(&mut rs.denoise, Some(true), "入れる");
                    ui.selectable_value(&mut rs.denoise, Some(false), "切る");
                });
            });
            let off = rs.denoise.or(sc.denoise) == Some(false);
            // 名前が長いので、シーンのままの時は「シーン: OIDN」のように短くする
            let scene_name = if sc.denoiser.is_empty() { "シーンのまま".to_string() } else { format!("シーン: {}", denoiser_name(&sc.denoiser)) };
            Self::prop_row(ui, "デノイザー", if off { FAINT } else { MUTED }, |ui, fw| {
                let cur = rs.denoiser.as_deref().map(|d| denoiser_name(d).to_string()).unwrap_or(scene_name.clone());
                ui.add_enabled_ui(!off, |ui| {
                    egui::ComboBox::from_id_salt("rp-dn").selected_text(cur).width(fw).truncate().show_ui(ui, |ui| {
                        ui.selectable_value(&mut rs.denoiser, None, scene_name.clone());
                        for (k, t) in denoisers(falcon) {
                            ui.selectable_value(&mut rs.denoiser, Some(k.to_string()), t);
                        }
                    });
                });
            });
            if rs.denoiser.as_deref() == Some("DLSS") && !falcon {
                ui.add(egui::Label::new(RichText::new("⚠ DLSS は Falcon の版だけです。この版ではシーンのデノイザーのまま描きます").size(11.0).color(WARN)).wrap());
            } else if rs.denoiser.as_deref() == Some("DLSS") {
                ui.add(egui::Label::new(RichText::new("Blender の設定で GPU の機器を選んでいる時だけ使えます。使えない時はシーンのまま描きます(ログに残ります)").size(11.0).color(FAINT)).wrap());
            }
        });
    }

    /// 割り当て(CPU・RAM。優先度と VRAM はここでは出さない)
    fn limit_section(&mut self, ui: &mut egui::Ui, j: &Job, lim: &mut Limits) {
        let g = self.queue.settings();
        let (ncpu, tpc, ram_total, _) = self.sys_caps();
        Self::panel_head(ui, "rp-limits", "割り当て", |ui| {
            ui.label(RichText::new("入れていない物は、一覧の全体の設定のまま").size(11.0).color(FAINT));
            ui.add_space(4.0);
            egui::Grid::new(("rp-lim-grid", j.id)).num_columns(2).spacing([8.0, 8.0]).show(ui, |ui| {
                // CPU はコア数、メモリは GB で動かす
                Self::lim_row(ui, "CPU", &mut lim.threads, g.threads, &Self::cpu_note(g.threads, tpc), &mut |ui, x| Self::cpu_slider(ui, x, ncpu, tpc));
                Self::lim_row(ui, "RAM の上限", &mut lim.ram_gb, g.ram_gb, &Self::ram_note(g.ram_gb), &mut |ui, x| Self::ram_slider(ui, x, ram_total));
            });
        });
    }
}
