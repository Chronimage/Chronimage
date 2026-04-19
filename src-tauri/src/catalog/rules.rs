//! Smart-album rule evaluator.
//!
//! Each `smart_albums` row has a `rule_json` column. This module parses that
//! JSON into a typed `AlbumRule` and produces a SQL WHERE fragment that can be
//! appended to a `SELECT … FROM photos` query.
//!
//! Rules that depend on AI data (tags, face clusters) that hasn't been run yet
//! produce a fragment that always returns zero rows — the album will be empty
//! until AI processing completes.

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
}

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
}
