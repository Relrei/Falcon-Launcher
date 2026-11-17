//! 建てる準備: Falcon Engine を各自の PC で建てるのに要る物を確かめ、取り方を案内する。
//! SDK や Toolkit を入れるのをランチャーで手伝う(Linux は半分ほどがパッケージで済む。Windows は後で)。
//! 今は Linux だけ。NVIDIA の物(CUDA・OptiX・DLSS)を自動で取ってくる形は、許諾を確かめてから足す
//! (それまでは取りに行く頁を開き、置いた場所を選んでもらう)。パッケージは sudo が要るので、ランチャーは打たずに命令を写すだけ。

use super::*;

/// 置いた場所の控え(キー<TAB>パス)
fn paths_file() -> PathBuf {
    config_dir().join("準備.tsv")
}

pub fn load_paths() -> HashMap<String, String> {
    std::fs::read_to_string(paths_file())
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once('\t').map(|(k, v)| (k.to_string(), v.to_string())))
        .collect()
}

pub fn save_paths(p: &HashMap<String, String>) {
    let _ = std::fs::create_dir_all(config_dir());
    let mut keys: Vec<_> = p.keys().collect();
    keys.sort();
    let text: String = keys.into_iter().filter(|k| !p[*k].trim().is_empty()).map(|k| format!("{k}\t{}\n", p[k].trim())).collect();
    let _ = std::fs::write(paths_file(), text);
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    Ok,
    /// あるが条件に足りない、または確かめきれない
    Warn,
    Missing,
}

