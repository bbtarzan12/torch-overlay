use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("failed to resolve app data directory: {0}")]
    Path(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Sql(#[from] rusqlite::Error),
}

pub fn path(app: &AppHandle) -> Result<PathBuf, DbError> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| DbError::Path(error.to_string()))?;

    Ok(app_data_dir.join("tracker.sqlite3"))
}

pub fn init(app: &AppHandle) -> Result<(), DbError> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| DbError::Path(error.to_string()))?;

    std::fs::create_dir_all(&app_data_dir)?;

    let db_path = app_data_dir.join("tracker.sqlite3");
    let connection = Connection::open(db_path)?;

    connection.execute_batch(
        r#"
    PRAGMA journal_mode = WAL;
    PRAGMA foreign_keys = ON;

    CREATE TABLE IF NOT EXISTS schema_migrations (
      version INTEGER PRIMARY KEY,
      applied_at TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS app_settings (
      key TEXT PRIMARY KEY,
      value TEXT NOT NULL,
      updated_at TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS runs (
      id INTEGER PRIMARY KEY,
      started_at TEXT NOT NULL,
      ended_at TEXT,
      map_code TEXT,
      map_name_ko TEXT,
      difficulty TEXT,
      area_lv INTEGER,
      level_uid TEXT,
      status TEXT NOT NULL DEFAULT 'open'
    );

    CREATE TABLE IF NOT EXISTS loot_events (
      id INTEGER PRIMARY KEY,
      run_id INTEGER REFERENCES runs(id),
      occurred_at TEXT NOT NULL,
      log_line INTEGER,
      proto_name TEXT NOT NULL,
      config_base_id INTEGER NOT NULL,
      item_instance_id TEXT,
      item_name_ko TEXT,
      quantity REAL NOT NULL,
      page_id INTEGER,
      slot_id INTEGER,
      market_key_id INTEGER REFERENCES market_keys(id),
      raw_line TEXT
    );

    CREATE TABLE IF NOT EXISTS inventory_events (
      id INTEGER PRIMARY KEY,
      occurred_at TEXT NOT NULL,
      log_line INTEGER,
      zone TEXT NOT NULL,
      proto_name TEXT NOT NULL,
      config_base_id INTEGER NOT NULL,
      delta REAL NOT NULL,
      total_after REAL,
      reason TEXT,
      raw_line TEXT
    );

    CREATE TABLE IF NOT EXISTS market_keys (
      id INTEGER PRIMARY KEY,
      key_hash TEXT NOT NULL UNIQUE,
      key_type TEXT NOT NULL,
      config_base_id INTEGER,
      item_gold_id INTEGER,
      typ3 INTEGER,
      canonical_json TEXT NOT NULL,
      display_name_ko TEXT,
      created_at TEXT NOT NULL,
      updated_at TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS price_observations (
      id INTEGER PRIMARY KEY,
      market_key_id INTEGER NOT NULL REFERENCES market_keys(id),
      observed_at TEXT NOT NULL,
      syn_id TEXT,
      currency_base_id INTEGER NOT NULL,
      sample_count INTEGER NOT NULL,
      unit_prices_json TEXT NOT NULL,
      min_price REAL,
      p10_price REAL,
      median_price REAL,
      selected_price REAL,
      estimator_version INTEGER NOT NULL,
      raw_request TEXT,
      raw_response TEXT
    );

    CREATE TABLE IF NOT EXISTS price_estimates (
      market_key_id INTEGER PRIMARY KEY REFERENCES market_keys(id),
      price_in_crystal REAL NOT NULL,
      source_observation_id INTEGER REFERENCES price_observations(id),
      confidence TEXT NOT NULL,
      observed_at TEXT NOT NULL,
      expires_at TEXT,
      estimator_version INTEGER NOT NULL
    );

    CREATE INDEX IF NOT EXISTS idx_loot_events_run ON loot_events(run_id);
    CREATE INDEX IF NOT EXISTS idx_loot_events_base ON loot_events(config_base_id);
    CREATE INDEX IF NOT EXISTS idx_loot_events_market_key ON loot_events(market_key_id);
    CREATE INDEX IF NOT EXISTS idx_inventory_events_base ON inventory_events(config_base_id);
    CREATE INDEX IF NOT EXISTS idx_inventory_events_reason ON inventory_events(reason);
    CREATE INDEX IF NOT EXISTS idx_market_keys_base ON market_keys(config_base_id);
    CREATE INDEX IF NOT EXISTS idx_price_observations_key_time
      ON price_observations(market_key_id, observed_at DESC);
    "#,
    )?;

    Ok(())
}

pub fn open(db_path: &PathBuf) -> Result<Connection, DbError> {
    Ok(Connection::open(db_path)?)
}

pub fn load_price_estimates(
    db_path: &PathBuf,
) -> Result<std::collections::BTreeMap<i64, f64>, DbError> {
    let connection = open(db_path)?;
    let mut statement = connection.prepare(
        r#"
        SELECT mk.config_base_id, pe.price_in_crystal
        FROM price_estimates pe
        JOIN market_keys mk ON mk.id = pe.market_key_id
        WHERE mk.config_base_id IS NOT NULL
        "#,
    )?;

    let rows = statement.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, f64>(1)?)))?;

    let mut prices = std::collections::BTreeMap::new();
    for row in rows {
        let (config_base_id, price) = row?;
        prices.insert(config_base_id, price);
    }

    Ok(prices)
}

