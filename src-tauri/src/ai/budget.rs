//! Hardware tier detection for model selection.
//!
//! On Windows we enumerate DXGI adapters to read `DedicatedVideoMemory`.
//! On non-Windows targets (or when enumeration fails) we return `CpuOnly`.

use serde::Serialize;

/// Coarse hardware tier that drives model variant selection.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum HardwareTier {
    /// No discrete GPU, or dedicated VRAM < 4 GB.
    CpuOnly,
    /// Dedicated VRAM 4–8 GB (e.g. RTX 3060).
    GpuLow,
    /// Dedicated VRAM > 8 GB (e.g. RTX 3080 / RX 7900 XT).
    GpuHigh,
}

/// Snapshot of relevant hardware capabilities at startup.
#[derive(Debug, Clone, Serialize)]
pub struct HardwareInfo {
    pub tier: HardwareTier,
    /// Dedicated VRAM in megabytes; 0 for `CpuOnly`.
    pub vram_mb: u64,
    /// Human-readable adapter description (e.g. "NVIDIA GeForce RTX 3060").
    pub adapter_name: String,
}

impl HardwareInfo {
    /// Recommended ONNX session batch size for this tier.
    pub fn batch_size(&self) -> usize {
        match self.tier {
            HardwareTier::CpuOnly => 8,
            HardwareTier::GpuLow | HardwareTier::GpuHigh => 64,
        }
    }
}

/// Detect hardware tier.
/// The first discrete (non-software) adapter with the most dedicated VRAM wins.
pub fn detect() -> HardwareInfo {
    #[cfg(target_os = "windows")]
    {
        detect_windows().unwrap_or_else(|_| cpu_only_info())
    }
    #[cfg(not(target_os = "windows"))]
    {
        cpu_only_info()
    }
}

fn cpu_only_info() -> HardwareInfo {
    HardwareInfo {
        tier: HardwareTier::CpuOnly,
        vram_mb: 0,
        adapter_name: "stub".to_string(),
    }
}

#[cfg(target_os = "windows")]
fn detect_windows() -> crate::AppResult<HardwareInfo> {
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE,
    };

    // Safety: CreateDXGIFactory1 is safe to call from any thread and does
    // not require COM initialisation.
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }
        .map_err(|e| crate::AppError::Internal(format!("CreateDXGIFactory1: {e}")))?;

    let mut best_vram: u64 = 0;
    let mut best_name = String::new();

    let mut idx: u32 = 0;
    loop {
        let adapter = unsafe { factory.EnumAdapters1(idx) };
        match adapter {
            Err(_) => break, // DXGI_ERROR_NOT_FOUND — enumeration complete
            Ok(adapter) => {
                let desc = unsafe { adapter.GetDesc1() }
                    .map_err(|e| crate::AppError::Internal(format!("GetDesc1: {e}")))?;

                // Skip software/WARP adapters.
                let is_software = (desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32) != 0;
                if !is_software {
                    let vram = desc.DedicatedVideoMemory as u64;
                    if vram > best_vram {
                        best_vram = vram;
                        // desc.Description is a null-terminated UTF-16 array.
                        let end = desc
                            .Description
                            .iter()
                            .position(|&c| c == 0)
                            .unwrap_or(desc.Description.len());
                        best_name = String::from_utf16_lossy(&desc.Description[..end]);
                    }
                }
                idx += 1;
            }
        }
    }

    let vram_mb = best_vram / (1024 * 1024);
    let tier = match vram_mb {
        0..=3_999 => HardwareTier::CpuOnly,
        4_000..=8_191 => HardwareTier::GpuLow,
        _ => HardwareTier::GpuHigh,
    };

    if best_name.is_empty() {
        best_name = "Unknown".to_string();
    }

    Ok(HardwareInfo {
        tier,
        vram_mb,
        adapter_name: best_name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_returns_valid_tier() {
        let info = detect();
        // On any machine the tier must be one of the three variants.
        assert!(matches!(
            info.tier,
            HardwareTier::CpuOnly | HardwareTier::GpuLow | HardwareTier::GpuHigh
        ));
    }

    #[test]
    fn cpu_only_has_zero_vram() {
        let info = cpu_only_info();
        assert_eq!(info.vram_mb, 0);
        assert_eq!(info.tier, HardwareTier::CpuOnly);
    }

    #[test]
    fn batch_size_for_cpu_is_8() {
        let info = HardwareInfo {
            tier: HardwareTier::CpuOnly,
            vram_mb: 0,
            adapter_name: "stub".into(),
        };
        assert_eq!(info.batch_size(), 8);
    }

    #[test]
    fn batch_size_for_gpu_is_64() {
        let info = HardwareInfo {
            tier: HardwareTier::GpuHigh,
            vram_mb: 10_000,
            adapter_name: "RTX 4090".into(),
        };
        assert_eq!(info.batch_size(), 64);
    }

    #[test]
    fn serializes_to_json() {
        let info = detect();
        let json = serde_json::to_string(&info).expect("serialize");
        assert!(json.contains("tier"));
        assert!(json.contains("vram_mb"));
        assert!(json.contains("adapter_name"));
    }
}
