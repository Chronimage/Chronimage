//! Smart-album rule evaluator.
//!
//! Each `smart_albums` row has a `rule_json` column. This module parses that
//! JSON into a typed `AlbumRule` and produces a SQL WHERE fragment that can be
//! appended to a `SELECT … FROM photos` query.
//!
//! Rules that depend on AI data (tags, face clusters) that hasn't been run yet
//! produce a fragment that always returns zero rows — the album will be empty
//! until AI processing completes.
//!
//! ## None vs Some("1 = 0") semantics
//!
//! - `None` — match-all (no constraint); used for rules that are vacuously true.
//! - `Some("1 = 0")` — match-nothing; used for AI-dependent predicates whose
//!   backing data doesn't exist yet, and for `Not { rule }` when `rule` is `None`.
//!
//! Composition:
//! - `All` (AND): any `None` child is skipped; if all children are `None`, return `None`.
//! - `Any` (OR): if any child is `None` (match-all), the whole `Any` is `None`.
//!   If all children are `None`, return `None`.
//! - `Not`: `None` inner → `Some("1 = 0")` (NOT-everything = nothing).

use chrono::DateTime;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AlbumRule {
    /// Photo has a tag with the given label (requires AI tags to be populated).
    Tag { value: String },
    /// Photo belongs to a named face cluster (requires AI clustering).
    Cluster { value: String },
    /// EXIF field comparison: field ∈ {iso, aperture, focal_mm}, op ∈ {gte, lte, gt, lt, eq}.
    Exif {
        field: String,
        op: String,
        value: serde_json::Value,
    },
    /// Quality score comparison: field ∈ {aesthetic, sharpness}, op ∈ {gte, lte, gt, lt}.
    Quality {
        field: String,
        op: String,
        value: f64,
    },
    /// RAW flag filter.
    IsRaw { value: bool },

    // ── Logical composition ──────────────────────────────────────────────────
    /// All child rules must match (AND). An empty `rules` vec returns `None` (match-all).
    All { rules: Vec<AlbumRule> },
    /// Any child rule must match (OR). An empty `rules` vec returns `None` (match-all).
    Any { rules: Vec<AlbumRule> },
    /// Negates the inner rule.
    Not { rule: Box<AlbumRule> },

    // ── New leaf predicates ──────────────────────────────────────────────────
    /// Temporal filter on `photos.captured_at` (ISO-8601 UTC).
    ///
    /// - `op = "gte"` | `"lte"`: `value` is a single RFC3339 string.
    /// - `op = "between"`: `value` is a JSON array `[start, end]`, both RFC3339.
    CapturedAt {
        op: String,
        value: serde_json::Value,
    },
    /// Camera make or model filter: `field ∈ {make, model}`.
    Camera { field: String, value: String },
    /// Photo has at least one face whose `cluster_id` is in the given list.
    /// Any negative id in the list causes the whole predicate to be skipped (returns `None`).
    FaceCluster { cluster_ids: Vec<i64> },
    /// Starred flag. Maps to `photos.is_starred` (added in migration 20260422000000).
    Starred { value: bool },
}

// ── Private helpers ──────────────────────────────────────────────────────────

/// SQL operator string from our op string.
fn sql_op(op: &str) -> Option<&'static str> {
    match op {
        "gte" => Some(">="),
        "lte" => Some("<="),
        "gt" => Some(">"),
        "lt" => Some("<"),
        "eq" => Some("="),
        _ => None,
    }
}

/// Allowed EXIF column names (whitelist against SQL injection).
fn exif_column(field: &str) -> Option<&'static str> {
    match field {
        "iso" => Some("iso"),
        "aperture" => Some("aperture"),
        "focal_mm" => Some("focal_mm"),
        _ => None,
    }
}

/// Allowed quality column names.
fn quality_column(field: &str) -> Option<&'static str> {
    match field {
        "aesthetic" | "aesthetic_score" => Some("aesthetic_score"),
        "sharpness" | "sharpness_score" => Some("sharpness_score"),
        _ => None,
    }
}

/// Allowed camera column names (whitelist against SQL injection).
fn camera_column(field: &str) -> Option<&'static str> {
    match field {
        "make" => Some("camera_make"),
        "model" => Some("camera_model"),
        _ => None,
    }
}

