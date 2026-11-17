//! ビルド先・枝・木を読む(書き込みはしない。起動と建てるは main.rs から子のプロセスで)。
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::places::places;
pub const MAIN_BRANCH: &str = "falcon/v4";
pub const TEST_PREFIXES: [&str; 2] = ["falcon/v4-test/", "falcon/v1.0-test/"];
/// 置き場は places.rs(~/.config/falcon-launcher/場所.tsv)。開発の場所(本線・試しの枝)
fn p_dir() -> &'static str {
    &places().project
}
/// Falcon Engine のソース
pub fn src_dir() -> &'static str {
    &places().source
}
fn apps_dir() -> String {
    format!("{}/.local/share/applications", std::env::var("HOME").unwrap_or_default())
}
/// 古い版(rofi では隠した .desktop・falcon-menu の OLD と同じ並び)
pub const OLD: [&str; 6] = ["falcon-demo-v0.7", "falcon-engine-v2fg", "falcon-engine-public-check", "falcon-engine-demo", "falcon-engine-53", "falcon-engine-rigperf"];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Main,
    Test,
    Old,
    /// 本体の外で動く物(クラスター)
    Ext,
}

/// 本体の機能の一覧(falcon_features.py の FEATURES)
#[derive(Clone, Debug)]
pub struct Feature {
    pub id: String,
    pub label: String,
    pub version: String,
    pub kind: String,
    pub units: Vec<String>,
    pub description: String,
}

/// 外の物の場面(data/<名前>.vgdag)
#[derive(Clone, Debug)]
pub struct Scene {
    pub name: String,
    pub bytes: u64,
    pub ts: i64,
    pub material: bool,
    pub look: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Progress {
    pub done: u32,
    pub total: u32,
    pub step: String,
}

#[derive(Clone, Debug)]
pub struct Build {
    pub key: String,
    pub kind: Kind,
    pub title: String,
    pub what: String,
    /// git のリポジトリ
    pub repo: PathBuf,
    pub branch: String,
    /// 建てた時の hash(建てた.txt)
    pub hash: String,
    pub dir: PathBuf,
    pub built_ts: i64,
    pub ok: Option<bool>,
    pub fails: u32,
    pub building: Option<Progress>,
    pub has_bin: bool,
    /// 古い版の起こし方(.desktop の Exec)
    pub exec: String,
    pub category: &'static str,
}

impl Build {
    pub fn launchable(&self) -> bool {
        self.building.is_none() && (self.kind == Kind::Old || self.has_bin)
    }
}

/// 単位 → 分け方
pub fn category(unit: &str) -> &'static str {
    match unit {
        "host" | "integrated" => "統合",
        "neural-render" | "neural-engine" | "neural-panel" | "glass-blend" | "camera-shake" | "clover" | "dragon-material" | "dlss" | "upstream-cycles" => "エンジン",
        "mnee" | "mnee-meshlight" | "mnee-reflect" | "sms" | "glass-guides" | "pathguide-gpu" => "集光",
        "anim-playback" | "texture-compress" | "viewer-host" | "compositor-viewer" => "最適化",
        "realtime-fg" => "フレーム生成",
        _ => "その他",
    }
}

pub const CATEGORIES: [&str; 7] = ["統合", "クラスター", "エンジン", "集光", "最適化", "フレーム生成", "その他"];

fn git(args: &[&str]) -> String {
    Command::new("git").arg("-C").arg(src_dir()).args(args).output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default()
}

fn git_in(dir: &Path, args: &[&str]) -> Option<String> {
    let o = Command::new("git").arg("-C").arg(dir).args(args).output().ok()?;
    o.status.success().then(|| String::from_utf8_lossy(&o.stdout).into_owned())
}

/// 単位 → 枝(同じ単位が両方にあれば falcon/v4-test/ の方)
pub fn test_refs() -> HashMap<String, String> {
    let mut args = vec!["for-each-ref", "--format=%(refname:short)"];
    let pats: Vec<String> = TEST_PREFIXES.iter().map(|p| format!("refs/heads/{p}")).collect();
    args.extend(pats.iter().map(|s| s.as_str()));
    let out = git(&args);
    let mut refs = HashMap::new();
    for prefix in TEST_PREFIXES.iter().rev() {
        for r in out.split_whitespace() {
            if let Some(u) = r.strip_prefix(prefix) {
                refs.insert(u.to_string(), r.to_string());
            }
        }
    }
    refs
}

fn read_kv(path: &Path) -> HashMap<String, String> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once('\t').map(|(k, v)| (k.to_string(), v.to_string())))
        .collect()
}

/// build.log の最後の ninja の行 [n/m] から進み具合
pub fn progress(dir: &Path) -> Progress {
    progress_file(&dir.join("build.log"))
}

/// 記録の終わりから ninja の [n/m] を拾う
pub fn progress_file(log: &Path) -> Progress {
    let mut p = Progress::default();
    let Ok(bytes) = read_tail(log, 16384) else { return p };
    let text = String::from_utf8_lossy(&bytes);
    for line in text.lines().rev() {
        let Some(rest) = line.strip_prefix('[') else { continue };
        let Some((nums, step)) = rest.split_once(']') else { continue };
        let Some((a, b)) = nums.split_once('/') else { continue };
        if let (Ok(a), Ok(b)) = (a.trim().parse(), b.trim().parse()) {
            p.done = a;
            p.total = b;
            let step = step.trim();
            // "Building CXX object source/.../foo.cc.o" → 最後の名前だけ
            p.step = match step.rsplit_once('/') {
                Some((head, name)) => format!("{} {}", head.split_whitespace().next().unwrap_or(""), name),
                None => step.to_string(),
            };
            break;
        }
    }
    p
}

pub fn read_tail(path: &Path, n: u64) -> std::io::Result<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = fs::File::open(path)?;
    let len = f.metadata()?.len();
    f.seek(SeekFrom::Start(len.saturating_sub(n)))?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;
    Ok(buf)
}

fn unit_what(unit: &str) -> String {
    let Ok(text) = fs::read_to_string(format!("{}/integration/{unit}/README.md", p_dir())) else { return String::new() };
    let line = text.lines().skip(1).map(str::trim).find(|l| !l.is_empty() && !l.starts_with('#')).unwrap_or("");
    let line: String = line.chars().filter(|c| *c != '`' && *c != '*').collect();
    line
}

