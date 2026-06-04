#![allow(non_snake_case)]
// On Windows release builds, hide the console window the OS would
// otherwise spawn alongside the GUI. Debug builds keep stdout/stderr
// attached so eprintln! / panics stay visible while developing.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod command;
mod entities;
mod io;
mod linetypes;
mod modules;
mod patterns;
mod scene;
mod snap;
mod ui;
mod update_check;

fn main() -> iced::Result {
    // On some Windows hybrid-GPU laptops the AMD OpenGL driver (atio6axx.dll)
    // access-violates the moment wgpu enumerates its GL backend at startup,
    // killing the app before any window appears — even though DX12 would work
    // fine (#55). Restrict wgpu to DX12/Vulkan so the GL ICD is never touched.
    // An explicit user-set WGPU_BACKEND still wins.
    #[cfg(target_os = "windows")]
    if std::env::var_os("WGPU_BACKEND").is_none() {
        std::env::set_var("WGPU_BACKEND", "dx12,vulkan");
    }
    app::run()
}
