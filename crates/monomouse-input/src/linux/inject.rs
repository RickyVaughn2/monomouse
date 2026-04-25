use anyhow::{Context, Result};
use monomouse_core::{InputEvent, KeyCode, KeyState, Monitor, MouseButton};
use crate::InputInjector;
use tracing::info;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::unix::io::AsRawFd;

// Linux uinput constants
const UINPUT_PATH: &str = "/dev/uinput";

// ioctl constants
const UI_SET_EVBIT: u64 = 0x40045564;
const UI_SET_KEYBIT: u64 = 0x40045565;
const UI_SET_RELBIT: u64 = 0x40045566;
const UI_SET_ABSBIT: u64 = 0x40045567;
const UI_DEV_CREATE: u64 = 0x5501;
const UI_DEV_DESTROY: u64 = 0x5502;

// Event types
const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const EV_REL: u16 = 0x02;
const EV_ABS: u16 = 0x03;

// Relative axes
const REL_X: u16 = 0x00;
const REL_Y: u16 = 0x01;
const REL_WHEEL: u16 = 0x08;
const REL_HWHEEL: u16 = 0x06;

// Absolute axes
const ABS_X: u16 = 0x00;
const ABS_Y: u16 = 0x01;

// Sync
const SYN_REPORT: u16 = 0x00;

// Mouse buttons
const BTN_LEFT: u16 = 0x110;
const BTN_RIGHT: u16 = 0x111;
const BTN_MIDDLE: u16 = 0x112;
const BTN_SIDE: u16 = 0x113;
const BTN_EXTRA: u16 = 0x114;

/// uinput_setup struct for creating the virtual device
#[repr(C)]
struct UinputSetup {
    id: InputId,
    name: [u8; 80],
    ff_effects_max: u32,
}

#[repr(C)]
struct InputId {
    bustype: u16,
    vendor: u16,
    product: u16,
    version: u16,
}

/// Input event struct matching the kernel's input_event
#[repr(C)]
struct RawInputEvent {
    time_sec: u64,
    time_usec: u64,
    type_: u16,
    code: u16,
    value: i32,
}

/// Input injection using Linux uinput virtual device.
pub struct UinputInjector {
    file: File,
}

impl UinputInjector {
    pub fn new() -> Result<Self> {
        let file = OpenOptions::new()
            .write(true)
            .open(UINPUT_PATH)
            .context("Failed to open /dev/uinput. Run as root or add user to 'input' group.")?;

        let fd = file.as_raw_fd();

        unsafe {
            // Enable event types
            ioctl(fd, UI_SET_EVBIT, EV_KEY as u64)?;
            ioctl(fd, UI_SET_EVBIT, EV_REL as u64)?;
            ioctl(fd, UI_SET_EVBIT, EV_ABS as u64)?;
            ioctl(fd, UI_SET_EVBIT, EV_SYN as u64)?;

            // Enable mouse buttons
            ioctl(fd, UI_SET_KEYBIT, BTN_LEFT as u64)?;
            ioctl(fd, UI_SET_KEYBIT, BTN_RIGHT as u64)?;
            ioctl(fd, UI_SET_KEYBIT, BTN_MIDDLE as u64)?;
            ioctl(fd, UI_SET_KEYBIT, BTN_SIDE as u64)?;
            ioctl(fd, UI_SET_KEYBIT, BTN_EXTRA as u64)?;

            // Enable keyboard keys (1-248)
            for key in 1u64..=248 {
                ioctl(fd, UI_SET_KEYBIT, key)?;
            }

            // Enable relative axes
            ioctl(fd, UI_SET_RELBIT, REL_X as u64)?;
            ioctl(fd, UI_SET_RELBIT, REL_Y as u64)?;
            ioctl(fd, UI_SET_RELBIT, REL_WHEEL as u64)?;
            ioctl(fd, UI_SET_RELBIT, REL_HWHEEL as u64)?;

            // Enable absolute axes
            ioctl(fd, UI_SET_ABSBIT, ABS_X as u64)?;
            ioctl(fd, UI_SET_ABSBIT, ABS_Y as u64)?;

            // Set up absolute axis ranges via uinput_abs_setup
            setup_abs_axis(fd, ABS_X, 0, 32767)?;
            setup_abs_axis(fd, ABS_Y, 0, 32767)?;

            // Create the device
            let mut setup = UinputSetup {
                id: InputId {
                    bustype: 0x03, // BUS_USB
                    vendor: 0x4d4d,  // "MM" for MonoMouse
                    product: 0x0001,
                    version: 1,
                },
                name: [0u8; 80],
                ff_effects_max: 0,
            };

            let name = b"MonoMouse Virtual Input";
            setup.name[..name.len()].copy_from_slice(name);

            // Write the setup struct
            let setup_bytes = std::slice::from_raw_parts(
                &setup as *const UinputSetup as *const u8,
                std::mem::size_of::<UinputSetup>(),
            );

            // Use UI_DEV_SETUP ioctl
            const UI_DEV_SETUP: u64 = 0x405C5503;
            libc::ioctl(fd, UI_DEV_SETUP, setup_bytes.as_ptr());

            // Create the device
            if libc::ioctl(fd, UI_DEV_CREATE) < 0 {
                anyhow::bail!("UI_DEV_CREATE failed: {}", std::io::Error::last_os_error());
            }
        }

        // Give the kernel time to register the device
        std::thread::sleep(std::time::Duration::from_millis(200));

        info!("Created MonoMouse virtual input device");
        Ok(Self { file })
    }

