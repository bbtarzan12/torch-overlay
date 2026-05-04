use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path, sync::OnceLock};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentRun {
    pub map_name_ko: String,
    pub difficulty: String,
    pub elapsed_seconds: i64,
    pub crystal: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    pub id: i64,
    pub map_name_ko: String,
    pub difficulty: String,
    pub duration_seconds: i64,
    pub crystal: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackerSnapshot {
    pub current_run: CurrentRun,
    pub runs: Vec<RunSummary>,
    pub total_crystal: i64,
    pub average_rate: f64,
    pub average_per_run: f64,
}

#[derive(Debug, Clone)]
struct RunBuilder {
    map_name_ko: String,
    difficulty: String,
    started_at: Option<DateTime<Utc>>,
    last_seen_at: Option<DateTime<Utc>>,
    crystal: i64,
}

#[derive(Debug, Default)]
struct ParseState {
    current_run: Option<RunBuilder>,
    completed_runs: Vec<RunSummary>,
    current_proto: Option<String>,
    last_crystal_total: Option<i64>,
    pending_area_lv: Option<i64>,
    pending_level_uid: Option<String>,
    pending_level_id: Option<String>,
    next_run_id: i64,
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

pub fn parse_log_file(path: impl AsRef<Path>) -> Result<TrackerSnapshot, ParseError> {
    let content = fs::read_to_string(path)?;
    Ok(parse_log_snapshot(&content))
}

pub fn parse_log_snapshot(content: &str) -> TrackerSnapshot {
    let mut state = ParseState {
        next_run_id: 1,
        ..ParseState::default()
    };

    for line in content.lines() {
        state.consume_line(line);
    }

    state.finish()
}

impl ParseState {
    fn consume_line(&mut self, line: &str) {
        let timestamp = parse_timestamp(line);

        if let Some(area_lv) = parse_area_lv(line) {
            self.pending_area_lv = Some(area_lv);
        }

        if let Some(level_info) = parse_level_info(line) {
            self.pending_level_uid = Some(level_info.level_uid);
            self.pending_level_id = level_info.level_id;
        }

        if is_town_line(line) {
            self.close_current_run(timestamp);
        }

        if let Some(map_code) = parse_map_code(line) {
            if !is_town_map_code(&map_code) {
                self.open_run(
                    map_name_from_code(&map_code, self.pending_level_id.as_deref()),
                    difficulty_from_area_lv(
                        self.pending_area_lv
                            .or_else(|| area_lv_from_level_uid(self.pending_level_uid.as_deref())),
                    ),
                    timestamp,
                );
            }
        }

        if let Some(proto) = parse_proto_start(line) {
            self.current_proto = Some(proto);
        }

        if line.contains("ItemChange@ ProtoName=") && line.contains(" end") {
            self.current_proto = None;
        }

        if let Some(total) = parse_crystal_total(line) {
            self.apply_crystal_total(total);
        }

        if let Some(run) = &mut self.current_run {
            if timestamp.is_some() {
                run.last_seen_at = timestamp;
            }
        }
    }

    fn open_run(
        &mut self,
        map_name_ko: String,
        difficulty: String,
        timestamp: Option<DateTime<Utc>>,
    ) {
        let should_start_new = self
            .current_run
            .as_ref()
            .map(|run| run.map_name_ko != map_name_ko || run.difficulty != difficulty)
            .unwrap_or(true);

        if !should_start_new {
            return;
        }

        self.close_current_run(timestamp);

        self.current_run = Some(RunBuilder {
            map_name_ko,
            difficulty,
            started_at: timestamp,
            last_seen_at: timestamp,
            crystal: 0,
        });
    }

    fn close_current_run(&mut self, timestamp: Option<DateTime<Utc>>) {
        let Some(run) = self.current_run.take() else {
            return;
        };

        let duration_seconds = duration_seconds(run.started_at, timestamp.or(run.last_seen_at));

        if duration_seconds == 0 && run.crystal == 0 {
            return;
        }

        self.completed_runs.push(RunSummary {
            id: self.next_run_id,
            map_name_ko: run.map_name_ko,
            difficulty: run.difficulty,
            duration_seconds,
            crystal: run.crystal,
        });
        self.next_run_id += 1;
    }

    fn apply_crystal_total(&mut self, total: i64) {
        let delta = self
            .last_crystal_total
            .map(|previous| total - previous)
            .unwrap_or(0);
        self.last_crystal_total = Some(total);

        if delta <= 0 {
            return;
        }

        if self.current_proto.as_deref() != Some("PickItems") {
            return;
        }

        if let Some(run) = &mut self.current_run {
            run.crystal += delta;
        }
    }

    fn finish(mut self) -> TrackerSnapshot {
        let current_run = self.current_run.take().unwrap_or(RunBuilder {
            map_name_ko: "대기 중".to_string(),
            difficulty: "-".to_string(),
            started_at: None,
            last_seen_at: None,
            crystal: 0,
        });

        let current_elapsed = duration_seconds(current_run.started_at, current_run.last_seen_at);
        let total_crystal: i64 = self.completed_runs.iter().map(|run| run.crystal).sum();
        let total_seconds: i64 = self
            .completed_runs
            .iter()
            .map(|run| run.duration_seconds)
            .sum();
        let average_rate = if total_seconds > 0 {
            total_crystal as f64 / total_seconds as f64 * 3600.0
        } else {
            0.0
        };
        let average_per_run = if self.completed_runs.is_empty() {
            0.0
        } else {
            total_crystal as f64 / self.completed_runs.len() as f64
        };

        TrackerSnapshot {
            current_run: CurrentRun {
                map_name_ko: current_run.map_name_ko,
                difficulty: current_run.difficulty,
                elapsed_seconds: current_elapsed,
                crystal: current_run.crystal,
            },
            runs: self.completed_runs,
            total_crystal,
            average_rate,
            average_per_run,
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

struct LevelInfo {
    level_uid: String,
    level_id: Option<String>,
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

fn parse_crystal_total(line: &str) -> Option<i64> {
    if line.contains("ItemChange@ Update Id=100300_") {
        return parse_number_after(line, "BagNum=");
    }

    if line.contains("ConfigBaseId = 100300") {
        return parse_number_after(line, "Num =");
    }

    None
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

fn duration_seconds(start: Option<DateTime<Utc>>, end: Option<DateTime<Utc>>) -> i64 {
    match (start, end) {
        (Some(start), Some(end)) if end >= start => (end - start).num_seconds(),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_only_pick_item_crystal_delta_inside_run() {
        let snapshot = parse_log_snapshot(
            r#"
[2026.05.04-06.00.00:000][  0]TLLua: MysteryAreaModel@UpdateMysteryMapDataList AreaId == 1000 AreaLv == 6
[2026.05.04-06.00.01:000][  0]Loading@ BeginLoadingScreen MapName = /Game/Art/Maps/01SD/SD_ZhongXiGaoQiang200/SD_ZhongXiGaoQiang200
[2026.05.04-06.01.00:000][  0]ItemChange@ ProtoName=PickItems start
[2026.05.04-06.01.00:000][  0]ItemChange@ Update Id=100300_1 BagNum=10 in PageId=102 SlotId=0
[2026.05.04-06.01.01:000][  0]ItemChange@ Update Id=100300_1 BagNum=14 in PageId=102 SlotId=0
[2026.05.04-06.01.01:000][  0]ItemChange@ ProtoName=PickItems end
[2026.05.04-06.02.00:000][  0]ItemChange@ ProtoName=XchgReceive start
[2026.05.04-06.02.00:000][  0]ItemChange@ Update Id=100300_1 BagNum=114 in PageId=102 SlotId=0
[2026.05.04-06.02.01:000][  0]ItemChange@ ProtoName=XchgReceive end
      "#,
        );

        assert_eq!(snapshot.current_run.map_name_ko, "종식의 벽");
        assert_eq!(snapshot.current_run.difficulty, "7-0");
        assert_eq!(snapshot.current_run.crystal, 4);
    }

    #[test]
    fn uses_offline_korean_map_names_for_internal_codes() {
        let snapshot = parse_log_snapshot(
            r#"
[2026.05.04-07.37.07:003][555]TLShipping: Display: [Game] LevelMgr@ LevelUid, LevelType, LevelId = 1061011 3 4611
[2026.05.04-07.37.07:123][556]TLShipping: Display: [Game] Loading@ BeginLoadingScreen MapName = /Game/Art/Maps/01SD/SD_GeBuLinYingDi/SD_GeBuLinYingDi
      "#,
        );

        assert_eq!(snapshot.current_run.map_name_ko, "바람의 협곡");
        assert_eq!(snapshot.current_run.difficulty, "7-0");
    }

    #[test]
    fn strips_numeric_suffix_after_exact_map_lookup_fails() {
        let snapshot = parse_log_snapshot(
            r#"
[2026.05.04-06.00.00:000][  0]TLLua: MysteryAreaModel@UpdateMysteryMapDataList AreaId == 1000 AreaLv == 6
[2026.05.04-06.00.01:000][  0]Loading@ BeginLoadingScreen MapName = /Game/Art/Maps/01SD/SD_ZhongXiGaoQiang200/SD_ZhongXiGaoQiang200
      "#,
        );

        assert_eq!(snapshot.current_run.map_name_ko, "종식의 벽");
    }
}