fn parse_ts(s: &str) -> i64 {
    // "2026-11-09 17:10"(建てた.txt の終わり)→ 秒(JST として)
    let s = s.trim();
    let (d, t) = s.split_once(' ').unwrap_or((s, "00:00"));
    let mut it = d.split('-').filter_map(|x| x.parse::<i64>().ok());
    let (Some(y), Some(m), Some(dd)) = (it.next(), it.next(), it.next()) else { return 0 };
    let mut tt = t.split(':').filter_map(|x| x.parse::<i64>().ok());
    let (h, mi) = (tt.next().unwrap_or(0), tt.next().unwrap_or(0));
    days_from_civil(y, m, dd) * 86400 + h * 3600 + mi * 60 - crate::tz_offset()
}

pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn build_dir(unit: &str) -> PathBuf {
    if unit == "host" { PathBuf::from(format!("{}/build/host", p_dir())) } else { PathBuf::from(format!("{}/build/test-{unit}", p_dir())) }
}

fn fill_state(b: &mut Build) {
    let dir = b.dir.clone();
    b.has_bin = dir.join("bin/blender").is_file();
    b.building = dir.join(".building").exists().then(|| progress(&dir));
    let kv = read_kv(&dir.join("建てた.txt"));
    if let Some(h) = kv.get("hash") {
        b.hash = h.clone();
    }
    if let Some(e) = kv.get("終わり") {
        b.built_ts = parse_ts(e);
    } else if b.has_bin {
        b.built_ts = fs::metadata(dir.join("bin/blender")).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0);
    }
    b.ok = kv.get("rc").map(|r| r == "0");
    b.fails = kv.get("失敗").and_then(|f| f.parse().ok()).unwrap_or(0);
}

fn desktop(name: &str) -> Option<(String, String, String)> {
    let text = fs::read_to_string(format!("{}/{name}.desktop", apps_dir())).ok()?;
    let get = |k: &str| text.lines().find_map(|l| l.strip_prefix(&format!("{k}="))).unwrap_or("").to_string();
    let exec = get("Exec").split_whitespace().filter(|w| !w.starts_with('%')).collect::<Vec<_>>().join(" ");
    Some((get("Name"), exec, get("Comment")))
}

/// 外で動くクラスターの物
fn cluster() -> Build {
    let repo = PathBuf::from(&places().vg);
    let dir = repo.join("build");
    let bin = dir.join("vgview_vk");
    let has_bin = bin.is_file();
    Build {
        key: "virtual-geometry".into(),
        kind: Kind::Ext,
        title: "クラスター(仮想ジオメトリ)".into(),
        what: "Blender の外で動く表示器。クラスタの木とサーフェイスキャッシュの GI".into(),
        branch: git_in(&repo, &["branch", "--show-current"]).unwrap_or_default().trim().to_string(),
        hash: git_in(&repo, &["rev-parse", "--short=11", "HEAD"]).unwrap_or_default().trim().to_string(),
        repo,
        built_ts: if has_bin { mtime(&bin) } else { 0 },
        dir,
        ok: None,
        fails: 0,
        building: None,
        has_bin,
        exec: String::new(),
        category: "クラスター",
    }
}

/// 表示器の場面。見た目まで揃った物を先に、軽い順。先頭が「起動する」で開く場面
pub fn scenes() -> Vec<Scene> {
    let data = Path::new(&places().vg).join("data");
    let mut v = vec![];
    for e in fs::read_dir(&data).into_iter().flatten().flatten() {
        let p = e.path();
        if p.extension().is_none_or(|x| x != "vgdag") {
            continue;
        }
        let name = p.file_stem().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default();
        v.push(Scene {
            bytes: fs::metadata(&p).map(|m| m.len()).unwrap_or(0),
            ts: mtime(&p),
            material: data.join(format!("{name}.vgin")).is_file(),
            look: data.join(format!("{name}.look.txt")).is_file(),
            name,
        });
    }
    v.sort_by_key(|s| (!(s.look && s.material), !s.material, s.bytes));
    v
}

/// 窓で開いて歩き回るコマンド。材質がある場面は GI も入れる
pub fn scene_command(s: &Scene) -> String {
    let vg = &places().vg;
    let d = format!("{vg}/data");
    let n = &s.name;
    let mut c = format!("exec '{vg}/build/vgview_vk' --windowed --fly --dag '{d}/{n}.vgdag' --grid 1 --nocull --occlusion");
    if s.material {
        c.push_str(&format!(" --materials '{d}/{n}.vgin' --color material --gi"));
    } else {
        c.push_str(" --color shade");
    }
    if s.look {
        c.push_str(&format!(" --look '{d}/{n}.look.txt'"));
    }
    if let Ok(a) = fs::read_to_string(format!("{d}/{n}.vgin.cam.args")) {
        c.push(' ');
        c.push_str(a.trim());
    }
    c
}

/// 本体の機能の一覧を読む(建ての中の物を先に、無ければ枝から)。読むだけ
pub fn features() -> Vec<Feature> {
    let src = fs::read_to_string(format!("{}/build/host/bin/5.3/scripts/addons_core/cycles/falcon_features.py", p_dir())).unwrap_or_else(|_| git(&["show", &format!("{MAIN_BRANCH}:intern/cycles/blender/addon/falcon_features.py")]));
    const READ: &str = r#"
import ast, sys
tree = ast.parse(sys.stdin.read())
rows = []
for node in tree.body:
    if isinstance(node, ast.Assign):
        names = [getattr(t, "id", "") for t in node.targets]
        if "FEATURES" in names:
            for call in node.value.elts:
                rows.append({k.arg: ast.literal_eval(k.value) for k in call.keywords})
for f in rows:
    print("	".join([f["id"], f["label"], f.get("version", ""), f.get("kind", ""), f.get("unit", ""), f.get("description", "")]))
"#;
    use std::io::Write;
    use std::process::Stdio;
    let Ok(mut child) = Command::new("python3").args(["-I", "-c", READ]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn() else { return vec![] };
    if let Some(mut i) = child.stdin.take() {
        let _ = i.write_all(src.as_bytes());
    }
    let Ok(o) = child.wait_with_output() else { return vec![] };
    String::from_utf8_lossy(&o.stdout)
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            (f.len() == 6).then(|| Feature {
                id: f[0].into(),
                label: f[1].into(),
                version: f[2].into(),
                kind: f[3].into(),
                units: f[4].split(',').map(|u| u.trim().to_string()).filter(|u| !u.is_empty()).collect(),
                description: f[5].into(),
            })
        })
        .collect()
}