    fn write_event(&mut self, type_: u16, code: u16, value: i32) -> Result<()> {
        let event = RawInputEvent {
            time_sec: 0,
            time_usec: 0,
            type_,
            code,
            value,
        };

        let bytes = unsafe {
            std::slice::from_raw_parts(
                &event as *const RawInputEvent as *const u8,
                std::mem::size_of::<RawInputEvent>(),
            )
        };

        self.file.write_all(bytes)?;
        Ok(())
    }

    fn sync(&mut self) -> Result<()> {
        self.write_event(EV_SYN, SYN_REPORT, 0)?;
        self.file.flush()?;
        Ok(())
    }
}

impl Drop for UinputInjector {
    fn drop(&mut self) {
        unsafe {
            libc::ioctl(self.file.as_raw_fd(), UI_DEV_DESTROY);
        }
    }
}

impl InputInjector for UinputInjector {
    fn move_to(&mut self, monitor: &Monitor, x: i32, y: i32) -> Result<()> {
        // Convert to absolute coordinates in the monitor's space
        let abs_x = monitor.x + x;
        let abs_y = monitor.y + y;

        // Scale to uinput absolute range (0-32767)
        // This is a simplification; proper implementation would account for
        // the full virtual desktop size across all monitors
        let scaled_x = ((abs_x as f64 / monitor.width as f64) * 32767.0) as i32;
        let scaled_y = ((abs_y as f64 / monitor.height as f64) * 32767.0) as i32;

        self.write_event(EV_ABS, ABS_X, scaled_x)?;
        self.write_event(EV_ABS, ABS_Y, scaled_y)?;
        self.sync()?;
        Ok(())
    }

    fn inject(&mut self, event: &InputEvent) -> Result<()> {
        match event {
            InputEvent::MouseMove { dx, dy } => {
                if *dx != 0 {
                    self.write_event(EV_REL, REL_X, *dx)?;
                }
                if *dy != 0 {
                    self.write_event(EV_REL, REL_Y, *dy)?;
                }
                self.sync()?;
            }
            InputEvent::MouseAbsolute { x, y } => {
                self.write_event(EV_ABS, ABS_X, *x)?;
                self.write_event(EV_ABS, ABS_Y, *y)?;
                self.sync()?;
            }
            InputEvent::MouseButton { button, pressed } => {
                let code = match button {
                    MouseButton::Left => BTN_LEFT,
                    MouseButton::Right => BTN_RIGHT,
                    MouseButton::Middle => BTN_MIDDLE,
                    MouseButton::Extra1 => BTN_SIDE,
                    MouseButton::Extra2 => BTN_EXTRA,
                };
                self.write_event(EV_KEY, code, if *pressed { 1 } else { 0 })?;
                self.sync()?;
            }
            InputEvent::MouseScroll { dx, dy } => {
                if *dy != 0 {
                    self.write_event(EV_REL, REL_WHEEL, *dy)?;
                }
                if *dx != 0 {
                    self.write_event(EV_REL, REL_HWHEEL, *dx)?;
                }
                self.sync()?;
            }
            InputEvent::Key { code: KeyCode(scancode), state } => {
                let value = match state {
                    KeyState::Pressed => 1,
                    KeyState::Released => 0,
                };
                self.write_event(EV_KEY, *scancode as u16, value)?;
                self.sync()?;
            }
        }
        Ok(())
    }
}

unsafe fn ioctl(fd: i32, request: u64, value: u64) -> Result<()> {
    if unsafe { libc::ioctl(fd, request, value) } < 0 {
        anyhow::bail!("ioctl 0x{:x} failed: {}", request, std::io::Error::last_os_error());
    }
    Ok(())
}

#[repr(C)]
struct UinputAbsSetup {
    code: u16,
    _padding: [u8; 2],
    absinfo: AbsInfo,
}

#[repr(C)]
struct AbsInfo {
    value: i32,
    minimum: i32,
    maximum: i32,
    fuzz: i32,
    flat: i32,
    resolution: i32,
}

unsafe fn setup_abs_axis(fd: i32, code: u16, min: i32, max: i32) -> Result<()> {
    const UI_ABS_SETUP: u64 = 0x401C5504;
    let setup = UinputAbsSetup {
        code,
        _padding: [0; 2],
        absinfo: AbsInfo {
            value: 0,
            minimum: min,
            maximum: max,
            fuzz: 0,
            flat: 0,
            resolution: 0,
        },
    };
    if unsafe { libc::ioctl(fd, UI_ABS_SETUP, &setup as *const UinputAbsSetup) } < 0 {
        anyhow::bail!("UI_ABS_SETUP failed: {}", std::io::Error::last_os_error());
    }
    Ok(())
}
