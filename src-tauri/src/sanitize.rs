// 問題文の HTML の無害化（Phase 3a 作業 2。D-118）
//
// 許可リスト方式：決まった要素だけを残し、属性はすべて捨てる（リンク・スタイル・イベントも残さない）。
// script などの危ない要素は中身ごと捨て、それ以外の知らない要素（span・a など）は、
// 要素を外して中の文字だけを残す。画像（img）は、呼び出し側が決めたもの（IDE が保存した画像の data: の URL か、決まった文）に
// 置き換える（Phase 23。D-240）。文字は HTML として読めないように置き換える。
// 出力は IDE がこの規則で組み立て直したものなので、元の HTML の書き方の違いに左右されにくい（推測）

use dom_query::{Document, NodeRef};

// 残す要素（属性は付けない）
const ALLOWED: &[&str] = &[
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "p",
    "pre",
    "var",
    "code",
    "ul",
    "ol",
    "li",
    "table",
    "thead",
    "tbody",
    "tr",
    "th",
    "td",
    "strong",
    "em",
    "b",
    "i",
    "sup",
    "sub",
    "section",
    "div",
    "blockquote",
];

// 中身を持たない要素
const VOID: &[&str] = &["br", "hr"];

// 中身ごと捨てる要素
const DROPPED: &[&str] = &[
    "script", "style", "iframe", "frame", "frameset", "object", "embed", "noscript", "template",
    "svg", "math", "form", "input", "button", "select", "textarea", "link", "meta", "title",
    "head", "base", "audio", "video", "canvas",
];

// 画像の代わりに出す文（Phase 7b 作業 2）。IDE が取得しない画像（img.atcoder.jp 以外など。D-240）は、あったことだけを知らせる
pub const IMAGE_NOTE: &str = "［画像があります。公式ページで見てください］";

// 問題文の img の、元の属性のうち使うもの（D-240。ほかの属性は見ない）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageTag {
    pub src: Option<String>,
    pub alt: Option<String>,
    pub width: Option<String>,
}

