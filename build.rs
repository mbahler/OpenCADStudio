// build.rs — Windows executable icon embedding.
//
// The core ribbon module registry used to be generated here; it is now a
// hand-written static list in src/modules/registry.rs.

#[cfg(windows)]
use std::path::Path;

fn main() {
    // Windows: embed AppIcon.ico into the .exe so the executable carries its
    // own icon (Explorer, taskbar, Start-menu tile, file associations). The
    // .ico is produced from assets/logo.svg by the release workflow before the
    // build; when it is absent (local/dev builds) this is skipped. See #107.
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=packaging/windows/AppIcon.ico");
        if Path::new("packaging/windows/AppIcon.ico").exists() {
            let mut res = winresource::WindowsResource::new();
            res.set_icon("packaging/windows/AppIcon.ico");
            if let Err(e) = res.compile() {
                println!("cargo:warning=failed to embed Windows icon: {e}");
            }
        }
    }
}
