//! Develop-screen decoded-pixel cache.
//!
//! Develop renders the same source pixels through the CPU pipeline on every
//! slider drag. Re-decoding from the on-disk JPEG thumbnail (or a RAW file)
//! every time costs hundreds of ms — well over the 80 ms debounce budget
//! that keeps slider drags feeling instant. This module keeps a tiny LRU
//! holding the decoded `RgbImage` for the currently-open photo plus the
//! previous one (so flipping back-and-forth in the filmstrip is free).
//!
//! Capacity intentionally tiny: 2 × ~8.4 MB at the 2048-px long-edge
//! preview source. Anything larger duplicates what AI's image-util cache
//! already provides for AI stages.

use image::RgbImage;
use std::sync::{Arc, Mutex};

/// Max long-edge resolution we decode for the develop preview path.
/// At 3072 px the canvas always sees a downsample (the editor canvas
/// is at most ~2200 px on common HiDPI setups), which is the only way
/// to get edge detail comparable to Lightroom. 3072 × 2048 × 3 ≈ 19 MB
/// of resident RGB per cached photo (× 2 slots = ~38 MB), and JPEG
/// Q95 at this size encodes to ~1–1.5 MB → ~1.4–2 MB after base64.
pub const PREVIEW_LONG_EDGE: u32 = 3072;

const SLOTS: usize = 2;

pub struct DevelopDecodeCache {
    inner: Mutex<Vec<(i64, Arc<RgbImage>)>>,
}

impl Default for DevelopDecodeCache {
    fn default() -> Self {
        Self::new()
    }
}

impl DevelopDecodeCache {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Vec::with_capacity(SLOTS)),
        }
    }

    pub fn get(&self, photo_id: i64) -> Option<Arc<RgbImage>> {
        let mut guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let pos = guard.iter().position(|(id, _)| *id == photo_id)?;
        let entry = guard.remove(pos);
        let image = Arc::clone(&entry.1);
        guard.insert(0, entry);
        Some(image)
    }

    pub fn insert(&self, photo_id: i64, image: Arc<RgbImage>) {
        let mut guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        guard.retain(|(id, _)| *id != photo_id);
        guard.insert(0, (photo_id, image));
        guard.truncate(SLOTS);
    }

    pub fn evict(&self, photo_id: i64) {
        let mut guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        guard.retain(|(id, _)| *id != photo_id);
    }

    pub fn clear(&self) {
        let mut guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        guard.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(value: u8) -> Arc<RgbImage> {
        Arc::new(RgbImage::from_pixel(
            2,
            2,
            image::Rgb([value, value, value]),
        ))
    }

    #[test]
    fn insert_and_get_round_trip() {
        let cache = DevelopDecodeCache::new();
        cache.insert(1, fixture(10));
        let img = cache.get(1).expect("present");
        assert_eq!(img.get_pixel(0, 0).0, [10, 10, 10]);
    }

    #[test]
    fn caps_at_two_slots_evicting_oldest() {
        let cache = DevelopDecodeCache::new();
        cache.insert(1, fixture(1));
        cache.insert(2, fixture(2));
        cache.insert(3, fixture(3));
        assert!(cache.get(1).is_none(), "id=1 should have been evicted");
        assert!(cache.get(2).is_some());
        assert!(cache.get(3).is_some());
    }

    #[test]
    fn re_inserting_same_id_does_not_duplicate() {
        let cache = DevelopDecodeCache::new();
        cache.insert(1, fixture(1));
        cache.insert(1, fixture(2));
        cache.insert(2, fixture(2));
        // Both ids fit in the 2-slot cap.
        assert!(cache.get(1).is_some());
        assert!(cache.get(2).is_some());
    }

    #[test]
    fn get_promotes_to_most_recent() {
        let cache = DevelopDecodeCache::new();
        cache.insert(1, fixture(1));
        cache.insert(2, fixture(2));
        // Touch id=1 so the next eviction targets id=2.
        let _ = cache.get(1);
        cache.insert(3, fixture(3));
        assert!(cache.get(1).is_some());
        assert!(cache.get(2).is_none(), "id=2 should have been evicted");
        assert!(cache.get(3).is_some());
    }

    #[test]
    fn evict_removes_targeted_id() {
        let cache = DevelopDecodeCache::new();
        cache.insert(1, fixture(1));
        cache.insert(2, fixture(2));
        cache.evict(1);
        assert!(cache.get(1).is_none());
        assert!(cache.get(2).is_some());
    }
}
