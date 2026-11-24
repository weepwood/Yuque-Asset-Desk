"""Native registration/conflict and minimize/restore smoke, using isolated databases.
No actual OS clipboard reads, no input injection, no permission escalation."""
import os, pathlib, subprocess, tempfile, sqlite3, time, json
root=pathlib.Path(__file__).resolve().parent.parent
binary=root/"target"/"release"/"yuque-asset-desk"
out=root/"dist/qa";out.mkdir(parents=True,exist_ok=True)
env={k:v for k,v in os.environ.items() if not k.startswith("YUQUE_")}
def launch(data,mode="1"):
    return subprocess.Popen([str(binary)],env={**env,"YUQUE_MODE":"demo","YUQUE_DATA_DIR":str(data),"YUQUE_DESKTOP_SMOKE":mode},stdout=subprocess.DEVNULL,stderr=subprocess.PIPE,text=True)
def stop(p):
    if p.poll() is None:p.terminate()
    try:_,err=p.communicate(timeout=8)
    except subprocess.TimeoutExpired:p.kill();_,err=p.communicate()
    return err
with tempfile.TemporaryDirectory(prefix="yuque-native-qa-") as tmp:
    rootdir=pathlib.Path(tmp)
    first=launch(rootdir/"first");time.sleep(1.6)
    assert first.poll() is None,"first native process exited"
    second=launch(rootdir/"second");time.sleep(1.6)
    second_log=stop(second);first_log=stop(first)
    data=rootdir/"alternate"
    p=launch(data);time.sleep(1.1);stop(p)
    with sqlite3.connect(data/"assets.db") as db:
        db.execute("INSERT INTO desk_preferences(id,value) VALUES(1,?) ON CONFLICT(id) DO UPDATE SET value=excluded.value",(json.dumps({"shortcut":"I","dark":False,"layout":"masonry","tile":196,"width":920,"height":680}),))
    third=launch(data,"2")
    _,third_log=third.communicate(timeout=12)
    assert third.returncode==0,third_log
    report={"default_registration":first_log,"simultaneous_registration":second_log,"alternate_and_window":third_log,
            "limitations":"Programmatic native window calls, not physical menu clicks or key events. No system clipboard modification or notification delivery test."}
    (out/"desktop-qa.json").write_text(json.dumps(report,ensure_ascii=False,indent=2))
    print(json.dumps(report,ensure_ascii=False,indent=2))
