# Falcon Launcher

[Falcon Engine](https://github.com/Relrei/Falcon-Engine) を使う人を支えるための、Linux 用のソフトです。

- Falcon Engine と、手元にある公式の Blender を、ここから起動できます。
- 同梱のアドオンを使い、Blender でレンダリングしたいシーンや範囲を決めて送ると、シーンごとに分けて順にレンダリングできます。

Falcon Engine が無くても、公式の Blender のランチャーとして使えます。

## 詳細

- **起動**: ビルドした Falcon Engine と、手元にある公式の Blender(`~/Applications`・`~/Downloads`・`~/.local/opt`・`/opt` の `blender-*`、`/usr/bin`・snap・Steam・flatpak)を見つけて、ビルドを選んで起動します。
- **続きから**: 最近開いた `.blend` を、使うビルドで開きます。
- **レンダリングの予約**: `.blend` を一覧に並べ、上から順にバックグラウンドで描きます。
  シーンごとに CPU のコア数・RAM の上限・VRAM の上限を決められます(決めていない物は全体の設定)。描いている間は、残りの時間と終わる時刻を出します。
- **Blender との連携**: 同梱のアドオン「Falcon Launcher Link」を入れると、Blender の出力のプロパティの「ランチャーに登録」で予約の一覧へ送れます。
  送る時に、そのシーンだけのサンプル数とデノイザー(OpenImageDenoise・OptiX。Falcon Engine では DLSS も)を、同じパネルで決められます。
- **ライブラリ**: 描いた絵と動画を、期間ごと(1 週間〜1 年)・組ごと・シーンごとにまとめて並べます。横の枚数も変えられます。連番の絵は動画にまとめて見られます。
  拡大表示では、ホイールで拡大・ドラッグで移動・ダブルクリックか Home で全体・`1` で等倍。動画は再生バー・倍速(0.25〜4 倍)・1 コマ送り(`,` `.`)で見られます。
- **アドオン**: どの Blender の版(`~/.config/blender/<版>/`)に、どのアドオンや拡張が、どの版で入っているかを表で見られます。ビルドに同梱のアドオンも出します。
- **取得してビルド・更新**: ビルドの準備の頁から、GitHub の Falcon Engine を取り、Blender の部品を取り、設定してビルドします
  ([docs/BUILD.md](https://github.com/Relrei/Falcon-Engine/blob/main/docs/BUILD.md) と同じ手順。`nice -n 10` で、使うスレッドの数は頁で決めます)。
  ランチャーを閉じても続き、止めても次は続きからビルドします。記録は `~/.local/share/falcon-launcher/取って建てる.log`。
  置き場のタグを起動の時と 3 時間ごとに見て、新しいビルドがあればホームに出します。押すとそのビルドへ切り替えて、変わった所だけビルドし直します。
  前に使っていたビルドは `<ビルドする場所>/前の版/<版>` に残り、ビルドの一覧から起動できます。
- **ビルドの準備**: Falcon Engine をビルドするのに要る物(GPU・ドライバー・ビルドの道具・CUDA・OptiX・DLSS SDK・空き)を確かめます。
  NVIDIA の物は、各自が NVIDIA の頁で許諾に同意して取り、ランチャーへ落とします(ランチャーは取ってきません。展開して場所を覚えるだけです)。
  パッケージを入れる命令は写すだけで、sudo はランチャーからは打ちません。

## 要る物

- Linux(X11 か Wayland)。予約の RAM の上限には systemd(`systemd-run --user`)を使います。
- Rust(`cargo`)。
- ffmpeg と ffprobe(ライブラリで動画を見る時と、連番の絵を動画にまとめる時。まとめる時は NVIDIA の NVENC を使います)。
- 字体(無くても動きます。無い時は Noto Sans CJK などで出ます):
  Inter、IBM Plex Sans JP、JetBrainsMono Nerd Font(Propo と無印)。
  `~/.local/share/fonts`・`/usr/share/fonts`・`~/.local/share/falcon-launcher/fonts` から名前で探します。

## ビルドする・起動する

```sh
git clone --branch Falcon-Launcher-v0.2.0 https://github.com/Relrei/Falcon-Launcher.git && cd Falcon-Launcher && ./install.sh
```

`install.sh` は、Rust(cargo)の有無を確かめ、無ければ公式の rustup で入れるかを聞きます。そのあとランチャーをビルドし、アプリのメニューへの登録を聞きます。管理者の権限は使いません。
ビルドしたら、ランチャーを開いて「ビルドの準備」の頁から Falcon Engine をビルドします。

## 置き場の設定

ランチャーが書く物は `~/.local/share/falcon-launcher/`、設定は `~/.config/falcon-launcher/` に置きます。
置き場を変える時は `~/.config/falcon-launcher/場所.tsv` に「キー<TAB>パス」で書きます(`~` が使えます。書いていない物は既定のまま)。

| キー | 中身 | 既定 |
|---|---|---|
| `source` | Falcon Engine のソース(git の木) | <ビルドする場所>/falcon-engine |
| `falcon_build` | Falcon Engine のビルド先(`bin/blender` がある所) | <ビルドする場所>/build |
| `falcon_repo` | Falcon Engine を取る・新しいビルドを見る置き場(写しや試しの置き場を使う時) | `https://github.com/Relrei/Falcon-Engine.git` |
| `dlss_runtime` | 起動の時に `FALCON_DLSS_RUNTIME_PATH` で渡す DLSS ランタイムの置き場 | ビルドの準備で置いた DLSS SDK の `lib/Linux_x86_64/rel` |
| `render` | 予約で描いた絵 | `~/.local/share/falcon-launcher/レンダリング` |
| `seq` | 連番を動画にした物 | `~/.local/share/falcon-launcher/連番の動画` |
| `gallery` | 手で置く絵(フォルダごとに 1 組) | `~/.local/share/falcon-launcher/ギャラリー` |
| `blender_search` | 公式の Blender を探す所に足す所(`:` で区切る) | なし |
| `disk` | 下の帯に空きを出すディスク | `falcon_build` |
| `fonts` | 字体を探す所に足す所(`:` で区切る) | なし |

<ビルドする場所>は、ビルドの準備の頁で決める場所です(既定は `~/.local/share/falcon-launcher/build`)。
Falcon Engine のビルドの仕方は [Falcon Engine の docs/BUILD.md](https://github.com/Relrei/Falcon-Engine/blob/main/docs/BUILD.md) にあります。

## 許諾

- ランチャー: MIT License([LICENSE](LICENSE))
- `addon/falcon_launcher_link.py`(Blender のアドオン): GPL-2.0-or-later(Blender のアドオンのため)
- 使っている部品: [egui・eframe](https://github.com/emilk/egui)(MIT か Apache-2.0)、[image](https://github.com/image-rs/image)(MIT か Apache-2.0)ほか(`Cargo.lock`)

Blender は Blender Foundation の商標です。NVIDIA・DLSS・OptiX・CUDA は NVIDIA Corporation の商標です。
このランチャーは個人で作っている物で、Blender Foundation・NVIDIA とは関係がありません。
不具合は [Issues](../../issues) へお願いします(公式の Blender へは送らないでください)。
