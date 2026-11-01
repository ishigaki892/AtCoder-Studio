# AtCoder Studio

AtCoder の問題を Rust・C++・Python で解くための macOS 向けデスクトップ IDE です。問題の取得からサンプルテスト、デバッグ、提出までをひとつのウィンドウで完結できます。

## 主な機能

- **問題の準備**：コンテスト ID を入力するだけで、全問題のフォルダ（サンプル・雛形入り）をまとめて作成できます。問題文は数式・画像付きでウィンドウ内に表示されます。また、問題文の入力形式から入力を読み込むコード（Rust は proconio の `input!`、C++ は `cin`、Python は `input()`）を自動生成することも可能です。
- **テスト**：サンプルテストの判定（AC・WA・RE・TLE・MLE・OLE・CE）に加え、生成器と愚直解を用いたランダムテストや、インタラクティブ問題の対話テストを行えます。
- **エディタ**：CodeMirror 6 をベースに、rust-analyzer・clangd・basedpyright による補完・診断・定義ジャンプに対応しています（コード整形は Rust と C++ のみ）。言語ごとのスニペット登録も可能です。
- **デバッグ**：ブレークポイント、ステップ実行、変数・ウォッチ式の表示に対応しています。Rust と C++ は Apple の `lldb-dap`、Python は `debugpy` を使用します。Rust の panic、C++ の例外や `abort`、Python の未処理例外で自動的に停止します。
- **提出**：開催中のコンテストであれば、IDE から直接提出して結果の判定をリアルタイムで追跡できます。ログインは IDE 内の専用ウィンドウで行うため、パスワードが IDE に直接渡ることはありません。サイドバーの「コンテスト」から、問題ごとの AC 状況に加えて、順位・予想パフォーマンス・各問題の予想 diff（いずれも近似値）も確認できます。また、ツールバーにコンテストの残り時間を表示し、ジャッジ結果の確定やコンテストの開始を macOS の通知でお知らせすることもできます（設定で有効にします）。

対応言語および提出時の対応環境は以下の通りです。

| 言語 | 手元の環境 | 提出時の言語 |
|---|---|---|
| Rust | rustup の Rust | 「Rust」で始まるもの |
| C++ | Homebrew の GCC（`g++-15`） | C++23 (GCC 15.2.0) |
| Python | CPython 3.13 または PyPy 3.11 | Python (CPython 3.13.7) / Python (PyPy 3.11-v7.3.20) |

> [!IMPORTANT]
> 本アプリは AtCoder の非公式ツールです。AtCoder 株式会社とは一切関係ありません。
>
> - 提出機能の利用は自己責任で行ってください。故意または重大な過失がある場合を除き、製作者は責任を負いません。
> - コンテストに参加する際は、生成 AI の利用に関するルールを含め、該当コンテストの規程を必ず遵守してください。

## 動作環境・必要なもの

動作確認環境：macOS 26.6.2（Apple Silicon）  
使用する言語に合わせて、事前に以下の環境をご用意ください。

