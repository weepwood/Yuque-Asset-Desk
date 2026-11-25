"""Isolated real encoded JPEG benchmark; no user images or network.
Reports CPU-side dispatch/draw duration, not hardware presentation FPS."""
import hashlib, json, os, pathlib, sqlite3, subprocess, tempfile, time, threading, re, platform
from PIL import Image, ImageDraw
root=pathlib.Path(__file__).resolve().parent.parent
profile=os.getenv("YUQUE_PERF_PROFILE","release")
memory=os.getenv("YUQUE_PERF_MEMORY")=="1"
assert profile in ("debug","release")
binary=root/"target"/profile/"yuque-asset-desk"
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
    report={"profile":profile,"platform":platform.platform(),"binary_bytes":binary.stat().st_size,"dataset":10000,"fixture_type":"synthetic encoded JPEG, distinct files, no original source","bytes":sum(p.stat().st_size for p in cache.iterdir()),"seed_seconds":round(time.monotonic()-start,3),"runs":[]}
    for mode in ["grid","masonry"]:
        samples=[];gpu=[];start=time.monotonic();markers=[];stderr=[];rss_trace=[]
        p=subprocess.Popen([str(binary)],env={**env,"YUQUE_QA_PAGE":mode,"YUQUE_QA_PERF":"memory" if memory else "1"},stdout=subprocess.DEVNULL,stderr=subprocess.PIPE,text=True)
        def sample():
            while p.poll() is None:
                q=subprocess.run(["ps","-p",str(p.pid),"-o","rss=,%cpu="],capture_output=True,text=True)
                try:
                    rss,cpu=q.stdout.split();samples.append([int(rss),float(cpu)])
                    if memory:rss_trace.append({"wall_ms":round((time.monotonic()-start)*1000,1),"rss_mib":round(int(rss)/1024,2),"cpu_percent":float(cpu)})
                except ValueError:pass
                time.sleep(.2)
        def read_stderr():
            for line in p.stderr:
                stderr.append(line)
                if line.startswith(("PERF ","MEMORY ")):
                    marker={"wall_ms":round((time.monotonic()-start)*1000,1),"message":line.strip()}
                    markers.append(marker)
                    print(mode,marker,flush=True)
        def sample_gpu():
            while p.poll() is None:
                q=subprocess.run(["ioreg","-r","-c","IOAccelerator","-l"],capture_output=True,text=True)
                vals=re.findall(r'"Device Utilization %"=(\d+)',q.stdout)
                if vals:gpu.append([round((time.monotonic()-start)*1000,1),max(map(int,vals))])
                time.sleep(.5)
        worker=threading.Thread(target=sample);worker.start()
        gpu_worker=threading.Thread(target=sample_gpu);gpu_worker.start()
        reader=threading.Thread(target=read_stderr);reader.start()
        timed_out=False
        try:p.wait(timeout=90 if memory else 25)
        except subprocess.TimeoutExpired:timed_out=True;p.kill();p.wait()
        finally:worker.join();reader.join();gpu_worker.join()
        err="".join(stderr)
        lines=[m["message"] for m in markers]
        if memory:
            assert not timed_out and p.returncode==0,err[-2000:]
            checkpoints=[dict(part.split("=",1) for part in line.split()[1:]) for line in lines if line.startswith("MEMORY ")]
            assert len(checkpoints)==27,checkpoints
            assert all(int(c["cache"])<=int(c["bound"]) for c in checkpoints),checkpoints
            assert all(int(c["cache"])==0 for c in checkpoints if c["phase"].startswith("leave-")),checkpoints
            assert all(int(c["loaded"])<=150 and c["loading"]=="false" for c in checkpoints if c["phase"].startswith("reset-")),checkpoints
            assert all(max(int(c["loaded"]) for c in checkpoints if c["phase"].startswith(f"scroll-{cycle}-"))>=1000 for cycle in range(3)),checkpoints
        else:
            assert any("render_cpu_ms" in line for line in lines),err[-2000:]
            if not timed_out:assert p.returncode==0,err[-2000:]
        run={"mode":mode,"timed_out":timed_out,"process_seconds":round(time.monotonic()-start,3),"metrics":lines,"wall_markers":markers,"peak_sampled_rss_mib":round(max((s[0] for s in samples),default=0)/1024,2),"mean_sampled_ps_cpu_percent":round(sum(s[1] for s in samples)/max(1,len(samples)),2),"sample_count":len(samples),"system_gpu_samples":gpu}
        if memory:run.update(checkpoints=checkpoints,rss_trace=rss_trace)
        report["runs"].append(run)
        if memory:print(mode,"checkpoints",len(checkpoints),"peak MiB",run["peak_sampled_rss_mib"],flush=True)
        else:print(run,flush=True)
    report["limits"]=f"{profile} QA build; 150 item batches; native callback cadence is not display presentation FPS; ioreg GPU samples are system-wide, not per-process; process launch cold, OS disk cache not flushed"
    if memory:report["limits"]+="; three 180-event scrolling cycles per layout; demo scope reset, not real credentials; RSS includes native allocator/renderer, not leak proof"
    (out/f"album-perf-{profile}{'-memory' if memory else ''}.json").write_text(json.dumps(report,indent=2,ensure_ascii=False))
