use anyhow::{Context, Result};
use evdev::{Device, EventType, InputEventKind, Key, RelativeAxisType, AbsoluteAxisType};
use monomouse_core::{InputEvent, KeyCode, KeyState, MouseButton};
use crate::InputCapture;
use tracing::{debug, info, warn};
use std::fs;
use std::path::PathBuf;

/// Input capture using Linux evdev.
/// Grabs the mouse and keyboard devices for exclusive access.
pub struct EvdevCapture {
    mouse: Option<Device>,
    keyboard: Option<Device>,
    _mouse_path: Option<PathBuf>,
    _keyboard_path: Option<PathBuf>,
    grabbed: bool,
    cursor_x: i32,
    cursor_y: i32,
    screen_width: i32,
    screen_height: i32,
}

impl EvdevCapture {
    pub fn new() -> Result<Self> {
        let (mouse_path, keyboard_path) = find_input_devices()?;

        info!("Found mouse: {:?}", mouse_path);
        info!("Found keyboard: {:?}", keyboard_path);

        let mouse = if let Some(ref path) = mouse_path {
            Some(Device::open(path).context("Failed to open mouse device")?)
        } else {
            warn!("No mouse device found");
            None
        };

        let keyboard = if let Some(ref path) = keyboard_path {
            Some(Device::open(path).context("Failed to open keyboard device")?)
        } else {
            warn!("No keyboard device found");
            None
        };

        // Default screen size; will be updated when grid config is received
        Ok(Self {
            mouse,
            keyboard,
            _mouse_path: mouse_path,
            _keyboard_path: keyboard_path,
            grabbed: false,
            cursor_x: 960,
            cursor_y: 540,
            screen_width: 1920,
            screen_height: 1080,
        })
    }

    pub fn set_screen_size(&mut self, width: i32, height: i32) {
        self.screen_width = width;
        self.screen_height = height;
    }

