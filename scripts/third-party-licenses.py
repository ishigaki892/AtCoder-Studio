# 配るアプリに入る依存（Rust の normal の依存と、npm の本番の依存）のライセンスの文を 1 つのファイルにまとめる（D-228）
# 使い方（最上位で）：python3 -I scripts/third-party-licenses.py . THIRD_PARTY_LICENSES.txt
import json, os, re, subprocess, sys
from pathlib import Path

repo = Path(sys.argv[1]).resolve()
out = Path(sys.argv[2])
home = str(Path.home())
env = dict(os.environ, RUSTUP_AUTO_INSTALL="0")
LIC = re.compile(r"^(LICEN[CS]E|COPYING|NOTICE|UNLICENSE)", re.I)

def license_files(d):
    files = sorted(p for p in d.iterdir() if p.is_file() and LIC.match(p.name))
    return files

meta = json.loads(subprocess.run(
    ["cargo", "metadata", "--offline", "--format-version", "1", "--filter-platform", "aarch64-apple-darwin"],
    cwd=repo / "src-tauri", env=env, capture_output=True, text=True, check=True).stdout)
tree = subprocess.run(
    ["cargo", "tree", "--offline", "-e", "normal", "--target", "aarch64-apple-darwin", "--prefix", "none", "--format", "{p}"],
    cwd=repo / "src-tauri", env=env, capture_output=True, text=True, check=True).stdout
wanted = set()
for line in tree.splitlines():
    m = re.match(r"(\S+) v(\S+)", line)
    if m:
        wanted.add((m.group(1), m.group(2)))
own = {p["name"] for p in meta["packages"] if p["source"] is None}
entries, missing = [], []
for p in sorted(meta["packages"], key=lambda p: (p["name"], p["version"])):
    if (p["name"], p["version"]) not in wanted or p["name"] in own:
        continue
    d = Path(p["manifest_path"]).parent
    files = license_files(d)
    if not files:
        missing.append(f'{p["name"]} {p["version"]}')
    entries.append((f'{p["name"]} {p["version"]}（Rust）', p.get("license") or p.get("license_file") or "", p.get("repository") or "", files, p.get("authors") or []))

npm = subprocess.run(["npm", "ls", "--omit=dev", "--all", "--parseable"], cwd=repo, capture_output=True, text=True).stdout
seen = set()
# npm は、パスの一部を *** に伏せることがあるので、最初の行（最上位）からの相対で場所を決める
npm_lines = npm.splitlines()
for line in npm_lines[1:]:
    d = repo / Path(line).relative_to(npm_lines[0])
    pj = d / "package.json"
    if not pj.exists():
        continue
    j = json.loads(pj.read_text())
    key = (j["name"], j["version"])
    if key in seen:
        continue
    seen.add(key)
    repo_url = j.get("repository", "")
    if isinstance(repo_url, dict):
        repo_url = repo_url.get("url", "")
    files = license_files(d)
    if not files:
        missing.append(f'{j["name"]} {j["version"]}')
    lic = j.get("license", "")
    if isinstance(lic, dict):
        lic = lic.get("type", "")
    author = j.get("author", "")
    if isinstance(author, dict):
        author = author.get("name", "")
    entries.append((f'{j["name"]} {j["version"]}（npm）', lic, repo_url, files, [author] if author else []))

parts = ["AtCoder Studio が使っている、ほかの人が作ったソフトウェアのライセンス\n"
         "Third-party software licenses for AtCoder Studio\n\n"
         "AtCoder Studio 自身のライセンスは LICENSE（MIT）を見てください。\n"
         "This file lists the Rust crates and npm packages included in the distributed app, with their license texts.\n"]
MIT = """MIT License

Copyright (c) {who}

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
"""
by_name = {name.split(" ")[0]: files for name, _, _, files, _ in entries}
mpl = next((f for name, lic, _, files, _ in entries if lic == "MPL-2.0" for f in files), None)
for name, lic, url, files, authors in entries:
    parts.append("=" * 78 + f"\n{name}\nLicense: {lic}\n" + (f"Repository: {url}\n" if url else "")
                 + (f"Authors: {', '.join(authors)}\n" if authors else ""))
    if not files:
        # パッケージにライセンスのファイルがないもの：表記のライセンスの全文を付ける
        who = ", ".join(authors) or "the " + name.split(" ")[0] + " authors"
        # 「MIT OR Apache-2.0」のほか、古い書き方の「MIT/Apache-2.0」も読む（mac-notification-sys。Phase 22）
        if "MIT" in lic.replace("/", " ").split(" "):
            parts.append("(パッケージにライセンスのファイルがないので、表記のうち MIT の全文を付けます)\n\n" + MIT.format(who=who))
        elif lic == "MPL-2.0" and mpl:
            parts.append("(パッケージにライセンスのファイルがないので、MPL-2.0 の全文を付けます)\n\n" + mpl.read_text().rstrip() + "\n")
        elif name.startswith("alloc-stdlib") and by_name.get("alloc-no-stdlib"):
            parts.append("(パッケージにライセンスのファイルがないので、同じ作者・同じリポジトリの alloc-no-stdlib の LICENSE を付けます)\n\n"
                         + by_name["alloc-no-stdlib"][0].read_text().rstrip() + "\n")
        else:
            sys.exit(f"no license text for {name} ({lic})")
    for f in files:
        text = f.read_text(errors="replace").replace(home, "~")
        parts.append("-" * 78 + f"\n{f.name}\n\n{text.rstrip()}\n")
out.write_text("\n".join(parts) + "\n")
print(f"{len(entries)} packages; without license files: {len(missing)}")
for m in missing:
    print("  ", m)
