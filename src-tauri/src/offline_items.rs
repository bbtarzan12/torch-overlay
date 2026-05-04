use serde::Deserialize;
use std::{collections::BTreeMap, sync::OnceLock};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OfflineItemSnapshot {
    items_by_config_base_id: BTreeMap<String, OfflineItem>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OfflineItem {
    name_ko: Option<String>,
}

static OFFLINE_ITEMS: OnceLock<Option<BTreeMap<i64, OfflineItem>>> = OnceLock::new();

pub fn item_name_ko(config_base_id: i64) -> Option<String> {
    offline_items()?
        .get(&config_base_id)
        .and_then(|item| item.name_ko.clone())
}

#[cfg(test)]
pub fn item_count() -> usize {
    offline_items().map_or(0, BTreeMap::len)
}

fn offline_items() -> Option<&'static BTreeMap<i64, OfflineItem>> {
    OFFLINE_ITEMS
        .get_or_init(|| {
            let snapshot: OfflineItemSnapshot =
                serde_json::from_str(include_str!("../../data/offline/items.ko.json")).ok()?;

            Some(
                snapshot
                    .items_by_config_base_id
                    .into_iter()
                    .filter_map(|(key, item)| key.parse::<i64>().ok().map(|id| (id, item)))
                    .collect(),
            )
        })
        .as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embeds_generated_item_mapping() {
        assert_eq!(item_count(), 1957);
        assert_eq!(item_name_ko(100300).as_deref(), Some("최초의 불꽃 결정"));
        assert_eq!(item_name_ko(391000).as_deref(), Some("거친 루나 가루"));
        assert_eq!(item_name_ko(112255).as_deref(), Some("상실의 포옹"));
    }
}
