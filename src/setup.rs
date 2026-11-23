//! 取得してビルド・更新: Falcon Engine を GitHub から取り、部品を取り、設定してビルドする。新しい版の検知もここ。
//! 手順は Falcon Engine の docs/BUILD.md と同じ。違いは 2 つ:
//! - tests/files(LFS で 890 MB ほど)はビルドに要らないので取らない(lfs.fetchexclude)。
//! - make update は部品(lib/linux_x64)だけにする(--no-blender --no-lfs-fallback)。タグの取り出しは枝が無く、
//!   本体の更新は飛ばされる。LFS の予備(projects.blender.org)を足すと、tests/files をそこからまた取りに行くため。
//! ランチャーが取ったソースだけを動かす(.git/falcon-launcher の印)。開発の木を取り違えて切り替えないため。

use super::*;

/// 公開の置き場。場所.tsv の falcon_repo で替えられる(写しや試しの置き場)
pub const REPO: &str = "https://github.com/Relrei/Falcon-Engine.git";
/// 版のタグの頭(タグ Falcon-Engine-v0.7.0)
pub const TAG_PREFIX: &str = "Falcon-Engine-v";

pub fn repo() -> String {
    let r = &places::places().falcon_repo;
    if r.is_empty() { REPO.into() } else { r.clone() }
}

/// "0.7.0" や "Falcon-Engine-v0.7.0" → (0, 7, 0)。3 つの数でない物は版と見ない
pub fn parse_ver(s: &str) -> Option<(u32, u32, u32)> {
    let v = s.strip_prefix(TAG_PREFIX).unwrap_or(s);
    let n: Vec<u32> = v.split('.').map(|x| x.parse().ok()).collect::<Option<_>>()?;
    (n.len() == 3).then(|| (n[0], n[1], n[2]))
}

/// 置き場の版のタグを、新しい順に(git ls-remote。取ってはこない)
pub fn remote_tags() -> Result<Vec<String>, String> {
    let o = Command::new("git")
        .args(["ls-remote", "--tags", "--refs"])
        .arg(repo())
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("git を起こせません: {e}"))?;
    if !o.status.success() {
        let err = String::from_utf8_lossy(&o.stderr);
        return Err(format!("{} の版を見られません: {}", repo(), err.lines().last().unwrap_or("").trim()));
    }
    let mut tags: Vec<String> = String::from_utf8_lossy(&o.stdout)
        .lines()
        .filter_map(|l| l.split('\t').nth(1)?.strip_prefix("refs/tags/").map(str::to_string))
        .filter(|t| t.starts_with(TAG_PREFIX) && parse_ver(t).is_some())
        .collect();
    tags.sort_by_key(|t| std::cmp::Reverse(parse_ver(t)));
    Ok(tags)
}

/// 見回りの結果
#[derive(Clone, Debug, Default)]
pub struct Remote {
    /// 一番新しいタグ
    pub latest: Option<String>,
    pub error: Option<String>,
    pub checked: Option<Instant>,
}

/// ビルドした版(ビルド先のアドオンの bl_info)
pub fn built_version() -> Option<String> {
    scan::falcon_version(Path::new(&places::places().falcon_build))
}

/// ソースが今いる版のタグ。describe はタグの中の名前を出すことがあるので(GitHub の Falcon-Engine-v0.7.0 は中の名前が
/// demo/Falcon-Engine-v0.7.0)、手元の参照の名前で見る
pub fn source_tag() -> Option<String> {
    let src = &places::places().source;
    let o = Command::new("git").args(["-C", src, "tag", "--points-at", "HEAD"]).stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    let mut tags: Vec<String> = String::from_utf8_lossy(&o.stdout).lines().map(str::trim).filter(|t| t.starts_with(TAG_PREFIX) && parse_ver(t).is_some()).map(str::to_string).collect();
    tags.sort_by_key(|t| std::cmp::Reverse(parse_ver(t)));
    tags.into_iter().next()
}

/// ランチャーが取ったソースか(取った時に .git/falcon-launcher を置く)
pub fn source_is_ours() -> bool {
    Path::new(&places::places().source).join(".git/falcon-launcher").is_file()
}

/// ソースがあるが、ランチャーが取った物ではない(開発の木など)。この時は切り替えない
pub fn source_is_foreign() -> bool {
    Path::new(&places::places().source).join(".git").exists() && !source_is_ours()
}