/// 枝ごとの最後の更新
pub fn updated(builds: &[Build]) -> HashMap<String, i64> {
    let times: HashMap<String, i64> = git(&["for-each-ref", "--format=%(refname:short)%09%(committerdate:unix)", "refs/heads/falcon/"])
        .lines()
        .filter_map(|l| l.split_once('\t').and_then(|(a, b)| b.parse().ok().map(|t| (a.to_string(), t))))
        .collect();
    let mut out = HashMap::new();
    for b in builds {
        if b.kind == Kind::Ext {
            if let Some(t) = git_in(&b.repo, &["log", "-1", "--format=%ct"]).and_then(|s| s.trim().parse().ok()) {
                out.insert(b.key.clone(), t);
            }
        } else if let Some(t) = times.get(&b.branch) {
            out.insert(b.key.clone(), *t);
        }
    }
    out
}

/// 一覧を作る(軽い: 枝の名前と、ビルド先のファイルを読むだけ)
pub fn builds() -> Vec<Build> {
    let mut out = vec![];
    // 本線と試しの枝は、開発の場所(場所.tsv の project)を書いた時だけ
    if !p_dir().is_empty() {
        let mut main = Build {
            key: "host".into(),
            kind: Kind::Main,
            title: "Falcon Engine v4".into(),
            what: "本線。全機能有効で開く".into(),
            repo: PathBuf::from(src_dir()),
            branch: MAIN_BRANCH.into(),
            hash: String::new(),
            dir: build_dir("host"),
            built_ts: 0,
            ok: None,
            fails: 0,
            building: None,
            has_bin: false,
            exec: String::new(),
            category: "統合",
        };
        fill_state(&mut main);
        out.push(main);
    }
    if !places().vg.is_empty() {
        out.push(cluster());
    }
    if !p_dir().is_empty() {
        let mut units: Vec<(String, String)> = test_refs().into_iter().collect();
        units.sort();
        // 単位の README が無い時の説明は、枝の最後のコミットの件名
        let subjects: HashMap<String, String> = git(&["for-each-ref", "--format=%(refname:short)%09%(subject)", "refs/heads/falcon/"])
            .lines()
            .filter_map(|l| l.split_once('\t').map(|(a, b)| (a.to_string(), b.to_string())))
            .collect();
        for (u, branch) in units {
            let what = unit_what(&u);
            let what = if what.is_empty() { subjects.get(&branch).cloned().unwrap_or_default() } else { what };
            let mut b = Build {
                key: u.clone(),
                kind: Kind::Test,
                title: u.clone(),
                what,
                repo: PathBuf::from(src_dir()),
                branch,
                hash: String::new(),
                dir: build_dir(&u),
                built_ts: 0,
                ok: None,
                fails: 0,
                building: None,
                has_bin: false,
                exec: String::new(),
                category: category(&u),
            };
            fill_state(&mut b);
            out.push(b);
        }
    }
    for name in OLD {
        let Some((title, exec, comment)) = desktop(name) else { continue };
        let bin = exec.split_whitespace().find_map(|w| w.strip_prefix("FALCON_DEMO_BIN=")).map(PathBuf::from);
        let dir = if name == "falcon-demo-v0.7" { PathBuf::from(format!("{}/build/demo-v0.7", p_dir())) } else { bin.and_then(|b| b.parent().and_then(|p| p.parent()).map(Path::to_path_buf)).unwrap_or_default() };
        let mut b = Build {
            key: format!("old:{name}"),
            kind: Kind::Old,
            title,
            what: comment,
            repo: PathBuf::new(),
            branch: String::new(),
            hash: String::new(),
            dir,
            built_ts: 0,
            ok: None,
            fails: 0,
            building: None,
            has_bin: true,
            exec,
            category: "その他",
        };
        if b.dir.as_os_str().len() > 0 {
            fill_state(&mut b);
            b.has_bin = true;
        }
        out.push(b);
    }
    out.extend(blender_versions());
    out
}

