use anyhow::Result;
use monomouse_core::{InputEvent, Monitor};

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "windows")]
pub mod windows;

/// Detect all monitors connected to this machine.
pub fn detect_monitors() -> Result<Vec<Monitor>> {
    #[cfg(target_os = "linux")]
    return linux::detect::detect_monitors();

    #[cfg(target_os = "windows")]
    return windows::detect::detect_monitors();

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    anyhow::bail!("Unsupported platform")
}

/// Create a platform-appropriate input capturer.
pub fn create_capturer() -> Result<Box<dyn InputCapture>> {
    #[cfg(target_os = "linux")]
    return Ok(Box::new(linux::capture::EvdevCapture::new()?));

    #[cfg(target_os = "windows")]
    return Ok(Box::new(windows::capture::WindowsCapture::new()?));

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    anyhow::bail!("Unsupported platform")
}

/// Create a platform-appropriate input injector.
pub fn create_injector() -> Result<Box<dyn InputInjector>> {
    #[cfg(target_os = "linux")]
    return Ok(Box::new(linux::inject::UinputInjector::new()?));

    #[cfg(target_os = "windows")]
    return Ok(Box::new(windows::inject::WindowsInjector::new()?));

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    anyhow::bail!("Unsupported platform")
}

/// Trait for capturing input from the local machine.
pub trait InputCapture: Send {
    /// Grab exclusive input control (the local cursor should freeze).
    fn grab(&mut self) -> Result<()>;

    /// Release input back to the local system.
    fn release(&mut self) -> Result<()>;

    /// Check if currently grabbing input.
    fn is_grabbed(&self) -> bool;

    /// Poll for the next input event (blocking).
    fn next_event(&mut self) -> Result<InputEvent>;

    /// Get the current absolute cursor position.
    fn cursor_position(&self) -> Result<(i32, i32)>;
}

/// Trait for injecting input events on the local machine.
pub trait InputInjector: Send {
    /// Move the cursor to an absolute position on a specific monitor.
    fn move_to(&mut self, monitor: &Monitor, x: i32, y: i32) -> Result<()>;

    /// Inject an input event.
    fn inject(&mut self, event: &InputEvent) -> Result<()>;
}
