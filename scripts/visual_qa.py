"""Capture real GPUI-rendered frames; does not capture desktop or use real accounts."""
import hashlib
import os
from pathlib import Path
import sqlite3
import subprocess
import tempfile
import struct
import json

root = Path(__file__).resolve().parent.parent
out = root / "dist" / "qa"
out.mkdir(parents=True, exist_ok=True)
binary = root / "target" / os.getenv("YUQUE_VISUAL_PROFILE","debug") / ("yuque-asset-desk.exe" if os.name == "nt" else "yuque-asset-desk")
def fixture_png(index):
    """Original synthetic visual assets: landscape, editorial, UI and abstract artwork.
    Not photos, not user data, no network."""
    from PIL import Image, ImageDraw
    import io, math
    w,h=[(480,320),(320,480),(560,240),(400,400)][index%4]
    palettes=[("#e9c9a8","#476c76","#203b45"),("#d5e1dd","#87a89b","#244941"),("#e2c9b9","#b76b57","#613c46"),("#cbd4ea","#546b99","#26344b")]
    a,b,c=palettes[index%4]
    im=Image.new("RGB",(w,h),a);d=ImageDraw.Draw(im)
    if index%3==0:
        d.ellipse((w*.65,h*.08,w*.85,h*.08+w*.2),fill="#f7e7bd")
        for level,col in enumerate([b,c]):
            y=h*(.45+level*.24)
            pts=[(0,h),(0,y)]+[(x,y+math.sin(x/w*9+index+level)*h*.14) for x in range(0,w+1,8)]+[(w,h)]
            d.polygon(pts,fill=col)
        d.line((0,h*.87,w,h*.87),fill="#efdbbd",width=3)
    elif index%3==1:
        d.rounded_rectangle((w*.13,h*.1,w*.87,h*.9),radius=14,fill=c)
        d.rectangle((w*.2,h*.19,w*.8,h*.21),fill=a)
        d.ellipse((w*.27,h*.28,w*.73,h*.28+w*.46),fill=b)
        for j in range(4):
            d.rectangle((w*.22,h*(.68+j*.035),w*(.79-j*.1),h*(.69+j*.035)),fill=a)
    else:
        d.rounded_rectangle((w*.06,h*.1,w*.94,h*.9),radius=12,fill="#faf9f7")
        d.rectangle((w*.07,h*.11,w*.23,h*.89),fill=c)
        for j in range(5):
            y=h*(.21+j*.13)
            d.rounded_rectangle((w*.29,y,w*.88,y+h*.09),radius=4,fill=b if j%2 else a)
        d.rectangle((w*.3,h*.13,w*.62,h*.16),fill=c)
    d.rectangle((0,0,2,2),fill=(index,0,0))
    buf=io.BytesIO();im.save(buf,format="PNG");return buf.getvalue()