/// Falcon Engine と、手元にある公式の Blender(版を選んで起動する)
fn blender_versions() -> Vec<Build> {
    let mut out = vec![];
    let mk = |key: String, title: String, what: String, dir: PathBuf, exec: String, category: &'static str, repo: &str, branch: &str| {
        let mut b = Build { key, kind: Kind::Old, title, what, repo: PathBuf::from(repo), branch: branch.into(), hash: String::new(), dir, built_ts: 0, ok: None, fails: 0, building: None, has_bin: true, exec, category };
        fill_state(&mut b);
        b.has_bin = true;
        b
    };
    let pl = places();
    let v5 = PathBuf::from(&pl.falcon_build);
    if v5.join("bin/blender").is_file() {
        let bin = v5.join("bin/blender").display().to_string();
        // DLSS のランタイムの置き場(場所.tsv の dlss_runtime か、建てる準備で置いた DLSS SDK の中)。空白を含む場所は env に渡せないので渡さない
        let exec = if !pl.dlss_runtime.is_empty() && !pl.dlss_runtime.contains(char::is_whitespace) { format!("env FALCON_DLSS_RUNTIME_PATH={} {bin}", pl.dlss_runtime) } else { bin };
        // 開発の場所がある PC は開発の名前の v5、公開の木はアドオンの版
        let title = if p_dir().is_empty() { falcon_version(&v5).map(|v| format!("Falcon Engine {v}")).unwrap_or_else(|| "Falcon Engine".into()) } else { "Falcon Engine v5".into() };
        let mut b = mk("v5".into(), title, "双方向・フォトンの集光・NR(DLSS RR)".into(), v5.clone(), exec, "Falcon Engine", src_dir(), &pl.falcon_branch);
        // 建てた.txt は build.sh でしか書かれないので、手で ninja を回すと古いまま残る(11-16: 本体は 17:11 に
        // 建て直したのに「11-15 09:52・5af2b2c66」と出ていた)。本体の方が新しければ、本体の時刻を使い、番号も推し量る
        let bin_ts = mtime(&v5.join("bin/blender"));
        if bin_ts > b.built_ts + 60 {
            b.built_ts = bin_ts;
            b.hash.clear();
        }
        if b.hash.is_empty() {
            // 建てた時刻より前の falcon/v5 の最後のコミット(推し量り)
            b.hash = git(&["log", "-1", "--format=%H", &format!("--before=@{}", b.built_ts), &pl.falcon_branch]).trim().to_string();
        }
        // GPU の核が建て先では新しいのに、起動する所(bin)へ入っていない時(ninja install をしていない)に知らせる
        let built_k = v5.join("intern/neural_render/kernel/device/cuda/kernel_sm_86.cubin.zst");
        let used_k = v5.join("bin/5.3/scripts/addons_core/neural_render/lib/kernel_sm_86.cubin.zst");
        if built_k.is_file() && used_k.is_file() && mtime(&built_k) > mtime(&used_k) + 60 {
            b.what = format!("{} · ⚠ 新しい GPU の核が入っていません(ninja install が要ります)", b.what);
        }
        out.push(b);
    }
    // 更新の時に残した前の版(<建てる場所>/前の版/<版>/blender。setup.rs が移す)
    if let Some(place) = v5.parent() {
        let mut prev: Vec<PathBuf> = fs::read_dir(place.join("前の版")).into_iter().flatten().flatten().map(|e| e.path()).filter(|d| d.join("blender").is_file()).collect();
        prev.sort();
        for d in prev.into_iter().rev() {
            let ver = d.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let bin = d.join("blender").display().to_string();
            let exec = if !pl.dlss_runtime.is_empty() && !pl.dlss_runtime.contains(char::is_whitespace) { format!("env FALCON_DLSS_RUNTIME_PATH={} {bin}", pl.dlss_runtime) } else { bin };
            let mut b = mk(format!("prev:{ver}"), format!("Falcon Engine {ver}(前の版)"), format!("更新の前に建てていた版({})", d.display()), d.clone(), exec, "Falcon Engine", "", "");
            b.built_ts = mtime(&d.join("blender"));
            out.push(b);
        }
    }
    for (bin, ver) in official_blenders() {
        let dir = bin.parent().map(Path::to_path_buf).unwrap_or_default();
        // key はフォルダ名にも使うので「/」を除く
        let key = format!("official:{}", bin.display().to_string().trim_start_matches('/').replace('/', "-"));
        let mut b = mk(key, format!("Blender {ver}"), format!("公式の Blender({})", bin.display()), dir, bin.display().to_string(), "公式", "", "");
        b.built_ts = fs::metadata(&bin).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|x| x.as_secs() as i64).unwrap_or(0);
        out.push(b);
    }
    out
}

/// 手元の公式 Blender を探す(見つかれば自動で登録)。
/// 展開した blender-<版>-linux-* のフォルダと、パッケージ・flatpak・Steam・snap の本体。Falcon の建てた物は除く
pub fn official_blenders() -> Vec<(PathBuf, String)> {
    let home = std::env::var("HOME").unwrap_or_default();
    let mut bins: Vec<PathBuf> = vec![];
    for root in places().blender_search.iter().cloned().chain([format!("{home}/Applications"), format!("{home}/Downloads"), format!("{home}/.local/opt"), "/opt".to_string()]) {
        for e in fs::read_dir(&root).into_iter().flatten().flatten() {
            let d = e.path();
            if d.file_name().is_some_and(|n| n.to_string_lossy().to_lowercase().starts_with("blender")) && d.join("blender").is_file() {
                bins.push(d.join("blender"));
            }
        }
    }
    for p in ["/usr/bin/blender".to_string(), "/usr/local/bin/blender".into(), "/snap/bin/blender".into(), format!("{home}/.local/share/Steam/steamapps/common/Blender/blender"), "/var/lib/flatpak/exports/bin/org.blender.Blender".into(), format!("{home}/.local/share/flatpak/exports/bin/org.blender.Blender")] {
        bins.push(PathBuf::from(p));
    }
    let mut seen = HashSet::new();
    let mut out = vec![];
    for b in bins {
        let Ok(real) = fs::canonicalize(&b) else { continue };
        let r = real.to_string_lossy().to_lowercase();
        if r.contains("falcon") || !seen.insert(real.clone()) {
            continue;
        }
        let ver = blender_version(&b);
        out.push((b, ver));
    }
    out.sort_by(|a, b| b.1.cmp(&a.1));
    out
}

/// Falcon Engine の版(建て先のアドオンの bl_info の "version": (0, 2, 0) → 0.2.0)
pub fn falcon_version(build: &Path) -> Option<String> {
    let addons = fs::read_dir(build.join("bin")).ok()?.flatten().map(|e| e.path().join("scripts/addons_core/neural_render/__init__.py")).find(|p| p.is_file())?;
    let text = fs::read_to_string(addons).ok()?;
    let line = text.lines().find(|l| l.trim_start().starts_with("\"version\""))?;
    let inner = line.split_once('(')?.1.split_once(')')?.0;
    Some(inner.split(',').map(str::trim).filter(|x| !x.is_empty()).collect::<Vec<_>>().join("."))
}

/// 「4.5.9」など。フォルダの名前から取れなければ --version を 1 度だけ聞いて覚えておく
fn blender_version(bin: &Path) -> String {
    let dir = bin.parent().and_then(|d| d.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if let Some(v) = dir.strip_prefix("blender-").map(|v| v.split("-linux").next().unwrap_or(v).to_string()) {
        return v;
    }
    let cache = PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config/falcon-launcher/公式の版.tsv");
    let mt = fs::metadata(bin).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
    let text = fs::read_to_string(&cache).unwrap_or_default();
    let tag = format!("{}\t{mt}\t", bin.display());
    if let Some(v) = text.lines().find_map(|l| l.strip_prefix(&tag)) {
        return v.to_string();
    }
    let v = Command::new(bin).arg("--version").stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null()).output().ok().and_then(|o| String::from_utf8_lossy(&o.stdout).lines().next().map(|l| l.trim_start_matches("Blender").trim().to_string())).unwrap_or_default();
    let _ = fs::create_dir_all(cache.parent().unwrap());
    let _ = fs::write(&cache, format!("{text}{tag}{v}\n"));
    v
}

