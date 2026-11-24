"""Copy the verified macOS app from a read-only DMG into isolation and launch in Demo mode."""
import argparse, hashlib, json, os, pathlib, sqlite3, subprocess, tempfile, time
from package_release import verify_app, APP
ap=argparse.ArgumentParser()
ap.add_argument('dmg',type=pathlib.Path)
a=ap.parse_args();dmg=a.dmg.resolve()
digest=hashlib.sha256(dmg.read_bytes()).hexdigest()
assert dmg.with_name(dmg.name+'.sha256').read_text(encoding='ascii').split()[0]==digest
with tempfile.TemporaryDirectory(prefix='yuque-dmg-smoke-') as tmp:
    root=pathlib.Path(tmp);mount=root/'volume';app=root/APP
    subprocess.run(['hdiutil','attach','-readonly','-nobrowse','-mountpoint',str(mount),str(dmg)],check=True)
    try:subprocess.run(['ditto',str(mount/APP),str(app)],check=True)
    finally:subprocess.run(['hdiutil','detach',str(mount)],check=True)
    verify_app(app)
    env={k:v for k,v in os.environ.items() if not k.startswith('YUQUE_')}
    env.update(YUQUE_MODE='demo',YUQUE_DATA_DIR=str(root/'data'),YUQUE_DESKTOP_SMOKE='1')
    proc=subprocess.Popen([str(app/'Contents/MacOS/yuque-asset-desk')],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.PIPE,text=True)
    try:
        time.sleep(4)
        assert proc.poll() is None
        with sqlite3.connect(root/'data'/'assets.db') as c:
            assert c.execute('select count(*) from assets').fetchone()[0]==0
        swift='import CoreGraphics; import Foundation; let windows = CGWindowListCopyWindowInfo(.optionAll,kCGNullWindowID) as? [[String:Any]] ?? []; for w in windows where (w[kCGWindowOwnerPID as String] as? Int)=='+str(proc.pid)+' { if let b=w[kCGWindowBounds as String]{print(b)} }'
        windows=subprocess.check_output(['swift','-e',swift],text=True).strip()
        assert windows
    finally:
        if proc.poll() is None:proc.terminate()
        _,err=proc.communicate(timeout=10)
    print(err)
    assert 'DESKTOP tray=true' in err
    report={'sha256':digest,'bytes':dmg.stat().st_size,'architecture':'arm64','signature_verified':True,'copy_from_readonly_dmg':True,'isolated_database_initialized':True,'window_bounds':windows,'native_registration':err.strip(),'limits':'no installation to Applications, no real credentials, no Gatekeeper notarization, no Windows runtime'}
    print(json.dumps(report,indent=2))
    dmg.with_name('smoke.json').write_text(json.dumps(report,indent=2),encoding='utf-8')
