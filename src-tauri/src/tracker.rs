use crate::{db, diagnostics, offline_items};
use chrono::{DateTime, NaiveDateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::PathBuf,
    sync::OnceLock,
};

const CRYSTAL_BASE_ID: i64 = 100300;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentRun {
    pub map_name_ko: String,
    pub difficulty: String,
    pub elapsed_seconds: i64,
    pub crystal: f64,
    pub estimated_item_value: f64,
    pub total_estimated_value: f64,
    pub unpriced_item_count: i64,
    pub item_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    pub id: i64,
    pub map_name_ko: String,
    pub difficulty: String,
    pub duration_seconds: i64,
    pub crystal: f64,
    pub estimated_item_value: f64,
    pub total_estimated_value: f64,
    pub unpriced_item_count: i64,
    pub item_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LootSummary {
    pub config_base_id: i64,
    pub quantity: f64,
    pub price_in_crystal: Option<f64>,
    pub value_in_crystal: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemValuationRow {
    pub config_base_id: i64,
    pub item_name_ko: Option<String>,
    pub quantity: f64,
    pub ignored: bool,
    pub price_in_crystal: Option<f64>,
    pub price_source: String,
    pub observed_at: Option<String>,
    pub observation_count: i64,
    pub value_in_crystal: f64,
    pub unpriced: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackerSnapshot {
    pub current_run: CurrentRun,
    pub runs: Vec<RunSummary>,
    pub total_crystal: f64,
    pub estimated_item_value: f64,
    pub total_estimated_value: f64,
    pub average_rate: f64,
    pub average_per_run: f64,
    pub unpriced_item_count: i64,
    pub known_price_count: i64,
    pub recent_loot: Vec<LootSummary>,
    pub items: Vec<ItemValuationRow>,
    pub debug: TrackerDebugInfo,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackerDebugInfo {
    pub game_log_path: String,
    pub game_log_exists: bool,
    pub game_log_size: Option<u64>,
    pub read_offset: u64,
    pub line_number: i64,
    pub idle_poll_count: u64,
    pub current_proto: Option<String>,
    pub active_run: bool,
    pub current_map: Option<String>,
    pub last_error: Option<String>,
    pub last_activity: String,
}

#[derive(Debug, Clone)]
struct RunState {
    id: i64,
    map_code: String,
    map_name_ko: String,
    difficulty: String,
    started_at: DateTime<Utc>,
    last_seen_at: DateTime<Utc>,
    ended_at: Option<DateTime<Utc>>,
    loot: Vec<LootEvent>,
}

#[derive(Debug, Clone)]
struct LootEvent {
    config_base_id: i64,
    quantity: f64,
}

#[derive(Debug, Clone)]
struct PriceInfo {
    price_in_crystal: f64,
    confidence: String,
    observed_at: Option<String>,
    observation_count: i64,
}

#[derive(Debug, Default)]
struct PriceRequest {
    refer_base_id: Option<i64>,
}

#[derive(Debug, Default)]
struct PriceResponse {
    item_gold_id: Option<i64>,
    currency_base_id: Option<i64>,
    unit_prices: Vec<f64>,
    raw_lines: Vec<String>,
}

#[derive(Debug, Default)]
struct PriceGroup {
    currency_base_id: Option<i64>,
    unit_prices: Vec<f64>,
}

#[derive(Debug)]
struct MessageBlock {
    syn_id: String,
    lines: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OfflineMaps {
    ambiguous_zones_by_internal_code: BTreeMap<String, BTreeMap<String, OfflineZone>>,
    zones_by_internal_code: BTreeMap<String, OfflineZone>,
    zones_by_level_id: BTreeMap<String, OfflineZone>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OfflineZone {
    name_ko: String,
}

static OFFLINE_MAPS: OnceLock<Option<OfflineMaps>> = OnceLock::new();

pub struct LogTracker {
    log_path: PathBuf,
    db_path: PathBuf,
    offset: u64,
    pending_fragment: String,
    line_number: i64,
    current_run: Option<RunState>,
    completed_runs: Vec<RunState>,
    current_proto: Option<String>,
    pending_area_lv: Option<i64>,
    pending_level_uid: Option<String>,
    pending_level_id: Option<String>,
    inventory_totals: BTreeMap<String, i64>,
    price_cache: BTreeMap<i64, PriceInfo>,
    item_names: BTreeMap<i64, String>,
    ignored_item_ids: BTreeSet<i64>,
    active_price_send: Option<MessageBlock>,
    active_price_recv: Option<MessageBlock>,
    pending_price_requests: BTreeMap<String, PriceRequest>,
    next_run_id: i64,
    last_log_open_error: Option<String>,
    idle_poll_count: u64,
    last_activity: String,
}

impl LogTracker {
    pub fn new(log_path: PathBuf, db_path: PathBuf) -> Self {
        let mut tracker = Self {
            log_path,
            db_path,
            offset: 0,
            pending_fragment: String::new(),
            line_number: 0,
            current_run: None,
            completed_runs: Vec::new(),
            current_proto: None,
            pending_area_lv: None,
            pending_level_uid: None,
            pending_level_id: None,
            inventory_totals: BTreeMap::new(),
            price_cache: BTreeMap::new(),
            item_names: BTreeMap::new(),
            ignored_item_ids: BTreeSet::new(),
            active_price_send: None,
            active_price_recv: None,
            pending_price_requests: BTreeMap::new(),
            next_run_id: 1,
            last_log_open_error: None,
            idle_poll_count: 0,
            last_activity: "tracker created".to_string(),
        };

        diagnostics::write(format!(
            "tracker init log_path=\"{}\" db_path=\"{}\"",
            tracker.log_path.display(),
            tracker.db_path.display()
        ));
        diagnostics::probe_file("tracker init game log", &tracker.log_path);
        let _ = tracker.reload_valuation_settings();
        let _ = tracker.bootstrap_inventory_baseline();
        tracker
    }

    pub fn reset_session(&mut self) -> Result<(), String> {
        diagnostics::write("tracker reset_session");
        self.offset = 0;
        self.pending_fragment.clear();
        self.line_number = 0;
        self.current_run = None;
        self.completed_runs.clear();
        self.current_proto = None;
        self.pending_area_lv = None;
        self.pending_level_uid = None;
        self.pending_level_id = None;
        self.inventory_totals.clear();
        self.active_price_send = None;
        self.active_price_recv = None;
        self.pending_price_requests.clear();
        self.next_run_id = 1;
        self.last_log_open_error = None;
        self.idle_poll_count = 0;
        self.last_activity = "session reset".to_string();
        self.reload_valuation_settings()?;
        self.bootstrap_inventory_baseline()?;
        Ok(())
    }

    pub fn game_log_path(&self) -> &PathBuf {
        &self.log_path
    }

    pub fn set_log_path(&mut self, log_path: PathBuf) -> Result<TrackerSnapshot, String> {
        diagnostics::write(format!(
            "game log path override requested old=\"{}\" new=\"{}\"",
            self.log_path.display(),
            log_path.display()
        ));
        diagnostics::probe_file("game log override", &log_path);
        self.log_path = log_path;
        self.reset_session()?;
        Ok(self.build_snapshot())
    }

    pub fn snapshot(&mut self) -> Result<TrackerSnapshot, String> {
        self.poll()?;
        Ok(self.build_snapshot())
    }

    pub fn set_manual_item_price(
        &mut self,
        config_base_id: i64,
        price_in_crystal: f64,
    ) -> Result<TrackerSnapshot, String> {
        if config_base_id == CRYSTAL_BASE_ID {
            return Err("crystal price is fixed".to_string());
        }

        if !price_in_crystal.is_finite() || price_in_crystal <= 0.0 {
            return Err("price_in_crystal must be a positive finite number".to_string());
        }

        self.poll()?;
        db::upsert_manual_price_estimate(&self.db_path, config_base_id, price_in_crystal)
            .map_err(|error| error.to_string())?;
        self.reload_price_cache()?;
        Ok(self.build_snapshot())
    }

    pub fn set_item_ignored(
        &mut self,
        config_base_id: i64,
        ignored: bool,
    ) -> Result<TrackerSnapshot, String> {
        if config_base_id == CRYSTAL_BASE_ID && ignored {
            return Err("crystal cannot be ignored".to_string());
        }

        self.poll()?;
        db::set_item_ignored(&self.db_path, config_base_id, ignored)
            .map_err(|error| error.to_string())?;

        if ignored {
            self.ignored_item_ids.insert(config_base_id);
        } else {
            self.ignored_item_ids.remove(&config_base_id);
        }

        Ok(self.build_snapshot())
    }

    fn reload_valuation_settings(&mut self) -> Result<(), String> {
        self.reload_price_cache()?;
        self.item_names =
            db::load_item_display_names(&self.db_path).map_err(|error| error.to_string())?;
        self.ignored_item_ids =
            db::load_ignored_item_ids(&self.db_path).map_err(|error| error.to_string())?;
        Ok(())
    }

    fn reload_price_cache(&mut self) -> Result<(), String> {
        self.price_cache = db::load_price_estimate_records(&self.db_path)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|(config_base_id, record)| {
                (
                    config_base_id,
                    PriceInfo {
                        price_in_crystal: record.price_in_crystal,
                        confidence: record.confidence,
                        observed_at: Some(record.observed_at),
                        observation_count: record.observation_count,
                    },
                )
            })
            .collect();
        self.price_cache.insert(
            CRYSTAL_BASE_ID,
            PriceInfo {
                price_in_crystal: 1.0,
                confidence: "fixed".to_string(),
                observed_at: None,
                observation_count: 0,
            },
        );
        Ok(())
    }

    fn bootstrap_inventory_baseline(&mut self) -> Result<(), String> {
        diagnostics::write(format!(
            "bootstrap inventory baseline starting log_path=\"{}\"",
            self.log_path.display()
        ));

        let mut file = match File::open(&self.log_path) {
            Ok(file) => file,
            Err(error) => {
                self.offset = 0;
                diagnostics::write(format!(
                    "bootstrap inventory baseline open failed log_path=\"{}\" error={error}",
                    self.log_path.display()
                ));
                return Ok(());
            }
        };

        let mut content = String::new();
        file.read_to_string(&mut content).map_err(|error| {
            diagnostics::write(format!(
                "bootstrap inventory baseline read failed log_path=\"{}\" error={error}",
                self.log_path.display()
            ));
            error.to_string()
        })?;

        self.inventory_totals.clear();
        self.line_number = 0;
        let mut item_update_count = 0;

        for line in content.lines() {
            self.line_number += 1;
            if let Some(update) = parse_item_update(line) {
                item_update_count += 1;
                self.inventory_totals
                    .insert(update.inventory_key(), update.bag_num);
            }
        }

        self.offset = file.stream_position().map_err(|error| error.to_string())?;
        self.last_activity = format!(
            "baseline ready lines={} item_updates={} offset={}",
            self.line_number, item_update_count, self.offset
        );
        diagnostics::write(format!(
            "bootstrap inventory baseline completed lines={} item_updates={} inventory_keys={} offset={}",
            self.line_number,
            item_update_count,
            self.inventory_totals.len(),
            self.offset
        ));
        Ok(())
    }

    fn poll(&mut self) -> Result<(), String> {
        let mut file = match File::open(&self.log_path) {
            Ok(file) => {
                if self.last_log_open_error.take().is_some() {
                    diagnostics::write(format!(
                        "game log open recovered log_path=\"{}\"",
                        self.log_path.display()
                    ));
                }
                self.last_activity = "game log open ok".to_string();
                file
            }
            Err(error) => {
                let message = format!("open failed error={error}");
                if self.last_log_open_error.as_deref() != Some(message.as_str()) {
                    diagnostics::write(format!(
                        "game log open failed log_path=\"{}\" error={error}",
                        self.log_path.display()
                    ));
                    self.last_log_open_error = Some(message);
                }
                self.last_activity = format!("game log open failed: {error}");
                return Ok(());
            }
        };

        let len = file
            .metadata()
            .map_err(|error| {
                self.last_activity = format!("game log metadata failed: {error}");
                error.to_string()
            })?
            .len();
        if len < self.offset {
            diagnostics::write(format!(
                "game log truncated len={} previous_offset={}",
                len, self.offset
            ));
            self.offset = 0;
            self.pending_fragment.clear();
            self.last_activity = "game log truncated; offset reset".to_string();
        }

        file.seek(SeekFrom::Start(self.offset))
            .map_err(|error| error.to_string())?;

        let mut chunk = String::new();
        file.read_to_string(&mut chunk)
            .map_err(|error| error.to_string())?;
        self.offset = file.stream_position().map_err(|error| error.to_string())?;

        if chunk.is_empty() {
            self.idle_poll_count = self.idle_poll_count.saturating_add(1);
            self.last_activity =
                format!("waiting for log changes len={} offset={}", len, self.offset);
            if self.idle_poll_count == 1 || self.idle_poll_count % 60 == 0 {
                diagnostics::write(format!(
                    "game log poll idle count={} len={} offset={}",
                    self.idle_poll_count, len, self.offset
                ));
            }
            return Ok(());
        }
        self.idle_poll_count = 0;

        let mut content = String::new();
        if !self.pending_fragment.is_empty() {
            content.push_str(&self.pending_fragment);
            self.pending_fragment.clear();
        }
        content.push_str(&chunk);

        if !content.ends_with('\n') {
            if let Some((complete, partial)) = content.rsplit_once('\n') {
                self.pending_fragment = partial.to_string();
                content = complete.to_string();
            } else {
                self.pending_fragment = content;
                return Ok(());
            }
        }

        let consumed_line_count = content.lines().count();
        for line in content.lines() {
            self.line_number += 1;
            self.consume_line(line)?;
        }

        self.last_activity = format!(
            "consumed bytes={} lines={} offset={}",
            chunk.len(),
            consumed_line_count,
            self.offset
        );
        diagnostics::write(format!(
            "game log poll consumed bytes={} lines={} offset={}",
            chunk.len(),
            consumed_line_count,
            self.offset
        ));

        Ok(())
    }

    fn consume_line(&mut self, line: &str) -> Result<(), String> {
        let timestamp = parse_timestamp(line).unwrap_or_else(Utc::now);

        self.consume_price_line(line)?;

        if let Some(area_lv) = parse_area_lv(line) {
            self.pending_area_lv = Some(area_lv);
        }

        if let Some(level_info) = parse_level_info(line) {
            self.pending_level_uid = Some(level_info.level_uid);
            self.pending_level_id = level_info.level_id;
        }

        if is_town_line(line) {
            self.close_current_run(timestamp)?;
        }

        if let Some(map_code) = parse_map_code(line) {
            if !is_town_map_code(&map_code) {
                let area_lv = self
                    .pending_area_lv
                    .or_else(|| area_lv_from_level_uid(self.pending_level_uid.as_deref()));
                let difficulty = difficulty_from_area_lv(area_lv);
                let map_name_ko = map_name_from_code(&map_code, self.pending_level_id.as_deref());

                self.open_run(map_code, map_name_ko, difficulty, area_lv, timestamp)?;
            }
        }

        if let Some(proto) = parse_proto_start(line) {
            self.current_proto = Some(proto);
        }

        if let Some(update) = parse_item_update(line) {
            self.consume_item_update(update, timestamp, line)?;
        }

        if line.contains("ItemChange@ ProtoName=") && line.contains(" end") {
            self.current_proto = None;
        }

        if let Some(run) = &mut self.current_run {
            run.last_seen_at = timestamp;
        }

        Ok(())
    }

    fn open_run(
        &mut self,
        map_code: String,
        map_name_ko: String,
        difficulty: String,
        area_lv: Option<i64>,
        timestamp: DateTime<Utc>,
    ) -> Result<(), String> {
        let should_start_new = self
            .current_run
            .as_ref()
            .map(|run| run.map_code != map_code || run.difficulty != difficulty)
            .unwrap_or(true);

        if !should_start_new {
            return Ok(());
        }

        self.close_current_run(timestamp)?;

        let run_id = self.next_run_id;
        self.next_run_id += 1;

        db::insert_run(
            &self.db_path,
            run_id,
            &timestamp.to_rfc3339_opts(SecondsFormat::Millis, true),
            &map_code,
            &map_name_ko,
            &difficulty,
            area_lv,
            self.pending_level_uid.as_deref(),
        )
        .map_err(|error| error.to_string())?;

        diagnostics::write(format!(
            "run opened id={} map_code={} map_name_ko={} difficulty={} area_lv={:?} level_uid={:?}",
            run_id, map_code, map_name_ko, difficulty, area_lv, self.pending_level_uid
        ));
        self.last_activity = format!("run opened {map_name_ko} {difficulty}");

        self.current_run = Some(RunState {
            id: run_id,
            map_code,
            map_name_ko,
            difficulty,
            started_at: timestamp,
            last_seen_at: timestamp,
            ended_at: None,
            loot: Vec::new(),
        });

        Ok(())
    }

    fn close_current_run(&mut self, timestamp: DateTime<Utc>) -> Result<(), String> {
        let Some(mut run) = self.current_run.take() else {
            return Ok(());
        };

        run.ended_at = Some(timestamp);

        db::close_run(
            &self.db_path,
            run.id,
            &timestamp.to_rfc3339_opts(SecondsFormat::Millis, true),
        )
        .map_err(|error| error.to_string())?;

        diagnostics::write(format!(
            "run closed id={} map_name_ko={} difficulty={} duration_seconds={} loot_events={}",
            run.id,
            run.map_name_ko,
            run.difficulty,
            duration_seconds(run.started_at, timestamp),
            run.loot.len()
        ));
        self.last_activity = format!("run closed {} {}", run.map_name_ko, run.difficulty);

        if duration_seconds(run.started_at, timestamp) > 0 || !run.loot.is_empty() {
            self.completed_runs.push(run);
        }

        Ok(())
    }

    fn consume_item_update(
        &mut self,
        update: ItemUpdate,
        timestamp: DateTime<Utc>,
        raw_line: &str,
    ) -> Result<(), String> {
        let key = update.inventory_key();
        let previous = self.inventory_totals.insert(key, update.bag_num);
        let Some(delta) = inventory_delta(update.change_kind, previous, update.bag_num) else {
            return Ok(());
        };

        if self.current_proto.as_deref() != Some("PickItems") {
            return Ok(());
        }

        let Some(run) = &mut self.current_run else {
            diagnostics::write(format!(
                "loot ignored because no current run config_base_id={} delta={} line_number={}",
                update.config_base_id, delta, self.line_number
            ));
            self.last_activity = format!(
                "loot ignored without active run id={} quantity={}",
                update.config_base_id, delta
            );
            return Ok(());
        };

        let loot = LootEvent {
            config_base_id: update.config_base_id,
            quantity: delta as f64,
        };

        db::insert_loot_event(
            &self.db_path,
            run.id,
            &timestamp.to_rfc3339_opts(SecondsFormat::Millis, true),
            self.line_number,
            self.current_proto.as_deref().unwrap_or("PickItems"),
            update.config_base_id,
            update.item_instance_id.as_deref(),
            delta as f64,
            update.page_id,
            update.slot_id,
            raw_line,
        )
        .map_err(|error| error.to_string())?;

        diagnostics::write(format!(
            "loot captured run_id={} config_base_id={} quantity={} page_id={:?} slot_id={:?} line_number={}",
            run.id,
            update.config_base_id,
            delta,
            update.page_id,
            update.slot_id,
            self.line_number
        ));
        self.last_activity = format!(
            "loot captured id={} quantity={} run_id={}",
            update.config_base_id, delta, run.id
        );

        run.loot.push(loot);
        Ok(())
    }

    fn consume_price_line(&mut self, line: &str) -> Result<(), String> {
        if let Some(syn_id) = parse_message_start(line, "SendMessage") {
            self.active_price_send = Some(MessageBlock {
                syn_id,
                lines: vec![line.to_string()],
            });
            return Ok(());
        }

        if let Some(syn_id) = parse_message_start(line, "RecvMessage") {
            self.active_price_recv = Some(MessageBlock {
                syn_id,
                lines: vec![line.to_string()],
            });
            return Ok(());
        }

        if line.contains("----Socket SendMessage End----") {
            if let Some(block) = self.active_price_send.take() {
                let request = parse_price_request(&block.lines);
                self.pending_price_requests.insert(block.syn_id, request);
            }
            return Ok(());
        }

        if line.contains("----Socket RecvMessage End----") {
            if let Some(block) = self.active_price_recv.take() {
                let response = parse_price_response(&block.lines);
                self.apply_price_response(block.syn_id, response)?;
            }
            return Ok(());
        }

        if let Some(block) = &mut self.active_price_send {
            block.lines.push(line.to_string());
        }

        if let Some(block) = &mut self.active_price_recv {
            block.lines.push(line.to_string());
        }

        Ok(())
    }

    fn apply_price_response(
        &mut self,
        syn_id: String,
        response: PriceResponse,
    ) -> Result<(), String> {
        if response.currency_base_id != Some(CRYSTAL_BASE_ID) || response.unit_prices.is_empty() {
            return Ok(());
        }

        let request = self
            .pending_price_requests
            .remove(&syn_id)
            .unwrap_or_default();
        let Some(config_base_id) = request.refer_base_id.or(response.item_gold_id) else {
            return Ok(());
        };

        let selected_price = round_collected_price(estimate_price(response.unit_prices.clone()));
        let unit_prices_json =
            serde_json::to_string(&response.unit_prices).map_err(|error| error.to_string())?;
        let raw_response = response.raw_lines.join("\n");

        db::upsert_price_estimate(
            &self.db_path,
            config_base_id,
            selected_price,
            response.unit_prices.len() as i64,
            &unit_prices_json,
            &raw_response,
        )
        .map_err(|error| error.to_string())?;

        diagnostics::write(format!(
            "price estimate updated config_base_id={} selected_price={} samples={} syn_id={}",
            config_base_id,
            selected_price,
            response.unit_prices.len(),
            syn_id
        ));
        self.last_activity = format!(
            "price updated id={} price={} samples={}",
            config_base_id,
            selected_price,
            response.unit_prices.len()
        );

        self.reload_price_cache()?;
        Ok(())
    }

    fn build_snapshot(&self) -> TrackerSnapshot {
        let active_valuation = self
            .current_run
            .as_ref()
            .map(|run| value_loot(&run.loot, &self.price_cache, &self.ignored_item_ids));
        let current_run = if let Some(run) = &self.current_run {
            current_run_summary(
                run,
                Utc::now(),
                active_valuation.as_ref().expect("active valuation exists"),
            )
        } else if let Some(run) = self.completed_runs.last() {
            let ended_at = run.ended_at.unwrap_or(run.last_seen_at);
            let valuation = value_loot(&run.loot, &self.price_cache, &self.ignored_item_ids);
            current_run_summary(run, ended_at, &valuation)
        } else {
            CurrentRun {
                map_name_ko: "대기 중".to_string(),
                difficulty: "-".to_string(),
                elapsed_seconds: 0,
                crystal: 0.0,
                estimated_item_value: 0.0,
                total_estimated_value: 0.0,
                unpriced_item_count: 0,
                item_count: 0,
            }
        };

        let mut runs = Vec::new();
        let mut total_crystal = active_valuation
            .as_ref()
            .map_or(0.0, |valuation| valuation.crystal);
        let mut estimated_item_value = active_valuation
            .as_ref()
            .map_or(0.0, |valuation| valuation.estimated_item_value);
        let mut total_estimated_value = active_valuation
            .as_ref()
            .map_or(0.0, |valuation| valuation.total_estimated_value);
        let mut completed_seconds = 0;

        for run in &self.completed_runs {
            let valuation = value_loot(&run.loot, &self.price_cache, &self.ignored_item_ids);
            let ended_at = run.ended_at.unwrap_or(run.last_seen_at);
            let duration = duration_seconds(run.started_at, ended_at);

            total_crystal += valuation.crystal;
            estimated_item_value += valuation.estimated_item_value;
            total_estimated_value += valuation.total_estimated_value;
            completed_seconds += duration;

            runs.push(RunSummary {
                id: run.id,
                map_name_ko: run.map_name_ko.clone(),
                difficulty: run.difficulty.clone(),
                duration_seconds: duration,
                crystal: valuation.crystal,
                estimated_item_value: valuation.estimated_item_value,
                total_estimated_value: valuation.total_estimated_value,
                unpriced_item_count: valuation.unpriced_item_count,
                item_count: valuation.item_count,
            });
        }

        let average_rate = if completed_seconds > 0 {
            total_estimated_value / completed_seconds as f64 * 3600.0
        } else {
            0.0
        };
        let average_per_run = if runs.is_empty() {
            0.0
        } else {
            total_estimated_value / runs.len() as f64
        };
        let items = self.item_valuation_rows();
        let item_table_unpriced_count = items.iter().filter(|item| item.unpriced).count() as i64;
        let recent_loot = self.recent_loot();

        TrackerSnapshot {
            current_run,
            runs,
            total_crystal,
            estimated_item_value,
            total_estimated_value,
            average_rate,
            average_per_run,
            unpriced_item_count: item_table_unpriced_count,
            known_price_count: self.price_cache.len() as i64,
            recent_loot,
            items,
            debug: self.debug_info(),
        }
    }

    fn debug_info(&self) -> TrackerDebugInfo {
        let metadata = fs::metadata(&self.log_path).ok();

        TrackerDebugInfo {
            game_log_path: self.log_path.display().to_string(),
            game_log_exists: metadata.as_ref().is_some_and(|metadata| metadata.is_file()),
            game_log_size: metadata.map(|metadata| metadata.len()),
            read_offset: self.offset,
            line_number: self.line_number,
            idle_poll_count: self.idle_poll_count,
            current_proto: self.current_proto.clone(),
            active_run: self.current_run.is_some(),
            current_map: self
                .current_run
                .as_ref()
                .map(|run| format!("{} {}", run.map_name_ko, run.difficulty)),
            last_error: self.last_log_open_error.clone(),
            last_activity: self.last_activity.clone(),
        }
    }

    fn recent_loot(&self) -> Vec<LootSummary> {
        let mut loot = Vec::new();

        for run in self
            .completed_runs
            .iter()
            .chain(self.current_run.iter())
            .rev()
        {
            for item in run.loot.iter().rev() {
                loot.push(value_single_loot(
                    item,
                    &self.price_cache,
                    &self.ignored_item_ids,
                ));
                if loot.len() >= 12 {
                    return loot;
                }
            }
        }

        loot
    }

    fn item_valuation_rows(&self) -> Vec<ItemValuationRow> {
        let mut quantities = BTreeMap::<i64, f64>::new();

        for run in self.completed_runs.iter().chain(self.current_run.iter()) {
            for item in &run.loot {
                *quantities.entry(item.config_base_id).or_default() += item.quantity;
            }
        }

        for config_base_id in self.price_cache.keys().chain(self.ignored_item_ids.iter()) {
            quantities.entry(*config_base_id).or_insert(0.0);
        }

        let mut rows = quantities
            .into_iter()
            .filter(|(config_base_id, quantity)| {
                *config_base_id != CRYSTAL_BASE_ID || *quantity > 0.0
            })
            .map(|(config_base_id, quantity)| {
                item_valuation_row(
                    config_base_id,
                    quantity,
                    &self.price_cache,
                    &self.item_names,
                    &self.ignored_item_ids,
                )
            })
            .collect::<Vec<_>>();

        rows.sort_by(|left, right| {
            left.ignored
                .cmp(&right.ignored)
                .then_with(|| left.unpriced.cmp(&right.unpriced))
                .then_with(|| {
                    right
                        .value_in_crystal
                        .partial_cmp(&left.value_in_crystal)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| left.config_base_id.cmp(&right.config_base_id))
        });

        rows
    }
}

#[derive(Debug)]
struct LootValuation {
    crystal: f64,
    estimated_item_value: f64,
    total_estimated_value: f64,
    unpriced_item_count: i64,
    item_count: i64,
}

fn current_run_summary(
    run: &RunState,
    ended_at: DateTime<Utc>,
    valuation: &LootValuation,
) -> CurrentRun {
    CurrentRun {
        map_name_ko: run.map_name_ko.clone(),
        difficulty: run.difficulty.clone(),
        elapsed_seconds: duration_seconds(run.started_at, ended_at),
        crystal: valuation.crystal,
        estimated_item_value: valuation.estimated_item_value,
        total_estimated_value: valuation.total_estimated_value,
        unpriced_item_count: valuation.unpriced_item_count,
        item_count: valuation.item_count,
    }
}

fn value_loot(
    loot: &[LootEvent],
    price_cache: &BTreeMap<i64, PriceInfo>,
    ignored_item_ids: &BTreeSet<i64>,
) -> LootValuation {
    let mut crystal = 0.0;
    let mut estimated_item_value = 0.0;
    let mut unpriced_item_count = 0;

    for item in loot {
        if item.config_base_id == CRYSTAL_BASE_ID {
            crystal += item.quantity;
            continue;
        }

        if ignored_item_ids.contains(&item.config_base_id) {
            continue;
        }

        if let Some(price) = price_cache.get(&item.config_base_id) {
            estimated_item_value += item.quantity * price.price_in_crystal;
        } else {
            unpriced_item_count += 1;
        }
    }

    LootValuation {
        crystal,
        estimated_item_value,
        total_estimated_value: crystal + estimated_item_value,
        unpriced_item_count,
        item_count: loot.len() as i64,
    }
}

fn value_single_loot(
    item: &LootEvent,
    price_cache: &BTreeMap<i64, PriceInfo>,
    ignored_item_ids: &BTreeSet<i64>,
) -> LootSummary {
    let price_in_crystal = if item.config_base_id == CRYSTAL_BASE_ID {
        Some(1.0)
    } else if ignored_item_ids.contains(&item.config_base_id) {
        None
    } else {
        price_cache
            .get(&item.config_base_id)
            .map(|price| price.price_in_crystal)
    };

    LootSummary {
        config_base_id: item.config_base_id,
        quantity: item.quantity,
        price_in_crystal,
        value_in_crystal: price_in_crystal.map_or(0.0, |price| price * item.quantity),
    }
}

fn item_valuation_row(
    config_base_id: i64,
    quantity: f64,
    price_cache: &BTreeMap<i64, PriceInfo>,
    item_names: &BTreeMap<i64, String>,
    ignored_item_ids: &BTreeSet<i64>,
) -> ItemValuationRow {
    let ignored = ignored_item_ids.contains(&config_base_id);
    let price = price_cache.get(&config_base_id);
    let price_in_crystal = price.map(|price| price.price_in_crystal);
    let unpriced = !ignored && price_in_crystal.is_none();
    let value_in_crystal = if ignored {
        0.0
    } else {
        price_in_crystal.map_or(0.0, |price| price * quantity)
    };

    ItemValuationRow {
        config_base_id,
        item_name_ko: if config_base_id == CRYSTAL_BASE_ID {
            Some("최초의 불꽃 결정".to_string())
        } else {
            item_names
                .get(&config_base_id)
                .cloned()
                .or_else(|| offline_items::item_name_ko(config_base_id))
        },
        quantity,
        ignored,
        price_in_crystal,
        price_source: if ignored {
            "ignored".to_string()
        } else {
            price
                .map(|price| price.confidence.clone())
                .unwrap_or_else(|| "unpriced".to_string())
        },
        observed_at: price.and_then(|price| price.observed_at.clone()),
        observation_count: price.map_or(0, |price| price.observation_count),
        value_in_crystal,
        unpriced,
    }
}

#[derive(Debug)]
struct LevelInfo {
    level_uid: String,
    level_id: Option<String>,
}

#[derive(Debug)]
struct ItemUpdate {
    change_kind: ItemChangeKind,
    config_base_id: i64,
    item_instance_id: Option<String>,
    bag_num: i64,
    page_id: Option<i64>,
    slot_id: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ItemChangeKind {
    Add,
    Update,
}

impl ItemUpdate {
    fn inventory_key(&self) -> String {
        if let Some(instance_id) = &self.item_instance_id {
            format!("{}:{instance_id}", self.config_base_id)
        } else {
            format!(
                "{}:{}:{}",
                self.config_base_id,
                self.page_id.unwrap_or(-1),
                self.slot_id.unwrap_or(-1)
            )
        }
    }
}

fn parse_timestamp(line: &str) -> Option<DateTime<Utc>> {
    let close = line.find(']')?;
    let raw = line.get(1..close)?;
    let parsed = NaiveDateTime::parse_from_str(raw, "%Y.%m.%d-%H.%M.%S:%3f").ok()?;
    Some(DateTime::<Utc>::from_naive_utc_and_offset(parsed, Utc))
}

fn parse_area_lv(line: &str) -> Option<i64> {
    parse_number_after(line, "AreaLv =").or_else(|| parse_number_after(line, "AreaLv =="))
}

fn parse_level_info(line: &str) -> Option<LevelInfo> {
    if let Some(rest) = line.split_once("LevelUid, LevelType, LevelId =") {
        let mut numbers = rest
            .1
            .split_whitespace()
            .filter(|part| part.chars().all(|character| character.is_ascii_digit()));

        let level_uid = numbers.next()?.to_string();
        let _level_type = numbers.next();
        let level_id = numbers.next().map(str::to_string);

        return Some(LevelInfo {
            level_uid,
            level_id,
        });
    }

    parse_number_text_after(line, "LevelUid =").map(|level_uid| LevelInfo {
        level_uid: level_uid.to_string(),
        level_id: None,
    })
}

fn parse_map_code(line: &str) -> Option<String> {
    if !line.contains("MapName =")
        && !line.contains("LoadMapName =")
        && !line.contains("InMainLevelPath =")
    {
        return None;
    }

    let marker = if line.contains("LoadMapName =") {
        "LoadMapName ="
    } else if line.contains("InMainLevelPath =") {
        "InMainLevelPath ="
    } else {
        "MapName ="
    };

    let rest = line.split_once(marker)?.1.trim();
    let path = rest.split_whitespace().next().unwrap_or(rest);
    let last = path.rsplit('/').next().unwrap_or(path);
    let map_code = clean_map_component(last);

    if map_code.is_empty() {
        None
    } else {
        Some(map_code.to_string())
    }
}

fn parse_proto_start(line: &str) -> Option<String> {
    let rest = line.split_once("ItemChange@ ProtoName=")?.1;
    let proto = rest.split_whitespace().next()?;

    if rest.contains(" start") {
        Some(proto.to_string())
    } else {
        None
    }
}

fn parse_item_update(line: &str) -> Option<ItemUpdate> {
    let (change_kind, rest) = if let Some((_, rest)) = line.split_once("ItemChange@ Update Id=") {
        (ItemChangeKind::Update, rest)
    } else if let Some((_, rest)) = line.split_once("ItemChange@ Add Id=") {
        (ItemChangeKind::Add, rest)
    } else {
        return None;
    };

    let id = rest.split_whitespace().next()?;
    let (base_id_text, instance_id) = id.split_once('_').map_or((id, None), |(base, instance)| {
        (base, Some(instance.to_string()))
    });
    let config_base_id = base_id_text.parse().ok()?;
    let bag_num = parse_number_after(line, "BagNum=")?;

    Some(ItemUpdate {
        change_kind,
        config_base_id,
        item_instance_id: instance_id,
        bag_num,
        page_id: parse_number_after(line, "PageId="),
        slot_id: parse_number_after(line, "SlotId="),
    })
}

fn inventory_delta(
    change_kind: ItemChangeKind,
    previous_bag_num: Option<i64>,
    current_bag_num: i64,
) -> Option<i64> {
    let delta = match change_kind {
        ItemChangeKind::Add => current_bag_num,
        ItemChangeKind::Update => current_bag_num - previous_bag_num?,
    };

    if delta > 0 {
        Some(delta)
    } else {
        None
    }
}

fn parse_message_start(line: &str, direction: &str) -> Option<String> {
    let marker = format!("----Socket {direction} STT----XchgSearchPrice----SynId =");
    parse_number_text_after(line, &marker).map(str::to_string)
}

fn parse_price_request(lines: &[String]) -> PriceRequest {
    let mut request = PriceRequest::default();

    for line in lines {
        if line.contains("+refer [") {
            request.refer_base_id = parse_bracket_i64(line);
        }
    }

    request
}

fn parse_price_response(lines: &[String]) -> PriceResponse {
    let mut response = PriceResponse {
        raw_lines: lines.to_vec(),
        ..PriceResponse::default()
    };
    let mut groups = BTreeMap::<i64, PriceGroup>::new();
    let mut current_group_id = None;

    for line in lines {
        if line.contains("+itemGoldId [") {
            response.item_gold_id = parse_bracket_i64(line);
            continue;
        }

        if let Some(group_id) = parse_price_group_id(line) {
            current_group_id = Some(group_id);
        }

        if line.contains("+currency [") {
            if let Some(currency_base_id) = parse_bracket_i64(line) {
                let group_id = current_group_id.unwrap_or(1);
                groups.entry(group_id).or_default().currency_base_id = Some(currency_base_id);
            }
            continue;
        }

        if line.contains('.') {
            if let Some(value) = parse_bracket_f64(line) {
                let group_id = current_group_id.unwrap_or(1);
                groups.entry(group_id).or_default().unit_prices.push(value);
            }
        }
    }

    if let Some((_, group)) = groups.into_iter().find(|(_, group)| {
        group.currency_base_id == Some(CRYSTAL_BASE_ID) && !group.unit_prices.is_empty()
    }) {
        response.currency_base_id = group.currency_base_id;
        response.unit_prices = group.unit_prices;
    }

    response
}

fn parse_price_group_id(line: &str) -> Option<i64> {
    parse_number_after(line, "+prices+").or_else(|| {
        if !line.contains("+unitPrices") && !line.contains("+currency") {
            return None;
        }

        let rest = line.split_once('+')?.1;
        let digits_len = rest
            .char_indices()
            .take_while(|(_, character)| character.is_ascii_digit())
            .map(|(index, character)| index + character.len_utf8())
            .last()?;

        rest.get(..digits_len)?.parse().ok()
    })
}

fn parse_number_after(line: &str, marker: &str) -> Option<i64> {
    parse_number_text_after(line, marker)?.parse().ok()
}

fn parse_number_text_after<'a>(line: &'a str, marker: &str) -> Option<&'a str> {
    let rest = line.split_once(marker)?.1.trim_start();
    let digits_len = rest
        .char_indices()
        .take_while(|(_, character)| character.is_ascii_digit())
        .map(|(index, character)| index + character.len_utf8())
        .last()?;

    rest.get(..digits_len)
}

fn parse_bracket_i64(line: &str) -> Option<i64> {
    let (_, rest) = line.split_once('[')?;
    let (value, _) = rest.split_once(']')?;
    value.trim().parse().ok()
}

fn parse_bracket_f64(line: &str) -> Option<f64> {
    let (_, rest) = line.split_once('[')?;
    let (value, _) = rest.split_once(']')?;
    value.trim().parse().ok()
}

fn strip_numeric_suffix(value: &str) -> &str {
    value.trim_end_matches(|character: char| character.is_ascii_digit())
}

fn clean_map_component(value: &str) -> &str {
    let trimmed = value
        .trim()
        .trim_matches(|character: char| matches!(character, '\'' | '"' | ',' | ')' | '('));

    let without_object_path = trimmed
        .rsplit_once('.')
        .map_or(trimmed, |(_, object_name)| object_name);

    without_object_path
        .trim_matches(|character: char| matches!(character, '\'' | '"' | ',' | ')' | '('))
}

fn is_town_line(line: &str) -> bool {
    line.contains("LevelUid=111000")
        || line.contains("LevelUid = 111000")
        || line.contains("LevelType, LevelId = 0")
        || line.contains("+checkType [MainCity]")
        || line.contains("XZ_YuJinZhiXiBiNanSuo")
}

fn is_town_map_code(map_code: &str) -> bool {
    map_code.starts_with("XZ_")
}

fn difficulty_from_area_lv(area_lv: Option<i64>) -> String {
    match area_lv {
        Some(4) => "5단계",
        Some(5) => "6단계",
        Some(6) => "7-0",
        Some(7) => "7-1",
        Some(8) => "7-2",
        Some(9) => "8-0",
        Some(10) => "8-1",
        Some(11) => "8-2",
        Some(12) => "아득한 8단계",
        Some(13) => "딥 스페이스",
        _ => "-",
    }
    .to_string()
}

fn area_lv_from_level_uid(level_uid: Option<&str>) -> Option<i64> {
    let level_uid = level_uid?;

    if level_uid.len() < 3 || !level_uid.starts_with('1') {
        return None;
    }

    level_uid.get(1..3)?.parse().ok()
}

fn map_name_from_code(map_code: &str, level_id: Option<&str>) -> String {
    lookup_map_name(map_code, level_id)
        .or_else(|| lookup_map_name(strip_numeric_suffix(map_code), level_id))
        .unwrap_or_else(|| map_code.to_string())
}

fn lookup_map_name(map_code: &str, level_id: Option<&str>) -> Option<String> {
    if map_code.is_empty() {
        return None;
    }

    let maps = offline_maps()?;

    if let Some(zone) = maps.zones_by_internal_code.get(map_code) {
        return Some(zone.name_ko.clone());
    }

    if let Some(level_id) = level_id {
        if let Some(zone) = maps.zones_by_level_id.get(level_id) {
            return Some(zone.name_ko.clone());
        }
    }

    maps.ambiguous_zones_by_internal_code
        .get(map_code)
        .and_then(|zones| zones.values().next())
        .map(|zone| zone.name_ko.clone())
}

fn offline_maps() -> Option<&'static OfflineMaps> {
    OFFLINE_MAPS
        .get_or_init(|| serde_json::from_str(include_str!("../../data/offline/maps.ko.json")).ok())
        .as_ref()
}

fn duration_seconds(start: DateTime<Utc>, end: DateTime<Utc>) -> i64 {
    if end >= start {
        (end - start).num_seconds()
    } else {
        0
    }
}

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    values[values.len() / 2]
}

fn estimate_price(mut values: Vec<f64>) -> f64 {
    values.retain(|value| value.is_finite() && *value > 0.0);
    values.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));

    match values.len() {
        0 => 0.0,
        1 | 2 => values[0],
        3..=9 => {
            let lower_count = values.len().min(3);
            median(values[..lower_count].to_vec())
        }
        len => {
            let index = ((len - 1) as f64 * 0.10).round() as usize;
            values[index.min(len - 1)]
        }
    }
}

fn round_collected_price(value: f64) -> f64 {
    if !value.is_finite() || value <= 0.0 {
        return 0.0;
    }

    let rounded = (value * 100.0).round() / 100.0;
    if rounded <= 0.0 {
        0.01
    } else {
        rounded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_crystal_price_group_when_other_currency_is_present() {
        let response = parse_price_response(&[
            "+prices+1+unitPrices+1 [0.13000327189443]".to_string(),
            "|      | |          +2 [0.13000946670874]".to_string(),
            "|      | +currency [100300]".to_string(),
            "|      +2+unitPrices+1 [10086.0]".to_string(),
            "|      | +currency [100200]".to_string(),
            "+errCode".to_string(),
        ]);

        assert_eq!(response.currency_base_id, Some(CRYSTAL_BASE_ID));
        assert_eq!(
            response.unit_prices,
            vec![0.13000327189443, 0.13000946670874]
        );
    }

    #[test]
    fn estimates_price_from_low_percentile_without_quantity_weighting() {
        let price = estimate_price(vec![10.0, 1.0, 1.1, 1.2, 1.3, 1.4, 1.5, 1.6, 1.7, 1.8]);

        assert_eq!(price, 1.1);
    }

    #[test]
    fn rounds_collected_price_to_two_decimals() {
        assert_eq!(round_collected_price(1.234), 1.23);
        assert_eq!(round_collected_price(1.235), 1.24);
        assert_eq!(round_collected_price(0.004), 0.01);
    }

    #[test]
    fn treats_first_update_as_inventory_baseline() {
        assert_eq!(inventory_delta(ItemChangeKind::Update, None, 8986), None);
        assert_eq!(
            inventory_delta(ItemChangeKind::Update, Some(8986), 8992),
            Some(6)
        );
        assert_eq!(
            inventory_delta(ItemChangeKind::Update, Some(8992), 8992),
            None
        );
        assert_eq!(
            inventory_delta(ItemChangeKind::Update, Some(8992), 8986),
            None
        );
    }

    #[test]
    fn treats_add_as_new_inventory_delta() {
        assert_eq!(inventory_delta(ItemChangeKind::Add, None, 11), Some(11));
        assert_eq!(inventory_delta(ItemChangeKind::Add, Some(4), 1), Some(1));
    }

    #[test]
    fn parses_add_and_update_item_change_kinds() {
        let update = parse_item_update(
            "[2026.05.05-03.57.28:287][738]TLLua: Display: [Game] ItemChange@ Update Id=5028_abc BagNum=584 in PageId=102 SlotId=9",
        )
        .expect("update item change should parse");
        assert_eq!(update.change_kind, ItemChangeKind::Update);
        assert_eq!(update.config_base_id, 5028);
        assert_eq!(update.bag_num, 584);

        let add = parse_item_update(
            "[2026.05.04-02.04.44:096][919]TLLua: Display: [Game] ItemChange@ Add Id=100300_def BagNum=11 in PageId=102 SlotId=0",
        )
        .expect("add item change should parse");
        assert_eq!(add.change_kind, ItemChangeKind::Add);
        assert_eq!(add.config_base_id, 100300);
        assert_eq!(add.bag_num, 11);
    }
}