/// フォルダ名に使える key(Windows 形式のディスク(NTFS・exFAT)では「:」「/」などが使えない)
pub fn safe_key(key: &str) -> String {
    key.chars().map(|c| if matches!(c, ':' | '/' | '\\' | '*' | '?' | '"' | '<' | '>' | '|') { '-' } else { c }).collect()
}

/// 起動に使う blender の場所(.blend を開く・予約で描く時)
pub fn blender_bin(b: &Build) -> Option<PathBuf> {
    [b.dir.join("bin/blender"), b.dir.join("blender")].into_iter().find(|p| p.is_file())
}

/// 起動の時に付ける環境(v5 の DLSS など)
pub fn blender_env(b: &Build) -> Vec<(String, String)> {
    b.exec.split_whitespace().filter_map(|w| w.split_once('=')).filter(|(k, _)| k.chars().all(|c| c.is_ascii_uppercase() || c == '_')).map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

/// /proc から、今動いている blender の場所
pub fn running_bins() -> HashSet<PathBuf> {
    let mut set = HashSet::new();
    let Ok(rd) = fs::read_dir("/proc") else { return set };
    for e in rd.flatten() {
        let name = e.file_name();
        if !name.to_string_lossy().bytes().all(|c| c.is_ascii_digit()) {
            continue;
        }
        if let Ok(exe) = fs::read_link(e.path().join("exe")) {
            if exe.file_name().is_some_and(|n| n == "blender" || n == "vgview_vk") {
                set.insert(exe);
            }
        }
    }
    set
}

// ---------- 重い物(背景の糸で読む) ----------

/// 枝の先端と、建てた物より何本先か
pub fn ahead(branch: &str, hash: &str) -> Option<u32> {
    if branch.is_empty() || hash.is_empty() {
        return None;
    }
    git(&["rev-list", "--count", &format!("{hash}..{branch}")]).trim().parse().ok()
}

#[derive(Clone, Debug)]
pub struct Tree {
    pub path: String,
    pub branch: String,
    pub ts: i64,
    pub dirty: usize,
    pub in_main: bool,
}

/// 木を 1 本ずつ調べて送る(未コミットの数は追跡中のファイルだけ)
pub fn trees(tx: std::sync::mpsc::Sender<Tree>, ctx: eframe::egui::Context) {
    let list = git(&["worktree", "list", "--porcelain"]);
    let mut cur: (String, String, String) = Default::default();
    let mut items = vec![];
    for line in list.lines().chain(std::iter::once("")) {
        if let Some(p) = line.strip_prefix("worktree ") {
            cur = (p.to_string(), String::new(), String::new());
        } else if let Some(h) = line.strip_prefix("HEAD ") {
            cur.2 = h[..h.len().min(11)].to_string();
        } else if let Some(b) = line.strip_prefix("branch ") {
            cur.1 = b.trim_start_matches("refs/heads/").to_string();
        } else if line.is_empty() && !cur.0.is_empty() {
            items.push(std::mem::take(&mut cur));
        }
    }
    for (path, branch, head) in items {
        let dir = Path::new(&path);
        let dirty = git_in(dir, &["status", "--porcelain", "-uno"]).map(|s| s.lines().count()).unwrap_or(0);
        let ts = git_in(dir, &["log", "-1", "--format=%ct"]).and_then(|s| s.trim().parse().ok()).unwrap_or(0);
        let in_main = Command::new("git").arg("-C").arg(src_dir()).args(["merge-base", "--is-ancestor", &head, MAIN_BRANCH]).status().map(|s| s.success()).unwrap_or(false);
        let branch = if branch.is_empty() { "(枝なし)".to_string() } else { branch };
        if tx.send(Tree { path, branch, ts, dirty, in_main }).is_err() {
            return;
        }
        ctx.request_repaint();
    }
}

/// ビルド先の大きさ(du -sb)。建て直すまで変わらないので、建てた時刻ごとに覚えておく
pub fn dir_size(dir: &Path) -> Option<u64> {
    let o = Command::new("du").arg("-sb").arg(dir).output().ok()?;
    String::from_utf8_lossy(&o.stdout).split_whitespace().next()?.parse().ok()
}

/// ディスクの空きと全体
pub fn disk(path: &str) -> Option<(u64, u64)> {
    let o = Command::new("df").args(["-B1", "--output=avail,size", path]).output().ok()?;
    let text = String::from_utf8_lossy(&o.stdout);
    let mut it = text.lines().nth(1)?.split_whitespace().filter_map(|x| x.parse().ok());
    Some((it.next()?, it.next()?))
}

// ---------- 更新履歴(折りたたみの一覧) ----------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChangeKind {
    /// 統合のコミット(枝を入れた)
    Merge,
    /// 普通のコミット
    Commit,
    /// 建てた記録
    Built,
    /// 版の台帳の行
    Version,
}

#[derive(Clone, Debug)]
pub struct Change {
    pub id: String,
    pub ts: i64,
    pub kind: ChangeKind,
    pub title: String,
    pub who: String,
    /// hash か版の種類
    pub tag: String,
    /// コミットの全 hash(中身は開いた時に読む)
    pub full: String,
    /// 版と建てた記録の中身
    pub body: Vec<String>,
    pub ok: bool,
}

#[derive(Clone, Debug, Default)]
pub struct ChangeDetail {
    pub body: Vec<String>,
    pub stat: String,
    pub merged: Vec<(String, String)>,
}