with tempfile.TemporaryDirectory(prefix="yuque-visual-") as folder:
    data = Path(folder)
    env = {k:v for k,v in os.environ.items() if not k.startswith("YUQUE_")}
    env.update(YUQUE_DATA_DIR=str(data), YUQUE_MODE="demo")

    selected = set(os.environ.get("YUQUE_VISUAL_ONLY","").split(",")) - {""}
    def capture(name, **extra):
        if selected and name not in selected and name not in {"empty-dark.png","resources-dark.png","persisted-preferences.png"}:
            return
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
    capture("preview-quality-dark.png",YUQUE_QA_PAGE="quality")
    capture("preview-quality-light.png",YUQUE_QA_PAGE="quality",YUQUE_QA_LIGHT="1")
    capture("preview-quality-narrow.png",YUQUE_QA_PAGE="quality",YUQUE_QA_NARROW="1")
    capture("preview-minimum.png",YUQUE_QA_PAGE="quality",YUQUE_QA_MINIMUM="1")
    for path in data.glob("*.png"):
        path.unlink()
    capture("album-offline-dark.png")
    capture("compact-dark.png",YUQUE_QA_PAGE="compact")
    capture("compact-light.png",YUQUE_QA_PAGE="compact",YUQUE_QA_LIGHT="1")
    capture("compact-masonry.png",YUQUE_QA_PAGE="compact-masonry")
    capture("resources-light.png",YUQUE_QA_LIGHT="1")
    capture("resources-narrow.png",YUQUE_QA_NARROW="1")
    capture("resources-minimum.png",YUQUE_QA_MINIMUM="1")
    capture("settings-minimum.png",YUQUE_QA_PAGE="settings",YUQUE_QA_MINIMUM="1")
    capture("rail-dark.png",YUQUE_QA_PAGE="rail")
    capture("rail-light.png",YUQUE_QA_PAGE="rail",YUQUE_QA_LIGHT="1")
    capture("album-immersive.png",YUQUE_QA_PAGE="immersive")
    capture("album-masonry-dark.png",YUQUE_QA_PAGE="masonry")
    capture("album-masonry-light.png",YUQUE_QA_PAGE="masonry",YUQUE_QA_LIGHT="1")
    capture("album-masonry-narrow.png",YUQUE_QA_PAGE="masonry",YUQUE_QA_NARROW="1")
    capture("album-justified.png",YUQUE_QA_PAGE="justified")
    capture("album-dense.png",YUQUE_QA_PAGE="dense",YUQUE_QA_NARROW="1")
    capture("browse-context-dark.png",YUQUE_QA_PAGE="browse-context")
    capture("browse-context-minimum.png",YUQUE_QA_PAGE="browse-context",YUQUE_QA_MINIMUM="1",YUQUE_QA_LIGHT="1")
    capture("browse-empty-minimum.png",YUQUE_QA_PAGE="browse-empty",YUQUE_QA_MINIMUM="1")
    capture("album-favorites.png",YUQUE_QA_PAGE="favorites",YUQUE_QA_LIGHT="1")
    capture("album-hover.png",YUQUE_QA_PAGE="hover")
    capture("detail-dark.png",YUQUE_QA_PAGE="detail")
    capture("detail-light.png",YUQUE_QA_PAGE="detail",YUQUE_QA_LIGHT="1")
    capture("detail-narrow.png",YUQUE_QA_PAGE="detail",YUQUE_QA_NARROW="1")
    capture("preview-dark.png",YUQUE_QA_PAGE="preview")
    capture("preview-offline-minimum.png",YUQUE_QA_PAGE="quality",YUQUE_QA_MINIMUM="1")
    capture("notification-dark.png",YUQUE_QA_PAGE="notice")
    capture("settings-narrow.png",YUQUE_QA_PAGE="settings",YUQUE_QA_NARROW="1")
    capture("updates-dark.png",YUQUE_QA_PAGE="updates")
    capture("updates-light.png",YUQUE_QA_PAGE="updates",YUQUE_QA_LIGHT="1")
    capture("data-dark.png",YUQUE_QA_PAGE="data")
    capture("data-light.png",YUQUE_QA_PAGE="data",YUQUE_QA_LIGHT="1")
    capture("data-narrow.png",YUQUE_QA_PAGE="data",YUQUE_QA_NARROW="1")
    capture("account-dark.png",YUQUE_QA_PAGE="account")
    capture("account-light.png",YUQUE_QA_PAGE="account-identified",YUQUE_QA_LIGHT="1")
    capture("account-narrow.png",YUQUE_QA_PAGE="account",YUQUE_QA_NARROW="1")
    capture("settings-dark.png",YUQUE_QA_PAGE="settings")
    capture("settings-light.png",YUQUE_QA_PAGE="settings",YUQUE_QA_LIGHT="1")
    capture("tasks-dark.png",YUQUE_QA_PAGE="tasks")
    capture("tags-light.png",YUQUE_QA_LIGHT="1",YUQUE_QA_PAGE="tags")
    assert (out/"empty-dark.png").read_bytes() != (out/"resources-dark.png").read_bytes(), "populated frame did not change"
    with sqlite3.connect(data/"assets.db") as conn:
        conn.execute("INSERT INTO desk_preferences(id,value) VALUES(1,?) ON CONFLICT(id) DO UPDATE SET value=excluded.value",
            (json.dumps({"dark":False,"layout":"masonry","tile":196,"oldest":True,"favorites":True,"dates":False,"width":920,"height":680,"shortcut":"I","comfortable":False}),))
    capture("persisted-preferences.png")
    with (out/"persisted-preferences.png").open("rb") as f:
        header=f.read(24)
        w,h=struct.unpack(">II",header[16:24])
        assert abs(w/h-920/680)<0.04, f"restored window aspect ratio mismatch: {w}x{h}"
