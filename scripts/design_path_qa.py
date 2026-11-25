"""Record actual compositor frames after injected GPUI pointer/key events.
No physical mouse control, screen-recording permission, user data or real accounts.
"""
import hashlib, os, pathlib, sqlite3, subprocess, tempfile
from PIL import Image,ImageDraw
root=pathlib.Path(__file__).resolve().parents[1]
out=root/'dist/design-research/path'
out.mkdir(parents=True,exist_ok=True)
exe=root/'target/debug/yuque-asset-desk'
env={k:v for k,v in os.environ.items() if not k.startswith('YUQUE_')}
with tempfile.TemporaryDirectory(prefix='yuque-design-path-') as tmp:
    data=pathlib.Path(tmp)
    env.update(YUQUE_DATA_DIR=str(data),YUQUE_MODE='demo',YUQUE_DESKTOP_INTEGRATION='0',YUQUE_QA_NARROW='1')
    subprocess.run([str(exe)],env={**env,'YUQUE_QA_CAPTURE':str(data/'empty.png')},timeout=15,check=True,capture_output=True)
    with sqlite3.connect(data/'assets.db') as db:
        for i in range(3):
            path=data/f'fixture-{i}.png'
            im=Image.new('RGB',(640,480),[(150,180,170),(90,120,170),(220,175,135)][i])
            draw=ImageDraw.Draw(im);draw.rectangle((20,20,620,460),outline='white',width=4)
            draw.text((40,40),f'Fixture {i}',fill='white');im.save(path)
            db.execute("INSERT INTO assets(name,bytes,category,provider,owner_id,url,sha256) VALUES(?,?,'image','demo','local',?,?)",(path.name,path.stat().st_size,path.as_uri(),hashlib.sha256(path.read_bytes()).hexdigest()))
    for p in out.glob('[0-9][0-9]-*.png'):p.unlink()
    proc=subprocess.run([str(exe)],env={**env,'YUQUE_QA_PATH':str(out)},timeout=25,capture_output=True,text=True)
    (out/'trace.log').write_text(proc.stderr)
    frames=sorted(out.glob('[0-9][0-9]-*.png'))
    assert proc.returncode==0 and len(frames)==8 and 'PATH_CAPTURE FAILED' not in proc.stderr,proc.stderr
    images=[]
    for p in frames:
        im=Image.open(p).convert('RGB');im.thumbnail((1000,800));images.append(im.copy())
    images[0].save(out/'operation-path.gif',save_all=True,append_images=images[1:],duration=1000,loop=0)
    print('\n'.join(line for line in proc.stderr.splitlines() if line.startswith('PATH_CAPTURE')))
    print(out/'operation-path.gif')