/// そのビルドの更新履歴: 枝の本筋のコミット + 建てた記録 + 版の台帳
pub fn changes(b: &Build) -> Vec<Change> {
    let mut out = vec![];
    if !b.branch.is_empty() {
        let text = git_in(&b.repo, &["log", "-200", "--first-parent", "--format=%h%x1f%H%x1f%ct%x1f%an%x1f%P%x1f%s", &b.branch]).unwrap_or_default();
        for l in text.lines() {
            let f: Vec<&str> = l.split('\x1f').collect();
            if f.len() != 6 {
                continue;
            }
            let merge = f[4].split_whitespace().count() > 1 || f[5].starts_with("テスト統合");
            out.push(Change {
                id: f[1].into(),
                ts: f[2].parse().unwrap_or(0),
                kind: if merge { ChangeKind::Merge } else { ChangeKind::Commit },
                title: f[5].into(),
                who: f[3].into(),
                tag: f[0].into(),
                full: f[1].into(),
                body: vec![],
                ok: true,
            });
        }
    }
    if b.built_ts > 0 && b.kind != Kind::Old {
        let ok = b.ok.unwrap_or(true);
        let mut body = vec![format!("枝: {}", b.branch), format!("ビルド先: {}", b.dir.display())];
        if !ok {
            body.push(format!("失敗: {} 件", b.fails));
        }
        out.push(Change { id: format!("built:{}:{}", b.key, b.built_ts), ts: b.built_ts, kind: ChangeKind::Built, title: if ok { format!("{} を建てた", b.title) } else { format!("{} を建てて失敗", b.title) }, who: "ビルド".into(), tag: b.hash.clone(), full: String::new(), body, ok });
    }
    let names: Vec<String> = if b.kind == Kind::Ext {
        vec![]
    } else if b.kind == Kind::Main { vec!["falcon".into(), "falcon_r".into()] } else { vec![b.key.split('-').next().unwrap_or("").to_lowercase()] };
    for l in fs::read_to_string(&places().versions).unwrap_or_default().lines() {
        let f: Vec<&str> = l.split('\t').collect();
        if f.len() < 6 || l.starts_with('#') || !names.contains(&f[0].to_lowercase()) {
            continue;
        }
        let mut body = vec![];
        if !f[5].is_empty() {
            body.push(f[5].to_string());
        }
        if !f[4].is_empty() {
            body.push(format!("場所: {}", f[4]));
        }
        out.push(Change { id: format!("ver:{l}"), ts: parse_ts(f[3]), kind: ChangeKind::Version, title: format!("{} {}", f[0], f[1]), who: "版の台帳".into(), tag: f[2].into(), full: String::new(), body, ok: true });
    }
    out.sort_by(|a, b| b.ts.cmp(&a.ts));
    out
}

pub fn change_detail(repo: &Path, full: &str) -> ChangeDetail {
    let run = |args: &[&str]| git_in(repo, args).unwrap_or_default();
    let text = run(&["show", "-s", "--format=%B", full]);
    let body = text.lines().skip(1).map(str::trim).filter(|l| !l.is_empty() && !l.starts_with("Co-Authored-By")).map(str::to_string).collect();
    let stat = run(&["show", "--first-parent", "--shortstat", "--format=", full]).trim().to_string();
    let parents = run(&["show", "-s", "--format=%P", full]);
    let p: Vec<&str> = parents.split_whitespace().collect();
    let merged = if p.len() > 1 {
        run(&["log", "-60", "--format=%h%x1f%s", &format!("{}..{}", p[0], p[1])])
            .lines()
            .filter_map(|l| l.split_once('\x1f').map(|(a, b)| (a.to_string(), b.to_string())))
            .collect()
    } else {
        vec![]
    };
    ChangeDetail { body, stat, merged }
}

// ---------- ギャラリー ----------

#[derive(Clone, Debug)]
pub struct Media {
    pub path: PathBuf,
    pub video: bool,
    pub caption: String,
    /// 連番の絵(動画にする元)。path はその動画の置き場所で、まだ無いこともある
    pub frames: Vec<PathBuf>,
}

/// 連番を動画にした物の置き場所。元の絵は消さない
pub fn seq_dir() -> &'static str {
    &places().seq
}
/// レンダリング予約の出力(<日付>/<時刻> <.blend の名前> [<ビルドの key>]/)
pub fn render_dir() -> &'static str {
    &places().render
}
/// この枚数から連番として動画にする
const SEQ_MIN: usize = 10;

/// 名前の末尾の数字を外した頭(frame_0012.png → "frame_")
fn seq_key(p: &Path) -> Option<(String, String)> {
    let stem = p.file_stem()?.to_string_lossy().into_owned();
    let head = stem.trim_end_matches(|c: char| c.is_ascii_digit());
    (head.len() < stem.len()).then(|| (head.to_string(), p.extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default()))
}

fn seq_media(frames: Vec<PathBuf>, ts: &mut i64) -> Media {
    let last = frames.iter().map(|f| mtime(f)).max().unwrap_or(0);
    *ts = (*ts).max(last);
    let key = format!("{}|{}|{}", frames[0].display(), frames.len(), last);
    let h = key.bytes().fold(14695981039346656037u64, |a, b| (a ^ b as u64).wrapping_mul(1099511628211));
    let caption = format!("連番 {} 枚を動画にした物", frames.len());
    Media { path: PathBuf::from(format!("{}/{h:016x}.mp4", seq_dir())), video: true, caption, frames }
}

/// 名前に「120fps」などがあればその速さ、無ければ 24
fn seq_fps(frames: &[PathBuf]) -> u32 {
    let text = frames[0].to_string_lossy().to_lowercase();
    text.match_indices("fps")
        .filter_map(|(i, _)| {
            let d: String = text[..i].chars().rev().take_while(|c| c.is_ascii_digit()).collect::<Vec<_>>().into_iter().rev().collect();
            d.parse().ok()
        })
        .find(|f| (1..=240).contains(f))
        .unwrap_or(24)
}