/// 手元の様子(git を起こすので、毎コマではなく数秒ごとに取る)
#[derive(Clone, Debug, Default)]
pub struct Info {
    /// ビルドした版(0.7.0)。bin/blender が無ければ None
    pub built: Option<String>,
    /// ソースが今いるタグ
    pub source_tag: Option<String>,
    pub has_source: bool,
    /// ソースがあるがランチャーが取った物ではない(開発の木)
    pub foreign: bool,
}

pub fn info() -> Info {
    let has_bin = Path::new(&places::places().falcon_build).join("bin/blender").is_file();
    Info {
        built: built_version().filter(|_| has_bin),
        source_tag: source_tag(),
        has_source: Path::new(&places::places().source).join(".git").exists(),
        foreign: source_is_foreign(),
    }
}

impl Info {
    /// latest が今の版(ビルドした版。まだなら取ったソースのタグ)より新しいか
    pub fn is_newer(&self, latest: &str) -> bool {
        let Some(l) = parse_ver(latest) else { return false };
        let cur = self.built.as_deref().and_then(parse_ver).or_else(|| self.source_tag.as_deref().and_then(parse_ver));
        cur.is_some_and(|c| l > c)
    }
}

// ---------- 走らせる ----------

fn job_dir() -> PathBuf {
    places::data_dir()
}
pub fn log_path() -> PathBuf {
    job_dir().join("取って建てる.log")
}
fn pid_path() -> PathBuf {
    job_dir().join("取って建てる.pid")
}
fn rc_path() -> PathBuf {
    job_dir().join("取って建てる.rc")
}
fn stage_path() -> PathBuf {
    job_dir().join("取って建てる.stage")
}
fn script_path() -> PathBuf {
    job_dir().join("取って建てる.sh")
}

/// 動いている時はその pid
pub fn running_pid() -> Option<i32> {
    let pid: i32 = std::fs::read_to_string(pid_path()).ok()?.trim().parse().ok()?;
    // 同じ pid が別の物に使い回されていないか(台本の名前で確かめる)
    let cmd = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    String::from_utf8_lossy(&cmd).contains("取って建てる.sh").then_some(pid)
}

/// 前に走らせた結果(終わっていれば rc)
pub fn last_rc() -> Option<i32> {
    std::fs::read_to_string(rc_path()).ok()?.trim().parse().ok()
}

#[derive(Clone, Debug, Default)]
pub struct Status {
    /// 「==> 2/4 …」の段
    pub stage: String,
    pub progress: scan::Progress,
    /// 最後の数行
    pub tail: Vec<String>,
}

pub fn status() -> Status {
    let mut s = Status { progress: scan::progress_file(&log_path()), ..Default::default() };
    let bytes = scan::read_tail(&log_path(), 65536).unwrap_or_default();
    let text = String::from_utf8_lossy(&bytes);
    // ninja の行は \r で上書きされることがあるので、改行と \r の両方で切る
    let lines: Vec<&str> = text.split(['\n', '\r']).map(str::trim_end).filter(|l| !l.is_empty()).collect();
    s.stage = std::fs::read_to_string(stage_path()).map(|t| t.trim().to_string()).unwrap_or_default();
    s.tail = lines.iter().rev().take(8).rev().map(|l| l.to_string()).collect();
    s
}

/// ビルドに使うスレッドの数(準備.tsv の jobs。無ければ CPU の半分。在席中の画面を重くしないため)
pub fn jobs(paths: &HashMap<String, String>) -> u32 {
    paths.get("jobs").and_then(|j| j.parse().ok()).filter(|&j| j > 0).unwrap_or_else(|| (std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2) as u32 / 2).max(1))
}

/// GPU に合わせた CUDA の核の世代(nvidia-smi の compute_cap 8.6 → sm_86)
pub fn cuda_arch() -> Option<String> {
    let o = Command::new("nvidia-smi").args(["--query-gpu=compute_cap", "--format=csv,noheader"]).stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    let line = String::from_utf8_lossy(&o.stdout).lines().next()?.trim().to_string();
    let (a, b) = line.split_once('.')?;
    Some(format!("sm_{a}{b}"))
}