- **Rust**：[rustup](https://rustup.rs) でインストールした Rust。ジャッジ環境と同じ rustc 1.89.0 を rustup の既定に設定しておくと、手元とジャッジの差異を抑えられます（現在の既定は `rustup toolchain list` の `(active, default)` 行で確認できます）。
- **C++**：Homebrew の GCC（`brew install gcc` で導入する `g++-15`）。Apple 製の clang は `#include <bits/stdc++.h>` に対応していないため使用できません。
  - AC Library を利用する場合は、ジャッジと同じバージョン 1.6 を [atcoder/ac-library](https://github.com/atcoder/ac-library) からダウンロードし、アプリフォルダ内の `libraries/ac-library/atcoder/` に配置してください（詳細は [C++](docs/usage.md#c) を参照）。
- **Python**：Homebrew の Python（`brew install python@3.13`。PyPy の場合は `brew install pypy3.11`）。コード補完等には basedpyright（`brew install basedpyright`）が必要です。デバッグに使用する `debugpy` は、初めてデバッグを実行する際に IDE がインストール方法を案内します。
- **デバッグおよび C++ の補完**：Xcode Command Line Tools（`lldb-dap` および `clangd` が含まれています）。未導入の場合は、IDE が `xcode-select --install` の実行を案内します。

## インストール方法

[Releases](https://github.com/ishigaki892/acd_studio/releases) から `.dmg` ファイルをダウンロードするのが最も手軽です。`.dmg` は Apple Silicon 用のみ提供されているため、Intel Mac の場合は[ソースからビルド](#ソースからビルドする場合)してください。

### .dmg からインストールする場合

1. Releases から `AtCoder.Studio_<バージョン>_aarch64.dmg` をダウンロードして開きます。
2. 表示されたウィンドウで、AtCoder Studio を「Applications」フォルダにドラッグ＆ドロップします。
3. Finder の「アプリケーション」から AtCoder Studio を起動します。

本アプリは Developer ID による正式な署名や Apple の公証を受けておらず（アドホック署名のみ）、初回起動時に macOS によってブロックされることがあります。その場合は「完了」をクリックした後、「システム設定」>「プライバシーとセキュリティ」を開き、下部にある「このまま開く」をクリックして再度起動してください。

または、ターミナルで次のコマンドを実行して quarantine 属性を削除してから開くことも可能です。

```sh
xattr -dr com.apple.quarantine "/Applications/AtCoder Studio.app"
```

**アップデート時の手順**：AtCoder Studio を終了（`⌘Q`）し、`/Applications/AtCoder Studio.app` を削除した上で、新しい `.dmg` を用いて手順 2〜3 を行ってください。設定・履歴・保存済み問題のデータは別フォルダに保持されるため、そのまま引き継がれます（[データの場所](docs/usage.md#データの場所)）。

### ソースからビルドする場合

ビルドには Node.js（`.nvmrc` で指定されたバージョン）、npm、および Rust が必要です。

1. リポジトリのルートディレクトリでビルドを実行します（約 3 分ほどかかります）。

   ```sh
   nvm use
   npm install
   npm run tauri build
   ```

2. 開発用アプリ（`npm run tauri dev`）を起動している場合は停止してください（同一の設定を使用するため同時起動はできません）。
3. ビルド成果物のアプリを `/Applications` にコピーします。

   ```sh
   cp -R "src-tauri/target/release/bundle/macos/AtCoder Studio.app" /Applications/
   ```

4. Finder の「アプリケーション」から AtCoder Studio を起動します。

新しいソースコードで再ビルドした際は、AtCoder Studio を終了して `/Applications/AtCoder Studio.app` を削除してから、手順 3 のコピーをやり直してください。

## 使い方

1. **フォルダを開く**：ファイル一覧の「フォルダを開く」（`⌘O`）から、問題を保存するディレクトリを選択します。初回起動時は「このフォルダを信頼しますか」と確認されるため、ご自身で作成したフォルダであれば「信頼する」を選択してください（信頼されていないフォルダではコードの実行操作が行えません）。
2. **コンテストの作成**：ファイル一覧の上の「⤓」ボタンを押し、コンテスト ID（例：`abc480`）を入力して「作る」をクリックします。`abc480/abc480_a/` のような問題別フォルダが生成され、A 問題のファイルが開くと同時に右側に問題文が表示されます。
   - C++ や Python で取り組む場合は、事前に設定画面（`⌘,`）の「問題文」タブで「問題を作るときの言語」を切り替えておいてください。開かれるファイルは、Rust なら `src/main.rs`、C++ なら `main.cpp`、Python なら `main.py` となります。
3. **解答とテスト**：コードを記述したら「サンプルテスト」（`⇧⌘R`）を実行します。下部の「テスト結果」タブに、テストケースごとの判定結果が表示されます。
4. **コードの提出**：開催中のコンテストであれば、右下の「提出結果」からログインし「提出」ボタンを押します。確認ダイアログで「提出する」を選択すると、提出後にジャッジ結果がリアルタイムで更新されます。コンテスト期間外の問題では、代わりに「コードをコピーして公式の提出ページを開く」機能を利用できます。

各機能の詳細な使い方は [docs/usage.md](docs/usage.md) をご覧ください。

## 開発について

- 公開リポジトリには、開発用のドキュメント（設計の記録・作業ログ・開発手順をまとめた `CLAUDE.md`）は含まれていません。`docs/` 配下に配置されているのは使い方ガイド（[docs/usage.md](docs/usage.md)）のみであるため、コード内のコメント等で言及されている他の `docs/` 内ファイルは一般には公開されていません。
- 開発用の主なコマンド：
  - `npm run tauri dev`（開発モードでの起動）
  - `npm run build`（フロントエンドの型チェックとビルド）
  - `src-tauri/` ディレクトリでの `cargo test`、`cargo clippy --all-targets`、`cargo fmt`
- Windows 環境には対応していません。
- 本ツールの開発には Claude Code が使用されています。

## ライセンス

[MIT ライセンス](LICENSE) です。依存ライブラリはそれぞれのライセンスに準拠します。配布アプリに含まれるライブラリ一覧およびライセンス全文は `THIRD_PARTY_LICENSES.txt` に記載されています（Releases の `.dmg` と共に対象ファイルも同梱されています）。
