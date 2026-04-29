# Changelog

## [0.2.0-beta.6](https://github.com/Chronimage/Chronimage/compare/v0.1.0-beta.6...v0.2.0-beta.6) (2026-04-29)


### Features

* **ai:** shared decode cache for ai stages and delete leaves originals alone ([#81](https://github.com/Chronimage/Chronimage/issues/81)) ([70b4d38](https://github.com/Chronimage/Chronimage/commit/70b4d38e2d81c99b4a425ac9ac2032e35957c30a))
* **catalog,develop:** wire facets + expand develop preset library ([#83](https://github.com/Chronimage/Chronimage/issues/83)) ([313a90c](https://github.com/Chronimage/Chronimage/commit/313a90cdc542d1435a4820f9a2d8c4719de5952f))
* **catalog:** phase 2 complete — cull verdict + export + manual tagging ([#62](https://github.com/Chronimage/Chronimage/issues/62)) ([08a9ffe](https://github.com/Chronimage/Chronimage/commit/08a9ffe471e9d1d9ca1b52bfe06da1d04f6f2d96))
* **catalog:** phase 2 finishing pass — audit log + retention + nima + upload ui + acceptance tests ([#65](https://github.com/Chronimage/Chronimage/issues/65)) ([44b06ee](https://github.com/Chronimage/Chronimage/commit/44b06ee54423af38b9af1711e810228367ddea42))
* **catalog:** phase 2 perf hotfix + masonry rehaul + clustering wire ([#58](https://github.com/Chronimage/Chronimage/issues/58)) ([1f7401c](https://github.com/Chronimage/Chronimage/commit/1f7401c32744bb9dda31510834b862fd3be9617d))
* **catalog:** phase 4 week 1 — xmp + .chronimage-ignore + trip clustering + shortcut overlay ([#67](https://github.com/Chronimage/Chronimage/issues/67)) ([bcedb78](https://github.com/Chronimage/Chronimage/commit/bcedb78f1cb96cfd1c951406d7f3900eb1638cd6))
* **catalog:** phase 4 week 5 — sidecar supervisor + geonames runtime load ([#70](https://github.com/Chronimage/Chronimage/issues/70)) ([422153c](https://github.com/Chronimage/Chronimage/commit/422153c44914c7cab0a0b5cac8b1f1d312f2e0f1))
* **catalog:** source disconnect progress, overlap auto-absorb, and heic fixes ([#75](https://github.com/Chronimage/Chronimage/issues/75)) ([b0accf7](https://github.com/Chronimage/Chronimage/commit/b0accf7d069dd6efaedfc67533afe90bab999409))
* **develop:** 3-pane resizable layout, shadcn sweep, drawable masks ([#95](https://github.com/Chronimage/Chronimage/issues/95)) ([7fd907d](https://github.com/Chronimage/Chronimage/commit/7fd907d09ab5d7843c4e37f8002b52d984ae3249))
* **develop:** box prompts, two-pass decode, guided-filter refine ([#97](https://github.com/Chronimage/Chronimage/issues/97)) ([05576b7](https://github.com/Chronimage/Chronimage/commit/05576b7805574f67168d4a243d0bfcdca704d363))
* **develop:** bundle sam masks for lightroom-style edits ([#85](https://github.com/Chronimage/Chronimage/issues/85)) ([0ce7d2a](https://github.com/Chronimage/Chronimage/commit/0ce7d2ae13fcb4f03ca7e5b94adb67587e8677e0))
* **develop:** interactive tone curves (master + r/g/b/luma) + readme refresh ([#73](https://github.com/Chronimage/Chronimage/issues/73)) ([6b93610](https://github.com/Chronimage/Chronimage/commit/6b93610f77ab9364198c0e347eb81cf51d94f094))
* **develop:** lightroom-parity mask toolkit + editable sliders ([#98](https://github.com/Chronimage/Chronimage/issues/98)) ([fcff926](https://github.com/Chronimage/Chronimage/commit/fcff9267197fe2ba3e50d0e419a7bbf16850af03))
* **develop:** lightroom-style overhaul — panels, real histogram, sharper preview, seamless zoom, new ops ([#91](https://github.com/Chronimage/Chronimage/issues/91)) ([f35fbd9](https://github.com/Chronimage/Chronimage/commit/f35fbd9725081ca817db7479f396ac96c3dd55ff))
* **develop:** remove people screen + zoom via box resize not scale ([#92](https://github.com/Chronimage/Chronimage/issues/92)) ([05f8968](https://github.com/Chronimage/Chronimage/commit/05f896861c416078fb7964d8a45f3c444b805b8d))
* **develop:** sync editor and side-panel edits via shared operations ([#82](https://github.com/Chronimage/Chronimage/issues/82)) ([6c91643](https://github.com/Chronimage/Chronimage/commit/6c91643be7b12ea03119620eed8a108be01efc0d))
* **export:** phase 2 deferred — ann + heic/mozjpeg + gphotos + onedrive upload ([#64](https://github.com/Chronimage/Chronimage/issues/64)) ([b9a907b](https://github.com/Chronimage/Chronimage/commit/b9a907b236a2e92ca8300df7352749a1654c670f))
* **release:** phase 5 week 1 — license + telemetry + governance docs ([#69](https://github.com/Chronimage/Chronimage/issues/69)) ([88b657e](https://github.com/Chronimage/Chronimage/commit/88b657ea02af3b22b10469614149d6733ed0a782))
* **screens:** full frontend stubs for cull · cull bin · develop ([#59](https://github.com/Chronimage/Chronimage/issues/59)) ([1e7fa44](https://github.com/Chronimage/Chronimage/commit/1e7fa448018c43fc9386c7aeeee12ebc585ab5c0))
* **tauri-cli:** chronimage cli beyond phase-0 stubs — real subcommands ([#71](https://github.com/Chronimage/Chronimage/issues/71)) ([47cda6c](https://github.com/Chronimage/Chronimage/commit/47cda6c12db8a9eab55e314774a706f79fda3c2e))
* **ui:** shadcn foundation + bolder redesign ([#94](https://github.com/Chronimage/Chronimage/issues/94)) ([50c6acc](https://github.com/Chronimage/Chronimage/commit/50c6acc2ff47e3b604fc719ef7a81f859c8c60d8))


### Bug Fixes

* **ai:** resolve 9 stale todos — moondream2 + lift-shift + rediscovery ([#72](https://github.com/Chronimage/Chronimage/issues/72)) ([31da85b](https://github.com/Chronimage/Chronimage/commit/31da85be5f4cc42143b0d01fc767f0d9dd1a57c1))
* **develop:** repair sam2 subject mask end-to-end ([#96](https://github.com/Chronimage/Chronimage/issues/96)) ([67b7789](https://github.com/Chronimage/Chronimage/commit/67b7789fb6102c51ffe72311eec0fd1726b507aa))


### Performance Improvements

* **ai:** instant settings open via apphandle bundled-dir + stat hash cache ([#57](https://github.com/Chronimage/Chronimage/issues/57)) ([64b8b28](https://github.com/Chronimage/Chronimage/commit/64b8b28d38c20fd3f1999115236e763146f4a3eb))
