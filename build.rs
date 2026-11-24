fn main() {
    println!("cargo:rerun-if-changed=assets/branding/app.ico");
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
        let icon = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap())
            .join("assets/branding/app.ico").to_string_lossy().replace('\\', "/");
        let version = env!("CARGO_PKG_VERSION");
        let numbers = version.split('.').collect::<Vec<_>>();
        let tuple = format!("{},{},{},0",numbers[0],numbers[1],numbers[2]);
        let rc = format!(r#"1 ICON "{icon}"
1 VERSIONINFO
FILEVERSION {tuple}
PRODUCTVERSION {tuple}
FILEOS 0x40004
FILETYPE 1
BEGIN
 BLOCK "StringFileInfo"
 BEGIN
  BLOCK "040904B0"
  BEGIN
   VALUE "CompanyName", "Weepwood"
   VALUE "FileDescription", "Yuque Asset Desk"
   VALUE "FileVersion", "{version}"
   VALUE "ProductName", "Yuque Asset Desk"
   VALUE "ProductVersion", "{version}"
   VALUE "OriginalFilename", "yuque-asset-desk.exe"
  END
 END
 BLOCK "VarFileInfo"
 BEGIN
  VALUE "Translation", 0x0409, 1200
 END
END
"#);
        let path = out.join("yuque-assets.rc");
        std::fs::write(&path, rc).expect("write Windows resources");
        embed_resource::compile(path, embed_resource::NONE).manifest_required().expect("embed icon and version resource");
    }
}
