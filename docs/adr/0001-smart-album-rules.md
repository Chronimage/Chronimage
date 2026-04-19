# ADR 0001 — Smart album rule schema

- **Status:** Accepted (Phase 1)
- **Date:** 2026-04-20
- **Supersedes:** —

## Context

Smart albums are dynamic collections whose membership is evaluated by applying a
rule against the `photos` table. Three consumers drive this design:

1. **UI rule editor (Phase 2):** Users build rules via a visual editor. Rules must
   be serializable to/from JSON and round-trippable without data loss.
2. **Auto-evaluator (every 10 min):** A background task calls `count_matching`
   and `matching_photo_ids` for every smart album. Evaluation must be expressible
   as a single SQL WHERE fragment appended to `SELECT … FROM photos`.
3. **Rediscovery rows (Phase 1 §13):** System albums with structured rules such as
   "unflagged favorites" and "first time on new camera" must be expressible in
   this schema.

The rules live in `smart_albums.rule_json` (TEXT with `CHECK (json_valid(…))`).
The evaluator in `src-tauri/src/catalog/rules.rs` owns parsing and SQL generation.

## Decision

### JSON schema

Rules are a tagged union over `"type"`. The full set of variants:

```jsonc
// Leaf predicates
{ "type": "tag",      "value": "golden_hour" }
{ "type": "cluster",  "value": "Ari" }
{ "type": "exif",     "field": "iso",        "op": "gte", "value": 3200 }
{ "type": "exif",     "field": "aperture",   "op": "lte", "value": 2.8 }
{ "type": "exif",     "field": "focal_mm",   "op": "gte", "value": 70 }
{ "type": "quality",  "field": "aesthetic",  "op": "gte", "value": 8.0 }
{ "type": "quality",  "field": "sharpness",  "op": "lt",  "value": 0.3 }
{ "type": "is_raw",   "value": true }
{ "type": "captured_at", "op": "between", "value": ["2024-01-01T00:00:00Z", "2024-01-31T23:59:59Z"] }
{ "type": "captured_at", "op": "gte",    "value": "2023-06-01T00:00:00Z" }
{ "type": "camera",   "field": "make",  "value": "Sony" }
{ "type": "camera",   "field": "model", "value": "ILCE-7M4" }
{ "type": "face_cluster", "cluster_ids": [1, 2, 3] }
{ "type": "starred",  "value": true }   // STUB — see Open issues

// Logical composition
{ "type": "all", "rules": [ … ] }   // AND
{ "type": "any", "rules": [ … ] }   // OR
{ "type": "not", "rule":  { … } }   // NOT
```

### Real-world examples from §13

**"Unflagged favorites"** — aesthetic score >= 8.0, not starred, not in any user album.
`Starred` is currently stubbed, so this collapses to the aesthetic gate until Phase 2:

```json
{
  "type": "all",
  "rules": [
    { "type": "quality", "field": "aesthetic", "op": "gte", "value": 8.0 },
    { "type": "not", "rule": { "type": "starred", "value": true } }
  ]
}
```

**"First time on new camera"** — captured in the first 30 days after the camera
first appeared in the library, on a specific camera model. The `first_camera_ts`
and the `+30d` bound are computed in Rust before embedding into the rule, so the
stored JSON contains concrete RFC3339 timestamps:

```json
{
  "type": "all",
  "rules": [
    { "type": "camera", "field": "model", "value": "ILCE-7M4" },
    {
      "type": "captured_at",
      "op": "between",
      "value": ["2024-03-01T00:00:00Z", "2024-03-31T23:59:59Z"]
    }
  ]
}
```

### Return-value semantics

`rule_to_sql(rule: &AlbumRule) -> Option<String>`:

| Return value | Meaning |
|---|---|
| `None` | Match-all — no WHERE filter added. Used when a rule is vacuously true (e.g. empty `All`). |
| `Some("1 = 0")` | Match-nothing — album is empty. Used for AI-dependent predicates whose backing data does not exist yet (tags, clusters, starred), and for `Not` applied to a match-all inner rule. |
| `Some(fragment)` | Append `AND <fragment>` to the photos query. |

