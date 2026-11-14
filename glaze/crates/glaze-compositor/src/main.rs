//! `glaze-compositor` binary. Refuses to run where the backend is unavailable
//! rather than pretending to work.

fn main() -> std::process::ExitCode {
    if !glaze_compositor::is_available() {
        eprintln!("glaze-compositor: needs Linux (Wayland); this host is not supported.");
        return std::process::ExitCode::FAILURE;
    }
    #[cfg(target_os = "linux")]
    {
        glaze_compositor::linux::init_logging();
        if let Err(e) = glaze_compositor::linux::run() {
            eprintln!("glaze-compositor: {e}");
            return std::process::ExitCode::FAILURE;
        }
        std::process::ExitCode::SUCCESS
    }
    #[cfg(not(target_os = "linux"))]
    std::process::ExitCode::FAILURE
}
