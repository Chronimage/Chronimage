//! Phase 3 acceptance test: every preset applied at strength = 0 must
//! yield bit-identical output to the baseline. This is the invariant
//! that guarantees users can drag the strength slider down to 0 without
//! fearing lossy round-trips.

use chronimage::catalog::db::{open_pool, PoolOptions};
use chronimage::develop::ops::Operations;
use chronimage::develop::pipeline;
use chronimage::develop::presets::{builtin_presets, seed_builtins, Preset};
use image::{ImageBuffer, Rgb};
use sqlx::SqlitePool;

async fn load_seeded_presets(pool: &SqlitePool) -> Vec<Preset> {
    seed_builtins(pool).await.expect("seed");
    chronimage::develop::presets::list(pool, None)
        .await
        .expect("list")
}

#[tokio::test]
async fn every_builtin_preset_at_strength_zero_is_identity() {
    let pool = open_pool(PoolOptions::new(":memory:".into()))
        .await
        .expect("pool");
    let presets = load_seeded_presets(&pool).await;
    assert!(!presets.is_empty(), "must have builtins seeded");

    let img: ImageBuffer<Rgb<u8>, Vec<u8>> =
        ImageBuffer::from_fn(32, 32, |x, y| Rgb([(x * 7) as u8, (y * 5) as u8, 128u8]));
    let baseline = pipeline::apply(&img, &Operations::identity());

    for preset in &presets {
        let preset_ops = preset.operations().expect("parse preset");
        // Blend baseline (identity) → preset at strength 0 must equal
        // identity.
        let blended = Operations::identity().blend(preset_ops, 0);
        assert!(
            blended.is_identity(),
            "preset {} blended at strength 0 was not identity: {blended:?}",
            preset.name
        );
        let out = pipeline::apply(&img, &blended);
        assert_eq!(
            out.as_raw(),
            baseline.as_raw(),
            "preset {} at strength 0 altered pixel output",
            preset.name
        );
    }
}

#[tokio::test]
async fn preset_at_strength_one_hundred_is_the_preset() {
    // Sanity pair: blend at 100 must equal the preset's own ops.
    let all = builtin_presets();
    for (name, _group, _desc, ops) in all {
        let blended = Operations::identity().blend(ops.clone(), 100);
        assert!(
            (blended.exposure - ops.exposure).abs() < 1e-6,
            "{name}: exposure mismatch"
        );
        assert!(
            (blended.saturation - ops.saturation).abs() < 1e-6,
            "{name}: saturation mismatch"
        );
    }
}