/// 取り方
#[derive(Clone, Debug)]
pub enum How {
    /// 仕組みのパッケージ(命令を写す。sudo は使う人が打つ)
    Package { arch: &'static str, debian: &'static str, fedora: &'static str },
    /// 頁を開いて使う人が取り、置いた場所を選ぶ
    Manual { url: &'static str },
    /// 自分で選ぶ場所
    Place,
    /// 案内だけ
    Note,
}

#[derive(Clone, Debug)]
pub struct Check {
    pub key: &'static str,
    pub group: &'static str,
    pub name: &'static str,
    /// 要る条件
    pub need: &'static str,
    pub state: State,
    /// 見つかった物(版・場所)
    pub found: String,
    pub how: How,
    /// 置いた場所を選べる(準備.tsv のキー)
    pub has_path: bool,
    /// 無くても建てられる
    pub optional: bool,
}

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let o = Command::new(cmd).args(args).stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    if !o.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
}

/// "12.8" や "580.95.05" の頭の 2 つを数にする
fn ver2(s: &str) -> (u32, u32) {
    let mut it = s.split(|c: char| !c.is_ascii_digit()).filter(|x| !x.is_empty()).filter_map(|x| x.parse().ok());
    (it.next().unwrap_or(0), it.next().unwrap_or(0))
}

/// 「仕組み」の名前(os-release の ID と ID_LIKE)
pub fn distro() -> (String, String) {
    let text = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
    let get = |k: &str| text.lines().find_map(|l| l.strip_prefix(&format!("{k}="))).map(|v| v.trim_matches('"').to_string()).unwrap_or_default();
    let pretty = get("PRETTY_NAME");
    let family = format!("{} {}", get("ID"), get("ID_LIKE"));
    (pretty, family)
}

/// その仕組みで入れる命令
pub fn package_command(how: &How) -> Option<String> {
    let How::Package { arch, debian, fedora } = how else { return None };
    let (_, fam) = distro();
    Some(if fam.contains("arch") {
        format!("sudo pacman -S --needed {arch}")
    } else if fam.contains("debian") || fam.contains("ubuntu") {
        format!("sudo apt install {debian}")
    } else if fam.contains("fedora") || fam.contains("rhel") {
        format!("sudo dnf install {fedora}")
    } else {
        format!("(Arch) {arch} · (Debian/Ubuntu) {debian} · (Fedora) {fedora}")
    })
}

/// 既定の建てる場所
pub fn default_build_dir() -> String {
    format!("{}/.local/share/falcon-launcher/build", std::env::var("HOME").unwrap_or_default())
}

/// 要る空き(目安)。この PC の 11-17 の大きさ: 歴史 2.6 GB・部品 2.4 GB・建て先 5.1 GB・控え 0.6 GB(1 回分)
pub const NEED_FREE_GB: f64 = 20.0;

fn nvcc_path(paths: &HashMap<String, String>) -> Option<PathBuf> {
    if let Some(p) = paths.get("cuda").filter(|p| !p.is_empty()) {
        let b = Path::new(p).join("bin/nvcc");
        return b.is_file().then_some(b);
    }
    if let Some(p) = run("sh", &["-c", "command -v nvcc"]).filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    ["/opt/cuda/bin/nvcc", "/usr/local/cuda/bin/nvcc"].into_iter().map(PathBuf::from).find(|p| p.is_file())
}

pub fn check_all(paths: &HashMap<String, String>) -> Vec<Check> {
    let mut out = vec![];
    let mk = |key, group, name, need, how, has_path, optional| Check { key, group, name, need, state: State::Missing, found: String::new(), how, has_path, optional };

    // ---- GPU とドライバー ----
    let mut c = mk("gpu", "GPU とドライバー", "NVIDIA の GPU", "RTX 20 以上(compute 7.5 以上)", How::Note, false, false);
    let mut d = mk("driver", "GPU とドライバー", "NVIDIA のドライバー", "580 以上", How::Manual { url: "https://www.nvidia.com/Download/index.aspx" }, false, false);
    if let Some(line) = run("nvidia-smi", &["--query-gpu=name,driver_version,compute_cap", "--format=csv,noheader"]).and_then(|t| t.lines().next().map(str::to_string)) {
        let f: Vec<&str> = line.split(',').map(str::trim).collect();
        if f.len() >= 3 {
            let cc = ver2(f[2]);
            c.found = format!("{} · compute {}", f[0], f[2]);
            c.state = if cc >= (7, 5) { State::Ok } else { State::Warn };
            d.found = f[1].to_string();
            d.state = if ver2(f[1]).0 >= 580 { State::Ok } else { State::Warn };
        }
    } else {
        c.found = "nvidia-smi が動きません".into();
    }
    out.push(c);
    out.push(d);

    // ---- 建てる道具(仕組みのパッケージ) ----
    let tools: [(&str, &str, &str, &str, &[&str], How, bool); 7] = [
        ("git", "Git", "ソースを取ってくる", "git", &["--version"], How::Package { arch: "git", debian: "git", fedora: "git" }, false),
        ("git-lfs", "Git LFS", "Blender の建てた部品を取ってくる", "git", &["lfs", "version"], How::Package { arch: "git-lfs", debian: "git-lfs", fedora: "git-lfs" }, false),
        ("cmake", "CMake", "建ての設定", "cmake", &["--version"], How::Package { arch: "cmake", debian: "cmake", fedora: "cmake" }, false),
        ("gcc", "C++ のコンパイラー(GCC)", "GCC 11 以上(目安)", "gcc", &["-dumpfullversion"], How::Package { arch: "base-devel", debian: "build-essential", fedora: "gcc-c++ make" }, false),
        ("python3", "Python 3", "Blender の部品を取る台本(make update)", "python3", &["--version"], How::Package { arch: "python", debian: "python3", fedora: "python3" }, false),
        ("ninja", "Ninja", "建てを速くする(無ければ make)", "ninja", &["--version"], How::Package { arch: "ninja", debian: "ninja-build", fedora: "ninja-build" }, true),
        ("ccache", "ccache", "更新の後の建て直しを速くする", "ccache", &["--version"], How::Package { arch: "ccache", debian: "ccache", fedora: "ccache" }, true),
    ];
    for (key, name, need, cmd, args, how, optional) in tools {
        let mut c = mk(key, "建てる道具", name, need, how, false, optional);
        if let Some(v) = run(cmd, args) {
            let first = v.lines().next().unwrap_or("").to_string();
            c.state = if key == "gcc" && ver2(&first).0 < 11 { State::Warn } else { State::Ok };
            c.found = first;
        }
        out.push(c);
    }

    // ---- NVIDIA の SDK ----
    let mut c = mk("cuda", "NVIDIA の SDK", "CUDA Toolkit", "12.8 以上(GPU の核を建てる)", How::Manual { url: "https://developer.nvidia.com/cuda-downloads" }, true, false);
    if let Some(nvcc) = nvcc_path(paths) {
        let v = run(nvcc.to_str().unwrap_or("nvcc"), &["--version"]).unwrap_or_default();
        let rel = v.lines().find_map(|l| l.split("release ").nth(1)).map(|s| s.split(',').next().unwrap_or("").to_string()).unwrap_or_default();
        c.state = if ver2(&rel) >= (12, 8) { State::Ok } else { State::Warn };
        c.found = format!("{rel} · {}", nvcc.display());
    }
    out.push(c);

    let mut c = mk("optix", "NVIDIA の SDK", "OptiX SDK", "9.0(NVIDIA のアカウントで取る)", How::Manual { url: "https://developer.nvidia.com/designworks/optix/download" }, true, false);
    let optix = paths.get("optix").cloned().filter(|p| !p.is_empty()).or_else(|| std::env::var("OPTIX_ROOT_DIR").ok());
    if let Some(p) = optix {
        let h = Path::new(&p).join("include/optix.h");
        if let Ok(t) = std::fs::read_to_string(&h) {
            let v = t.lines().find_map(|l| l.strip_prefix("#define OPTIX_VERSION ")).and_then(|x| x.trim().parse::<u32>().ok()).unwrap_or(0);
            c.state = if v >= 90000 { State::Ok } else { State::Warn };
            c.found = format!("{}.{} · {p}", v / 10000, v % 10000 / 100);
        } else {
            c.state = State::Warn;
            c.found = format!("include/optix.h がありません · {p}");
        }
    }
    out.push(c);

    let mut c = mk("dlss", "NVIDIA の SDK", "DLSS SDK", "ヘッダー(include/nvsdk_ngx.h)。DLSS を使う時だけ", How::Manual { url: "https://github.com/NVIDIA/DLSS" }, true, true);
    if let Some(p) = paths.get("dlss").filter(|p| !p.is_empty()) {
        if Path::new(p).join("include/nvsdk_ngx.h").is_file() {
            c.state = State::Ok;
            c.found = p.clone();
        } else {
            c.state = State::Warn;
            c.found = format!("include/nvsdk_ngx.h がありません · {p}");
        }
    }
    out.push(c);

    // ---- 建てる場所 ----
    let place = paths.get("build_dir").cloned().filter(|p| !p.is_empty()).unwrap_or_else(default_build_dir);
    let mut c = mk("build_dir", "建てる場所", "建てる場所", "空き 20 GB 以上(ソース・部品・建て先・控え)", How::Place, true, false);
    // まだ無い場所は、在る一番近い親で空きを見る
    let mut probe = PathBuf::from(&place);
    while !probe.exists() {
        if !probe.pop() {
            break;
        }
    }
    if let Some((avail, _)) = scan::disk(&probe.display().to_string()) {
        let g = avail as f64 / 1e9;
        c.state = if g >= NEED_FREE_GB { State::Ok } else { State::Warn };
        c.found = format!("空き {g:.0} GB · {place}{}", if Path::new(&place).exists() { "" } else { "(まだ無い・始める時に作る)" });
    }
    out.push(c);

    // ---- 後の段 ----
    // 公開の置き場は Relrei/Falcon-Engine
    // 取るのは下の「取って建てる」(setup.rs)。ここでは数に入れない(取る前でも「揃っています」になるように)
    let mut c = mk("source", "ソースと部品", "Falcon Engine のソースと Blender の部品", "GitHub から取る(下の「取って建てる」)", How::Note, false, true);
    let src = crate::places::places().source.clone();
    if Path::new(&src).join(".git").exists() {
        c.state = State::Ok;
        c.found = match crate::setup::source_tag() {
            Some(t) => format!("{t} · {src}"),
            None => src,
        };
    } else {
        c.state = State::Missing;
        c.found = format!("まだ取っていません(取る先: {src})");
    }
    out.push(c);
    out
}

// ---------- 落とす置き場 ----------
// GitHub の README の URL から取った物を、この枠へ落として入れる。
// フォルダはその場所を使い、圧縮した物はランチャーの SDK の置き場へ展開する。許諾に同意して入れる形の物(OptiX の .sh・CUDA の .run)は
// ランチャーが同意を代わりにせず、端末で打つ命令を出す。

/// ランチャーが展開した SDK の置き場
pub fn sdk_dir() -> PathBuf {
    PathBuf::from(format!("{}/.local/share/falcon-launcher/sdk", std::env::var("HOME").unwrap_or_default()))
}

/// フォルダの中身から、何の SDK かを見分ける(そのフォルダか、1〜2 段下)
pub fn identify(dir: &Path) -> Option<(&'static str, PathBuf)> {
    let marks: [(&str, &str); 3] = [("optix", "include/optix.h"), ("dlss", "include/nvsdk_ngx.h"), ("cuda", "bin/nvcc")];
    let mut cands = vec![dir.to_path_buf()];
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if e.path().is_dir() {
            cands.push(e.path());
            for e2 in std::fs::read_dir(e.path()).into_iter().flatten().flatten() {
                if e2.path().is_dir() {
                    cands.push(e2.path());
                }
            }
        }
    }
    for c in &cands {
        for (key, mark) in marks {
            if c.join(mark).is_file() {
                return Some((key, c.clone()));
            }
        }
    }
    None
}

