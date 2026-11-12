// 生成器の雛形（Phase 35 作業 1。D-259）
//
// 問題文の「入力」の形式と制約（input_gen.rs の read_shape）から、ランダムテストの生成器（examples/gen.*）を作る。
// - seed（1 つ目の引数）から、小さな入力を 1 つ作って標準出力に書く
// - 乱数：Rust は中に書いた xorshift（外部のクレートを使わない）、C++ は std::mt19937_64、Python は random.Random
// - 値の範囲はファイルの頭の定数：長さ・個数・ケースの数に使う変数は下限〜下限+7、ほかの整数は下限〜下限+9
//   （下限が負なら −10〜10。上限の数が小さければその数まで。上限が先に出てきた変数ならその値まで）、
//   実数は下限〜下限+10 の小数 2 桁、文字は制約の選択肢か英小文字 a〜c、文字列の長さは 1〜8
// - 「相異なる」などの制約は満たさず、頭のコメントに書く。クエリ・読めない行は、そこまで作ってコメントを残す
use std::collections::{HashMap, HashSet};

use crate::input_gen::{self, Field, Shape, ShapeItem, ShapeLen, ShapeValue, Upper};
use crate::language::Language;
use crate::problem::Problem;

// 名前ごとの範囲
#[derive(Debug, Clone, PartialEq, Eq)]
enum Range {
    // lo 以上、hi まで（hi は cap と var の小さいほう。どちらかはある）
    Int {
        lo: i128,
        cap: Option<i128>,
        var: Option<String>,
    },
    Real {
        lo: i128,
        hi: i128,
    },
    Chars(Vec<String>),
}

// 変数の名前に使えないもの（3 つの言語の予約語と、生成器の中の名前）
const RESERVED: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
    "ref", "return", "self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use",
    "where", "while", "abstract", "become", "box", "do", "final", "gen", "macro", "override",
    "priv", "try", "typeof", "unsized", "virtual", "yield", "and", "or", "not", "is", "def", "del",
    "elif", "except", "from", "global", "import", "lambda", "nonlocal", "pass", "raise", "with",
    "none", "int", "long", "double", "float", "char", "string", "auto", "bool", "case", "class",
    "default", "delete", "new", "this", "throw", "void", "switch", "union", "signed", "unsigned",
    "short", "rng", "rint", "rreal", "rpick", "rword", "main", "seed", "sys", "random", "print",
    "min", "max", "range", "std", "cout", "argc", "argv", "len", "out",
];

// 変数の名前（小文字。英数字と _ だけ。予約語なら末尾に _）
fn ident(name: &str) -> String {
    let mut text: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    if text.is_empty() || text.starts_with(|c: char| c.is_ascii_digit()) {
        text.insert(0, 'v');
    }
    if RESERVED.contains(&text.as_str()) {
        text.push('_');
    }
    text
}

fn quote(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

struct Writer<'a> {
    language: Language,
    shape: &'a Shape,
    // 長さ・個数・ケースの数に使う名前（制約で使う名前）
    counts: HashSet<String>,
    // 名前ごとの範囲と、定数の頭の名前（例：N）。順番は初めて出てきた順
    ranges: HashMap<String, Range>,
    order: Vec<String>,
    const_names: HashMap<String, String>,
    // 文字列の長さの定数を使う名前
    string_bases: HashSet<String>,
    // 1 つの値として作った変数（問題文のままの名前 → 変数の名前。制約で使う名前 → 変数の名前）
    scalars: HashMap<String, String>,
    scalar_bases: HashMap<String, String>,
    // 上限が値どうしの関係（A_i < B_i）で、満たしていない名前 → 相手の名前
    relations: HashMap<String, String>,
    body: Vec<String>,
    indent: usize,
}

impl<'a> Writer<'a> {
    fn new(language: Language, shape: &'a Shape) -> Self {
        let base = |name: &str| name.split('_').next().unwrap_or(name).to_string();
        let mut counts = HashSet::new();
        let mut add = |len: &ShapeLen| {
            if let Some(name) = &len.name {
                counts.insert(base(name));
            }
        };
        for item in &shape.items {
            match item {
                ShapeItem::Line(_) => {}
                ShapeItem::Row { len, .. } | ShapeItem::Column { len, .. } => add(len),
                ShapeItem::Grid { rows, cols, .. } => {
                    add(rows);
                    add(cols);
                }
            }
        }
        if let Some(cases) = &shape.cases {
            counts.insert(cases.base.clone());
        }
        Writer {
            language,
            shape,
            counts,
            ranges: HashMap::new(),
            order: Vec::new(),
            const_names: HashMap::new(),
            string_bases: HashSet::new(),
            scalars: HashMap::new(),
            scalar_bases: HashMap::new(),
            relations: HashMap::new(),
            body: Vec::new(),
            indent: 1,
        }
    }

