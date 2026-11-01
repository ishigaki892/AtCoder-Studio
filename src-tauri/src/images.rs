// 問題文の画像（Phase 23。D-240）
//
// 問題文の img のうち https://img.atcoder.jp/ のものだけを、問題文の欄に表示するときに取得し、アプリのフォルダの
// problems/<コンテスト>/images/<ファイルの名前> に保存する。表示では、保存した画像を data: の URL（base64）にして
// 無害化の中で img に置き換える（CSP の img-src は 'self' data: のまま。D-085）。
// 中身の最初のバイトで PNG・JPEG・GIF・WebP と確かめたものだけを出す。1 枚 5 MB・1 問 20 枚まで。
// 取得は /usr/bin/curl・https だけ・リダイレクトはたどらない・Cookie は付けない・問題ページと同じ 1 秒の間隔
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::problem::ProblemId;
use crate::sanitize::{IMAGE_NOTE, ImageTag, ImageView, sanitize_with};

const HOST: &str = "https://img.atcoder.jp/";
// 1 枚の大きさの上限（curl の --max-filesize と、保存してあるものを読むとき）
const MAX_IMAGE_BYTES: u64 = 5_000_000;
const MAX_IMAGE_BYTES_ARG: &str = "5000000";
// 1 問で画像にする数の上限（それより後は IMAGE_NOTE）
const MAX_IMAGES: usize = 20;
// ファイルの名前（拡張子の前）の長さの上限
const MAX_NAME_CHARS: usize = 100;
const FETCH_TIMEOUT_SECS: &str = "20";

// 画像の所に出す文
pub const LOADING_NOTE: &str = "［画像を読み込んでいます…］";
pub const FAILED_NOTE: &str = "［画像を読み込めませんでした。公式ページで見てください］";

// 取得してよい画像：URL と、保存するファイルの名前
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageRef {
    pub url: String,
    pub name: String,
}

// src が https://img.atcoder.jp/… で、最後の部分が「英数字・_・-（100 文字まで）＋ .png／.jpg／.jpeg／.gif／.webp」の
// ときだけ取得する。? や # が付いたもの・途中に .. などがあるものは取得しない
pub fn image_ref(src: &str) -> Option<ImageRef> {
    let path = src.strip_prefix(HOST)?;
    if path.contains(['?', '#', '\\', '%'])
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return None;
    }
    let file = path.rsplit('/').next()?;
    let (stem, extension) = file.rsplit_once('.')?;
    let name_ok = !stem.is_empty()
        && stem.chars().count() <= MAX_NAME_CHARS
        && stem
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    let extension = extension.to_ascii_lowercase();
    if !name_ok || !["png", "jpg", "jpeg", "gif", "webp"].contains(&extension.as_str()) {
        return None;
    }
    Some(ImageRef {
        url: src.to_string(),
        name: format!("{stem}.{extension}"),
    })
}

