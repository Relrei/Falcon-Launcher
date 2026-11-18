#!/bin/sh
# Falcon Launcher を建てて、起動できるようにする。使い方: Falcon-Launcher の中で ./install.sh
# 取ってくる所から: git clone https://github.com/Relrei/Falcon-Launcher.git && cd Falcon-Launcher && ./install.sh
# ビルド済みは配らない。Rust(cargo)が無ければ、公式の rustup を入れるかを聞く。管理者の権限は使わない。
set -eu

cd "$(dirname "$0")"

ask() { # ask "質問" → 「y」「yes」の時だけ 0
  printf '%s [y/N] ' "$1"
  read -r ans || ans=""
  case "$ans" in
    y|Y|yes|YES) return 0 ;;
    *) return 1 ;;
  esac
}

PATH="$HOME/.cargo/bin:$PATH"
export PATH

if ! command -v cargo >/dev/null 2>&1; then
  echo "Rust(cargo)が見つかりません。Rust は公式の rustup で入れます(https://rustup.rs)。"
  if ask "今、rustup で Rust を入れますか?"; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    PATH="$HOME/.cargo/bin:$PATH"
    export PATH
  else
    echo "Rust を入れてから、もう一度 ./install.sh を実行してください。"
    exit 1
  fi
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo が見つかりません。Rust を入れ直してから、もう一度 ./install.sh を実行してください。"
  exit 1
fi

echo "ランチャーを建てています(初めての時は数分かかります)..."
cargo build --release

BIN="$(pwd)/target/release/falcon-launcher"
echo "建てられました: $BIN"

if ask "アプリのメニューに登録しますか?"; then
  DIR="$HOME/.local/share/applications"
  mkdir -p "$DIR"
  cat > "$DIR/falcon-launcher.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Falcon Launcher
Comment=Falcon Engine と Blender を起動し、レンダリングを送る
Exec="$BIN"
Terminal=false
Categories=Graphics;3DGraphics;
EOF
  echo "登録しました。アプリのメニューから起動できます。"
fi

echo
echo "起動するには: $BIN"
echo "次に、ランチャーの「建てる準備」の頁で、Falcon Engine を建てます。"
