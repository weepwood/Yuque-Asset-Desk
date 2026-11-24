"""Open actual macOS GPUI path dialogs with isolated Demo data, no OS input automation."""
import json, os, pathlib, subprocess, tempfile, time, sqlite3
ROOT=pathlib.Path(__file__).resolve().parents[1]
EXE=ROOT/'target/debug/yuque-asset-desk'
assert EXE.is_file()
with tempfile.TemporaryDirectory(prefix='yuque-native-picker-') as tmp:
    root=pathlib.Path(tmp)
    source=root/'windows.swift'
    source.write_text('''
import CoreGraphics
import Foundation
let pid = Int(CommandLine.arguments[1])!
let windows = CGWindowListCopyWindowInfo(.optionAll,kCGNullWindowID) as? [[String:Any]] ?? []
let found = windows.filter { ($0[kCGWindowOwnerPID as String] as? Int)==pid }.compactMap { w -> [String:Any]? in
    guard let b=w[kCGWindowBounds as String] as? [String:Any], (b["Width"] as? Double ?? 0)>100, (b["Height"] as? Double ?? 0)>100 else { return nil }
    return ["bounds":b,"layer":w[kCGWindowLayer as String] ?? 0]
}
print(String(data:try! JSONSerialization.data(withJSONObject:found),encoding:.utf8)!)
''')
    helper=root/'windows'
    subprocess.run(['swiftc',str(source),'-o',str(helper)],check=True)
    report=[]
    for kind in ['upload','backup']:
        data=root/kind
        env={k:v for k,v in os.environ.items() if not k.startswith('YUQUE_')}
        env.update(YUQUE_MODE='demo',YUQUE_DATA_DIR=str(data),YUQUE_DESKTOP_INTEGRATION='0',YUQUE_QA_PICKER=kind)
        log=root/(kind+'.log')
        with log.open('w+') as out:
            proc=subprocess.Popen([str(EXE)],env=env,stdout=out,stderr=out)
            try:
                time.sleep(9)
                assert proc.poll() is None,log.read_text()
                logs=log.read_text()
                assert f'PICKER requested={kind} pending=true' in logs,logs
                windows=json.loads(subprocess.check_output([str(helper),str(proc.pid)],text=True))
                print(kind,repr(logs),windows,flush=True)
                assert len(windows)>=2,windows
                assert "PICKER alive pending=true" in logs,logs
                with sqlite3.connect(data/'assets.db') as c:
                    assert c.execute('SELECT count(*) FROM upload_jobs').fetchone()[0]==0
                report.append(dict(kind=kind,process_alive=True,picker_requested=True,callback_while_open="PICKER alive pending=true" in logs,windows=windows,queued_jobs=0))
            finally:
                if proc.poll() is None:proc.terminate()
                proc.wait(timeout=10)
    print(json.dumps(report,ensure_ascii=False,indent=2))
