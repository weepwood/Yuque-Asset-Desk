from pathlib import Path
root = Path(__file__).resolve().parent.parent
files = ["Cargo.toml","src/main.rs","src/storage.rs","src/uploader.rs","src/config.rs","src/format.rs",".github/workflows/build.yml","README.md"]
for f in files:
    assert (root/f).is_file(), f
main="\n".join(p.read_text() for p in (root/"src").rglob("*.rs"))
cargo=(root/"Cargo.toml").read_text()
assert 'gpui_kit::*' in main
assert 'background_spawn' in main
assert 'gpui-kit = "=0.9.0"' in cargo
assert 'slint' not in cargo.lower()
assert 'UNIQUE(provider, owner_id, sha256)' in (root/"src/storage.rs").read_text()
print("GPUI Kit structure OK; cargo check mandatory")
