---
description: Run the Chronimage CLI against a test library path without writing to the catalog.
argument-hint: <path-to-photos-directory>
---

Run `cargo run -p chronimage-cli -- import --dry-run "$ARGUMENTS"` and summarize:

- Number of files found by extension
- RAW+JPG pairs auto-detected
- Potential dupe groups (pHash pre-filter only)
- Estimated import time at current host throughput

Report any RAW format decode failures with the exact file path and error.

If `$ARGUMENTS` is empty or not a directory, print usage and exit.

This command does NOT touch the catalog DB.