/// 連番を mp4 にする(NVENC。CPU の x264 には落とさない)。できたら true
pub fn encode_seq(m: &Media) -> bool {
    if m.path.is_file() {
        return true;
    }
    let Some(first) = m.frames.first() else { return false };
    let Ok((w0, h0)) = image::image_dimensions(first) else { return false };
    let k = (1920.0 / w0 as f32).min(1.0);
    let (w, h) = (((w0 as f32 * k) as u32 / 2 * 2).max(2), ((h0 as f32 * k) as u32 / 2 * 2).max(2));
    if fs::create_dir_all(seq_dir()).is_err() {
        return false;
    }
    let fps = seq_fps(&m.frames);
    let list = m.path.with_extension(format!("{}.list", std::process::id()));
    let tmp = m.path.with_extension(format!("{}.tmp.mp4", std::process::id()));
    let mut text = String::new();
    for f in m.frames.iter().chain(m.frames.last()) {
        text.push_str(&format!("file '{}'\nduration {:.6}\n", f.display().to_string().replace('\'', "'\\''"), 1.0 / fps as f64));
    }
    if fs::write(&list, text).is_err() {
        return false;
    }
    let vf = format!("scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2,format=yuv420p");
    let ok = Command::new("nice")
        .args(["-n", "15", "ffmpeg", "-nostdin", "-y", "-loglevel", "error", "-f", "concat", "-safe", "0", "-i"])
        .arg(&list)
        .args(["-vf", &vf, "-r", &fps.to_string(), "-c:v", "h264_nvenc", "-preset", "p5", "-cq", "23", "-movflags", "+faststart"])
        .arg(&tmp)
        .stdin(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    let _ = fs::remove_file(&list);
    if ok && fs::rename(&tmp, &m.path).is_ok() {
        return true;
    }
    let _ = fs::remove_file(&tmp);
    false
}

#[derive(Clone, Debug)]
pub struct Album {
    pub title: String,
    pub dir: PathBuf,
    pub ts: i64,
    pub note: String,
    pub items: Vec<Media>,
    /// 手で置いた説明つきの絵(フォルダの名前)
    pub curated: Option<String>,
    pub neural: bool,
}

const IMAGE_EXT: [&str; 3] = ["png", "jpg", "jpeg"];
const VIDEO_EXT: [&str; 4] = ["mp4", "mkv", "webm", "mov"];

fn mtime(p: &Path) -> i64 {
    fs::metadata(p).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// 同じフォルダに絵が多い(連番)時は 1 枚だけにする
fn collect_media(dir: &Path, depth: u32, out: &mut Vec<Media>, ts: &mut i64) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    let (mut images, mut subs) = (vec![], vec![]);
    for e in rd.flatten() {
        let p = e.path();
        let Ok(t) = e.file_type() else { continue };
        if t.is_symlink() {
            continue;
        }
        if t.is_dir() {
            // 場面から抜き出した材質の絵(<名前>.images)は結果ではないので出さない
            if !p.to_string_lossy().ends_with(".images") {
                subs.push(p);
            }
            continue;
        }
        let ext = p.extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
        if VIDEO_EXT.contains(&ext.as_str()) {
            *ts = (*ts).max(mtime(&p));
            let caption = fs::read_to_string(p.with_extension("txt")).unwrap_or_default().trim().to_string();
            out.push(Media { path: p, video: true, caption, frames: vec![] });
        } else if IMAGE_EXT.contains(&ext.as_str()) {
            images.push(p);
        }
    }
    images.sort();
    // 連番(同じ頭で SEQ_MIN 枚以上)は 1 本の動画にまとめる
    let mut groups: HashMap<(String, String), Vec<PathBuf>> = HashMap::new();
    for p in &images {
        if let Some(k) = seq_key(p) {
            groups.entry(k).or_default().push(p.clone());
        }
    }
    let mut seqs: Vec<Vec<PathBuf>> = groups.into_values().filter(|v| v.len() >= SEQ_MIN).collect();
    seqs.sort();
    let in_seq: HashSet<PathBuf> = seqs.iter().flatten().cloned().collect();
    for frames in seqs {
        out.push(seq_media(frames, ts));
    }
    let rest: Vec<PathBuf> = images.into_iter().filter(|p| !in_seq.contains(p)).collect();
    let many = rest.len() > 30;
    for p in rest.into_iter().take(if many { 1 } else { 30 }) {
        *ts = (*ts).max(mtime(&p));
        let caption = fs::read_to_string(p.with_extension("txt")).unwrap_or_default().trim().to_string();
        out.push(Media { path: p, video: false, caption, frames: vec![] });
    }
    if depth > 0 {
        subs.sort();
        for s in subs {
            collect_media(&s, depth - 1, out, ts);
        }
    }
}

fn album_note(dir: &Path) -> String {
    for n in ["説明.md", "README.md", "結果メモ.md"] {
        let Ok(text) = fs::read_to_string(dir.join(n)) else { continue };
        let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with("---") && !l.contains(": ")).take(4).collect();
        if !lines.is_empty() {
            return lines.join("\n");
        }
    }
    String::new()
}

fn album(dir: &Path, title: String, curated: Option<String>, neural: bool) -> Option<Album> {
    let (mut items, mut ts) = (vec![], 0);
    collect_media(dir, 4, &mut items, &mut ts);
    if items.is_empty() {
        return None;
    }
    items.sort_by_key(|m| !m.video);
    items.truncate(60);
    choose_cover(&mut items);
    Some(Album { title, dir: dir.to_path_buf(), ts, note: album_note(dir), items, curated, neural })
}

/// 表紙を選ぶ: 動画が無ければ、比べの横長や図ではなく 16:9 に近い絵を先頭へ
fn choose_cover(items: &mut [Media]) {
    if items.first().is_none_or(|m| m.video) {
        return;
    }
    let good = items.iter().take(40).position(|m| image::image_dimensions(&m.path).is_ok_and(|(w, h)| w >= 640 && (1.3..=2.2).contains(&(w as f32 / h.max(1) as f32))));
    if let Some(i) = good {
        items.swap(0, i);
    }
}

/// ギャラリーの除外の一覧(フォルダのパスの一部)
fn excluded() -> Vec<String> {
    fs::read_to_string(crate::places::config_dir().join("ギャラリーの除外.txt"))
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect()
}

fn sub_dirs(dir: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir).map(|r| r.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).map(|e| e.path()).collect()).unwrap_or_default();
    v.sort();
    v
}

fn name_of(p: &Path) -> String {
    p.file_name().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default()
}

