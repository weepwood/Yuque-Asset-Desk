"""Build and verify two-platform release packages. No notarization or unsigned bypass."""
import argparse, hashlib, pathlib, plistlib, re, shutil, struct, subprocess, tempfile, zipfile
ROOT=pathlib.Path(__file__).resolve().parents[1]
BRAND=ROOT/'assets'/'branding'
VERSION=re.search(r'^version = "([^"]+)"', (ROOT/'Cargo.toml').read_text(encoding='utf-8'),re.M)[1]
MAC='aarch64-apple-darwin'
WIN='x86_64-pc-windows-msvc'
APP='Yuque Asset Desk.app'

def run(*args):
    subprocess.run([str(a) for a in args],check=True)

def verify_app(app):
    contents=app/'Contents'
    p=plistlib.loads((contents/'Info.plist').read_bytes())
    assert p['CFBundleShortVersionString']==p['CFBundleVersion']==VERSION
    assert p['CFBundleIdentifier']=='com.weepwood.yuque-asset-desk'
    assert p['CFBundleIconFile']=='AppIcon.icns'
    assert (contents/'Resources'/'AppIcon.icns').read_bytes()==(BRAND/'app.icns').read_bytes()
    exe=contents/'MacOS'/'yuque-asset-desk'
    assert exe.stat().st_size>1000000 and exe.stat().st_mode&0o111
    assert subprocess.check_output(['lipo','-archs',str(exe)],text=True).strip()=='arm64'
    run('codesign','--verify','--deep','--strict',app)

def verify_pe(b):
    # Parse PE directory entries to verify icon, group icon and version resources.
    pe=struct.unpack_from('<I',b,0x3c)[0]
    assert b[:2]==b'MZ' and b[pe:pe+4]==b'PE\0\0'
    machine,nsections=struct.unpack_from('<HH',b,pe+4)
    opt=pe+24
    assert machine==0x8664 and struct.unpack_from('<H',b,opt)[0]==0x20b
    rva,sz=struct.unpack_from('<II',b,opt+112+2*8)
    assert rva and sz
    section=opt+struct.unpack_from('<H',b,pe+20)[0]
    for i in range(nsections):
        vs,va,rawsize,raw=struct.unpack_from('<IIII',b,section+i*40+8)
        if va<=rva<va+max(vs,rawsize):
            base=raw+rva-va
            break
    else: raise ValueError('PE resource section missing')
    named,ids=struct.unpack_from('<HH',b,base+12)
    types={struct.unpack_from('<I',b,base+16+i*8)[0] for i in range(named+ids)}
    assert {3,14,16}<=types, f'PE resource types: {types}'

def verify(path,target):
    if target==MAC:
        from ds_store import DSStore
        run('hdiutil','verify',path)
        with tempfile.TemporaryDirectory(prefix='yuque-dmg-verify-') as tmp:
            mount=pathlib.Path(tmp)/'volume'
            run('hdiutil','attach','-readonly','-nobrowse','-mountpoint',mount,path)
            try:
                verify_app(mount/APP)
                assert (mount/'Applications').is_symlink() and (mount/'Applications').readlink()==pathlib.Path('/Applications')
                assert (mount/'.DS_Store').is_file()
                with DSStore.open(str(mount/'.DS_Store'),'r') as ds:
                    assert tuple(ds[APP]['Iloc'])==(165,205)
                    assert tuple(ds['Applications']['Iloc'])==(475,205)
                    settings=ds['.']['icvp']
                    assert settings['backgroundType']==2 and settings['backgroundImageAlias']
                    assert settings['iconSize']==112
                    assert ds['.']['bwsp']['WindowBounds']=='{{120, 120}, {640, 420}}'
                assert (mount/'.background.png').read_bytes()==(BRAND/'dmg-background.png').read_bytes()
                assert (mount/'.VolumeIcon.icns').read_bytes()==(BRAND/'app.icns').read_bytes()
            finally:
                run('hdiutil','detach',mount)
    else:
        with zipfile.ZipFile(path) as z:
            assert z.testzip() is None
            assert z.getinfo('yuque-asset-desk.exe').file_size>1000000
            verify_pe(z.read('yuque-asset-desk.exe'))
            assert z.getinfo('INSTALL.txt').file_size>20

