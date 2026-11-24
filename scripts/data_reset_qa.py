"""macOS isolated process test of the exit helper; never uses a real data directory."""
import fcntl, hashlib, os, pathlib, sqlite3, subprocess, tempfile, time, json
root_repo=pathlib.Path(__file__).resolve().parents[1]
exe=root_repo/'target/debug/yuque-asset-desk'
with tempfile.TemporaryDirectory(prefix='yuque-reset-qa-') as tmp:
    root=pathlib.Path(tmp);data=root/'data';data.mkdir()
    env={k:v for k,v in os.environ.items() if not k.startswith('YUQUE_')}
    env.update(YUQUE_MODE='demo',YUQUE_DATA_DIR=str(data),YUQUE_DESKTOP_INTEGRATION='0')
    subprocess.run([str(exe)],env=dict(env,YUQUE_QA_CAPTURE=str(root/'capture.png')),check=True,timeout=20)
    db=data/'assets.db';assert db.exists()
    preview=data/'album-previews';preview.mkdir(exist_ok=True)
    image=preview/('a'*64+'.jpg');image.write_bytes(b'fixture preview')
    clip=data/'clipboard';clip.mkdir(exist_ok=True)
    b=b'synthetic clipboard';source=clip/(hashlib.sha256(b).hexdigest()+'.png');source.write_bytes(b)
    original=root/'user-original.png';original.write_bytes(b'never delete')
    unknown=data/'unknown.txt';unknown.write_bytes(b'preserve unknown')
    external_backup=root/'backup';external_backup.mkdir();(external_backup/'assets.db').write_bytes(b'backup fixture')
    marker=data/'.reset-request';token='isolated-test-token'
    marker.write_text(token)
    invalid=subprocess.run([str(exe),'--reset-after-exit','wrong-token'],env=env,timeout=5)
    assert invalid.returncode!=0 and marker.exists() and db.exists() and image.exists()
    with sqlite3.connect(db) as c:
        c.execute("INSERT INTO upload_jobs(provider,owner_id,path,state) VALUES('yuque','999',?,'unknown')",(str(original),))
    blocked=subprocess.run([str(exe),'--reset-after-exit',token],env=env,timeout=5)
    assert blocked.returncode!=0 and db.exists() and image.exists() and source.exists() and not marker.exists()
    report=data/'reset-error.txt';assert report.exists()
    with sqlite3.connect(db) as c:
        c.execute("UPDATE upload_jobs SET state='done'")
    marker.write_text(token)
    with (data/'instance.lock').open('r+b') as lock:
        fcntl.flock(lock,fcntl.LOCK_EX)
        proc=subprocess.Popen([str(exe),'--reset-after-exit',token],env=env)
        try:
            time.sleep(0.5)
            assert proc.poll() is None and db.exists() and image.exists() and source.exists()
        finally:
            fcntl.flock(lock,fcntl.LOCK_UN)
        assert proc.wait(timeout=10)==0
    assert not db.exists() and not image.exists() and not source.exists() and not marker.exists()
    assert original.read_bytes()==b'never delete' and unknown.read_bytes()==b'preserve unknown' and (external_backup/'assets.db').read_bytes()==b'backup fixture'
    print(json.dumps({'wrong_token_rejected':True,'unfinished_unknown_blocked':True,'waited_for_instance_lock':True,'owned_files_removed':True,'external_original_and_backup_preserved':True,'unknown_preserved':True,'real_credentials_used':False}))