pub fn insert_run(
    db_path: &PathBuf,
    id: i64,
    started_at: &str,
    map_code: &str,
    map_name_ko: &str,
    difficulty: &str,
    area_lv: Option<i64>,
    level_uid: Option<&str>,
) -> Result<(), DbError> {
    let connection = open(db_path)?;
    connection.execute(
        r#"
        INSERT OR REPLACE INTO runs
          (id, started_at, ended_at, map_code, map_name_ko, difficulty, area_lv, level_uid, status)
        VALUES
          (?1, ?2, NULL, ?3, ?4, ?5, ?6, ?7, 'open')
        "#,
        params![
            id,
            started_at,
            map_code,
            map_name_ko,
            difficulty,
            area_lv,
            level_uid
        ],
    )?;

    Ok(())
}

pub fn close_run(db_path: &PathBuf, id: i64, ended_at: &str) -> Result<(), DbError> {
    let connection = open(db_path)?;
    connection.execute(
        "UPDATE runs SET ended_at = ?1, status = 'closed' WHERE id = ?2",
        params![ended_at, id],
    )?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn insert_loot_event(
    db_path: &PathBuf,
    run_id: i64,
    occurred_at: &str,
    log_line: i64,
    proto_name: &str,
    config_base_id: i64,
    item_instance_id: Option<&str>,
    quantity: f64,
    page_id: Option<i64>,
    slot_id: Option<i64>,
    raw_line: &str,
) -> Result<(), DbError> {
    let connection = open(db_path)?;
    let market_key_id = ensure_market_key(&connection, config_base_id, None)?;

    connection.execute(
        r#"
        INSERT INTO loot_events
          (run_id, occurred_at, log_line, proto_name, config_base_id, item_instance_id,
           item_name_ko, quantity, page_id, slot_id, market_key_id, raw_line)
        VALUES
          (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7, ?8, ?9, ?10, ?11)
        "#,
        params![
            run_id,
            occurred_at,
            log_line,
            proto_name,
            config_base_id,
            item_instance_id,
            quantity,
            page_id,
            slot_id,
            market_key_id,
            raw_line
        ],
    )?;

    Ok(())
}

pub fn upsert_price_estimate(
    db_path: &PathBuf,
    config_base_id: i64,
    price_in_crystal: f64,
    sample_count: i64,
    unit_prices_json: &str,
    raw_response: &str,
) -> Result<(), DbError> {
    let connection = open(db_path)?;
    let market_key_id = ensure_market_key(&connection, config_base_id, None)?;
    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);

    connection.execute(
        r#"
        INSERT INTO price_observations
          (market_key_id, observed_at, syn_id, currency_base_id, sample_count, unit_prices_json,
           min_price, p10_price, median_price, selected_price, estimator_version, raw_request, raw_response)
        VALUES
          (?1, ?2, NULL, 100300, ?3, ?4, ?5, ?5, ?5, ?5, 1, NULL, ?6)
        "#,
        params![
            market_key_id,
            now,
            sample_count,
            unit_prices_json,
            price_in_crystal,
            raw_response
        ],
    )?;
    let observation_id = connection.last_insert_rowid();

    connection.execute(
        r#"
        INSERT INTO price_estimates
          (market_key_id, price_in_crystal, source_observation_id, confidence,
           observed_at, expires_at, estimator_version)
        VALUES
          (?1, ?2, ?3, 'fresh', ?4, NULL, 1)
        ON CONFLICT(market_key_id) DO UPDATE SET
          price_in_crystal = excluded.price_in_crystal,
          source_observation_id = excluded.source_observation_id,
          confidence = excluded.confidence,
          observed_at = excluded.observed_at,
          expires_at = excluded.expires_at,
          estimator_version = excluded.estimator_version
        "#,
        params![market_key_id, price_in_crystal, observation_id, now],
    )?;

    Ok(())
}

fn ensure_market_key(
    connection: &Connection,
    config_base_id: i64,
    display_name_ko: Option<&str>,
) -> Result<i64, DbError> {
    let key_hash = format!("base:{config_base_id}");
    let existing = connection
        .query_row(
            "SELECT id FROM market_keys WHERE key_hash = ?1",
            params![key_hash],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;

    if let Some(id) = existing {
        return Ok(id);
    }

    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    let canonical_json = format!(r#"{{"configBaseId":{config_base_id}}}"#);

    connection.execute(
        r#"
        INSERT INTO market_keys
          (key_hash, key_type, config_base_id, item_gold_id, typ3,
           canonical_json, display_name_ko, created_at, updated_at)
        VALUES
          (?1, 'stackable', ?2, NULL, NULL, ?3, ?4, ?5, ?5)
        "#,
        params![
            key_hash,
            config_base_id,
            canonical_json,
            display_name_ko,
            now
        ],
    )?;

    Ok(connection.last_insert_rowid())
}