def build_mac(binary,path):
    import dmgbuild
    with tempfile.TemporaryDirectory(prefix='yuque-package-') as tmp:
        app=pathlib.Path(tmp)/APP
        contents=app/'Contents'
        (contents/'MacOS').mkdir(parents=True)
        (contents/'Resources').mkdir()
        exe=contents/'MacOS'/'yuque-asset-desk'
        shutil.copy2(binary,exe);exe.chmod(0o755)
        shutil.copy2(BRAND/'app.icns',contents/'Resources'/'AppIcon.icns')
        (contents/'Info.plist').write_bytes(plistlib.dumps(dict(
            CFBundleIdentifier='com.weepwood.yuque-asset-desk',CFBundleExecutable='yuque-asset-desk',
            CFBundleName='Yuque Asset Desk',CFBundleDisplayName='Yuque Asset Desk',CFBundlePackageType='APPL',
            CFBundleIconFile='AppIcon.icns',CFBundleShortVersionString=VERSION,CFBundleVersion=VERSION,
            LSMinimumSystemVersion='15.0',NSHighResolutionCapable=True)))
        run('plutil','-lint',contents/'Info.plist')
        run('codesign','--force','--sign','-',app)
        verify_app(app)
        dmgbuild.build_dmg(str(path),'Yuque Asset Desk',settings={
            'files':[str(app)],'symlinks':{'Applications':'/Applications'},
            'icon_locations':{APP:(165,205),'Applications':(475,205)},
            'icon':str(BRAND/'app.icns'),'background':str(BRAND/'dmg-background.png'),
            'window_rect':((120,120),(640,420)),'icon_size':112,'text_size':13,
            'show_status_bar':False,'show_tab_view':False,'show_toolbar':False,'show_pathbar':False,
            'show_sidebar':False,'default_view':'icon-view','format':'UDZO',
        })

def main():
    ap=argparse.ArgumentParser()
    ap.add_argument('--target',choices=[MAC,WIN],required=True)
    ap.add_argument('--binary',type=pathlib.Path)
    ap.add_argument('--output',type=pathlib.Path,default=ROOT/'dist'/'release')
    ap.add_argument('--verify-only',action='store_true')
    a=ap.parse_args()
    out=a.output.resolve();out.mkdir(parents=True,exist_ok=True)
    path=out/f'yuque-asset-desk-{VERSION}-{a.target}.{"dmg" if a.target==MAC else "zip"}'
    checksum=path.with_name(path.name+'.sha256')
    if not a.verify_only:
        assert a.binary and a.binary.is_file()
        # A failed rebuild must not leave a stale valid-looking checksum.
        checksum.unlink(missing_ok=True)
        path.unlink(missing_ok=True)
        if a.target==MAC:
            build_mac(a.binary,path)
        else:
            with zipfile.ZipFile(path,'w',zipfile.ZIP_DEFLATED) as z:
                z.write(a.binary,'yuque-asset-desk.exe')
                z.writestr('INSTALL.txt','Extract ZIP and run yuque-asset-desk.exe. Portable, unsigned executable; no installer or automatic startup.\n')
    digest=hashlib.sha256(path.read_bytes()).hexdigest()
    expected=f'{digest}  {path.name}\n'
    if a.verify_only:
        assert checksum.read_text(encoding='ascii')==expected,'Original SHA256 mismatch'
    verify(path,a.target)
    if not a.verify_only: checksum.write_text(expected,encoding='ascii')
    print(f'{path.name}: {path.stat().st_size} bytes SHA256 {digest}')
if __name__=='__main__':main()
