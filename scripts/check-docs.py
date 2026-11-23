"""Check repository-local Markdown links without Rust, Node or network access."""
import re
import sys
from pathlib import Path
from urllib.parse import unquote

root = Path(__file__).resolve().parents[1]
files = [root / "README.md", root / "AGENTS.md", root / "web/README.md",
         *sorted((root / "docs").rglob("*.md"))]
failures = []
for file in files:
    text = file.read_text(encoding="utf-8")
    if file.parent == root / "docs" / "phases" and re.match(r"r\d\d-", file.name):
        if not re.search(r"^## Manual test\s*$", text, flags=re.MULTILINE):
            failures.append(f"{file.relative_to(root)} -> missing Manual test procedure")
    for target in re.findall(r"\[[^\]]*\]\(([^)]+)\)", text):
        if re.match(r"[a-zA-Z][a-zA-Z0-9+.-]*:", target) or target.startswith("#"):
            continue
        path = unquote(target.split("#", 1)[0])
        if not (file.parent / path).exists():
            failures.append(f"{file.relative_to(root)} -> {target}")
if failures:
    print("\n".join(failures), file=sys.stderr)
    sys.exit(1)
print(f"Local links passed in {len(files)} Markdown files")