/// 今まで取ってきた絵と動画を、フォルダごとの組にする(読むだけ)
pub fn gallery() -> Vec<Album> {
    let mut out = vec![];
    let pl = places();
    for d in sub_dirs(&pl.gallery) {
        let n = name_of(&d);
        out.extend(album(&d, n.clone(), Some(n), false));
    }
    for d in if pl.project.is_empty() { vec![] } else { sub_dirs(&format!("{}/tests/neural/結果", pl.project)) } {
        // 20261031-2210-junkshop… → 頭の日付を落とす
        let n = name_of(&d);
        let t = n.trim_start_matches(|c: char| c.is_ascii_digit() || c == '-').to_string();
        let ts = name_time(&n);
        out.extend(album(&d, if t.is_empty() { n } else { t }, None, true).map(|a| dated(a, ts)));
    }
    for day in pl.gallery_days.iter().flat_map(|r| sub_dirs(r)) {
        let ts = name_time(&name_of(&day));
        for d in sub_dirs(&day.to_string_lossy()) {
            let n = name_of(&d);
            let t = n.split_once(' ').map(|x| x.1.to_string()).unwrap_or(n);
            out.extend(album(&d, t, None, false).map(|a| dated(a, ts)));
        }
    }
    for day in sub_dirs(render_dir()) {
        for d in sub_dirs(&day.to_string_lossy()) {
            out.extend(album(&d, format!("予約 {}", name_of(&d)), None, false));
        }
    }
    for (label, root) in &pl.gallery_groups {
        for d in sub_dirs(root) {
            out.extend(album(&d, format!("{label} {}", name_of(&d)), None, false));
        }
    }
    for r in &pl.gallery_dirs {
        let p = PathBuf::from(r);
        out.extend(album(&p, name_of(&p), None, false));
    }
    let skip = excluded();
    out.retain(|a: &Album| {
        let d = a.dir.to_string_lossy();
        !skip.iter().any(|s| d.contains(s.as_str()))
    });
    out.sort_by(|a, b| b.curated.is_some().cmp(&a.curated.is_some()).then(b.ts.cmp(&a.ts)));
    out
}

/// 組の日付を、フォルダの名前の日付にする(2026-11-09 の整理で EXR から作った動画は、ファイルの時刻が変換の時刻になった)
fn dated(mut a: Album, ts: Option<i64>) -> Album {
    if let Some(t) = ts {
        a.ts = t;
    }
    a
}

/// 名前の頭の日付: 2026-11-05 か 20261026-0055
fn name_time(name: &str) -> Option<i64> {
    let digits: String = name.chars().take(13).filter(|c| c.is_ascii_digit()).collect();
    let d = |r: std::ops::Range<usize>| digits.get(r).and_then(|x| x.parse::<i64>().ok());
    let (y, m, day) = (d(0..4)?, d(4..6)?, d(6..8)?);
    if !(2000..2100).contains(&y) || !(1..=12).contains(&m) || !(1..=31).contains(&day) {
        return None;
    }
    let (hh, mm) = (d(8..10).unwrap_or(0), d(10..12).unwrap_or(0));
    // 暦の日付から 1970-01-01 からの日数へ
    let yy = if m <= 2 { y - 1 } else { y };
    let era = yy.div_euclid(400);
    let yoe = yy - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(days * 86400 + hh * 3600 + mm * 60 - crate::tz_offset())
}

/// 機能のページに出す組か(host は全部)
pub fn album_matches(a: &Album, b: &Build) -> bool {
    let t = a.title.to_lowercase();
    let cluster = ["仮想ジオメトリ", "クラスタ", "ハイポリ"].iter().any(|x| t.contains(x));
    if a.dir.starts_with(render_dir()) {
        return a.title.ends_with(&format!("[{}]", safe_key(&b.key)));
    }
    if b.kind == Kind::Main {
        return !cluster;
    }
    if b.kind == Kind::Ext {
        return cluster;
    }
    if let Some(c) = &a.curated {
        return c == &b.key || c == "全体";
    }
    let any = |w: &[&str]| w.iter().any(|x| t.contains(x));
    match b.key.as_str() {
        "neural-render" | "neural-engine" | "neural-panel" => a.neural || any(&["ニューラル", "neural", "nrc"]),
        "glass-blend" | "glass-guides" => any(&["ガラス", "glass", "グラス", "瓶"]),
        "camera-shake" => any(&["揺れ", "shake", "flicker"]),
        "dragon-material" => any(&["dragon", "材質"]),
        "dlss" => any(&["dlss", "rr"]),
        "upstream-cycles" => any(&["上流", "cycles"]),
        "mnee" | "mnee-meshlight" | "mnee-reflect" | "sms" => any(&["mnee", "sms", "集光"]),
        "pathguide-gpu" => any(&["ガイド", "guide"]),
        "anim-playback" => any(&["アニメ", "anim"]),
        "compositor-viewer" => any(&["コンポジ", "compositor"]),
        "realtime-fg" => any(&["オフラインfg", "フレーム", "fg"]),
        "texture-compress" => any(&["テクスチャ", "texture"]),
        k => t.contains(k.split('-').next().unwrap_or(k)),
    }
}

/// 小さな絵を作る(~/.cache/falcon-launcher/thumbs)。作れなければ None。
pub fn make_thumb(path: &Path, video: bool) -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var("HOME").ok()?).join(".cache/falcon-launcher/thumbs");
    fs::create_dir_all(&dir).ok()?;
    let key = format!("2|{}|{}", path.display(), mtime(path));
    let h = key.bytes().fold(14695981039346656037u64, |a, b| (a ^ b as u64).wrapping_mul(1099511628211));
    let out = dir.join(format!("{h:016x}.jpg"));
    if out.is_file() {
        return Some(out);
    }
    if video {
        let ok = Command::new("nice").args(["-n", "15", "ffmpeg", "-y", "-loglevel", "error", "-ss", "0.3", "-i"]).arg(path).args(["-frames:v", "1", "-vf", "scale=512:-2", "-q:v", "4"]).arg(&out).status().map(|s| s.success()).unwrap_or(false);
        return (ok && out.is_file()).then_some(out);
    }
    // 短い辺を残す(横に長い比べの絵は、左の 1 枚を切り出して表紙にする)
    let src = image::open(path).ok()?;
    let wide = src.width() as f32 / src.height().max(1) as f32 > 16.0 / 9.0;
    let img = if wide { src.thumbnail(2048, 288) } else { src.thumbnail(512, 1024) }.to_rgb8();
    img.save(&out).ok()?;
    Some(out)
}
