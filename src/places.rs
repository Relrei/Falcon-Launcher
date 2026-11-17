//! 置き場。場所を決め打ちにせず、`~/.config/falcon-launcher/場所.tsv`(キー<TAB>パス)から読む。
//! 書いていないキーは、建てる準備の「建てる場所」(準備.tsv の build_dir)と `~/.local/share/falcon-launcher` から決める。
//! 開発の木・試しの枝・クラスター・ギャラリーの足し(開発用)は、キーを書いた時だけ出る。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub struct Places {
    /// Falcon Engine のソース(git の木)
    pub source: String,
    /// Falcon Engine の建て先(bin/blender がある所)
    pub falcon_build: String,
    /// Falcon Engine の置き場(空なら GitHub の Relrei/Falcon-Engine)。写しや試しの置き場を指す時に書く
    pub falcon_repo: String,
    /// 建て先の版を推し量る枝(公開の木は HEAD)
    pub falcon_branch: String,
    /// 起動の時に渡す DLSS のランタイムの置き場(FALCON_DLSS_RUNTIME_PATH)。空なら渡さない
    pub dlss_runtime: String,
    /// 開発の場所(build/host・build/test-<単位>・scripts/build.sh・integration)。空なら本線と試しの枝を出さない
    pub project: String,
    /// 本線と試しの枝を起こす台本(空なら <建て先>/bin/blender を直に起こす)
    pub wrapper: String,
    /// クラスター(仮想ジオメトリ)の木。空なら出さない
    pub vg: String,
    /// 版の一覧(engine-versions.tsv)。空なら読まない
    pub versions: String,
    /// 説明の文(<版の key>.md)
    pub about: String,
    /// 手で置く絵
    pub gallery: String,
    /// ギャラリーに足す所(「:」で区切る)。days は <日付>/<組> の 2 段、groups は「名前=パス」で中の 1 段ずつ、dirs はそのフォルダで 1 組
    pub gallery_days: Vec<String>,
    pub gallery_groups: Vec<(String, String)>,
    pub gallery_dirs: Vec<String>,
    /// 予約で描いた絵
    pub render: String,
    /// 連番の絵を動画にした物
    pub seq: String,
    /// 公式の Blender を探す所に足す所(「:」で区切る)
    pub blender_search: Vec<String>,
    /// 下の帯に空きを出すディスク
    pub disk: String,
    /// 下の帯に出すディスクの名前(disk を書いた時だけ、その名前)
    pub disk_label: String,
    /// 字体を探す所に足す所(「:」で区切る)
    pub fonts: Vec<String>,
}

fn home() -> String {
    std::env::var("HOME").unwrap_or_default()
}

pub fn config_dir() -> PathBuf {
    PathBuf::from(home()).join(".config/falcon-launcher")
}

/// ランチャーが書く物の置き場($XDG_DATA_HOME/falcon-launcher)
pub fn data_dir() -> PathBuf {
    match std::env::var("XDG_DATA_HOME") {
        Ok(d) if d.starts_with('/') => PathBuf::from(d).join("falcon-launcher"),
        _ => PathBuf::from(home()).join(".local/share/falcon-launcher"),
    }
}

fn read_tsv(p: &Path) -> HashMap<String, String> {
    std::fs::read_to_string(p)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once('\t').map(|(k, v)| (k.trim().to_string(), expand(v.trim()))))
        .filter(|(_, v)| !v.is_empty())
        .collect()
}

/// 頭の ~ を HOME にする
fn expand(v: &str) -> String {
    match v.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => format!("{}{rest}", home()),
        _ => v.to_string(),
    }
}

fn list(v: Option<&String>) -> Vec<String> {
    v.map(|s| s.split(':').map(expand).filter(|x| !x.is_empty()).collect()).unwrap_or_default()
}

pub fn places() -> &'static Places {
    static P: OnceLock<Places> = OnceLock::new();
    P.get_or_init(|| {
        let m = read_tsv(&config_dir().join("場所.tsv"));
        let prep = read_tsv(&config_dir().join("準備.tsv"));
        let data = data_dir();
        let get = |k: &str| m.get(k).cloned();
        let at = |sub: &str| data.join(sub).display().to_string();
        // 建てる場所(建てる準備の頁で決める)の中: falcon-engine(ソース)と build(建て先)。docs/BUILD.md と同じ並び
        let place = prep.get("build_dir").cloned().unwrap_or_else(|| at("build"));
        let source = get("source").unwrap_or_else(|| format!("{place}/falcon-engine"));
        let falcon_build = get("falcon_build").unwrap_or_else(|| format!("{place}/build"));
        // DLSS SDK を置いてあれば、その中のランタイム(lib/Linux_x86_64/rel)
        let dlss_runtime = get("dlss_runtime").unwrap_or_else(|| {
            prep.get("dlss").map(|d| format!("{d}/lib/Linux_x86_64/rel")).filter(|d| Path::new(d).is_dir()).unwrap_or_default()
        });
        Places {
            falcon_repo: get("falcon_repo").unwrap_or_default(),
            falcon_branch: get("falcon_branch").unwrap_or_else(|| "HEAD".into()),
            project: get("project").unwrap_or_default(),
            wrapper: get("wrapper").unwrap_or_default(),
            vg: get("vg").unwrap_or_default(),
            versions: get("versions").unwrap_or_default(),
            about: get("about").unwrap_or_else(|| at("説明")),
            gallery: get("gallery").unwrap_or_else(|| at("ギャラリー")),
            gallery_days: list(m.get("gallery_days")),
            gallery_groups: list(m.get("gallery_groups")).into_iter().filter_map(|x| x.split_once('=').map(|(a, b)| (a.to_string(), expand(b)))).collect(),
            gallery_dirs: list(m.get("gallery_dirs")),
            render: get("render").unwrap_or_else(|| at("レンダリング")),
            seq: get("seq").unwrap_or_else(|| at("連番の動画")),
            blender_search: list(m.get("blender_search")),
            disk_label: get("disk").and_then(|d| Path::new(&d).file_name().map(|n| format!("{} ", n.to_string_lossy()))).unwrap_or_default(),
            disk: get("disk").unwrap_or_else(|| falcon_build.clone()),
            fonts: list(m.get("fonts")),
            source,
            falcon_build,
            dlss_runtime,
        }
    })
}

/// 字体のファイルを探す(足した所 → ランチャーの fonts → ~/.local/share/fonts → /usr/share/fonts。2 段下まで)
pub fn find_font(name: &str) -> Option<PathBuf> {
    let p = places();
    let mut roots: Vec<PathBuf> = p.fonts.iter().map(PathBuf::from).collect();
    roots.push(data_dir().join("fonts"));
    roots.push(PathBuf::from(home()).join(".local/share/fonts"));
    roots.push(PathBuf::from("/usr/share/fonts"));
    fn walk(dir: &Path, name: &str, depth: u32) -> Option<PathBuf> {
        let direct = dir.join(name);
        if direct.is_file() {
            return Some(direct);
        }
        if depth == 0 {
            return None;
        }
        let mut subs: Vec<PathBuf> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        subs.sort();
        subs.iter().find_map(|s| walk(s, name, depth - 1))
    }
    roots.iter().find_map(|r| walk(r, name, 2))
}
