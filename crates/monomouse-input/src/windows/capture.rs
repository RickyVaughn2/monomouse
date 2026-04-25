use anyhow::{Context, Result};
use monomouse_core::{InputEvent, KeyCode, KeyState, MouseButton};
use crate::InputCapture;
use tracing::info;
use std::sync::mpsc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use windows::Win32::Foundation::{LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetCursorPos, GetMessageW, SetWindowsHookExW, UnhookWindowsHookEx,
    HHOOK, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT,
    WH_KEYBOARD_LL, WH_MOUSE_LL,
    WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_RBUTTONDOWN, WM_RBUTTONUP,
    WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL,
    WM_XBUTTONDOWN, WM_XBUTTONUP, WM_MOUSEHWHEEL,
};

use std::cell::RefCell;

thread_local! {
    static EVENT_SENDER: RefCell<Option<mpsc::Sender<InputEvent>>> = const { RefCell::new(None) };
    static GRABBED: RefCell<bool> = const { RefCell::new(false) };
    static MOUSE_HOOK: RefCell<Option<HHOOK>> = const { RefCell::new(None) };
    static KB_HOOK: RefCell<Option<HHOOK>> = const { RefCell::new(None) };
}

/// Input capture on Windows using low-level hooks.
pub struct WindowsCapture {
    receiver: mpsc::Receiver<InputEvent>,
    sender: mpsc::Sender<InputEvent>,
    hook_thread: Option<std::thread::JoinHandle<()>>,
    grabbed: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
}

impl WindowsCapture {
    pub fn new() -> Result<Self> {
        let (sender, receiver) = mpsc::channel();
        let grabbed = Arc::new(AtomicBool::new(false));
        let running = Arc::new(AtomicBool::new(true));

        let sender_clone = sender.clone();
        let grabbed_clone = Arc::clone(&grabbed);
        let running_clone = Arc::clone(&running);

        let hook_thread = std::thread::spawn(move || {
            run_hook_loop(sender_clone, grabbed_clone, running_clone);
        });

        Ok(Self {
            receiver,
            sender,
            hook_thread: Some(hook_thread),
            grabbed,
            running,
        })
    }
}

impl Drop for WindowsCapture {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(thread) = self.hook_thread.take() {
            let _ = thread.join();
        }
    }
}

impl InputCapture for WindowsCapture {
    fn grab(&mut self) -> Result<()> {
        self.grabbed.store(true, Ordering::SeqCst);
        info!("Windows input grabbed");
        Ok(())
    }

    fn release(&mut self) -> Result<()> {
        self.grabbed.store(false, Ordering::SeqCst);
        info!("Windows input released");
        Ok(())
    }

    fn is_grabbed(&self) -> bool {
        self.grabbed.load(Ordering::SeqCst)
    }

    fn next_event(&mut self) -> Result<InputEvent> {
        self.receiver
            .recv()
            .context("Hook thread disconnected")
    }

    fn cursor_position(&self) -> Result<(i32, i32)> {
        unsafe {
            let mut point = POINT::default();
            GetCursorPos(&mut point)?;
            Ok((point.x, point.y))
        }
    }
}

fn run_hook_loop(
    sender: mpsc::Sender<InputEvent>,
    grabbed: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
) {
    unsafe {
        EVENT_SENDER.with(|s| *s.borrow_mut() = Some(sender));

        let mouse_hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), None, 0)
            .expect("Failed to set mouse hook");

        let kb_hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), None, 0)
            .expect("Failed to set keyboard hook");

        MOUSE_HOOK.with(|h| *h.borrow_mut() = Some(mouse_hook));
        KB_HOOK.with(|h| *h.borrow_mut() = Some(kb_hook));

        let mut msg = MSG::default();
        while running.load(Ordering::SeqCst) {
            GRABBED.with(|g| *g.borrow_mut() = grabbed.load(Ordering::SeqCst));
            if GetMessageW(&mut msg, None, 0, 0).as_bool() {
                // Just pump messages
            }
        }

        UnhookWindowsHookEx(mouse_hook);
        UnhookWindowsHookEx(kb_hook);
    }
}

