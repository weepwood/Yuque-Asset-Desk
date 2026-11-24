"""Isolated real encoded JPEG benchmark; no user images or network.
Reports CPU-side dispatch/draw duration, not hardware presentation FPS."""
import hashlib, json, os, pathlib, sqlite3, subprocess, tempfile, time, threading
from PIL import Image, ImageDraw
root=pathlib.Path(__file__).resolve().parent.parent
binary=root/"target/debug/yuque-asset-desk"
out=root/"dist/qa";out.mkdir(parents=True,exist_ok=True)
with tempfile.TemporaryDirectory(prefix="yuque-perf-") as folder:
    data=pathlib.Path(folder)
    env={k:v for k,v in os.environ.items() if not k.startswith("YUQUE_")}
    env.update(YUQUE_DATA_DIR=str(data),YUQUE_MODE="demo",YUQUE_DESKTOP_INTEGRATION="0")
    subprocess.run([str(binary)],env={**env,"YUQUE_QA_CAPTURE":str(out/"perf-empty.png")},timeout=20,check=True,capture_output=True)
    cache=data/"album-previews";cache.mkdir()
    start=time.monotonic()
    with sqlite3.connect(data/"assets.db") as c:
        for i in range(10000):
            w,h=[(160,100),(100,160),(160,160),(160,65),(75,160)][i%5]
            im=Image.new("RGB",(w,h),((i*17)%256,(i*31)%256,(i*53)%256))
            d=ImageDraw.Draw(im);d.rectangle((4,4,w-5,h-5),outline=(240,240,240),width=2);d.text((8,12),str(i),fill=(255,255,255))
            key=hashlib.sha256(f"perf-fixture-{i}".encode()).hexdigest()
            path=cache/(hashlib.sha256(f"v2:demo:local:{key}".encode()).hexdigest()+".jpg")
            im.save(path,quality=45)
            c.execute("INSERT INTO assets(name,bytes,category,provider,owner_id,url,sha256) VALUES(?,?,'image','demo','local',?,?)",(f"perf-{i}.jpg",path.stat().st_size,f"file:///nonexistent/perf-{i}.jpg",key))
    report={"dataset":10000,"fixture_type":"synthetic encoded JPEG, distinct files, no original source","bytes":sum(p.stat().st_size for p in cache.iterdir()),"seed_seconds":round(time.monotonic()-start,3),"runs":[]}
    for mode in ["grid","masonry"]:
        samples=[];start=time.monotonic();markers=[];stderr=[]
        p=subprocess.Popen([str(binary)],env={**env,"YUQUE_QA_PAGE":mode,"YUQUE_QA_PERF":"1"},stdout=subprocess.DEVNULL,stderr=subprocess.PIPE,text=True)
        def sample():
            while p.poll() is None:
                q=subprocess.run(["ps","-p",str(p.pid),"-o","rss=,%cpu="],capture_output=True,text=True)
                try:
                    rss,cpu=q.stdout.split();samples.append([int(rss),float(cpu)])
                except ValueError:pass
                time.sleep(.2)
        def read_stderr():
            for line in p.stderr:
                stderr.append(line)
                if line.startswith("PERF "):markers.append({"wall_ms":round((time.monotonic()-start)*1000,1),"message":line.strip()})
        worker=threading.Thread(target=sample);worker.start()
        reader=threading.Thread(target=read_stderr);reader.start()
        try:p.wait(timeout=25)
        except subprocess.TimeoutExpired:p.kill();p.wait();raise
        finally:worker.join();reader.join()
        err="".join(stderr)
        lines=[m["message"] for m in markers]
        assert p.returncode==0 and any("render_cpu_ms" in line for line in lines),err[-2000:]
        run={"mode":mode,"process_seconds":round(time.monotonic()-start,3),"metrics":lines,"wall_markers":markers,"peak_sampled_rss_mib":round(max((s[0] for s in samples),default=0)/1024,2),"mean_sampled_ps_cpu_percent":round(sum(s[1] for s in samples)/max(1,len(samples)),2),"sample_count":len(samples)}
        report["runs"].append(run);print(run,flush=True)
    report["limits"]="debug QA build; 150 items per page; CPU draw time excludes GPU presentation, GPU utilization and true FPS unmeasured; first run cold app process, OS disk cache not flushed"
    (out/"album-perf.json").write_text(json.dumps(report,indent=2,ensure_ascii=False))
