use anyhow::Result;
use monomouse_core::Monitor;
use uuid::Uuid;

use windows::core::BOOL;
use windows::Win32::Foundation::{LPARAM, RECT, TRUE};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR,
    MONITORINFOEXW,
};

/// Detect monitors on Windows using EnumDisplayMonitors.
pub fn detect_monitors() -> Result<Vec<Monitor>> {
    let machine_id = get_machine_id();
    let mut monitors: Vec<Monitor> = Vec::new();

    unsafe {
        let monitors_ptr = &mut monitors as *mut Vec<Monitor> as isize;
        let machine_id_ptr = &machine_id as *const Uuid as isize;

        // Pack both pointers into a struct on the stack
        let context = [monitors_ptr, machine_id_ptr];

        EnumDisplayMonitors(
            None,
            None,
            Some(enum_monitor_callback),
            LPARAM(&context as *const _ as isize),
        ).ok()?;
    }

    Ok(monitors)
}

unsafe extern "system" fn enum_monitor_callback(
    hmonitor: HMONITOR,
    _hdc: HDC,
    _rect: *mut RECT,
    lparam: LPARAM,
) -> BOOL {
    let context = &*(lparam.0 as *const [isize; 2]);
    let monitors = &mut *(context[0] as *mut Vec<Monitor>);
    let machine_id = *(context[1] as *const Uuid);

    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;

    if GetMonitorInfoW(hmonitor, &mut info as *mut _ as *mut _).as_bool() {
        let rect = info.monitorInfo.rcMonitor;
        let width = (rect.right - rect.left) as u32;
        let height = (rect.bottom - rect.top) as u32;
        let x = rect.left;
        let y = rect.top;

        // Get the device name
        let name = String::from_utf16_lossy(&info.szDevice)
            .trim_end_matches('\0')
            .to_string();

        let name = if name.is_empty() {
            format!("Monitor-{}", monitors.len())
        } else {
            name
        };

        // Check DPI scaling
        let scale = get_monitor_scale(hmonitor);

        let mut monitor = Monitor::new(machine_id, name, width, height, x, y);
        monitor.scale = scale;
        monitors.push(monitor);
    }

    TRUE
}

fn get_monitor_scale(hmonitor: HMONITOR) -> f64 {
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};

    let mut dpi_x: u32 = 96;
    let mut dpi_y: u32 = 96;

    unsafe {
        let _ = GetDpiForMonitor(hmonitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
    }

    dpi_x as f64 / 96.0
}

fn get_machine_id() -> Uuid {
    if let Ok(name) = std::env::var("COMPUTERNAME") {
        Uuid::new_v5(&Uuid::NAMESPACE_DNS, name.as_bytes())
    } else {
        Uuid::new_v4()
    }
}