    fn comment(&self) -> &'static str {
        match self.language {
            Language::Python => "#",
            Language::Rust | Language::Cpp => "//",
        }
    }

    fn line(&mut self, text: impl Into<String>) {
        let text = text.into();
        self.body
            .push(format!("{}{text}", "    ".repeat(self.indent)));
    }

    // 名前の範囲（初めて出てきたときに決める）
    fn range(&mut self, field: &Field) -> Range {
        if let Some(range) = self.ranges.get(&field.base) {
            return range.clone();
        }
        let bound = self
            .shape
            .bounds
            .get(&field.base)
            .cloned()
            .unwrap_or_default();
        let range = match field.value {
            ShapeValue::Int => {
                let (lo, mut cap) = if self.counts.contains(&field.base) {
                    let lo = bound.lower.unwrap_or(1).max(0);
                    (lo, lo + 7)
                } else {
                    match bound.lower {
                        Some(lower) if lower < 0 => (-10, 10),
                        Some(lower) => (lower, lower + 9),
                        None if self.shape.negative.contains(&field.base) => (-10, 10),
                        None => (1, 10),
                    }
                };
                let mut var = None;
                // 上限が、先に作った 1 つの値でない変数（同じ行の B_i など）なら、その変数の上限をたどって使う
                let mut upper = bound.upper.clone();
                for step in 0..5 {
                    match upper {
                        Some(Upper::Number(number)) => {
                            cap = cap.min(number.max(lo));
                            break;
                        }
                        Some(Upper::Var(name, offset)) => {
                            if let Some(variable) = self.scalar_bases.get(&name) {
                                var = Some(match offset {
                                    0 => variable.clone(),
                                    k if k > 0 => format!("{variable} + {k}"),
                                    k => format!("{variable} - {}", -k),
                                });
                                break;
                            }
                            if step == 0 {
                                self.relations.insert(field.base.clone(), name.clone());
                            }
                            upper = self.shape.bounds.get(&name).and_then(|b| b.upper.clone());
                        }
                        None => break,
                    }
                }
                // 上限が変数のとき：個数は小さいほう、ほかの値はその変数の値まで
                let cap = if var.is_some() && !self.counts.contains(&field.base) {
                    None
                } else {
                    Some(cap)
                };
                Range::Int { lo, cap, var }
            }
            ShapeValue::Real => {
                let lo = bound.lower.unwrap_or(0);
                let (lo, mut hi) = if lo < 0 { (-10, 10) } else { (lo, lo + 10) };
                if let Some(Upper::Number(upper)) = bound.upper {
                    hi = hi.min(upper.max(lo));
                }
                Range::Real { lo, hi }
            }
            ShapeValue::Char | ShapeValue::Str => Range::Chars(
                self.shape
                    .chars
                    .get(&field.base)
                    .cloned()
                    .unwrap_or_else(|| ["a", "b", "c"].map(str::to_string).to_vec()),
            ),
        };
        let mut head = ident(&field.base).to_ascii_uppercase();
        while self.const_names.values().any(|name| *name == head) {
            head.push('_');
        }
        self.const_names.insert(field.base.clone(), head);
        self.ranges.insert(field.base.clone(), range.clone());
        self.order.push(field.base.clone());
        range
    }

    fn min_expr(&self, a: &str, b: &str) -> String {
        match self.language {
            Language::Rust => format!("{a}.min({b})"),
            Language::Cpp | Language::Python => format!("min({a}, {b})"),
        }
    }

    // 1 つの値を作る式（Rust の実数は String、文字は &str。C++・Python の実数・文字は文字列）
    fn value(&mut self, field: &Field) -> String {
        let range = self.range(field);
        let head = self.const_names[&field.base].clone();
        let rust = self.language == Language::Rust;
        match (field.value, range) {
            (_, Range::Int { cap, var, .. }) => {
                let hi = match (cap, var) {
                    (Some(_), Some(var)) => self.min_expr(&format!("{head}_MAX"), &var),
                    (None, Some(var)) => var,
                    _ => format!("{head}_MAX"),
                };
                if rust {
                    format!("rng.int({head}_MIN, {hi})")
                } else {
                    format!("rint({head}_MIN, {hi})")
                }
            }
            (_, Range::Real { .. }) => {
                if rust {
                    format!("rng.real({head}_MIN, {head}_MAX)")
                } else {
                    format!("rreal({head}_MIN, {head}_MAX)")
                }
            }
            (ShapeValue::Str, Range::Chars(_)) => {
                self.string_bases.insert(field.base.clone());
                if rust {
                    format!("rng.word({head}_CHARS, {head}_LEN_MIN, {head}_LEN_MAX)")
                } else {
                    format!("rword({head}_CHARS, {head}_LEN_MIN, {head}_LEN_MAX)")
                }
            }
            (_, Range::Chars(_)) => {
                if rust {
                    format!("rng.pick({head}_CHARS)")
                } else {
                    format!("rpick({head}_CHARS)")
                }
            }
        }
    }

    // 長さの式。名前が、先に作った 1 つの値でなければ 3（コメントで知らせる）
    fn len(&mut self, len: &ShapeLen) -> String {
        let Some(name) = &len.name else {
            return len.offset.to_string();
        };
        let Some(variable) = self.scalars.get(name).cloned() else {
            let comment = self.comment();
            self.line(format!(
                "{comment} 長さ {name} を先に作れなかったので、3 にしています"
            ));
            return "3".to_string();
        };
        let term = if len.coef == 1 {
            variable
        } else {
            format!("{} * {variable}", len.coef)
        };
        match len.offset {
            0 => term,
            k if k > 0 => format!("{term} + {k}"),
            k => format!("{term} - {}", -k),
        }
    }

    // 行の出力（値は式か変数）
    fn print(&mut self, values: &[String]) {
        let text = match self.language {
            Language::Rust => format!(
                "println!(\"{}\", {});",
                vec!["{}"; values.len()].join(" "),
                values.join(", ")
            ),
            Language::Cpp => format!("cout << {} << '\\n';", values.join(" << ' ' << ")),
            Language::Python => format!("print({})", values.join(", ")),
        };
        self.line(text);
    }

    fn assign(&mut self, name: &str, expr: &str) {
        let text = match self.language {
            Language::Rust => format!("let {name} = {expr};"),
            Language::Cpp => format!("auto {name} = {expr};"),
            Language::Python => format!("{name} = {expr}"),
        };
        self.line(text);
    }

    // count 回くり返す（ブロックを開く）。閉じるのは close
    fn repeat(&mut self, count: &str, counter: &str) {
        let text = match self.language {
            Language::Rust => format!("for _ in 0..{} {{", parenthesize(count)),
            Language::Cpp => {
                format!("for (long long {counter} = 0; {counter} < {count}; {counter}++) {{")
            }
            Language::Python => format!("for _ in range({count}):"),
        };
        self.line(text);
        self.indent += 1;
    }

    fn close(&mut self) {
        self.indent -= 1;
        if self.language != Language::Python {
            self.line("}");
        }
    }

    // 1 行に並べる（横の列・格子の 1 段）
    fn print_row(&mut self, count: &str, expr: &str, counter: &str) {
        let text = match self.language {
            Language::Rust => format!(
                "println!(\"{{}}\", (0..{}).map(|_| {expr}.to_string()).collect::<Vec<_>>().join(\" \"));",
                parenthesize(count)
            ),
            Language::Cpp => format!(
                "for (long long {counter} = 0; {counter} < {count}; {counter}++) cout << ({counter} ? \" \" : \"\") << {expr};"
            ),
            Language::Python => format!("print(*[{expr} for _ in range({count})])"),
        };
        self.line(text);
        if self.language == Language::Cpp {
            self.line("cout << '\\n';");
        }
    }

    fn scalar(&mut self, field: &Field) -> String {
        let expr = self.value(field);
        let name = ident(&field.name);
        self.assign(&name, &expr);
        self.scalars.insert(field.name.clone(), name.clone());
        self.scalar_bases
            .entry(field.base.clone())
            .or_insert_with(|| name.clone());
        name
    }

    fn items(&mut self, items: &[ShapeItem]) {
        for item in items {
            match item {
                ShapeItem::Line(fields) => {
                    let names: Vec<String> =
                        fields.iter().map(|field| self.scalar(field)).collect();
                    self.print(&names);
                }
                ShapeItem::Row { field, len } => {
                    let count = self.len(len);
                    let expr = self.value(field);
                    self.print_row(&count, &expr, "_i");
                }
                ShapeItem::Column { fields, len } => {
                    let count = self.len(len);
                    self.repeat(&count, "_i");
                    let names: Vec<String> = fields
                        .iter()
                        .map(|field| {
                            let expr = self.value(field);
                            let name = ident(&field.name);
                            self.assign(&name, &expr);
                            name
                        })
                        .collect();
                    self.print(&names);
                    self.close();
                }
                ShapeItem::Grid {
                    field,
                    rows,
                    cols,
                    spaced,
                } => {
                    let rows = self.len(rows);
                    let cols = self.len(cols);
                    self.repeat(&rows, "_i");
                    if *spaced {
                        let expr = self.value(field);
                        self.print_row(&cols, &expr, "_j");
                    } else {
                        // 1 段を cols 文字の文字列にする
                        let chars = Field {
                            value: ShapeValue::Char,
                            ..field.clone()
                        };
                        self.range(&chars);
                        let head = self.const_names[&field.base].clone();
                        let expr = match self.language {
                            Language::Rust => format!("rng.word({head}_CHARS, {cols}, {cols})"),
                            Language::Cpp | Language::Python => {
                                format!("rword({head}_CHARS, {cols}, {cols})")
                            }
                        };
                        self.print(&[expr]);
                    }
                    self.close();
                }
            }
        }
    }

    // ファイルの頭の定数
    fn constants(&self) -> Vec<String> {
        let mut lines = Vec::new();
        for base in &self.order {
            let head = &self.const_names[base];
            let int = |name: String, value: i128| match self.language {
                Language::Rust => format!("const {name}: i64 = {value};"),
                Language::Cpp => format!("const long long {name} = {value};"),
                Language::Python => format!("{name} = {value}"),
            };
            let real = |name: String, value: i128| match self.language {
                Language::Rust => format!("const {name}: f64 = {value}.0;"),
                Language::Cpp => format!("const double {name} = {value}.0;"),
                Language::Python => format!("{name} = {value}.0"),
            };
            match &self.ranges[base] {
                Range::Int { lo, cap, var } => {
                    lines.push(int(format!("{head}_MIN"), *lo));
                    if let Some(cap) = cap {
                        lines.push(int(format!("{head}_MAX"), *cap));
                    }
                    if let Some(var) = var {
                        lines.push(format!(
                            "{} {base} の上限は {var}（制約による）",
                            self.comment()
                        ));
                    }
                    if let Some(other) = self.relations.get(base) {
                        lines.push(format!(
                            "{} {base} が {other} 以下（未満）という制約は満たしていません。直してください",
                            self.comment()
                        ));
                    }
                }
                Range::Real { lo, hi } => {
                    lines.push(real(format!("{head}_MIN"), *lo));
                    lines.push(real(format!("{head}_MAX"), *hi));
                }
                Range::Chars(chars) => {
                    let list = chars
                        .iter()
                        .map(|c| quote(c))
                        .collect::<Vec<_>>()
                        .join(", ");
                    lines.push(match self.language {
                        Language::Rust => format!("const {head}_CHARS: &[&str] = &[{list}];"),
                        Language::Cpp => format!("const vector<string> {head}_CHARS = {{{list}}};"),
                        Language::Python => format!("{head}_CHARS = [{list}]"),
                    });
                    if self.string_bases.contains(base) {
                        lines.push(int(format!("{head}_LEN_MIN"), 1));
                        lines.push(int(format!("{head}_LEN_MAX"), 8));
                    }
                }
            }
        }
        lines
    }
}

