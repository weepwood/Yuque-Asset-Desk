"""Create versioned, inspectable desktop ZIPs. macOS ad-hoc signed, not notarized."""
import argparse, hashlib, pathlib, plistlib, re, shutil, subprocess, zipfile
ROOT=pathlib.Path(__file__).resolve().parents[1]
VERSION=re.search(r'^version = "([^"]+)"', (ROOT/'Cargo.toml').read_text(encoding="utf-8"),re.M)[1]
TARGETS={'aarch64-apple-darwin','x86_64-apple-darwin','x86_64-pc-windows-msvc'}
def verify(path,target):
    with zipfile.ZipFile(path) as z:
        assert z.testzip() is None
        if target.endswith('apple-darwin'):
            base='Yuque Asset Desk.app/Contents/'
            p=plistlib.loads(z.read(base+'Info.plist'))
            assert p['CFBundleShortVersionString']==VERSION and p['CFBundleVersion']==VERSION
            assert p['CFBundleIdentifier']=='com.weepwood.yuque-asset-desk'
            exe=z.getinfo(base+'MacOS/yuque-asset-desk')
            assert exe.file_size>1000000 and (exe.external_attr>>16)&0o111
        else:
            assert z.getinfo('yuque-asset-desk.exe').file_size>1000000
        assert z.getinfo('INSTALL.txt').file_size>20
def main():
    ap=argparse.ArgumentParser()
    ap.add_argument('--target',choices=sorted(TARGETS),required=True)
    ap.add_argument('--binary',type=pathlib.Path)
    ap.add_argument('--output',type=pathlib.Path,default=ROOT/'dist'/'release')
    ap.add_argument('--verify-only',action='store_true')
    a=ap.parse_args();out=a.output.resolve();out.mkdir(parents=True,exist_ok=True)
    zip_path=out/f'yuque-asset-desk-{VERSION}-{a.target}.zip'
    if not a.verify_only:
        assert a.binary and a.binary.is_file()
        import tempfile
        with tempfile.TemporaryDirectory(prefix='yuque-package-') as tmp:
            stage=pathlib.Path(tmp)
            if a.target.endswith('apple-darwin'):
                contents=stage/'Yuque Asset Desk.app'/'Contents';(contents/'MacOS').mkdir(parents=True)
                exe=contents/'MacOS'/'yuque-asset-desk';shutil.copy2(a.binary,exe);exe.chmod(0o755)
                (contents/'Info.plist').write_bytes(plistlib.dumps(dict(CFBundleIdentifier='com.weepwood.yuque-asset-desk',CFBundleExecutable='yuque-asset-desk',CFBundleName='Yuque Asset Desk',CFBundleDisplayName='Yuque Asset Desk',CFBundlePackageType='APPL',CFBundleShortVersionString=VERSION,CFBundleVersion=VERSION,LSMinimumSystemVersion='15.0',NSHighResolutionCapable=True)))
                subprocess.run(['plutil','-lint',str(contents/'Info.plist')],check=True)
                subprocess.run(['codesign','--force','--sign','-',str(contents.parent)],check=True)
                subprocess.run(['codesign','--verify','--deep','--strict',str(contents.parent)],check=True)
                (stage/'INSTALL.txt').write_text('Copy Yuque Asset Desk.app to Applications. Ad-hoc signed; not notarized. macOS 15 or later. Review source and checksum before opening.\n',encoding='utf-8')
            else:
                shutil.copy2(a.binary,stage/'yuque-asset-desk.exe')
                (stage/'INSTALL.txt').write_text('Extract ZIP and run yuque-asset-desk.exe. Portable, unsigned executable; no installer or automatic startup.\n',encoding='utf-8')
            if a.target.endswith('apple-darwin'):
                subprocess.run(['ditto','-c','-k','--sequesterRsrc',str(stage),str(zip_path)],check=True)
            else:
                with zipfile.ZipFile(zip_path,'w',zipfile.ZIP_DEFLATED) as z:
                    for f in stage.iterdir():z.write(f,f.name)
    verify(zip_path,a.target)
    digest=hashlib.sha256(zip_path.read_bytes()).hexdigest()
    (out/(zip_path.name+'.sha256')).write_text(f'{digest}  {zip_path.name}\n',encoding='ascii')
    print(f'{zip_path.name}: {zip_path.stat().st_size} bytes SHA256 {digest}')
if __name__=='__main__':main()
