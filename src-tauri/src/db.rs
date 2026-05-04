use rusqlite::Connection;
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

    CREATE TABLE IF NOT EXISTS run_value_cache (
      run_id INTEGER PRIMARY KEY REFERENCES runs(id),
      direct_crystal REAL NOT NULL DEFAULT 0,
      estimated_item_value REAL NOT NULL DEFAULT 0,
      total_estimated_value REAL NOT NULL DEFAULT 0,
      unpriced_item_count INTEGER NOT NULL DEFAULT 0,
      valuation_version INTEGER NOT NULL,
      calculated_at TEXT NOT NULL
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