Composition rules:

- `All` (AND): `None` children are skipped (vacuously true). If all children are
  `None`, return `None`. Otherwise return AND of the non-None fragments.
- `Any` (OR): if any child returns `None`, the whole `Any` returns `None`
  (one match-all branch makes the OR match-all). If all children are `None`,
  return `None`. Otherwise return OR of all fragments.
- `Not`: if inner returns `None`, return `Some("1 = 0")`. NOT-everything = nothing.

### SQL injection safety

User-supplied strings (`Tag.value`, `Cluster.value`, `Camera.value`) are escaped
with `.replace('\'', "''")` — standard SQLite string literal escaping. Column
names for `Exif`, `Quality`, and `Camera.field` pass through a whitelist function
(`exif_column`, `quality_column`, `camera_column`) that returns `None` for any
unknown value, silently dropping the rule from evaluation. `CapturedAt` values are
parsed through `chrono::DateTime::parse_from_rfc3339` and the validated string is
re-serialized, ensuring no raw user input reaches the SQL string. `FaceCluster`
ids are `i64` values joined with commas — no string interpolation.

## Consequences

**Good:**

- Rules are composable into arbitrary trees — the UI editor in Phase 2 can
  represent any combination without schema changes.
- All SQL generation is whitelisted in Rust; no raw user strings reach the
  database engine through unescaped paths.
- AI-dependent predicates degrade gracefully to empty albums rather than errors.
- The schema is forward-compatible: adding a new leaf variant requires only a new
  enum arm in Rust; no migration is needed.

**Bad / limitations:**

- No arithmetic expressions (e.g., `photo_count_of_album > 10`). Compound
  comparisons involving aggregates require a different evaluation path.
- Rules are evaluated via string-interpolated SQL fragments. If user-authored rule
  input paths are added in Phase 2, this must be revisited in favour of a
  parameterised query builder to eliminate the escaping burden.
- The auto-evaluator runs the full WHERE fragment for every smart album every 10
  minutes. For 200k-photo catalogs with deeply nested rules, this may become a
  bottleneck. Consider a dirty-flag + incremental re-evaluation approach in Phase 3.

## Resolved

- `is_starred` column: added by migration `20260422000000_photos_starred.sql`
  (`ALTER TABLE photos ADD COLUMN is_starred INTEGER NOT NULL DEFAULT 0 CHECK (is_starred IN (0,1))`
  plus a paired `starred_at TEXT` and a partial index). The `Starred` predicate
  in `rules.rs` now emits `is_starred = 1` / `is_starred = 0` directly.
  The "unflagged favorites" rule (`aesthetic.gte 8.0 AND NOT starred`) is fully
  expressible as of Phase 1.

## Resolved (continued)

- `CapturedAt` `op: "on_mmdd"` added in Phase 1 (migration
  `20260423000000_smart_albums_kind.sql`). Accepts `value: "MM-DD"` (e.g.
  `"04-20"`). SQL: `strftime('%m-%d', captured_at) = 'MM-DD'`. Validation
  via `rules::validated_mmdd` (rejects non-numeric, out-of-range month/day).
  Used by the "On this day" rediscovery album; the background re-evaluator
  rewrites the stored `rule_json` to today's MM-DD on every pass for albums
  with `smart_albums.kind = 'rediscovery_today'`.

- `smart_albums.kind` column added (migration `20260423000000_smart_albums_kind.sql`).
  `NULL` = static rule; `'rediscovery_today'` = re-evaluator rewrites MM-DD on
  every pass. Partial index on `kind` for fast re-evaluator queries.

## Open issues

- `captured_at` (set by Phase 0 initial migration) is the correct column name.
  No `captured_at_utc` alias exists. The `CapturedAt` predicate targets
  `photos.captured_at` directly.
- The "not in any user album" predicate from the "unflagged favorites" rule is not
  yet implementable as an `AlbumRule` variant because there is no
  `photo_album_membership` table. This must be added alongside the Phase 2 manual
  album feature.
