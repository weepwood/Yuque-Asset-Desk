"""Offline structural sanity checks only. Not a Rust compiler or a Slint parser."""
from pathlib import Path
root = Path(__file__).resolve().parent.parent
required = ["Cargo.toml", "build.rs", "src/main.rs", "src/storage.rs", "src/uploader.rs", "src/config.rs", "src/format.rs", "ui/app.slint", "assets/tray.png", ".github/workflows/build.yml", "README.md"]
for rel in required:
    p = root / rel
    assert p.is_file() and p.stat().st_size > 0, f"Missing {rel}"
ui = (root / "ui/app.slint").read_text()
code = (root / "src/main.rs").read_text()
for callback in ["pick-files", "copy-url", "copy-markdown", "hide-main", "show-help", "show-about", "search-changed", "open-main", "quit-app"]:
    assert f"callback {callback}" in ui, callback
for name in ["on_pick_files", "on_copy_url", "on_copy_markdown", "on_hide_main", "on_show_help", "on_show_about", "on_search_changed", "on_open_main", "on_quit_app"]:
    assert name in code, name
assert "webview" not in (root / "Cargo.toml").read_text().lower()
assert "sqlite" in (root / "README.md").read_text().lower()
assert "multipart::Part::file" in (root / "src/uploader.rs").read_text()
assert "UNIQUE(provider, owner_id, sha256)" in (root / "src/storage.rs").read_text()
print("OFFLINE STATIC CHECKS PASSED — Rust compilation still required")