/// Validate an RFC3339 timestamp and re-serialize it as a single-quoted SQL string.
/// Returns `None` if the value is not a valid RFC3339 string.
fn validated_ts_sql(v: &serde_json::Value) -> Option<String> {
    let s = v.as_str()?;
    // Parse to validate; ignore the result — we just want the canonical string.
    DateTime::parse_from_rfc3339(s).ok()?;
    Some(format!("'{}'", s.replace('\'', "''")))
}

// ── Public API ───────────────────────────────────────────────────────────────

/// Parse `rule_json` string into an `AlbumRule`.
pub fn parse_rule(rule_json: &str) -> Result<AlbumRule, serde_json::Error> {
    serde_json::from_str(rule_json)
}

/// Outcome of [`rule_to_sql`]:
/// - `Some(fragment)` — append `AND <fragment>` to filter the photos query.
/// - `None` — rule always matches all photos (no filter needed).
///
/// For rules that depend on data not yet available (AI tags, clusters), returns
/// `Some("1 = 0")` so the album correctly shows 0 photos until AI runs.
pub fn rule_to_sql(rule: &AlbumRule) -> Option<String> {
    match rule {
        AlbumRule::Tag { value } => {
            // Will match once tags table is populated by AI.
            Some(format!(
                "id IN (SELECT photo_id FROM tags WHERE label = '{}')",
                value.replace('\'', "''")
            ))
        }
        AlbumRule::Cluster { value } => {
            // Will match once face clusters are populated.
            Some(format!(
                "id IN (SELECT f.photo_id FROM faces f \
                 JOIN clusters c ON c.id = f.cluster_id \
                 WHERE c.label = '{}')",
                value.replace('\'', "''")
            ))
        }
        AlbumRule::Exif { field, op, value } => {
            let col = exif_column(field)?;
            let oper = sql_op(op)?;
            let num = value.as_f64()?;
            Some(format!("{col} IS NOT NULL AND {col} {oper} {num}"))
        }
        AlbumRule::Quality { field, op, value } => {
            let col = quality_column(field)?;
            let oper = sql_op(op)?;
            Some(format!("{col} IS NOT NULL AND {col} {oper} {value}"))
        }
        AlbumRule::IsRaw { value } => {
            let flag = if *value { 1 } else { 0 };
            Some(format!("is_raw = {flag}"))
        }

        // ── Logical composition ──────────────────────────────────────────────
        AlbumRule::All { rules } => {
            // Collect non-None fragments; None children are vacuously true (skip).
            let fragments: Vec<String> = rules.iter().filter_map(rule_to_sql).collect();
            if fragments.is_empty() {
                None
            } else {
                Some(format!("({})", fragments.join(" AND ")))
            }
        }
        AlbumRule::Any { rules } => {
            // If any child is None (match-all), the OR is also match-all.
            let mut fragments: Vec<String> = Vec::new();
            for r in rules {
                match rule_to_sql(r) {
                    None => return None,
                    Some(f) => fragments.push(f),
                }
            }
            if fragments.is_empty() {
                None
            } else {
                Some(format!("({})", fragments.join(" OR ")))
            }
        }
        AlbumRule::Not { rule } => match rule_to_sql(rule) {
            None => Some("1 = 0".to_owned()),
            Some(inner) => Some(format!("NOT ({inner})")),
        },

        // ── New leaf predicates ──────────────────────────────────────────────
        AlbumRule::CapturedAt { op, value } => match op.as_str() {
            "gte" | "lte" => {
                let oper = sql_op(op)?;
                let ts = validated_ts_sql(value)?;
                Some(format!(
                    "captured_at IS NOT NULL AND captured_at {oper} {ts}"
                ))
            }
            "between" => {
                let arr = value.as_array()?;
                if arr.len() != 2 {
                    return None;
                }
                let start = validated_ts_sql(&arr[0])?;
                let end = validated_ts_sql(&arr[1])?;
                Some(format!(
                    "captured_at IS NOT NULL AND captured_at BETWEEN {start} AND {end}"
                ))
            }
            _ => None,
        },

        AlbumRule::Camera { field, value } => {
            let col = camera_column(field)?;
            Some(format!(
                "{col} IS NOT NULL AND {col} = '{}'",
                value.replace('\'', "''")
            ))
        }

        AlbumRule::FaceCluster { cluster_ids } => {
            // Sanitize: skip (None) if any id is negative.
            if cluster_ids.iter().any(|&id| id < 0) {
                return None;
            }
            if cluster_ids.is_empty() {
                return Some("1 = 0".to_owned());
            }
            let ids: Vec<String> = cluster_ids.iter().map(|id| id.to_string()).collect();
            Some(format!(
                "id IN (SELECT photo_id FROM faces WHERE cluster_id IN ({}))",
                ids.join(", ")
            ))
        }

        AlbumRule::Starred { value } => {
            Some(format!("is_starred = {}", if *value { 1 } else { 0 }))
        }
    }
}