// 中身の最初のバイトで決める形式（拡張子は信じない）。画像でなければ None
pub fn mime_of(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

// base64（標準の文字と = の詰め物。RFC 4648）
pub fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(TABLE[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

// 保存先：problems/<コンテスト>/images/（同じコンテストの問題で共有する）
fn image_dir(app_dir: &Path, id: &ProblemId) -> PathBuf {
    app_dir.join("problems").join(&id.contest).join("images")
}

// 保存してある画像を読む。大きすぎる・画像でないものは使わない
fn read_saved(dir: &Path, name: &str) -> Option<(&'static str, Vec<u8>)> {
    let path = dir.join(name);
    let metadata = std::fs::symlink_metadata(&path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_IMAGE_BYTES {
        return None;
    }
    let bytes = std::fs::read(&path).ok()?;
    Some((mime_of(&bytes)?, bytes))
}

// 問題文の中の、取得する画像（出てくる順。無害化と同じ順に数える。同じ画像は 1 回だけ）
pub fn planned(statement_html: &str) -> Vec<ImageRef> {
    let mut refs: Vec<ImageRef> = Vec::new();
    let mut count = 0;
    sanitize_with(statement_html, &mut |tag| {
        if let Some(image) = tag.src.as_deref().and_then(image_ref) {
            if count < MAX_IMAGES && !refs.iter().any(|r| r.name == image.name) {
                refs.push(image);
            }
            count += 1;
        }
        ImageView::Note(IMAGE_NOTE)
    });
    refs
}

// 問題文を無害化して、画像を置き換える。保存していない画像は missing の文にし、あったかを返す
pub fn render(
    app_dir: &Path,
    id: &ProblemId,
    statement_html: &str,
    missing: &'static str,
) -> (String, bool) {
    let dir = image_dir(app_dir, id);
    let mut count = 0;
    let mut pending = false;
    let html = sanitize_with(statement_html, &mut |tag: &ImageTag| {
        let Some(image) = tag.src.as_deref().and_then(image_ref) else {
            return ImageView::Note(IMAGE_NOTE);
        };
        count += 1;
        if count > MAX_IMAGES {
            return ImageView::Note(IMAGE_NOTE);
        }
        match read_saved(&dir, &image.name) {
            Some((mime, bytes)) => ImageView::Data {
                mime,
                base64: base64(&bytes),
            },
            None => {
                pending = true;
                ImageView::Note(missing)
            }
        }
    });
    (html, pending)
}

// 取得の失敗
#[derive(Debug, PartialEq, Eq)]
pub enum FetchFailure {
    // その画像だけの失敗（HTTP の誤り・大きすぎる・画像でない）。残りの画像は読む
    Image,
    // 通信の失敗（つながらない・時間切れなど）。その問題の残りの画像は読まない
    Network,
}

// まだ保存していない画像を取得して保存する。fetch は 1 枚を読む（テストでは差し替える）
pub fn fetch_missing(
    app_dir: &Path,
    id: &ProblemId,
    statement_html: &str,
    fetch: &mut dyn FnMut(&str) -> Result<Vec<u8>, FetchFailure>,
) -> Result<(), String> {
    let dir = image_dir(app_dir, id);
    let mut tried = HashSet::new();
    for image in planned(statement_html) {
        if read_saved(&dir, &image.name).is_some() || !tried.insert(image.name.clone()) {
            continue;
        }
        match fetch(&image.url) {
            Ok(bytes) => {
                if bytes.len() as u64 <= MAX_IMAGE_BYTES && mime_of(&bytes).is_some() {
                    crate::storage::write_atomic(&dir.join(&image.name), &bytes)?;
                }
            }
            Err(FetchFailure::Image) => {}
            Err(FetchFailure::Network) => break,
        }
    }
    Ok(())
}

// 1 枚を curl で読む。Cookie は付けず、リダイレクトはたどらない（3xx は画像でないので使わない）
pub fn fetch_image(url: &str) -> Result<Vec<u8>, FetchFailure> {
    if image_ref(url).is_none() {
        return Err(FetchFailure::Image);
    }
    crate::problem::wait_for_interval();
    let output = Command::new("/usr/bin/curl")
        .args([
            "--silent",
            "--fail",
            "--proto",
            "=https",
            "--max-time",
            FETCH_TIMEOUT_SECS,
            "--max-filesize",
            MAX_IMAGE_BYTES_ARG,
            url,
        ])
        .output()
        .map_err(|_| FetchFailure::Network)?;
    match output.status.code() {
        Some(0) => Ok(output.stdout),
        // 22：HTTP の誤り（--fail）。63：--max-filesize を超えた
        Some(22) | Some(63) => Err(FetchFailure::Image),
        _ => Err(FetchFailure::Network),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 1, 2, 3];

    struct TempDir(PathBuf);
    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "atcoder-studio-images-{name}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn id() -> ProblemId {
        ProblemId::new("abc481", "abc481_b").unwrap()
    }

    #[test]
    fn accepts_only_atcoder_image_urls() {
        let ok = image_ref("https://img.atcoder.jp/abc481/6b091bccaf9c134909377119703ded9f.PNG")
            .unwrap();
        assert_eq!(ok.name, "6b091bccaf9c134909377119703ded9f.png");
        for src in [
            "http://img.atcoder.jp/abc481/a.png",
            "https://img.atcoder.jp.example.com/a.png",
            "https://example.com/img.atcoder.jp/a.png",
            "/img/abc481/a.png",
            "https://img.atcoder.jp/abc481/a.svg",
            "https://img.atcoder.jp/abc481/a.png?x=1",
            "https://img.atcoder.jp/abc481/a.png#x",
            "https://img.atcoder.jp/abc481/../a.png",
            "https://img.atcoder.jp/abc481//a.png",
            "https://img.atcoder.jp/abc481/a%2e.png",
            "https://img.atcoder.jp/%2e%2e/a.png",
            "https://img.atcoder.jp/abc481/a b.png",
            "https://img.atcoder.jp/abc481/.png",
            "https://img.atcoder.jp/abc481/a.b.png",
        ] {
            assert_eq!(image_ref(src), None, "{src}");
        }
        let long = format!("https://img.atcoder.jp/x/{}.png", "a".repeat(101));
        assert_eq!(image_ref(&long), None);
        assert!(image_ref(&format!("https://img.atcoder.jp/x/{}.gif", "a".repeat(100))).is_some());
    }

    #[test]
    fn decides_the_format_by_content() {
        assert_eq!(mime_of(PNG), Some("image/png"));
        assert_eq!(mime_of(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
        assert_eq!(mime_of(b"GIF89a..."), Some("image/gif"));
        assert_eq!(mime_of(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
        assert_eq!(
            mime_of(b"<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>"),
            None
        );
        assert_eq!(mime_of(b"<html>"), None);
        assert_eq!(mime_of(b"RIFF\0\0\0\0WAVE"), None);
        assert_eq!(mime_of(&[]), None);
    }

    #[test]
    fn encodes_base64() {
        for (input, expected) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(input.as_bytes()), expected);
        }
        assert_eq!(base64(&[0xFB, 0xFF, 0xBF]), "+/+/");
    }

    #[test]
    fn fetches_missing_images_and_renders_them() {
        let dir = TempDir::new("fetch");
        let html = r#"<p>a<img src="https://img.atcoder.jp/abc481/a.png" width="300">b<img src="https://img.atcoder.jp/abc481/fake.png"><img src="https://example.com/c.png"><img src="https://img.atcoder.jp/abc481/a.png"></p>"#;
        // 保存していないうちは「読み込んでいます」
        let (out, pending) = render(&dir.0, &id(), html, LOADING_NOTE);
        assert!(pending);
        assert_eq!(out.matches(LOADING_NOTE).count(), 3);
        assert!(out.contains(IMAGE_NOTE));
        // 取得する（同じ画像は 1 回だけ。img.atcoder.jp 以外は読まない）。fake.png は中身が画像でない
        let mut asked = Vec::new();
        fetch_missing(&dir.0, &id(), html, &mut |url| {
            asked.push(url.to_string());
            Ok(if url.ends_with("a.png") {
                PNG.to_vec()
            } else {
                b"<html>".to_vec()
            })
        })
        .unwrap();
        assert_eq!(
            asked,
            [
                "https://img.atcoder.jp/abc481/a.png",
                "https://img.atcoder.jp/abc481/fake.png"
            ]
        );
        assert!(dir.0.join("problems/abc481/images/a.png").is_file());
        assert!(!dir.0.join("problems/abc481/images/fake.png").exists());
        let (out, pending) = render(&dir.0, &id(), html, FAILED_NOTE);
        assert!(pending);
        let data = format!("<img src=\"data:image/png;base64,{}\"", base64(PNG));
        assert_eq!(out.matches(&data).count(), 2);
        assert!(out.contains(&format!("{data} width=\"300\">")));
        assert_eq!(out.matches(FAILED_NOTE).count(), 1);
        assert!(!out.contains("img.atcoder.jp") && !out.contains("example.com"));
        // 保存してあるものは読み直さない
        let mut again = 0;
        fetch_missing(&dir.0, &id(), html, &mut |_| {
            again += 1;
            Err(FetchFailure::Image)
        })
        .unwrap();
        assert_eq!(again, 1);
        // 保存したファイルが画像でなくなっていたら使わない
        std::fs::write(dir.0.join("problems/abc481/images/a.png"), b"not an image").unwrap();
        assert!(!render(&dir.0, &id(), html, FAILED_NOTE).0.contains("data:"));
    }

    #[test]
    fn stops_at_network_failures_and_limits() {
        let dir = TempDir::new("limits");
        let html: String = (0..25)
            .map(|i| format!("<img src=\"https://img.atcoder.jp/x/i{i}.png\">"))
            .collect();
        // 20 枚まで
        assert_eq!(planned(&html).len(), 20);
        let mut asked = 0;
        fetch_missing(&dir.0, &id(), &html, &mut |_| {
            asked += 1;
            Ok(PNG.to_vec())
        })
        .unwrap();
        assert_eq!(asked, 20);
        let (out, pending) = render(&dir.0, &id(), &html, FAILED_NOTE);
        assert!(!pending);
        assert_eq!(out.matches("<img ").count(), 20);
        assert_eq!(out.matches(IMAGE_NOTE).count(), 5);
        // 大きすぎるものは保存しない
        let big_dir = TempDir::new("big");
        let mut big = PNG.to_vec();
        big.resize(MAX_IMAGE_BYTES as usize + 1, 0);
        fetch_missing(&big_dir.0, &id(), &html, &mut |_| Ok(big.clone())).unwrap();
        assert!(!big_dir.0.join("problems/abc481/images/i0.png").exists());
        // 通信の失敗のあとは読まない
        let net_dir = TempDir::new("network");
        let mut asked = 0;
        fetch_missing(&net_dir.0, &id(), &html, &mut |_| {
            asked += 1;
            Err(FetchFailure::Network)
        })
        .unwrap();
        assert_eq!(asked, 1);
    }

    #[test]
    fn does_not_fetch_other_urls() {
        assert_eq!(
            fetch_image("https://example.com/a.png"),
            Err(FetchFailure::Image)
        );
    }
}