fn parenthesize(expr: &str) -> String {
    if expr.contains(' ') {
        format!("({expr})")
    } else {
        expr.to_string()
    }
}

// 頭のコメント
fn header(comment: &str, shape: Option<&Shape>, missing: Option<&str>) -> Vec<String> {
    let mut lines = vec![
        format!(
            "{comment} ランダムテストの生成器の雛形（AtCoder Studio が問題文の「入力」の形式と制約から作りました）"
        ),
        format!("{comment} - seed（1 つ目の引数）から小さな入力を 1 つ作り、標準出力に書きます"),
        format!(
            "{comment} - 値の範囲は下の定数です（制約より小さくしてあります）。問題に合わせて直してください"
        ),
    ];
    if let Some(reason) = missing {
        lines.push(format!("{comment} - {reason}"));
    }
    if let Some(shape) = shape
        && !shape.unmet.is_empty()
    {
        lines.push(comment.to_string());
        lines.push(format!(
            "{comment} 満たしていない制約（自分で直してください）："
        ));
        for line in &shape.unmet {
            lines.push(format!("{comment} - {line}"));
        }
    }
    lines
}

const RUST_HELPERS: &str = r#"struct Rng(u64);

#[allow(dead_code)]
impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    // xorshift64
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    // lo 以上 hi 以下の整数（hi が lo より小さければ lo）
    fn int(&mut self, lo: i64, hi: i64) -> i64 {
        if hi <= lo {
            return lo;
        }
        lo + (self.next() % (hi - lo + 1) as u64) as i64
    }

    // lo 以上 hi 以下の実数（小数 2 桁）
    fn real(&mut self, lo: f64, hi: f64) -> String {
        let steps = ((hi - lo) * 100.0).max(0.0) as i64;
        format!("{:.2}", lo + self.int(0, steps) as f64 / 100.0)
    }

    fn pick(&mut self, items: &[&'static str]) -> &'static str {
        items[(self.next() % items.len() as u64) as usize]
    }

    // 長さ lo 以上 hi 以下の文字列
    fn word(&mut self, chars: &[&'static str], lo: i64, hi: i64) -> String {
        let len = self.int(lo, hi);
        (0..len).map(|_| self.pick(chars)).collect()
    }
}
"#;

const CPP_HELPERS: &str = r#"mt19937_64 rng;

// lo 以上 hi 以下の整数（hi が lo より小さければ lo）
long long rint(long long lo, long long hi) {
    if (hi <= lo) return lo;
    return lo + (long long)(rng() % (unsigned long long)(hi - lo + 1));
}

// lo 以上 hi 以下の実数（小数 2 桁）
string rreal(double lo, double hi) {
    long long steps = max(0LL, (long long)((hi - lo) * 100));
    ostringstream out;
    out << fixed << setprecision(2) << lo + rint(0, steps) / 100.0;
    return out.str();
}

string rpick(const vector<string>& items) { return items[rng() % items.size()]; }

// 長さ lo 以上 hi 以下の文字列
string rword(const vector<string>& chars, long long lo, long long hi) {
    long long len = rint(lo, hi);
    string s;
    for (long long i = 0; i < len; i++) s += rpick(chars);
    return s;
}
"#;

const PYTHON_HELPERS: &str = r#"rng = random.Random(int(sys.argv[1]) if len(sys.argv) > 1 else 0)


def rint(lo, hi):
    """lo 以上 hi 以下の整数（hi が lo より小さければ lo）"""
    return lo if hi <= lo else rng.randint(lo, hi)


def rreal(lo, hi):
    """lo 以上 hi 以下の実数（小数 2 桁）"""
    return f"{lo + rint(0, max(0, int((hi - lo) * 100))) / 100:.2f}"


def rpick(items):
    return rng.choice(items)


def rword(chars, lo, hi):
    """長さ lo 以上 hi 以下の文字列"""
    return "".join(rng.choice(chars) for _ in range(rint(lo, hi)))
"#;

// 生成器のソースと、知らせ（読めなかったところ・満たしていない制約があるとき）
pub fn generate(problem: &Problem, language: Language) -> (String, Option<String>) {
    let shape = input_gen::read_shape(&problem.statement_html, &problem.samples);
    let comment = match language {
        Language::Python => "#",
        Language::Rust | Language::Cpp => "//",
    };
    let (header_lines, constants, body, note) = match &shape {
        Ok(shape) => {
            let mut writer = Writer::new(language, shape);
            if let Some(cases) = &shape.cases {
                let name = writer.scalar(cases);
                writer.print(std::slice::from_ref(&name));
                writer.repeat(&name, "_c");
                writer.items(&shape.items);
                if let Some(stop) = &shape.stop {
                    writer.line(format!(
                        "{comment} ここから先（{stop}）は形式を読めなかったので、作っていません。書き足してください"
                    ));
                }
                if language == Language::Python
                    && writer
                        .body
                        .last()
                        .is_some_and(|l| l.trim_end().ends_with(':'))
                {
                    writer.line("pass");
                }
                writer.close();
            } else {
                writer.items(&shape.items);
                if let Some(stop) = &shape.stop {
                    writer.line(format!(
                        "{comment} ここから先（{stop}）は形式を読めなかったので、作っていません。書き足してください"
                    ));
                }
            }
            if writer.body.is_empty() {
                writer.line(format!(
                    "{comment} 入力の形式を読めませんでした。ここで入力を書き出してください"
                ));
            }
            let mut notes = Vec::new();
            if let Some(stop) = &shape.stop {
                notes.push(format!("{stop}は作っていません"));
            }
            if !shape.unmet.is_empty() {
                notes.push(format!(
                    "満たしていない制約が {} 行あります（頭のコメント）",
                    shape.unmet.len()
                ));
            }
            (
                header(comment, Some(shape), None),
                writer.constants(),
                writer.body,
                (!notes.is_empty()).then(|| format!("生成器：{}", notes.join("。"))),
            )
        }
        Err(reason) => {
            let reason = format!("{reason}。入力を書き出すところは、自分で書いてください");
            (
                header(comment, None, Some(&reason)),
                Vec::new(),
                vec![format!("    {comment} ここで入力を書き出してください")],
                Some(format!("生成器：{reason}")),
            )
        }
    };
    let mut text = header_lines.join("\n");
    text.push('\n');
    let constants = if constants.is_empty() {
        String::new()
    } else {
        format!("\n{}\n", constants.join("\n"))
    };
    let body = body.join("\n");
    match language {
        Language::Rust => {
            text.push_str(&constants);
            text.push('\n');
            text.push_str(RUST_HELPERS);
            text.push_str(&format!(
                "\nfn main() {{\n    let seed: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);\n    #[allow(unused_mut, unused_variables)]\n    let mut rng = Rng::new(seed);\n{body}\n}}\n"
            ));
        }
        Language::Cpp => {
            text.push_str("#include <bits/stdc++.h>\nusing namespace std;\n");
            text.push_str(&constants);
            text.push('\n');
            text.push_str(CPP_HELPERS);
            text.push_str(&format!(
                "\nint main(int argc, char* argv[]) {{\n    rng.seed(argc > 1 ? strtoull(argv[1], nullptr, 10) : 0);\n{body}\n}}\n"
            ));
        }
        Language::Python => {
            text.push_str("import random\nimport sys\n");
            text.push_str(&constants);
            text.push('\n');
            text.push_str(PYTHON_HELPERS);
            text.push_str(&format!("\n\ndef main():\n{body}\n\n\nmain()\n"));
        }
    }
    (text, note)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::problem::Sample;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("atcoder-studio-gen-{name}-{}", std::process::id()));
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

    // 問題文（「制約」と「入力」の区画）
    fn statement(inputs: &[&str], constraints: &[&str]) -> String {
        let mut html = String::from("<div class=\"part\"><section><h3>制約</h3><ul>");
        for line in constraints {
            html.push_str(&format!("<li><var>{line}</var></li>"));
        }
        html.push_str("</ul></section></div><div class=\"part\"><section><h3>入力</h3>");
        for pre in inputs {
            html.push_str(&format!("<pre>{pre}</pre>"));
        }
        html.push_str("</section></div>");
        html
    }

    fn problem(html: &str, sample: &str) -> Problem {
        Problem {
            title: "A - 例".to_string(),
            time_limit: "2 sec".to_string(),
            memory_limit: "1024 MiB".to_string(),
            statement_html: html.to_string(),
            samples: vec![Sample {
                input: sample.to_string(),
                output: "0\n".to_string(),
            }],
        }
    }

    // 作った Rust の生成器をビルドして、seed 1〜count の出力を返す
    fn run_rust(dir: &Path, source: &str, count: u64) -> Vec<String> {
        let src = dir.join("gen.rs");
        std::fs::write(&src, source).unwrap();
        let exe = dir.join("gen");
        let output = Command::new("rustc")
            .args(["--edition", "2024", "-o"])
            .arg(&exe)
            .arg(&src)
            .current_dir(dir)
            .env("RUSTUP_AUTO_INSTALL", "0")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{source}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !String::from_utf8_lossy(&output.stderr).contains("warning"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        (1..=count)
            .map(|seed| {
                let output = Command::new(&exe).arg(seed.to_string()).output().unwrap();
                assert!(output.status.success());
                String::from_utf8(output.stdout).unwrap()
            })
            .collect()
    }

    // 出力が、入力の形式どおりか（入力のコードを作る処理と同じ確かめ方。型は生成器と同じく、文字の選択肢も使う）
    fn assert_matches_format(html: &str, input: &str) {
        assert!(input_gen::matches_shape(html, input), "{input}");
    }

    fn numbers(input: &str) -> Vec<i64> {
        input
            .split_whitespace()
            .map(|token| token.parse().unwrap())
            .collect()
    }

    #[test]
    fn generates_small_inputs_in_the_format() {
        let dir = TempDir::new("rust");
        // 横の列：N は 1〜8、A_i は 1〜10
        let html = statement(
            &["<var>N</var>\n<var>A_1</var> <var>A_2</var> <var>\\ldots</var> <var>A_N</var>"],
            &["1 \\leq N \\leq 2\\times 10^5", "1 \\leq A_i \\leq 10^9"],
        );
        let (source, note) = generate(&problem(&html, "3\n1 2 3\n"), Language::Rust);
        assert_eq!(note, None);
        assert!(source.contains("const N_MIN: i64 = 1;\nconst N_MAX: i64 = 8;"));
        assert!(source.contains("const A_MIN: i64 = 1;\nconst A_MAX: i64 = 10;"));
        for input in run_rust(&dir.0, &source, 30) {
            assert_matches_format(&html, &input);
            let values = numbers(&input);
            assert!((1..=8).contains(&values[0]), "{input}");
            assert!(values[1..].iter().all(|a| (1..=10).contains(a)), "{input}");
        }

        // 縦の組：上限が変数（B_i ≤ N）。連結は満たさないのでコメントに書く
        let html = statement(
            &[
                "<var>N</var> <var>M</var>\n<var>A_1</var> <var>B_1</var>\n<var>\\vdots</var>\n<var>A_M</var> <var>B_M</var>",
            ],
            &[
                "2 \\leq N \\leq 10",
                "1 \\leq M \\leq \\frac{N(N-1)}{2}",
                "1 \\leq A_i \\lt B_i \\leq N",
                "与えられるグラフは連結である",
            ],
        );
        let (source, note) = generate(&problem(&html, "3 2\n1 2\n2 3\n"), Language::Rust);
        assert!(note.unwrap().contains("満たしていない制約が 1 行"));
        assert!(source.contains(
            "// 満たしていない制約（自分で直してください）：\n// - 与えられるグラフは連結である"
        ));
        // N は長さに使わないので、ほかの整数と同じ（2〜11 を上限 10 で切る）。M は長さなので 1〜8
        assert!(
            source.contains("const N_MIN: i64 = 2;\nconst N_MAX: i64 = 10;"),
            "{source}"
        );
        assert!(
            source.contains("const M_MIN: i64 = 1;\nconst M_MAX: i64 = 8;"),
            "{source}"
        );
        // B_i の上限は N（変数）。A_i の上限は B_i（同じ行の値）なので、B_i の上限の N をたどって使い、
        // A_i < B_i は満たしていないとコメントに書く
        assert!(source.contains("const A_MIN: i64 = 1;\n// A の上限は n（制約による）\n// A が B 以下（未満）という制約は満たしていません。直してください"), "{source}");
        assert!(
            source.contains("const B_MIN: i64 = 1;\n// B の上限は n（制約による）"),
            "{source}"
        );
        for input in run_rust(&dir.0, &source, 30) {
            assert_matches_format(&html, &input);
            let values = numbers(&input);
            let n = values[0];
            assert!(values[2..].iter().all(|v| (1..=n).contains(v)), "{input}");
        }

        // 格子：入力例が行ごとの文字列なので、間なし。文字は制約の . と #
        let html = statement(
            &[
                "<var>H</var> <var>W</var>\n<var>S_{1,1} S_{1,2} \\cdots S_{1,W}</var>\n<var>\\vdots</var>\n<var>S_{H,1} S_{H,2} \\cdots S_{H,W}</var>",
            ],
            &["1 \\leq H,W \\leq 10", "S_{i,j} は . または # である"],
        );
        let (source, _) = generate(&problem(&html, "2 3\n.#.\n##.\n"), Language::Rust);
        assert!(
            source.contains("const S_CHARS: &[&str] = &[\".\", \"#\"];"),
            "{source}"
        );
        for input in run_rust(&dir.0, &source, 20) {
            assert_matches_format(&html, &input);
            let mut lines = input.lines();
            let hw = numbers(lines.next().unwrap());
            let rows: Vec<&str> = lines.collect();
            assert_eq!(rows.len() as i64, hw[0]);
            assert!(
                rows.iter()
                    .all(|row| row.len() as i64 == hw[1]
                        && row.chars().all(|c| c == '.' || c == '#')),
                "{input}"
            );
        }

        // 複数のテストケース
        let html = statement(
            &[
                "<var>T</var>\n<var>\\text{case}_1</var>\n<var>\\text{case}_2</var>\n<var>\\vdots</var>\n<var>\\text{case}_T</var>",
                "<var>N</var>\n<var>A_1</var> <var>A_2</var> <var>\\ldots</var> <var>A_N</var>",
            ],
            &[
                "1 \\leq T \\leq 10",
                "1 \\leq N \\leq 10",
                "-10^9 \\leq A_i \\leq 10^9",
            ],
        );
        let (source, _) = generate(&problem(&html, "2\n1\n5\n3\n1 2 3\n"), Language::Rust);
        assert!(source.contains("const A_MIN: i64 = -10;\nconst A_MAX: i64 = 10;"));
        for input in run_rust(&dir.0, &source, 20) {
            assert_matches_format(&html, &input);
        }

        // 文字列・実数・1 文字
        let html = statement(
            &["<var>S</var>\n<var>X</var> <var>C</var>"],
            &[
                "S は英小文字からなる文字列",
                "0 \\leq X \\leq 100 は実数",
                "C は L, R のいずれか",
            ],
        );
        let (source, _) = generate(&problem(&html, "abc\n1.5 L\n"), Language::Rust);
        assert!(
            source.contains("const C_CHARS: &[&str] = &[\"L\", \"R\"];"),
            "{source}"
        );
        for input in run_rust(&dir.0, &source, 20) {
            assert_matches_format(&html, &input);
            let lines: Vec<&str> = input.lines().collect();
            assert!(
                (1..=8).contains(&lines[0].len())
                    && lines[0].chars().all(|c| ('a'..='c').contains(&c)),
                "{input}"
            );
        }
    }

    #[test]
    fn stops_at_queries_and_unreadable_statements() {
        let dir = TempDir::new("stop");
        let html = statement(
            &[
                "<var>N</var> <var>Q</var>\n<var>A_1</var> <var>\\ldots</var> <var>A_N</var>\n<var>\\text{query}_1</var>\n<var>\\vdots</var>\n<var>\\text{query}_Q</var>",
            ],
            &["1 \\leq N, Q \\leq 10"],
        );
        let (source, note) = generate(&problem(&html, "1 1\n5\n1 1\n"), Language::Rust);
        assert!(source.contains("ここから先（クエリ（q 個））は形式を読めなかったので、作っていません。書き足してください"), "{source}");
        assert!(note.unwrap().contains("クエリ（q 個）は作っていません"));
        assert_eq!(run_rust(&dir.0, &source, 3).len(), 3);
        // 「入力」の形式がない：乱数と seed の読み取りだけの雛形
        let (source, note) = generate(&problem("<p>問題文</p>", "1\n"), Language::Rust);
        assert!(note.unwrap().contains("「入力」の形式が見つかりません"));
        assert!(source.contains("// ここで入力を書き出してください"));
        assert_eq!(run_rust(&dir.0, &source, 1), [""]);
    }

    // C++・Python でも同じ形の出力になる（g++-15・python3 がなければ確かめない）
    #[test]
    fn generates_cpp_and_python_too() {
        let dir = TempDir::new("cpp-python");
        let html = statement(
            &[
                "<var>T</var>\n<var>\\text{case}_1</var>\n<var>\\vdots</var>\n<var>\\text{case}_T</var>",
                "<var>H</var> <var>W</var> <var>X</var>\n<var>S_{1,1} S_{1,2} \\cdots S_{1,W}</var>\n<var>\\vdots</var>\n<var>S_{H,1} S_{H,2} \\cdots S_{H,W}</var>\n<var>A_1</var> <var>B_1</var>\n<var>\\vdots</var>\n<var>A_H</var> <var>B_H</var>",
            ],
            &[
                "1 \\leq T \\leq 5",
                "1 \\leq H,W \\leq 10",
                "0 \\leq X \\leq 3 は実数",
                "S_{i,j} は . または # である",
                "1 \\leq A_i \\leq W",
                "-5 \\leq B_i \\leq 5",
            ],
        );
        let sample = "1\n2 3 1.5\n.#.\n##.\n1 -2\n3 4\n";
        let check = |outputs: Vec<String>| {
            for input in outputs {
                assert_matches_format(&html, &input);
            }
        };
        if let Ok(compiler) = crate::cpp::find_compiler(&crate::cpp::CppSettings::default()) {
            let (source, _) = generate(&problem(&html, sample), Language::Cpp);
            let src = dir.0.join("gen.cpp");
            std::fs::write(&src, &source).unwrap();
            let exe = dir.0.join("gen-cpp");
            let output = Command::new(&compiler)
                .args(["-std=gnu++23", "-O2", "-Wall", "-Wextra", "-o"])
                .arg(&exe)
                .arg(&src)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{source}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                output.stderr.is_empty(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            check(
                (1..=20)
                    .map(|seed| {
                        String::from_utf8(
                            Command::new(&exe)
                                .arg(seed.to_string())
                                .output()
                                .unwrap()
                                .stdout,
                        )
                        .unwrap()
                    })
                    .collect(),
            );
        }
        let python = ["/usr/bin/python3", "/opt/homebrew/bin/python3"]
            .into_iter()
            .find(|path| Path::new(path).is_file());
        if let Some(python) = python {
            let (source, _) = generate(&problem(&html, sample), Language::Python);
            let src = dir.0.join("gen.py");
            std::fs::write(&src, &source).unwrap();
            check(
                (1..=20)
                    .map(|seed| {
                        let output = Command::new(python)
                            .arg("-I")
                            .arg(&src)
                            .arg(seed.to_string())
                            .output()
                            .unwrap();
                        assert!(
                            output.status.success(),
                            "{}\n{source}",
                            String::from_utf8_lossy(&output.stderr)
                        );
                        String::from_utf8(output.stdout).unwrap()
                    })
                    .collect(),
            );
        }
        // Rust でも同じ
        let (source, _) = generate(&problem(&html, sample), Language::Rust);
        check(run_rust(&dir.0, &source, 20));
    }
}