/// Count how many photos match `rule_json`. Returns 0 on parse failure.
pub async fn count_matching(pool: &sqlx::SqlitePool, rule_json: &str) -> i64 {
    let Ok(rule) = parse_rule(rule_json) else {
        return 0;
    };
    let where_clause = match rule_to_sql(&rule) {
        None => String::new(),
        Some(fragment) => format!("WHERE {fragment}"),
    };
    let sql = format!("SELECT COUNT(*) FROM photos {where_clause}");
    sqlx::query_scalar::<_, i64>(&sql)
        .fetch_one(pool)
        .await
        .unwrap_or(0)
}

/// Return up to `limit` photo IDs that match `rule_json`, ordered by
/// `imported_at DESC`. Used to populate `cover_photo_ids`.
pub async fn matching_photo_ids(pool: &sqlx::SqlitePool, rule_json: &str, limit: i64) -> Vec<i64> {
    let Ok(rule) = parse_rule(rule_json) else {
        return Vec::new();
    };
    let where_clause = match rule_to_sql(&rule) {
        None => String::new(),
        Some(fragment) => format!("WHERE {fragment}"),
    };
    let sql =
        format!("SELECT id FROM photos {where_clause} ORDER BY imported_at DESC LIMIT {limit}");
    sqlx::query_scalar::<_, i64>(&sql)
        .fetch_all(pool)
        .await
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Existing tests (must not be changed) ─────────────────────────────────

    #[test]
    fn parses_tag_rule() {
        let r: AlbumRule = parse_rule(r#"{"type":"tag","value":"faces"}"#).unwrap();
        assert!(matches!(r, AlbumRule::Tag { value } if value == "faces"));
    }

    #[test]
    fn parses_exif_rule() {
        let r: AlbumRule =
            parse_rule(r#"{"type":"exif","field":"iso","op":"gte","value":3200}"#).unwrap();
        assert!(matches!(r, AlbumRule::Exif { field, .. } if field == "iso"));
    }

    #[test]
    fn parses_quality_rule() {
        let r: AlbumRule =
            parse_rule(r#"{"type":"quality","field":"sharpness","op":"lt","value":0.3}"#).unwrap();
        assert!(matches!(r, AlbumRule::Quality { field, .. } if field == "sharpness"));
    }

    #[test]
    fn parses_is_raw_rule() {
        let r: AlbumRule = parse_rule(r#"{"type":"is_raw","value":true}"#).unwrap();
        assert!(matches!(r, AlbumRule::IsRaw { value: true }));
    }

    #[test]
    fn exif_rule_produces_correct_sql() {
        let r = parse_rule(r#"{"type":"exif","field":"iso","op":"gte","value":3200}"#).unwrap();
        let sql = rule_to_sql(&r).unwrap();
        assert!(sql.contains("iso >= 3200"), "got: {sql}");
        assert!(sql.contains("IS NOT NULL"), "got: {sql}");
    }

    #[test]
    fn quality_rule_produces_correct_sql() {
        let r =
            parse_rule(r#"{"type":"quality","field":"sharpness","op":"lt","value":0.3}"#).unwrap();
        let sql = rule_to_sql(&r).unwrap();
        assert!(sql.contains("sharpness_score < 0.3"), "got: {sql}");
    }

    #[test]
    fn is_raw_rule_produces_correct_sql() {
        let r = parse_rule(r#"{"type":"is_raw","value":true}"#).unwrap();
        let sql = rule_to_sql(&r).unwrap();
        assert_eq!(sql, "is_raw = 1");
    }

    #[test]
    fn tag_rule_produces_subquery() {
        let r = parse_rule(r#"{"type":"tag","value":"golden_hour"}"#).unwrap();
        let sql = rule_to_sql(&r).unwrap();
        assert!(sql.contains("SELECT photo_id FROM tags"), "got: {sql}");
        assert!(sql.contains("golden_hour"), "got: {sql}");
    }

    #[test]
    fn unknown_exif_field_returns_none() {
        let r = AlbumRule::Exif {
            field: "evil_sql".into(),
            op: "gte".into(),
            value: serde_json::json!(100),
        };
        assert!(rule_to_sql(&r).is_none());
    }

    #[test]
    fn unknown_op_returns_none() {
        let r = AlbumRule::Exif {
            field: "iso".into(),
            op: "BETWEEN".into(),
            value: serde_json::json!(100),
        };
        assert!(rule_to_sql(&r).is_none());
    }

    #[tokio::test]
    async fn count_matching_exif_rule() {
        let tmp = tempfile::TempDir::new().unwrap();
        let pool = crate::catalog::db::open_pool(crate::catalog::db::PoolOptions::new(
            tmp.path().join("c.db"),
        ))
        .await
        .unwrap();

        let now = chrono::Utc::now().to_rfc3339();
        // Insert two photos: one high-ISO, one low-ISO
        sqlx::query(
            "INSERT INTO photos (sha256, filename, width, height, imported_at, is_raw, iso) \
             VALUES (?1, 'hi.jpg', 0, 0, ?2, 0, 6400)",
        )
        .bind("a".repeat(64))
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO photos (sha256, filename, width, height, imported_at, is_raw, iso) \
             VALUES (?1, 'lo.jpg', 0, 0, ?2, 0, 100)",
        )
        .bind("b".repeat(64))
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();

        let count = count_matching(
            &pool,
            r#"{"type":"exif","field":"iso","op":"gte","value":3200}"#,
        )
        .await;
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn count_matching_is_raw_rule() {
        let tmp = tempfile::TempDir::new().unwrap();
        let pool = crate::catalog::db::open_pool(crate::catalog::db::PoolOptions::new(
            tmp.path().join("c.db"),
        ))
        .await
        .unwrap();

        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO photos (sha256, filename, width, height, imported_at, is_raw) \
             VALUES (?1, 'raw.arw', 0, 0, ?2, 1)",
        )
        .bind("c".repeat(64))
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO photos (sha256, filename, width, height, imported_at, is_raw) \
             VALUES (?1, 'jpg.jpg', 0, 0, ?2, 0)",
        )
        .bind("d".repeat(64))
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();

        let count = count_matching(&pool, r#"{"type":"is_raw","value":true}"#).await;
        assert_eq!(count, 1);
    }

    // ── New tests ─────────────────────────────────────────────────────────────

    #[test]
    fn and_composes_two_fragments() {
        let rule = AlbumRule::All {
            rules: vec![
                AlbumRule::IsRaw { value: true },
                AlbumRule::Quality {
                    field: "aesthetic".into(),
                    op: "gte".into(),
                    value: 7.0,
                },
            ],
        };
        let sql = rule_to_sql(&rule).unwrap();
        assert!(sql.contains("is_raw = 1"), "got: {sql}");
        assert!(sql.contains("aesthetic_score >= 7"), "got: {sql}");
        assert!(sql.contains(" AND "), "got: {sql}");
        assert!(sql.starts_with('(') && sql.ends_with(')'), "got: {sql}");
    }

    #[test]
    fn or_composes_two_fragments() {
        let rule = AlbumRule::Any {
            rules: vec![
                AlbumRule::IsRaw { value: true },
                AlbumRule::IsRaw { value: false },
            ],
        };
        let sql = rule_to_sql(&rule).unwrap();
        assert!(sql.contains("is_raw = 1"), "got: {sql}");
        assert!(sql.contains("is_raw = 0"), "got: {sql}");
        assert!(sql.contains(" OR "), "got: {sql}");
    }

    #[test]
    fn not_wraps_fragment() {
        let rule = AlbumRule::Not {
            rule: Box::new(AlbumRule::IsRaw { value: true }),
        };
        let sql = rule_to_sql(&rule).unwrap();
        assert!(sql.starts_with("NOT ("), "got: {sql}");
        assert!(sql.contains("is_raw = 1"), "got: {sql}");
    }

    #[test]
    fn and_with_all_matchall_returns_none() {
        // All children return None (match-all) → the And is also None.
        // We use two FaceCluster rules with a negative id, which return None.
        let rule = AlbumRule::All {
            rules: vec![
                AlbumRule::FaceCluster {
                    cluster_ids: vec![-1],
                },
                AlbumRule::FaceCluster {
                    cluster_ids: vec![-2],
                },
            ],
        };
        assert!(rule_to_sql(&rule).is_none());
    }

    #[test]
    fn or_with_one_matchall_returns_none() {
        // Any child is None → the whole Or is None (short-circuit to match-all).
        let rule = AlbumRule::Any {
            rules: vec![
                AlbumRule::IsRaw { value: true },
                AlbumRule::FaceCluster {
                    cluster_ids: vec![-1],
                }, // returns None
            ],
        };
        assert!(rule_to_sql(&rule).is_none());
    }

    #[test]
    fn captured_at_between_produces_sql() {
        let rule = AlbumRule::CapturedAt {
            op: "between".into(),
            value: serde_json::json!(["2024-01-01T00:00:00Z", "2024-01-31T23:59:59Z"]),
        };
        let sql = rule_to_sql(&rule).unwrap();
        assert!(sql.contains("BETWEEN"), "got: {sql}");
        assert!(sql.contains("2024-01-01T00:00:00Z"), "got: {sql}");
        assert!(sql.contains("2024-01-31T23:59:59Z"), "got: {sql}");
        assert!(sql.contains("captured_at IS NOT NULL"), "got: {sql}");
    }

    #[test]
    fn face_cluster_produces_in_subquery() {
        let rule = AlbumRule::FaceCluster {
            cluster_ids: vec![1, 2, 3],
        };
        let sql = rule_to_sql(&rule).unwrap();
        assert!(sql.contains("SELECT photo_id FROM faces"), "got: {sql}");
        assert!(sql.contains("cluster_id IN ("), "got: {sql}");
        assert!(
            sql.contains('1') && sql.contains('2') && sql.contains('3'),
            "got: {sql}"
        );
    }

    #[test]
    fn camera_model_escapes_quotes() {
        let rule = AlbumRule::Camera {
            field: "model".into(),
            value: "Sony' OR '1'='1".into(),
        };
        let sql = rule_to_sql(&rule).unwrap();
        // The injected single-quote must be doubled, not left raw.
        assert!(!sql.contains("OR '1'='1"), "injection not escaped: {sql}");
        assert!(sql.contains("Sony'' OR ''1''=''1"), "got: {sql}");
    }

    #[test]
    fn starred_true_produces_sql() {
        let rule = AlbumRule::Starred { value: true };
        let sql = rule_to_sql(&rule).unwrap();
        assert_eq!(sql, "is_starred = 1");
    }

    #[test]
    fn starred_false_produces_sql() {
        let rule = AlbumRule::Starred { value: false };
        let sql = rule_to_sql(&rule).unwrap();
        assert_eq!(sql, "is_starred = 0");
    }

    #[tokio::test]
    async fn count_matching_starred_rule() {
        let tmp = tempfile::TempDir::new().unwrap();
        let pool = crate::catalog::db::open_pool(crate::catalog::db::PoolOptions::new(
            tmp.path().join("c.db"),
        ))
        .await
        .unwrap();

        let now = chrono::Utc::now().to_rfc3339();
        // Insert one starred photo and one unstarred photo.
        sqlx::query(
            "INSERT INTO photos (sha256, filename, width, height, imported_at, is_raw, is_starred) \
             VALUES (?1, 'star.jpg', 0, 0, ?2, 0, 1)",
        )
        .bind("e".repeat(64))
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO photos (sha256, filename, width, height, imported_at, is_raw, is_starred) \
             VALUES (?1, 'plain.jpg', 0, 0, ?2, 0, 0)",
        )
        .bind("f".repeat(64))
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();

        let count_starred = count_matching(&pool, r#"{"type":"starred","value":true}"#).await;
        assert_eq!(count_starred, 1, "expected exactly one starred photo");

        let count_unstarred = count_matching(&pool, r#"{"type":"starred","value":false}"#).await;
        assert_eq!(count_unstarred, 1, "expected exactly one unstarred photo");
    }
}
