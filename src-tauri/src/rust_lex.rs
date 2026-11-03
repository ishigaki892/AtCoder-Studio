// Rust のソースの、簡単な字句の読み取り（Phase 28 作業 1。D-250。Phase 29 の書き換えでも使う）。
//
// 名前（キーワードを含む）・記号（1 文字ずつ）・リテラル・ライフタイムに分け、位置（バイト）を付けて返す。
// コメント・空白は返さない。文字列・生の文字列（r"…"・r#"…"#）・バイト列・C 文字列・文字（'a'・'\n'・'あ'）と
// ライフタイム（'a）を区別し、入れ子のブロックコメント（/* /* */ */）も飛ばす。
// 目的は「文字列・コメントの中を数えない」ことなので、数の細かい形などは正しく読まなくてよい

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    // 名前・キーワード（生の名前 r#type は「type」）
    Ident(String),
    // 記号（1 文字）
    Punct(char),
    // 文字列・文字・数などのリテラル
    Literal,
    // ライフタイム・ラベル（'a）
    Lifetime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: Kind,
    pub start: usize,
    pub end: usize,
}

impl Token {
    pub fn is_ident(&self, name: &str) -> bool {
        matches!(&self.kind, Kind::Ident(ident) if ident == name)
    }

    pub fn is_punct(&self, c: char) -> bool {
        self.kind == Kind::Punct(c)
    }
}

fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_alphabetic()
}

fn is_ident_continue(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

struct Lexer<'a> {
    text: &'a str,
    pos: usize,
}

impl Lexer<'_> {
    fn peek(&self, ahead: usize) -> Option<char> {
        self.text[self.pos..].chars().nth(ahead)
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.text[self.pos..].chars().next()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    // 閉じる引用符まで読む（\ のあとの 1 文字は飛ばす）。開く引用符は読んだあと
    fn quoted(&mut self, quote: char) {
        while let Some(c) = self.bump() {
            if c == '\\' {
                self.bump();
            } else if c == quote {
                return;
            }
        }
    }

    // 生の文字列（r の次から）：# の数を数え、" … "### まで読む
    fn raw_string(&mut self) {
        let mut hashes = 0;
        while self.peek(0) == Some('#') {
            self.bump();
            hashes += 1;
        }
        if self.peek(0) != Some('"') {
            return;
        }
        self.bump();
        let closing = format!("\"{}", "#".repeat(hashes));
        match self.text[self.pos..].find(&closing) {
            Some(at) => self.pos += at + closing.len(),
            None => self.pos = self.text.len(),
        }
    }

    fn block_comment(&mut self) {
        // /* は読んだあと
        let mut depth = 1;
        while depth > 0 {
            let rest = &self.text[self.pos..];
            if rest.is_empty() {
                return;
            }
            if rest.starts_with("/*") {
                depth += 1;
                self.pos += 2;
            } else if rest.starts_with("*/") {
                depth -= 1;
                self.pos += 2;
            } else {
                self.bump();
            }
        }
    }
}

