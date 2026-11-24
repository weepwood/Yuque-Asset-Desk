"""Capture real GPUI-rendered frames; does not capture desktop or use real accounts."""
import hashlib
import os
from pathlib import Path
import sqlite3
import subprocess
import tempfile
import struct
import zlib
import json

root = Path(__file__).resolve().parent.parent
out = root / "dist" / "qa"
out.mkdir(parents=True, exist_ok=True)
binary = root / "target" / os.getenv("YUQUE_VISUAL_PROFILE","debug") / ("yuque-asset-desk.exe" if os.name == "nt" else "yuque-asset-desk")
def fixture_png(index):
    """Deterministic RGB test pattern; not user image content."""
    w,h=[(320,240),(210,320),(440,180),(320,320)][index%4]
    colors=[(40,88,150),(130,84,112),(52,120,95),(180,123,62),(76,90,140),(125,80,45)]
    base=colors[index%len(colors)]
    def pixel(x,y):
        if x==0 and y==0:
            return (index,0,0)
        if 18<x<302 and 20<y<220 and ((x//40+y//40+index)%3==0):
            return tuple(min(255,c+70) for c in base)
        return tuple(min(255,c+(x+y)//12) for c in base)
    def chunk(kind,body):
        return struct.pack(">I",len(body))+kind+body+struct.pack(">I",zlib.crc32(kind+body)&0xffffffff)
    raw=b"".join(b"\0"+bytes(c for x in range(w) for c in pixel(x,y)) for y in range(h))
    return b"\x89PNG\r\n\x1a\n"+chunk(b"IHDR",struct.pack(">IIBBBBB",w,h,8,2,0,0,0))+chunk(b"IDAT",zlib.compress(raw))+chunk(b"IEND",b"")

with tempfile.TemporaryDirectory(prefix="yuque-visual-") as folder:
    data = Path(folder)
    env = {k:v for k,v in os.environ.items() if not k.startswith("YUQUE_")}
    env.update(YUQUE_DATA_DIR=str(data), YUQUE_MODE="demo")

    def capture(name, **extra):
        path = out / name
        path.unlink(missing_ok=True)
        result = subprocess.run([str(binary)], env={**env, **extra, "YUQUE_QA_CAPTURE": str(path)}, timeout=15, capture_output=True, text=True)
        assert result.returncode == 0 and "QA_CAPTURE Ok(())" in result.stderr and path.stat().st_size > 1000, result.stderr
        print(f"{name}: {path.stat().st_size} bytes", flush=True)

    capture("empty-dark.png")
    with sqlite3.connect(data/"assets.db") as conn:
        for i,name in enumerate(["产品界面参考.png","摄影素材.png","工作台设计说明.md","演示视频.mp4","图标资源.zip","交互状态记录.png","配色研究.png","应用图标.png","活动海报.png","界面细节.png","首页设计.png","组件参考.png","演示封面.png","素材参考.png"]):
            category = "image" if name.endswith((".png",".webp")) else "video" if name.endswith(".mp4") else "attachment"
            path = data/name
            path.write_bytes(fixture_png(i) if category=="image" else f"fixture-{i}".encode())
            conn.execute("INSERT INTO assets(name,bytes,category,provider,owner_id,url,sha256) VALUES(?,?,?,?,?,?,?)", (name,path.stat().st_size,category,"demo","local",path.as_uri(),hashlib.sha256(path.read_bytes()).hexdigest()))
            if i<3:
                conn.execute("INSERT INTO asset_tags VALUES(?,?)",(i+1,"设计/UI" if i<2 else "设计/文档"))
        conn.execute("UPDATE assets SET created_at='2026-11-23T08:00:00Z' WHERE id<=6")
        conn.execute("INSERT INTO asset_favorites(asset_id) VALUES(1),(8)")
        for i,state in enumerate(["done","failed","pending","paused","unknown"]):
            conn.execute("INSERT INTO upload_jobs(provider,owner_id,path,state,error) VALUES(?,?,?,?,?)",
                ("demo","local",str(data/f"fixture-task-{i}.png"),state,"演示错误：文件不可读取" if state=="failed" else "演示状态：需核实远端" if state=="unknown" else ""))
    capture("resources-dark.png")
    previews=list((data/"album-previews").glob("*.jpg"))
    assert len(previews)==11, f"expected 11 offline previews, got {len(previews)}"
    for path in data.glob("*.png"):
        path.unlink()
    capture("album-offline-dark.png")
    capture("resources-light.png",YUQUE_QA_LIGHT="1")
    capture("resources-narrow.png",YUQUE_QA_NARROW="1")
    capture("album-masonry-dark.png",YUQUE_QA_PAGE="masonry")
    capture("album-masonry-light.png",YUQUE_QA_PAGE="masonry",YUQUE_QA_LIGHT="1")
    capture("album-masonry-narrow.png",YUQUE_QA_PAGE="masonry",YUQUE_QA_NARROW="1")
    capture("album-justified.png",YUQUE_QA_PAGE="justified")
    capture("album-dense.png",YUQUE_QA_PAGE="dense",YUQUE_QA_NARROW="1")
    capture("album-favorites.png",YUQUE_QA_PAGE="favorites",YUQUE_QA_LIGHT="1")
    capture("tasks-dark.png",YUQUE_QA_PAGE="tasks")
    capture("tags-light.png",YUQUE_QA_LIGHT="1",YUQUE_QA_PAGE="tags")
    assert (out/"empty-dark.png").read_bytes() != (out/"resources-dark.png").read_bytes(), "populated frame did not change"
    with sqlite3.connect(data/"assets.db") as conn:
        conn.execute("INSERT INTO desk_preferences(id,value) VALUES(1,?) ON CONFLICT(id) DO UPDATE SET value=excluded.value",
            (json.dumps({"dark":False,"layout":"masonry","tile":196,"oldest":True,"favorites":True,"dates":False,"width":920,"height":680,"shortcut":"I"}),))
    capture("persisted-preferences.png")
    with (out/"persisted-preferences.png").open("rb") as f:
        header=f.read(24)
        w,h=struct.unpack(">II",header[16:24])
        assert abs(w/h-920/680)<0.04, f"restored window aspect ratio mismatch: {w}x{h}"
