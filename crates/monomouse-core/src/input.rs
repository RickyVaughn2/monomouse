use serde::{Deserialize, Serialize};

/// Represents an input event to be forwarded across machines.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InputEvent {
    MouseMove { dx: i32, dy: i32 },
    MouseAbsolute { x: i32, y: i32 },
    MouseButton { button: MouseButton, pressed: bool },
    MouseScroll { dx: i32, dy: i32 },
    Key { code: KeyCode, state: KeyState },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Extra1,
    Extra2,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum KeyState {
    Pressed,
    Released,
}

/// Platform-independent key code.
/// We use a u32 scancode internally and convert per-platform.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeyCode(pub u32);