unsafe extern "system" fn mouse_hook_proc(
    ncode: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if ncode >= 0 {
        let data = &*(lparam.0 as *const MSLLHOOKSTRUCT);
        let msg = wparam.0 as u32;

        let event = match msg {
            WM_MOUSEMOVE => {
                Some(InputEvent::MouseAbsolute {
                    x: data.pt.x,
                    y: data.pt.y,
                })
            }
            WM_LBUTTONDOWN => Some(InputEvent::MouseButton {
                button: MouseButton::Left,
                pressed: true,
            }),
            WM_LBUTTONUP => Some(InputEvent::MouseButton {
                button: MouseButton::Left,
                pressed: false,
            }),
            WM_RBUTTONDOWN => Some(InputEvent::MouseButton {
                button: MouseButton::Right,
                pressed: true,
            }),
            WM_RBUTTONUP => Some(InputEvent::MouseButton {
                button: MouseButton::Right,
                pressed: false,
            }),
            WM_MBUTTONDOWN => Some(InputEvent::MouseButton {
                button: MouseButton::Middle,
                pressed: true,
            }),
            WM_MBUTTONUP => Some(InputEvent::MouseButton {
                button: MouseButton::Middle,
                pressed: false,
            }),
            WM_MOUSEWHEEL => {
                let delta = (data.mouseData >> 16) as i16 as i32;
                Some(InputEvent::MouseScroll {
                    dx: 0,
                    dy: delta / 120,
                })
            }
            WM_MOUSEHWHEEL => {
                let delta = (data.mouseData >> 16) as i16 as i32;
                Some(InputEvent::MouseScroll {
                    dx: delta / 120,
                    dy: 0,
                })
            }
            WM_XBUTTONDOWN => {
                let button_num = (data.mouseData >> 16) & 0xFFFF;
                let button = if button_num == 1 {
                    MouseButton::Extra1
                } else {
                    MouseButton::Extra2
                };
                Some(InputEvent::MouseButton {
                    button,
                    pressed: true,
                })
            }
            WM_XBUTTONUP => {
                let button_num = (data.mouseData >> 16) & 0xFFFF;
                let button = if button_num == 1 {
                    MouseButton::Extra1
                } else {
                    MouseButton::Extra2
                };
                Some(InputEvent::MouseButton {
                    button,
                    pressed: false,
                })
            }
            _ => None,
        };

        if let Some(event) = event {
            EVENT_SENDER.with(|s| {
                if let Some(ref sender) = *s.borrow() {
                    let _ = sender.send(event);
                }
            });

            let is_grabbed = GRABBED.with(|g| *g.borrow());
            if is_grabbed {
                return LRESULT(1);
            }
        }
    }

    MOUSE_HOOK.with(|h| {
        CallNextHookEx(h.borrow().as_ref().copied(), ncode, wparam, lparam)
    })
}

unsafe extern "system" fn keyboard_hook_proc(
    ncode: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if ncode >= 0 {
        let data = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let msg = wparam.0 as u32;

        let state = match msg {
            WM_KEYDOWN | WM_SYSKEYDOWN => KeyState::Pressed,
            WM_KEYUP | WM_SYSKEYUP => KeyState::Released,
            _ => {
                return KB_HOOK.with(|h| {
                    CallNextHookEx(h.borrow().as_ref().copied(), ncode, wparam, lparam)
                });
            }
        };

        let event = InputEvent::Key {
            code: KeyCode(data.scanCode),
            state,
        };

        EVENT_SENDER.with(|s| {
            if let Some(ref sender) = *s.borrow() {
                let _ = sender.send(event);
            }
        });

        let is_grabbed = GRABBED.with(|g| *g.borrow());
        if is_grabbed {
            return LRESULT(1);
        }
    }

    KB_HOOK.with(|h| {
        CallNextHookEx(h.borrow().as_ref().copied(), ncode, wparam, lparam)
    })
}
