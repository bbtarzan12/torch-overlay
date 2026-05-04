use crate::db;
use chrono::{DateTime, NaiveDateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::File,
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

#[derive(Debug, Clone)]
struct MapContext {
    map_code: String,
    map_name_ko: String,
    difficulty: String,
    area_lv: Option<i64>,
    level_uid: Option<String>,
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
    price_cache: BTreeMap<i64, f64>,
    active_price_send: Option<MessageBlock>,
    active_price_recv: Option<MessageBlock>,
    pending_price_requests: BTreeMap<String, PriceRequest>,
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
            active_price_send: None,
            active_price_recv: None,
            pending_price_requests: BTreeMap::new(),
        };

        let _ = tracker.reload_price_cache();
        let _ = tracker.bootstrap_inventory_baseline();
        tracker
    }

    pub fn reset_session(&mut self) -> Result<(), String> {
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
        self.reload_price_cache()?;
        self.bootstrap_inventory_baseline()?;
        Ok(())
    }

    pub fn snapshot(&mut self) -> Result<TrackerSnapshot, String> {
        self.poll()?;
        Ok(self.build_snapshot())
    }

    fn reload_price_cache(&mut self) -> Result<(), String> {
        self.price_cache =
            db::load_price_estimates(&self.db_path).map_err(|error| error.to_string())?;
        self.price_cache.insert(CRYSTAL_BASE_ID, 1.0);
        Ok(())
    }

    fn bootstrap_inventory_baseline(&mut self) -> Result<(), String> {
        let Ok(mut file) = File::open(&self.log_path) else {
            self.offset = 0;
            return Ok(());
        };

        let mut content = String::new();
        file.read_to_string(&mut content)
            .map_err(|error| error.to_string())?;

        self.inventory_totals.clear();
        self.line_number = 0;
        let mut latest_context: Option<MapContext> = None;
        let mut pending_area_lv = None;
        let mut pending_level_uid: Option<String> = None;
        let mut pending_level_id: Option<String> = None;

        for line in content.lines() {
            self.line_number += 1;
            if let Some(update) = parse_item_update(line) {
                self.inventory_totals
                    .insert(update.inventory_key(), update.bag_num);
            }
            if let Some(area_lv) = parse_area_lv(line) {
                pending_area_lv = Some(area_lv);
            }
            if let Some(level_info) = parse_level_info(line) {
                pending_level_uid = Some(level_info.level_uid);
                pending_level_id = level_info.level_id;
            }
            if is_town_line(line) {
                latest_context = None;
            }
            if let Some(map_code) = parse_map_code(line) {
                if is_town_map_code(&map_code) {
                    latest_context = None;
                } else {
                    let area_lv = pending_area_lv
                        .or_else(|| area_lv_from_level_uid(pending_level_uid.as_deref()));
                    latest_context = Some(MapContext {
                        map_name_ko: map_name_from_code(&map_code, pending_level_id.as_deref()),
                        difficulty: difficulty_from_area_lv(area_lv),
                        map_code,
                        area_lv,
                        level_uid: pending_level_uid.clone(),
                    });
                }
            }
        }

        self.offset = file.stream_position().map_err(|error| error.to_string())?;
        self.pending_area_lv = pending_area_lv;
        self.pending_level_uid = pending_level_uid;
        self.pending_level_id = pending_level_id;

        if let Some(context) = latest_context {
            self.start_bootstrap_run(context)?;
        }

        Ok(())
    }

    fn poll(&mut self) -> Result<(), String> {
        let Ok(mut file) = File::open(&self.log_path) else {
            return Ok(());
        };

        let len = file.metadata().map_err(|error| error.to_string())?.len();
        if len < self.offset {
            self.offset = 0;
            self.pending_fragment.clear();
        }

        file.seek(SeekFrom::Start(self.offset))
            .map_err(|error| error.to_string())?;

        let mut chunk = String::new();
        file.read_to_string(&mut chunk)
            .map_err(|error| error.to_string())?;
        self.offset = file.stream_position().map_err(|error| error.to_string())?;

        if chunk.is_empty() {
            return Ok(());
        }

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

        for line in content.lines() {
            self.line_number += 1;
            self.consume_line(line)?;
        }

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

        let run_id = db::insert_run(
            &self.db_path,
            &timestamp.to_rfc3339_opts(SecondsFormat::Millis, true),
            &map_code,
            &map_name_ko,
            &difficulty,
            area_lv,
            self.pending_level_uid.as_deref(),
        )
        .map_err(|error| error.to_string())?;

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

    fn start_bootstrap_run(&mut self, context: MapContext) -> Result<(), String> {
        let timestamp = Utc::now();
        let run_id = db::insert_run(
            &self.db_path,
            &timestamp.to_rfc3339_opts(SecondsFormat::Millis, true),
            &context.map_code,
            &context.map_name_ko,
            &context.difficulty,
            context.area_lv,
            context.level_uid.as_deref(),
        )
        .map_err(|error| error.to_string())?;

        self.current_run = Some(RunState {
            id: run_id,
            map_code: context.map_code,
            map_name_ko: context.map_name_ko,
            difficulty: context.difficulty,
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
        let delta = previous.map_or(update.bag_num, |value| update.bag_num - value);

        if delta <= 0 || self.current_proto.as_deref() != Some("PickItems") {
            return Ok(());
        }

        let Some(run) = &mut self.current_run else {
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

        let selected_price = median(response.unit_prices.clone());
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

        self.price_cache.insert(config_base_id, selected_price);
        Ok(())
    }

    fn build_snapshot(&self) -> TrackerSnapshot {
        let current_run = self.current_run.as_ref().map_or_else(
            || CurrentRun {
                map_name_ko: "대기 중".to_string(),
                difficulty: "-".to_string(),
                elapsed_seconds: 0,
                crystal: 0.0,
                estimated_item_value: 0.0,
                total_estimated_value: 0.0,
                unpriced_item_count: 0,
                item_count: 0,
            },
            |run| {
                let valuation = value_loot(&run.loot, &self.price_cache);
                CurrentRun {
                    map_name_ko: run.map_name_ko.clone(),
                    difficulty: run.difficulty.clone(),
                    elapsed_seconds: duration_seconds(run.started_at, Utc::now()),
                    crystal: valuation.crystal,
                    estimated_item_value: valuation.estimated_item_value,
                    total_estimated_value: valuation.total_estimated_value,
                    unpriced_item_count: valuation.unpriced_item_count,
                    item_count: valuation.item_count,
                }
            },
        );

        let mut runs = Vec::new();
        let mut total_crystal = current_run.crystal;
        let mut estimated_item_value = current_run.estimated_item_value;
        let mut total_estimated_value = current_run.total_estimated_value;
        let mut unpriced_item_count = current_run.unpriced_item_count;
        let mut completed_seconds = 0;

        for run in &self.completed_runs {
            let valuation = value_loot(&run.loot, &self.price_cache);
            let ended_at = run.ended_at.unwrap_or(run.last_seen_at);
            let duration = duration_seconds(run.started_at, ended_at);

            total_crystal += valuation.crystal;
            estimated_item_value += valuation.estimated_item_value;
            total_estimated_value += valuation.total_estimated_value;
            unpriced_item_count += valuation.unpriced_item_count;
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
        let recent_loot = self.recent_loot();

        TrackerSnapshot {
            current_run,
            runs,
            total_crystal,
            estimated_item_value,
            total_estimated_value,
            average_rate,
            average_per_run,
            unpriced_item_count,
            known_price_count: self.price_cache.len() as i64,
            recent_loot,
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
                loot.push(value_single_loot(item, &self.price_cache));
                if loot.len() >= 12 {
                    return loot;
                }
            }
        }

        loot
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

fn value_loot(loot: &[LootEvent], price_cache: &BTreeMap<i64, f64>) -> LootValuation {
    let mut crystal = 0.0;
    let mut estimated_item_value = 0.0;
    let mut unpriced_item_count = 0;

    for item in loot {
        if item.config_base_id == CRYSTAL_BASE_ID {
            crystal += item.quantity;
            continue;
        }

        if let Some(price) = price_cache.get(&item.config_base_id) {
            estimated_item_value += item.quantity * price;
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

fn value_single_loot(item: &LootEvent, price_cache: &BTreeMap<i64, f64>) -> LootSummary {
    let price_in_crystal = if item.config_base_id == CRYSTAL_BASE_ID {
        Some(1.0)
    } else {
        price_cache.get(&item.config_base_id).copied()
    };

    LootSummary {
        config_base_id: item.config_base_id,
        quantity: item.quantity,
        price_in_crystal,
        value_in_crystal: price_in_crystal.map_or(0.0, |price| price * item.quantity),
    }
}

#[derive(Debug)]
struct LevelInfo {
    level_uid: String,
    level_id: Option<String>,
}

#[derive(Debug)]
struct ItemUpdate {
    config_base_id: i64,
    item_instance_id: Option<String>,
    bag_num: i64,
    page_id: Option<i64>,
    slot_id: Option<i64>,
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
    let rest = line.split_once("ItemChange@ Update Id=")?.1;
    let id = rest.split_whitespace().next()?;
    let (base_id_text, instance_id) = id.split_once('_').map_or((id, None), |(base, instance)| {
        (base, Some(instance.to_string()))
    });
    let config_base_id = base_id_text.parse().ok()?;
    let bag_num = parse_number_after(line, "BagNum=")?;

    Some(ItemUpdate {
        config_base_id,
        item_instance_id: instance_id,
        bag_num,
        page_id: parse_number_after(line, "PageId="),
        slot_id: parse_number_after(line, "SlotId="),
    })
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

    for line in lines {
        if line.contains("+currency [") {
            response.currency_base_id = parse_bracket_i64(line);
        } else if line.contains("+itemGoldId [") {
            response.item_gold_id = parse_bracket_i64(line);
        } else if line.contains('.') {
            if let Some(value) = parse_bracket_f64(line) {
                response.unit_prices.push(value);
            }
        }
    }

    response
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
