# Falcon Launcher

[Falcon Engine](https://github.com/Relrei/Falcon-Engine) を使う人を支えるための、Linux 用のソフトです。

- Falcon Engine と、手元にある公式の Blender を、ここから起動できます。
- 同梱のアドオンを使い、Blender でレンダリングしたいシーンや範囲を決めて送ると、シーンごとに分けて順にレンダリングできます。

Falcon Engine が無くても、公式の Blender のランチャーとして使えます。

## 詳細

- **起動**: 建てた Falcon Engine と、手元にある公式の Blender(`~/Applications`・`~/Downloads`・`~/.local/opt`・`/opt` の `blender-*`、`/usr/bin`・snap・Steam・flatpak)を見つけて、版を選んで起動します。
- **続きから**: 最近開いた `.blend` を、使う版で開きます。
- **レンダリングの予約**: `.blend` を一覧に並べ、上から順にバックグラウンドで描きます。
  シーンごとに CPU のスレッド数・RAM の上限・VRAM の上限・優先度(nice)を決められます(決めていない物は全体の設定)。
- **Blender との連携**: 同梱のアドオン「Falcon Launcher Link」を入れると、Blender の出力のプロパティの「ランチャーに登録」で予約の一覧へ送れます。
  送る時に、そのシーンだけのサンプル数とデノイザー(OpenImageDenoise・OptiX。Falcon Engine では DLSS も)を、同じパネルで決められます。
- **レンダリング結果**: 描いた絵と動画を組ごとに並べます。連番の絵は動画にまとめて見られます。
- **取って建てる・更新**: 建てる準備の頁から、GitHub の Falcon Engine を取り、Blender の部品を取り、設定して建てます
  ([docs/BUILD.md](https://github.com/Relrei/Falcon-Engine/blob/main/docs/BUILD.md) と同じ手順。`nice -n 10` で、並びの数は頁で決めます)。
  ランチャーを閉じても続き、止めても次は続きから建てます。記録は `~/.local/share/falcon-launcher/取って建てる.log`。
  置き場の版のタグを起動の時と 3 時間ごとに見て、新しい版があればホームに出します。押すとその版へ切り替えて、変わった所だけ建て直します。
  前に建てていた版は `<建てる場所>/前の版/<版>` に残り、版の一覧から起動できます。
- **建てる準備**: Falcon Engine を建てるのに要る物(GPU・ドライバー・建てる道具・CUDA・OptiX・DLSS SDK・空き)を確かめます。
  NVIDIA の物は、各自が NVIDIA の頁で許諾に同意して取り、ランチャーへ落とします(ランチャーは取ってきません。展開して場所を覚えるだけです)。
  パッケージを入れる命令は写すだけで、sudo はランチャーからは打ちません。

## 要る物

- Linux(X11 か Wayland)。予約の RAM の上限には systemd(`systemd-run --user`)を使います。
- Rust(`cargo`)。
- 字体(無くても動きます。無い時は Noto Sans CJK などで出ます):
  Inter、IBM Plex Sans JP、JetBrainsMono Nerd Font(Propo と無印)。
  `~/.local/share/fonts`・`/usr/share/fonts`・`~/.local/share/falcon-launcher/fonts` から名前で探します。

## 建てる・起動する

```sh
git clone https://github.com/Relrei/Falcon-Launcher.git && cd Falcon-Launcher && ./install.sh
```

`install.sh` は、Rust(cargo)の有無を確かめ、無ければ公式の rustup で入れるかを聞きます。そのあとランチャーを建て、アプリのメニューへの登録を聞きます。管理者の権限は使いません。
建てたら、ランチャーを開いて「建てる準備」の頁から Falcon Engine を建てます。

## 置き場の設定

ランチャーが書く物は `~/.local/share/falcon-launcher/`、設定は `~/.config/falcon-launcher/` に置きます。
置き場を変える時は `~/.config/falcon-launcher/場所.tsv` に「キー<TAB>パス」で書きます(`~` が使えます。書いていない物は既定のまま)。

| キー | 中身 | 既定 |
|---|---|---|
| `source` | Falcon Engine のソース(git の木) | <建てる場所>/falcon-engine |
| `falcon_build` | Falcon Engine の建て先(`bin/blender` がある所) | <建てる場所>/build |
| `falcon_repo` | Falcon Engine を取る・新しい版を見る置き場(写しや試しの置き場を使う時) | `https://github.com/Relrei/Falcon-Engine.git` |
| `dlss_runtime` | 起動の時に `FALCON_DLSS_RUNTIME_PATH` で渡す DLSS ランタイムの置き場 | 建てる準備で置いた DLSS SDK の `lib/Linux_x86_64/rel` |
| `render` | 予約で描いた絵 | `~/.local/share/falcon-launcher/レンダリング` |
| `seq` | 連番を動画にした物 | `~/.local/share/falcon-launcher/連番の動画` |
| `gallery` | 手で置く絵(フォルダごとに 1 組) | `~/.local/share/falcon-launcher/ギャラリー` |
| `blender_search` | 公式の Blender を探す所に足す所(`:` で区切る) | なし |
| `disk` | 下の帯に空きを出すディスク | `falcon_build` |
| `fonts` | 字体を探す所に足す所(`:` で区切る) | なし |

<建てる場所>は、建てる準備の頁で決める場所です(既定は `~/.local/share/falcon-launcher/build`)。
Falcon Engine の建て方は [Falcon Engine の docs/BUILD.md](https://github.com/Relrei/Falcon-Engine/blob/main/docs/BUILD.md) にあります。

## 許諾

- ランチャー: MIT License([LICENSE](LICENSE))
- `addon/falcon_launcher_link.py`(Blender のアドオン): GPL-2.0-or-later(Blender のアドオンのため)
- 使っている部品: [egui・eframe](https://github.com/emilk/egui)(MIT か Apache-2.0)、[image](https://github.com/image-rs/image)(MIT か Apache-2.0)ほか(`Cargo.lock`)

Blender は Blender Foundation の商標です。NVIDIA・DLSS・OptiX・CUDA は NVIDIA Corporation の商標です。
このランチャーは個人で作っている物で、Blender Foundation・NVIDIA とは関係がありません。
不具合は [Issues](../../issues) へお願いします(公式の Blender へは送らないでください)。