// img をどう出すか
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageView {
    // IDE が確かめた画像。mime は IDE の決まった値（image/png など）、base64 は IDE が変換したもの
    Data { mime: &'static str, base64: String },
    // 文で知らせる
    Note(&'static str),
}

// alt の長さの上限と、width の範囲
const MAX_ALT_CHARS: usize = 200;
const MAX_WIDTH: u32 = 2000;

// width は 1〜2000 の整数（数字だけ）のときだけ使う
fn checked_width(width: Option<&str>) -> Option<u32> {
    let width = width?.trim();
    if width.is_empty() || width.len() > 4 || !width.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    width.parse().ok().filter(|w| (1..=MAX_WIDTH).contains(w))
}

fn write_image(tag: &ImageTag, view: ImageView, out: &mut String) {
    match view {
        ImageView::Note(note) => escape(note, out),
        ImageView::Data { mime, base64 } => {
            out.push_str("<img src=\"data:");
            out.push_str(mime);
            out.push_str(";base64,");
            out.push_str(&base64);
            out.push('"');
            if let Some(alt) = &tag.alt {
                let alt: String = alt.chars().take(MAX_ALT_CHARS).collect();
                out.push_str(" alt=\"");
                escape(&alt, out);
                out.push('"');
            }
            if let Some(width) = checked_width(tag.width.as_deref()) {
                out.push_str(&format!(" width=\"{width}\""));
            }
            out.push('>');
        }
    }
}

// 入れ子の深さの上限（それより深いところは捨てる）
const MAX_DEPTH: usize = 128;

fn escape(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
}

fn walk(
    node: &NodeRef,
    depth: usize,
    image: &mut dyn FnMut(&ImageTag) -> ImageView,
    out: &mut String,
) {
    if depth > MAX_DEPTH {
        return;
    }
    for child in node.children() {
        if child.is_text() {
            escape(child.text().as_ref(), out);
        } else if child.is_element() {
            let name = child
                .node_name()
                .map(|name| name.to_ascii_lowercase())
                .unwrap_or_default();
            if DROPPED.contains(&name.as_str()) {
                continue;
            }
            if name == "img" {
                let attr = |key: &str| child.attr(key).map(|value| value.to_string());
                let tag = ImageTag {
                    src: attr("src"),
                    alt: attr("alt"),
                    width: attr("width"),
                };
                let view = image(&tag);
                write_image(&tag, view, out);
                continue;
            }
            if VOID.contains(&name.as_str()) {
                out.push('<');
                out.push_str(&name);
                out.push('>');
            } else if ALLOWED.contains(&name.as_str()) {
                out.push('<');
                out.push_str(&name);
                out.push('>');
                walk(&child, depth + 1, image, out);
                out.push_str("</");
                out.push_str(&name);
                out.push('>');
            } else {
                // 知らない要素は外して、中だけを残す
                walk(&child, depth + 1, image, out);
            }
        }
        // コメントなどは捨てる
    }
}

// HTML の断片を無害化する。画像は image で決める（出てくる順に呼ぶ）
pub fn sanitize_with(fragment: &str, image: &mut dyn FnMut(&ImageTag) -> ImageView) -> String {
    let document = Document::fragment(fragment);
    let mut out = String::new();
    walk(&document.root(), 0, image, &mut out);
    out
}

// 画像をすべて文にして無害化する
#[cfg(test)]
pub fn sanitize(fragment: &str) -> String {
    sanitize_with(fragment, &mut |_| ImageView::Note(IMAGE_NOTE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_allowed_elements_without_attributes() {
        let html = r#"<div class="part"><section><h3>問題文</h3><p style="color:red" onclick="x()"><var>N</var> 個</p><pre id="pre-sample0">1 2
3</pre></section></div>"#;
        assert_eq!(
            sanitize(html),
            "<div><section><h3>問題文</h3><p><var>N</var> 個</p><pre>1 2\n3</pre></section></div>"
        );
    }

    #[test]
    fn drops_dangerous_elements_with_content() {
        let html = r#"<p>a<script>alert(1)</script>b<style>p{}</style>c<iframe src="x"></iframe><svg><script>1</script></svg></p>"#;
        assert_eq!(sanitize(html), "<p>abc</p>");
    }

    #[test]
    fn unwraps_unknown_elements_and_drops_links_and_images() {
        let html = r#"<p><a href="javascript:alert(1)">リンク</a><img src="x" onerror="alert(1)"><span class="lang-ja">日本語</span></p>"#;
        // 画像は読み込まず、あったことを文で知らせる（Phase 7b 作業 2）
        assert_eq!(sanitize(html), format!("<p>リンク{IMAGE_NOTE}日本語</p>"));
    }

    #[test]
    fn escapes_text() {
        // 文字として書かれた「<」などは、要素にならない
        let html = "<p>1 &lt; N &amp;&amp; &lt;script&gt;</p>";
        assert_eq!(sanitize(html), "<p>1 &lt; N &amp;&amp; &lt;script&gt;</p>");
    }

    #[test]
    fn drops_comments_and_keeps_void_elements() {
        assert_eq!(
            sanitize("<p>a<!-- x -->b<br>c</p><hr>"),
            "<p>ab<br>c</p><hr>"
        );
    }

    // Phase 7b 作業 1（AC7b-03）：外を読みに行く・動く・形を変える要素は、中身ごと捨てるか、外して文字だけにする
    #[test]
    fn drops_other_active_elements() {
        let html = r#"<p>a<object data="x"><param name="m" value="y">o</object>b<embed src="x">c<form action="https://example.com">フォーム<input name="q"><button>送る</button></form>d<base href="https://example.com/"><meta http-equiv="refresh" content="0;url=https://example.com"><link rel="stylesheet" href="https://example.com/x.css">e<math><mi>x</mi></math>f<textarea>&lt;img src=x onerror=alert(1)&gt;</textarea>g</p>"#;
        // HTML の読み方の決まりで、p の中の form は p を閉じる（残る文字と要素は危なくない）
        assert_eq!(sanitize(html), "<p>abc</p>defg<p></p>");
    }

    #[test]
    fn does_not_rebuild_markup_from_text() {
        // noscript・title・template の中身を、作り直すときに要素として読み直さない（中身ごと捨てる。文字は置き換える）
        let html = r#"<noscript><p title="</noscript><img src=x onerror=alert(1)>"></p></noscript><p>&lt;img src=x onerror=alert(1)&gt;</p><title><img src=x onerror=alert(2)></title><div><template>x<img src=x onerror=alert(3)></template></div>"#;
        let out = sanitize(html);
        assert!(!out.contains("<img"), "{out}");
        assert!(!out.contains("onerror=alert(1)>"), "{out}");
        assert!(
            out.contains("<p>&lt;img src=x onerror=alert(1)&gt;</p>"),
            "{out}"
        );
        // noscript・template は中身ごと捨てる（文字としても残さない）
        assert!(!out.contains("title="), "{out}");
        assert!(!out.contains("alert(3)"), "{out}");
    }

    // Phase 23（D-240）：画像は呼び出し側が決めたものだけを出し、元の属性は alt・width を確かめてから付ける
    #[test]
    fn writes_only_checked_image_attributes() {
        let html = r#"<p><img src="https://img.atcoder.jp/x/a.png" alt="図 &quot;1&quot; <b>" width="300" style="max-width: 60%" onerror="alert(1)" class="c"><img src="x" width="300px"><img src="y" width="2001"><img src="z" width="0"></p>"#;
        let mut seen = Vec::new();
        let out = sanitize_with(html, &mut |tag| {
            seen.push(tag.clone());
            ImageView::Data {
                mime: "image/png",
                base64: "QUJD".to_string(),
            }
        });
        assert_eq!(
            out,
            "<p><img src=\"data:image/png;base64,QUJD\" alt=\"図 &quot;1&quot; &lt;b&gt;\" width=\"300\"><img src=\"data:image/png;base64,QUJD\"><img src=\"data:image/png;base64,QUJD\"><img src=\"data:image/png;base64,QUJD\"></p>"
        );
        assert_eq!(seen.len(), 4);
        assert_eq!(
            seen[0].src.as_deref(),
            Some("https://img.atcoder.jp/x/a.png")
        );
        assert_eq!(seen[0].width.as_deref(), Some("300"));
        // alt は 200 文字まで
        let long = format!("<img alt=\"{}\">", "あ".repeat(300));
        let out = sanitize_with(&long, &mut |_| ImageView::Data {
            mime: "image/gif",
            base64: String::new(),
        });
        assert_eq!(out.matches('あ').count(), 200);
        // 捨てる要素の中の画像は、呼び出し側に聞かない（数えない）
        let mut count = 0;
        sanitize_with(
            "<noscript><img src=a></noscript><template><img src=b></template><p><img src=c></p>",
            &mut |_| {
                count += 1;
                ImageView::Note(IMAGE_NOTE)
            },
        );
        assert_eq!(count, 1);
    }

    #[test]
    fn stops_at_deep_nesting() {
        let html = "<div>".repeat(1000) + "x" + &"</div>".repeat(1000);
        let out = sanitize(&html);
        assert!(!out.contains('x'));
    }
}
