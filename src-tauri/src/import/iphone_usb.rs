//! iPhone USB device enumeration via Windows Portable Devices (WPD).
//!
//! Uses `IPortableDeviceManager` to list connected MTP/WPD devices and
//! filters for Apple-manufactured devices (iPhone / iPad). File transfer
//! is deferred — this module only enumerates what is connected.

#[cfg(windows)]
pub use windows_impl::*;

#[cfg(not(windows))]
pub use stub::*;

// ── Windows implementation ────────────────────────────────────────────────────

#[cfg(windows)]
mod windows_impl {
    use windows::{
        core::PWSTR,
        Win32::{
            Devices::PortableDevices::{IPortableDeviceManager, PortableDeviceManager},
            System::Com::{
                CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
            },
        },
    };

    #[derive(Debug, Clone, serde::Serialize)]
    pub struct UsbDevice {
        pub device_id: String,
        pub friendly_name: String,
        pub manufacturer: String,
        pub description: String,
    }

    /// List all WPD/MTP devices, filtered to Apple-manufactured devices.
    pub fn list_iphone_devices() -> crate::AppResult<Vec<UsbDevice>> {
        unsafe { list_inner() }
    }

    unsafe fn list_inner() -> crate::AppResult<Vec<UsbDevice>> {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

        let manager: IPortableDeviceManager =
            CoCreateInstance(&PortableDeviceManager, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| crate::AppError::Internal(format!("CoCreateInstance WPD: {e}")))?;

        let mut count = 0u32;
        manager
            .GetDevices(std::ptr::null_mut(), &mut count)
            .map_err(|e| crate::AppError::Internal(format!("GetDevices count: {e}")))?;

        if count == 0 {
            return Ok(vec![]);
        }

        let mut ids: Vec<PWSTR> = vec![PWSTR::null(); count as usize];
        manager
            .GetDevices(ids.as_mut_ptr(), &mut count)
            .map_err(|e| crate::AppError::Internal(format!("GetDevices fill: {e}")))?;

        let mut devices = Vec::new();

        for raw_id in ids.into_iter().take(count as usize) {
            if raw_id.is_null() {
                continue;
            }
            let device_id = raw_id.to_string().unwrap_or_default();
            let wide_id: Vec<u16> = device_id.encode_utf16().chain(std::iter::once(0)).collect();
            let pcwstr = windows::core::PCWSTR(wide_id.as_ptr());

            let friendly_name =
                get_wpd_string(|buf, len| manager.GetDeviceFriendlyName(pcwstr, buf, len));
            let manufacturer =
                get_wpd_string(|buf, len| manager.GetDeviceManufacturer(pcwstr, buf, len));
            let description =
                get_wpd_string(|buf, len| manager.GetDeviceDescription(pcwstr, buf, len));

            if manufacturer.to_lowercase().contains("apple") {
                devices.push(UsbDevice {
                    device_id,
                    friendly_name,
                    manufacturer,
                    description,
                });
            }
        }

        Ok(devices)
    }

    /// Helper: call a WPD getter twice (len-probe then fill) and return the string.
    unsafe fn get_wpd_string<F>(mut f: F) -> String
    where
        F: FnMut(PWSTR, &mut u32) -> windows::core::Result<()>,
    {
        let mut len = 0u32;
        if f(PWSTR::null(), &mut len).is_err() || len == 0 {
            return String::new();
        }
        let mut buf: Vec<u16> = vec![0u16; len as usize];
        if f(PWSTR(buf.as_mut_ptr()), &mut len).is_err() {
            return String::new();
        }
        let end = len.saturating_sub(1) as usize;
        String::from_utf16_lossy(&buf[..end])
    }
}

// ── Non-Windows stub ──────────────────────────────────────────────────────────

#[cfg(not(windows))]
mod stub {
    #[derive(Debug, Clone, serde::Serialize)]
    pub struct UsbDevice {
        pub device_id: String,
        pub friendly_name: String,
        pub manufacturer: String,
        pub description: String,
    }

    pub fn list_iphone_devices() -> crate::AppResult<Vec<UsbDevice>> {
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_iphone_devices_does_not_panic() {
        // Without a connected iPhone this returns Ok([]); just ensure no crash.
        let result = list_iphone_devices();
        assert!(result.is_ok(), "unexpected error: {:?}", result.err());
    }
}