pub fn tokens(text: &str) -> Vec<Token> {
    let mut lexer = Lexer { text, pos: 0 };
    let mut result = Vec::new();
    while let Some(c) = lexer.peek(0) {
        let start = lexer.pos;
        if c.is_whitespace() {
            lexer.bump();
            continue;
        }
        let rest = &text[start..];
        if rest.starts_with("//") {
            lexer.pos += rest.find('\n').unwrap_or(rest.len());
            continue;
        }
        if rest.starts_with("/*") {
            lexer.pos += 2;
            lexer.block_comment();
            continue;
        }
        let kind = if c == '"' {
            lexer.bump();
            lexer.quoted('"');
            Kind::Literal
        } else if c == '\'' {
            lexer.bump();
            // 文字（'a'・'\n'・'あ'）か、ライフタイム（'a）か
            if lexer.peek(0) == Some('\\') {
                lexer.quoted('\'');
                Kind::Literal
            } else if lexer.peek(1) == Some('\'') {
                lexer.bump();
                lexer.bump();
                Kind::Literal
            } else {
                while lexer.peek(0).is_some_and(is_ident_continue) {
                    lexer.bump();
                }
                Kind::Lifetime
            }
        } else if is_ident_start(c) {
            while lexer.peek(0).is_some_and(is_ident_continue) {
                lexer.bump();
            }
            let word = &text[start..lexer.pos];
            let next = lexer.peek(0);
            match (word, next) {
                // 生の文字列・バイト列・C 文字列（r"…"・r#"…"#・br"…"・cr"…"）
                ("r" | "br" | "cr", Some('"')) => {
                    lexer.raw_string();
                    Kind::Literal
                }
                ("r" | "br" | "cr", Some('#')) if lexer.peek(1) != Some('#') && word == "r" => {
                    // r#名前（生の名前）か r#"…"#（生の文字列）か
                    if lexer.peek(1) == Some('"') {
                        lexer.raw_string();
                        Kind::Literal
                    } else {
                        lexer.bump();
                        let name_start = lexer.pos;
                        while lexer.peek(0).is_some_and(is_ident_continue) {
                            lexer.bump();
                        }
                        Kind::Ident(text[name_start..lexer.pos].to_string())
                    }
                }
                ("r" | "br" | "cr", Some('#')) => {
                    lexer.raw_string();
                    Kind::Literal
                }
                // バイト列・C 文字列・バイト（b"…"・c"…"・b'x'）
                ("b" | "c", Some('"')) => {
                    lexer.bump();
                    lexer.quoted('"');
                    Kind::Literal
                }
                ("b", Some('\'')) => {
                    lexer.bump();
                    lexer.quoted('\'');
                    Kind::Literal
                }
                _ => Kind::Ident(word.to_string()),
            }
        } else if c.is_ascii_digit() {
            while lexer.peek(0).is_some_and(|c| {
                is_ident_continue(c)
                    || (c == '.' && lexer.peek(1).is_some_and(|d| d.is_ascii_digit()))
            }) {
                lexer.bump();
            }
            Kind::Literal
        } else {
            lexer.bump();
            Kind::Punct(c)
        };
        result.push(Token {
            kind,
            start,
            end: lexer.pos,
        });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idents(text: &str) -> Vec<String> {
        tokens(text)
            .into_iter()
            .filter_map(|token| match token.kind {
                Kind::Ident(name) => Some(name),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn skips_strings_comments_and_chars() {
        let text = r####"
            // mod hidden_a;
            /* mod hidden_b; /* mod hidden_c; */ mod hidden_d; */
            let s = "mod hidden_e; \" mod hidden_f;";
            let r = r#"mod hidden_g; " mod hidden_h;"#;
            let bytes = b"mod hidden_i;"; let br = br##"mod hidden_j;"##; let cs = c"mod hidden_k;";
            let ch = '"'; let q = '\''; let jp = 'あ'; let bc = b'"';
            fn f<'a>(x: &'a str) -> &'a str { 'outer: loop { break 'outer; } x }
            mod real;
            let r#type = 1;
        "####;
        let names = idents(text);
        for hidden in [
            "hidden_a", "hidden_b", "hidden_c", "hidden_d", "hidden_e", "hidden_f", "hidden_g",
            "hidden_h", "hidden_i", "hidden_j", "hidden_k",
        ] {
            assert!(
                !names.iter().any(|name| name == hidden),
                "{hidden}: {names:?}"
            );
        }
        assert!(names.contains(&"real".to_string()));
        assert!(names.contains(&"type".to_string()));
        assert!(names.contains(&"str".to_string()));
        let lifetimes = tokens(text)
            .iter()
            .filter(|token| token.kind == Kind::Lifetime)
            .count();
        assert_eq!(lifetimes, 5);
    }

    #[test]
    fn keeps_positions_and_ranges() {
        let text = "mod x; for i in 0..10 { 1.5; }";
        let tokens = tokens(text);
        assert_eq!(&text[tokens[0].start..tokens[0].end], "mod");
        assert!(tokens[2].is_punct(';'));
        // 0..10 の .. を数の一部にしない
        let dots = tokens.iter().filter(|token| token.is_punct('.')).count();
        assert_eq!(dots, 2);
        assert!(
            tokens
                .iter()
                .any(|token| &text[token.start..token.end] == "1.5")
        );
    }
}
