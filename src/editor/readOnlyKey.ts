// 読み取り専用のタブ（作業領域の外の標準ライブラリ・依存クレートのソース。Phase 3b-2 作業 5。D-133）のキー。
// 作業領域のファイルのタブのキーは作業領域のルートからの相対パスで、読み取り専用のタブのキーは
// rust-analyzer が返した file: の URI。相対パスが file: で始まることはないので、これで見分ける
export function isReadOnlyKey(key: string): boolean {
  return key.startsWith("file://");
}

// アプリのフォルダのファイル（settings.json・言語のスニペットのファイル）のタブ（D-247）のキー。`app://` のあとにファイルの名前。
// 作業領域の相対パスに `//` は入らないので、作業領域のファイルと取り違えない。読み書きは Rust 側の read_app_file など
const APP_PREFIX = "app://";

export function isAppKey(key: string): boolean {
  return key.startsWith(APP_PREFIX);
}

export function appKey(name: string): string {
  return APP_PREFIX + name;
}

export function appName(key: string): string {
  return key.slice(APP_PREFIX.length);
}