fn has(cmd: &str) -> bool {
    Command::new("sh").arg("-c").arg(format!("command -v {cmd}")).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}

/// sh の 1 語にする
fn q(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// 走らせる中身(確かめの画面にも出す)
pub struct Plan {
    pub tag: String,
    pub source: String,
    pub build: String,
    pub fresh: bool,
    pub jobs: u32,
    pub arch: String,
    pub dlss: Option<String>,
    pub script: String,
}

pub fn plan(tag: &str, paths: &HashMap<String, String>) -> Result<Plan, String> {
    let pl = places::places();
    let source = pl.source.clone();
    let build = pl.falcon_build.clone();
    if source_is_foreign() {
        return Err(format!("{source} はランチャーが取ったソースではないので、切り替えません(開発の木など)。ビルドの準備で別の「ビルドする場所」を選んでください"));
    }
    let fresh = !Path::new(&source).join(".git").exists();
    let optix = paths.get("optix").cloned().filter(|p| Path::new(p).join("include/optix.h").is_file()).ok_or("OptiX SDK の場所がまだです(ビルドの準備で置いてください)")?;
    let dlss = paths.get("dlss").cloned().filter(|p| Path::new(p).join("include/nvsdk_ngx.h").is_file());
    let cuda = paths.get("cuda").cloned().filter(|p| Path::new(p).join("bin/nvcc").is_file());
    let arch = cuda_arch().unwrap_or_else(|| "sm_86".into());
    let jobs = jobs(paths);
    let ninja = has("ninja");
    let ccache = has("ccache");
    let prev = built_version().filter(|_| Path::new(&build).join("bin/blender").is_file());
    let place = Path::new(&build).parent().map(|p| p.display().to_string()).unwrap_or_default();

    let mut s = String::new();
    s += "#!/bin/bash\n# Falcon Launcher が書いた台本(取得してビルド)。走らせるたびに書き直す\nset -eo pipefail\n";
    // 止めた時(TERM)は 143。EXIT の $? は、信号で止まった時に 0 になることがあった(11-17 の試し)
    let rc = q(&rc_path().display().to_string());
    s += &format!("RC={rc}\nSTAGE={}\n", q(&stage_path().display().to_string()));
    s += "trap 'echo 143 > \"$RC\"; exit 143' TERM INT HUP\n";
    s += "trap 'r=$?; [ -e \"$RC\" ] || echo $r > \"$RC\"' EXIT\n";
    // 段は別のファイルにも書く(記録の終わりだけを読むと、ninja の行で段の行が押し出される)
    s += "stage() { echo \"==> $1\"; echo \"$1\" > \"$STAGE\"; }\n";
    s += &format!("REPO={}\nTAG={}\nSRC={}\nBUILD={}\n", q(&repo()), q(tag), q(&source), q(&build));
    if let Some(c) = &cuda {
        s += &format!("export PATH={}/bin:\"$PATH\"\n", q(c));
    }
    s += "export GIT_TERMINAL_PROMPT=0\n";
    s += "stage \"1/4 ソースを取る($TAG)\"\n";
    if fresh {
        s += "mkdir -p \"$(dirname \"$SRC\")\"\n";
        // 中身(LFS)は後で tests/files を除いて取る
        s += "GIT_LFS_SKIP_SMUDGE=1 git clone --progress --branch \"$TAG\" \"$REPO\" \"$SRC\"\n";
        s += "touch \"$SRC/.git/falcon-launcher\"\n";
        s += "git -C \"$SRC\" config lfs.fetchexclude 'tests/**'\n";
    } else {
        s += "if [ -n \"$(git -C \"$SRC\" status --porcelain --untracked-files=no)\" ]; then echo \"ソースに手で変えた所があるので止めます: $SRC\"; exit 3; fi\n";
        s += "git -C \"$SRC\" fetch --progress \"$REPO\" \"refs/tags/$TAG:refs/tags/$TAG\"\n";
        s += "GIT_LFS_SKIP_SMUDGE=1 git -C \"$SRC\" -c advice.detachedHead=false checkout \"$TAG\"\n";
    }
    s += "git -C \"$SRC\" lfs pull\n";
    s += "stage \"2/4 Blender の部品を取る(lib/linux_x64)\"\n";
    s += "cd \"$SRC\"\npython3 build_files/utils/make_update.py --no-blender --no-lfs-fallback\n";
    s += "stage \"3/4 設定する(cmake)\"\n";
    // 前の版の本体は 前の版/<版> へ移して残す(戻れるように。install は bin を作り直す)
    if let Some(v) = &prev {
        if parse_ver(tag) != parse_ver(v) {
            let to = format!("{place}/前の版/{v}");
            s += &format!("if [ -x \"$BUILD/bin/blender\" ] && [ ! -e {t} ]; then mkdir -p {p} && mv \"$BUILD/bin\" {t} && echo \"前の版 {v} を {to} へ移しました\"; fi\n", t = q(&to), p = q(&format!("{place}/前の版")));
        }
    }
    s += "mkdir -p \"$BUILD\"\ncd \"$BUILD\"\n";
    let mut args: Vec<String> = vec![];
    if ninja {
        args.push("-G Ninja".into());
    }
    args.push("\"$SRC\"".into());
    args.push("-C \"$SRC/build_files/cmake/config/blender_release.cmake\"".into());
    args.push("-DCMAKE_BUILD_TYPE=Release".into());
    args.push("-DWITH_CYCLES_CUDA_BINARIES=ON".into());
    args.push(format!("-DCYCLES_CUDA_BINARIES_ARCH={arch}"));
    args.push(format!("-DOPTIX_ROOT_DIR={}", q(&optix)));
    if let Some(c) = &cuda {
        args.push(format!("-DCUDA_TOOLKIT_ROOT_DIR={}", q(c)));
    }
    match &dlss {
        Some(d) => args.push(format!("-DWITH_DLSS=ON -DDLSS_SDK_ROOT={}", q(d))),
        None => args.push("-DWITH_DLSS=OFF".into()),
    }
    args.push("-DWITH_CYCLES_DEVICE_HIP=OFF -DWITH_CYCLES_HIP_BINARIES=OFF -DWITH_CYCLES_DEVICE_HIPRT=OFF".into());
    args.push("-DWITH_CYCLES_DEVICE_ONEAPI=OFF -DWITH_CYCLES_ONEAPI_BINARIES=OFF".into());
    if ccache {
        args.push("-DCMAKE_C_COMPILER_LAUNCHER=ccache -DCMAKE_CXX_COMPILER_LAUNCHER=ccache".into());
    }
    s += &format!("cmake {}\n", args.join(" \\\n  "));
    s += "stage \"4/4 ビルドする\"\n";
    let tool = if ninja { format!("ninja -j{jobs} install") } else { format!("make -j{jobs} install") };
    s += &format!("nice -n 10 {tool}\n");
    s += "stage \"終わり($TAG)\"\n";
    Ok(Plan { tag: tag.into(), source, build, fresh, jobs, arch, dlss, script: s })
}

/// 台本を書いて、背景で走らせる(ランチャーを閉じても続く)
pub fn start(p: &Plan) -> Result<(), String> {
    if running_pid().is_some() {
        return Err("もう走っています".into());
    }
    std::fs::create_dir_all(job_dir()).map_err(|e| e.to_string())?;
    std::fs::write(script_path(), &p.script).map_err(|e| format!("台本を書けません: {e}"))?;
    let _ = std::fs::remove_file(rc_path());
    let _ = std::fs::remove_file(stage_path());
    let log = std::fs::File::create(log_path()).map_err(|e| format!("記録を書けません: {e}"))?;
    let log2 = log.try_clone().map_err(|e| e.to_string())?;
    use std::os::unix::process::CommandExt;
    let child = Command::new("bash").arg(script_path()).process_group(0).stdin(Stdio::null()).stdout(log).stderr(log2).spawn().map_err(|e| format!("起こせません: {e}"))?;
    let _ = std::fs::write(pid_path(), child.id().to_string());
    // 終わりを待つ糸(ゾンビを残さない)
    std::thread::spawn(move || {
        let mut c = child;
        let _ = c.wait();
    });
    Ok(())
}

/// 止める(台本と、その下の git・ninja をまとめて)
pub fn stop() {
    if let Some(pid) = running_pid() {
        // 「-pid」は組(プロセスグループ)。-- が無いと印と取り違えられる
        let _ = Command::new("kill").args(["-s", "TERM", "--"]).arg(format!("-{pid}")).status();
    }
}
