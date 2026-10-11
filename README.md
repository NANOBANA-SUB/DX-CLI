# dx

Windows向けDXLibプロジェクト管理CLIです。生成プロジェクトはCMake + Visual Studio 2022（MSVC、x64）を使います。

## 必要なもの

- Windows、Visual Studio 2022「C++によるデスクトップ開発」
- CMake 3.20以上、DXLib
- Rust（CLIをソースから導入する場合）

DXLibの展開先を設定します。

```powershell
[Environment]::SetEnvironmentVariable("DXLIB_ROOT", "C:\Libraries\DxLib", "User")
```

新しいターミナルで、CLIをインストールします。

```powershell
cargo install --path .
```

## 使い方

```powershell
dx new mygame --git
cd mygame
dx add class Player
dx run
```

- `dx new <name> [--git]`: プロジェクトを生成。`--git` 付きならGitも初期化
- `dx git init`: 現在のプロジェクトをGitリポジトリにする
- `dx add class <Name>`: ヘッダーと実装ファイルを新規生成
- `dx add file <path>`: 既存のC/C++ソースまたはヘッダーをコピーして追加
- `dx add include <directory>`: includeディレクトリをCMakeへ登録
- `dx build`: CMake configure + build
- `dx run`: ビルド後にexeを起動
- `dx clean`: buildディレクトリを削除

例:

```powershell
dx add class Player
# include/Player.h と src/Player.cpp

dx add class game/Enemy
# include/game/Enemy.h と src/game/Enemy.cpp

dx add file C:\work\Camera.cpp
# src/Camera.cpp へコピー

dx add file C:\work\Camera.h
# include/Camera.h へコピー

dx add include vendor\mylib\include
# CMakeLists.txtへ登録
```

`src/` と `include/` 以下のC/C++ファイルはCMakeが自動検出するため、ファイルを追加するたびにCMakeLists.txtを編集する必要はありません。既存のv0.1プロジェクトも、最初に `dx add class` または `dx add file` を実行すると自動検出形式へ更新されます。

`dx.toml`:

```toml
name = "mygame"
dxlib_root = ""
build_dir = "build"
configuration = "Debug"
```

`dxlib_root` が空でなければ環境変数より優先されます。`configuration` は `Release` に変更できます。
指定したDXLibルート直下には `プロジェクトに追加すべきファイル_VC用` が必要です。

## 開発

```powershell
cargo check
cargo test
cargo build --release
```