    fn read_mouse_event(&mut self) -> Result<Option<InputEvent>> {
        let mouse = match self.mouse.as_mut() {
            Some(m) => m,
            None => return Ok(None),
        };

        let events: Vec<_> = mouse
            .fetch_events()
            .context("Failed to fetch mouse events")?
            .collect();

        for ev in events {
            match ev.kind() {
                InputEventKind::RelAxis(axis) => {
                    let value = ev.value();
                    match axis {
                        RelativeAxisType::REL_X => {
                            self.cursor_x = (self.cursor_x + value).clamp(0, self.screen_width - 1);
                            return Ok(Some(InputEvent::MouseMove { dx: value, dy: 0 }));
                        }
                        RelativeAxisType::REL_Y => {
                            self.cursor_y = (self.cursor_y + value).clamp(0, self.screen_height - 1);
                            return Ok(Some(InputEvent::MouseMove { dx: 0, dy: value }));
                        }
                        RelativeAxisType::REL_WHEEL => {
                            return Ok(Some(InputEvent::MouseScroll { dx: 0, dy: value }));
                        }
                        RelativeAxisType::REL_HWHEEL => {
                            return Ok(Some(InputEvent::MouseScroll { dx: value, dy: 0 }));
                        }
                        _ => {}
                    }
                }
                InputEventKind::Key(key) => {
                    let pressed = ev.value() == 1;
                    if let Some(button) = evdev_key_to_mouse_button(key) {
                        return Ok(Some(InputEvent::MouseButton { button, pressed }));
                    }
                }
                InputEventKind::AbsAxis(axis) => {
                    let value = ev.value();
                    match axis {
                        AbsoluteAxisType::ABS_X => {
                            self.cursor_x = value;
                            return Ok(Some(InputEvent::MouseAbsolute {
                                x: value,
                                y: self.cursor_y,
                            }));
                        }
                        AbsoluteAxisType::ABS_Y => {
                            self.cursor_y = value;
                            return Ok(Some(InputEvent::MouseAbsolute {
                                x: self.cursor_x,
                                y: value,
                            }));
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        Ok(None)
    }

    fn read_keyboard_event(&mut self) -> Result<Option<InputEvent>> {
        let keyboard = match self.keyboard.as_mut() {
            Some(k) => k,
            None => return Ok(None),
        };

        let events: Vec<_> = keyboard
            .fetch_events()
            .context("Failed to fetch keyboard events")?
            .collect();

        for ev in events {
            if let InputEventKind::Key(key) = ev.kind() {
                let state = match ev.value() {
                    0 => KeyState::Released,
                    1 => KeyState::Pressed,
                    2 => KeyState::Pressed, // repeat treated as pressed
                    _ => continue,
                };
                return Ok(Some(InputEvent::Key {
                    code: KeyCode(key.0 as u32),
                    state,
                }));
            }
        }

        Ok(None)
    }
}

impl InputCapture for EvdevCapture {
    fn grab(&mut self) -> Result<()> {
        if self.grabbed {
            return Ok(());
        }
        if let Some(ref mut mouse) = self.mouse {
            mouse.grab().context("Failed to grab mouse")?;
        }
        if let Some(ref mut keyboard) = self.keyboard {
            keyboard.grab().context("Failed to grab keyboard")?;
        }
        self.grabbed = true;
        info!("Input grabbed");
        Ok(())
    }

    fn release(&mut self) -> Result<()> {
        if !self.grabbed {
            return Ok(());
        }
        if let Some(ref mut mouse) = self.mouse {
            let _ = mouse.ungrab();
        }
        if let Some(ref mut keyboard) = self.keyboard {
            let _ = keyboard.ungrab();
        }
        self.grabbed = false;
        info!("Input released");
        Ok(())
    }

    fn is_grabbed(&self) -> bool {
        self.grabbed
    }

    fn next_event(&mut self) -> Result<InputEvent> {
        loop {
            // Try mouse first
            if let Some(event) = self.read_mouse_event()? {
                return Ok(event);
            }
            // Then keyboard
            if let Some(event) = self.read_keyboard_event()? {
                return Ok(event);
            }
            // Small sleep to avoid busy-waiting
            std::thread::sleep(std::time::Duration::from_micros(100));
        }
    }

    fn cursor_position(&self) -> Result<(i32, i32)> {
        Ok((self.cursor_x, self.cursor_y))
    }
}

/// Find the primary mouse and keyboard evdev devices.
fn find_input_devices() -> Result<(Option<PathBuf>, Option<PathBuf>)> {
    let mut mouse_path: Option<PathBuf> = None;
    let mut keyboard_path: Option<PathBuf> = None;

    let input_dir = PathBuf::from("/dev/input");
    if !input_dir.exists() {
        anyhow::bail!("/dev/input not found");
    }

    let mut entries: Vec<_> = fs::read_dir(&input_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.starts_with("event"))
        })
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        if let Ok(device) = Device::open(&path) {
            let supported = device.supported_events();
            let name = device.name().unwrap_or("unknown");

            // Skip virtual devices we might have created
            if name.contains("monomouse") || name.contains("MonoMouse") {
                continue;
            }

            debug!("Checking device: {} ({})", path.display(), name);

            // Mouse: has relative axes (REL_X, REL_Y) or absolute axes, plus buttons
            if mouse_path.is_none()
                && supported.contains(EventType::RELATIVE)
                && supported.contains(EventType::KEY)
            {
                if let Some(keys) = device.supported_keys() {
                    if keys.contains(Key::BTN_LEFT) {
                        info!("Selected mouse: {} ({})", path.display(), name);
                        mouse_path = Some(path.clone());
                    }
                }
            }

            // Keyboard: has key events with actual keyboard keys
            if keyboard_path.is_none() && supported.contains(EventType::KEY) {
                if let Some(keys) = device.supported_keys() {
                    if keys.contains(Key::KEY_A)
                        && keys.contains(Key::KEY_Z)
                        && keys.contains(Key::KEY_ENTER)
                    {
                        info!("Selected keyboard: {} ({})", path.display(), name);
                        keyboard_path = Some(path);
                    }
                }
            }

            if mouse_path.is_some() && keyboard_path.is_some() {
                break;
            }
        }
    }

    Ok((mouse_path, keyboard_path))
}

fn evdev_key_to_mouse_button(key: Key) -> Option<MouseButton> {
    match key {
        Key::BTN_LEFT => Some(MouseButton::Left),
        Key::BTN_RIGHT => Some(MouseButton::Right),
        Key::BTN_MIDDLE => Some(MouseButton::Middle),
        Key::BTN_SIDE => Some(MouseButton::Extra1),
        Key::BTN_EXTRA => Some(MouseButton::Extra2),
        _ => None,
    }
}
