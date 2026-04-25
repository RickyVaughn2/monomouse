use anyhow::Result;
use monomouse_core::{InputEvent, KeyCode, KeyState, Monitor, MouseButton};
use crate::InputInjector;
use tracing::debug;

use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE,
    KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE,
    MOUSEINPUT, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL,
    MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, MOUSEEVENTF_HWHEEL,
    MOUSEEVENTF_VIRTUALDESK, MOUSE_EVENT_FLAGS,
};
use windows::Win32::UI::WindowsAndMessaging::GetSystemMetrics;
use windows::Win32::UI::WindowsAndMessaging::{SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN};

/// Input injection on Windows using SendInput.
pub struct WindowsInjector {
    virtual_screen_x: i32,
    virtual_screen_y: i32,
    virtual_screen_width: i32,
    virtual_screen_height: i32,
}

impl WindowsInjector {
    pub fn new() -> Result<Self> {
        let (vx, vy, vw, vh) = unsafe {
            (
                GetSystemMetrics(SM_XVIRTUALSCREEN),
                GetSystemMetrics(SM_YVIRTUALSCREEN),
                GetSystemMetrics(SM_CXVIRTUALSCREEN),
                GetSystemMetrics(SM_CYVIRTUALSCREEN),
            )
        };

        Ok(Self {
            virtual_screen_x: vx,
            virtual_screen_y: vy,
            virtual_screen_width: vw,
            virtual_screen_height: vh,
        })
    }

    fn send_mouse_input(&self, input: MOUSEINPUT) -> Result<()> {
        let raw = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 { mi: input },
        };
        unsafe {
            SendInput(&[raw], std::mem::size_of::<INPUT>() as i32);
        }
        Ok(())
    }

    fn send_key_input(&self, input: KEYBDINPUT) -> Result<()> {
        let raw = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 { ki: input },
        };
        unsafe {
            SendInput(&[raw], std::mem::size_of::<INPUT>() as i32);
        }
        Ok(())
    }

    /// Convert absolute screen coordinates to Windows normalized coordinates (0-65535).
    fn to_normalized(&self, abs_x: i32, abs_y: i32) -> (i32, i32) {
        let nx = ((abs_x - self.virtual_screen_x) as f64 / self.virtual_screen_width as f64
            * 65535.0) as i32;
        let ny = ((abs_y - self.virtual_screen_y) as f64 / self.virtual_screen_height as f64
            * 65535.0) as i32;
        (nx.clamp(0, 65535), ny.clamp(0, 65535))
    }
}

impl InputInjector for WindowsInjector {
    fn move_to(&mut self, monitor: &Monitor, x: i32, y: i32) -> Result<()> {
        let abs_x = monitor.x + x;
        let abs_y = monitor.y + y;
        let (nx, ny) = self.to_normalized(abs_x, abs_y);

        self.send_mouse_input(MOUSEINPUT {
            dx: nx,
            dy: ny,
            mouseData: 0,
            dwFlags: MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
            time: 0,
            dwExtraInfo: 0,
        })
    }

    fn inject(&mut self, event: &InputEvent) -> Result<()> {
        match event {
            InputEvent::MouseMove { dx, dy } => {
                self.send_mouse_input(MOUSEINPUT {
                    dx: *dx,
                    dy: *dy,
                    mouseData: 0,
                    dwFlags: MOUSEEVENTF_MOVE,
                    time: 0,
                    dwExtraInfo: 0,
                })
            }
            InputEvent::MouseAbsolute { x, y } => {
                let (nx, ny) = self.to_normalized(*x, *y);
                self.send_mouse_input(MOUSEINPUT {
                    dx: nx,
                    dy: ny,
                    mouseData: 0,
                    dwFlags: MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
                    time: 0,
                    dwExtraInfo: 0,
                })
            }
            InputEvent::MouseButton { button, pressed } => {
                let flags = match (button, pressed) {
                    (MouseButton::Left, true) => MOUSEEVENTF_LEFTDOWN,
                    (MouseButton::Left, false) => MOUSEEVENTF_LEFTUP,
                    (MouseButton::Right, true) => MOUSEEVENTF_RIGHTDOWN,
                    (MouseButton::Right, false) => MOUSEEVENTF_RIGHTUP,
                    (MouseButton::Middle, true) => MOUSEEVENTF_MIDDLEDOWN,
                    (MouseButton::Middle, false) => MOUSEEVENTF_MIDDLEUP,
                    (MouseButton::Extra1, true) | (MouseButton::Extra2, true) => MOUSEEVENTF_XDOWN,
                    (MouseButton::Extra1, false) | (MouseButton::Extra2, false) => MOUSEEVENTF_XUP,
                };

                let mouse_data = match button {
                    MouseButton::Extra1 => 1,
                    MouseButton::Extra2 => 2,
                    _ => 0,
                };

                self.send_mouse_input(MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: mouse_data,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                })
            }
            InputEvent::MouseScroll { dx, dy } => {
                if *dy != 0 {
                    self.send_mouse_input(MOUSEINPUT {
                        dx: 0,
                        dy: 0,
                        mouseData: (*dy * 120) as u32,
                        dwFlags: MOUSEEVENTF_WHEEL,
                        time: 0,
                        dwExtraInfo: 0,
                    })?;
                }
                if *dx != 0 {
                    self.send_mouse_input(MOUSEINPUT {
                        dx: 0,
                        dy: 0,
                        mouseData: (*dx * 120) as u32,
                        dwFlags: MOUSEEVENTF_HWHEEL,
                        time: 0,
                        dwExtraInfo: 0,
                    })?;
                }
                Ok(())
            }
            InputEvent::Key {
                code: KeyCode(scancode),
                state,
            } => {
                let mut flags = KEYEVENTF_SCANCODE;
                if *state == KeyState::Released {
                    flags |= KEYEVENTF_KEYUP;
                }

                self.send_key_input(KEYBDINPUT {
                    wVk: Default::default(),
                    wScan: *scancode as u16,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                })
            }
        }
    }
}