pub enum Taken {
    /// 見分けられた(キー・場所)
    Set(&'static str, PathBuf),
    /// 使う人が端末で打つ命令と、その説明
    Command(String, String),
}

/// 落とされた物を受け取る(展開があるので、別の糸で呼ぶ)
pub fn take(path: &Path) -> Result<Taken, String> {
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let lower = name.to_lowercase();
    if path.is_dir() {
        return identify(path).map(|(k, p)| Taken::Set(k, p)).ok_or_else(|| format!("{name} の中に OptiX・DLSS SDK・CUDA の印(include/optix.h・include/nvsdk_ngx.h・bin/nvcc)が見つかりません"));
    }
    if !path.is_file() {
        return Err(format!("見つかりません: {}", path.display()));
    }
    if lower.ends_with(".sh") && lower.contains("optix") {
        let to = sdk_dir().join(lower.trim_end_matches(".sh"));
        let cmd = format!("mkdir -p '{}' && sh '{}' --prefix='{}' --exclude-subdir", to.display(), path.display(), to.display());
        return Ok(Taken::Command(cmd, format!("OptiX は NVIDIA の許諾を読んで同意して入れます。端末で打ち、出来た {} をここへ落としてください", to.display())));
    }
    if lower.ends_with(".run") && lower.contains("cuda") {
        let to = sdk_dir().join("cuda");
        let cmd = format!("sh '{}'", path.display());
        return Ok(Taken::Command(cmd, format!("CUDA は NVIDIA の許諾に同意して入れます。端末で打ち、Toolkit だけを選び、Options で入れる場所を {} にしてください。入ったら、そのフォルダをここへ落とします", to.display())));
    }
    let stem = ["", ".zip", ".tar.gz", ".tgz", ".tar.xz", ".txz", ".tar.zst", ".tar"].iter().rev().find_map(|ext| if !ext.is_empty() && lower.ends_with(ext) { Some(name[..name.len() - ext.len()].to_string()) } else { None });
    let Some(stem) = stem else {
        return Err(format!("{name} は受け取れない形です(フォルダ・.zip・.tar.* と、OptiX の .sh・CUDA の .run を受け取ります)"));
    };
    let to = sdk_dir().join(&stem);
    std::fs::create_dir_all(&to).map_err(|e| format!("{} を作れません: {e}", to.display()))?;
    let ok = if lower.ends_with(".zip") {
        Command::new("unzip").args(["-q", "-o"]).arg(path).arg("-d").arg(&to).status()
    } else {
        Command::new("tar").arg("-xf").arg(path).arg("-C").arg(&to).status()
    }
    .map(|s| s.success())
    .unwrap_or(false);
    if !ok {
        return Err(format!("{name} を展開できませんでした({})", to.display()));
    }
    identify(&to).map(|(k, p)| Taken::Set(k, p)).ok_or_else(|| format!("{name} を {} へ展開しましたが、OptiX・DLSS SDK・CUDA の印が見つかりません", to.display()))
}

pub fn key_name(key: &str) -> &'static str {
    match key {
        "optix" => "OptiX SDK",
        "dlss" => "DLSS SDK",
        "cuda" => "CUDA Toolkit",
        _ => "?",
    }
}
