use anyhow::{Context, Result};
use arboard::Clipboard;
use tracing::{debug, warn};

/// Cross-platform clipboard manager for MonoMouse.
/// Uses the `arboard` crate which supports X11, Wayland, and Windows.
pub struct ClipboardManager {
    clipboard: Clipboard,
    last_content: String,
}

impl ClipboardManager {
    pub fn new() -> Result<Self> {
        let clipboard = Clipboard::new().context("Failed to initialize clipboard")?;
        Ok(Self {
            clipboard,
            last_content: String::new(),
        })
    }

    /// Get the current clipboard text content.
    pub fn get_text(&mut self) -> Result<String> {
        self.clipboard
            .get_text()
            .context("Failed to get clipboard text")
    }

    /// Set the clipboard text content.
    pub fn set_text(&mut self, text: &str) -> Result<()> {
        self.clipboard
            .set_text(text)
            .context("Failed to set clipboard text")?;
        self.last_content = text.to_string();
        debug!("Clipboard set ({} bytes)", text.len());
        Ok(())
    }

    /// Check if the clipboard content has changed since the last check.
    /// Returns the new content if changed, None if unchanged.
    pub fn poll_changes(&mut self) -> Option<String> {
        match self.get_text() {
            Ok(current) => {
                if current != self.last_content {
                    self.last_content = current.clone();
                    Some(current)
                } else {
                    None
                }
            }
            Err(e) => {
                warn!("Failed to poll clipboard: {e}");
                None
            }
        }
    }
}
