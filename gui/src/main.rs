use iced::{
    window::{self, PlatformSpecific},
    Application, Result, Settings,
};

mod app;
use app::OnlineFiltering;

/// Graph refresh rate when streaming
pub const FPS: u64 = 60;
/// Serial baud rate
pub const BAUD_RATE: u32 = 115_200;
/// Minimum number of points to visualize on graph
pub const MIN_WINDOW_SIZE: usize = 32;
/// Number of points to look-back when displaying streaming data
pub const STREAMING_WINDOW_SIZE: usize = 384;
/// Useful numpy functions to bring to the global scope
pub const NUMPY_IMPORTS: &[&str] = &["abs", "sin", "cos", "pi"];
/// End of transmission marker (Equal to [`f32::NaN`])
pub const EOT: &[u8] = &(0x7F_C0_00_00u32.to_le_bytes());
/// Serial synchronization marker
pub const SYN: &[u8] = b"SYN\x00";
/// Name of the file to export filtered data to
pub const FILENAME: &str = "filtered.json";

pub fn main() -> Result {
    tracing_subscriber::fmt::init();
    pyo3::prepare_freethreaded_python();

    OnlineFiltering::run(Settings {
        antialiasing: true,
        window: window::Settings {
            min_size: Some((400, 600)),
            platform_specific: PlatformSpecific {
                title_hidden: true,
                titlebar_transparent: true,
                fullsize_content_view: true,
            },
            ..Default::default()
        },
        ..Default::default()
    })
}
