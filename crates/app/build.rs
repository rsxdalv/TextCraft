//! Embeds the application icon and version information in the Windows executable, so
//! Explorer, the taskbar, shortcuts and the installer all show the TextCraft icon.

fn main() {
    println!("cargo:rerun-if-changed=../../assets/textcraft.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    #[cfg(windows)]
    windows_resources();
}

#[cfg(windows)]
fn windows_resources() {
    let mut resource = winresource::WindowsResource::new();
    resource
        .set_icon("../../assets/textcraft.ico")
        .set("ProductName", "TextCraft")
        .set("FileDescription", "TextCraft text editor")
        .set("OriginalFilename", "textcraft.exe")
        .set("LegalCopyright", "TextCraft contributors");
    if let Err(error) = resource.compile() {
        // A release must carry its icon; a developer build without the Windows SDK
        // resource compiler should still build.
        if std::env::var_os("TEXTCRAFT_REQUIRE_WINRES").is_some() {
            panic!("could not embed the Windows resources: {error}");
        }
        println!("cargo:warning=could not embed the Windows resources: {error}");
    }
}
