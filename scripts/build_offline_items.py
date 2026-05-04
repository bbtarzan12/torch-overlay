#!/usr/bin/env python3
"""Build an offline Korean item map for Torch Overlay.

The public YiHuo ETor tracker already carries a broad item seed list in
Rt-data.json. This script uses that list only as an ID/slug discovery source,
then resolves Korean display names from TLIDB autocomplete data.
"""

from __future__ import annotations

import argparse
import hashlib
import html
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.parse
import urllib.request
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


TLIDB_KO_BASE = "https://tlidb.com/ko"
TLIDB_BASE = "https://tlidb.com"
TLIDB_ASSET_VERSION = "1776388776"
PUBLIC_TRACKER_ASAR_URL = (
    "https://raw.githubusercontent.com/Giboork/TLI-tracker-translated/"
    "main/resources/app.asar"
)

DEFAULT_START_PATHS = [
    "/ko/",
    "/ko/Inventory",
    "/ko/Legendary_Gear",
    "/ko/Destiny",
    "/ko/Active_Skill",
    "/ko/Support_Skill",
    "/ko/Passive_Skill",
    "/ko/Triggered_Skill",
    "/ko/Activation_Medium_Skill",
    "/ko/Magnificent_Support_Skill",
    "/ko/Noble_Support_Skill",
    "/ko/Modularization_Skill",
    "/ko/Lunaria_Season_items",
    "/ko/Lunaria_Season_skills",
    "/ko/Overrealm_exclusive_drop",
    "/ko/Vorax_Gameplay_Exclusive_Drops",
]

AUTOCOMPLETE_SEED_DESC_PATTERNS = [
    "시즌 나침반",
    "추억의 부활 재료",
    "추억 강화 재료",
]

CURATED_KO_FALLBACKS: dict[str, dict[str, str]] = {
    "7090": {"nameKo": "질주 이슬", "categoryKo": "지속", "slug": "Swiftness_Dew"},
    "7130": {"nameKo": "위축", "categoryKo": "주술", "slug": "Timid"},
    "7143": {"nameKo": "생명 물약", "categoryKo": "회복", "slug": "Life_Tonic"},
    "7144": {"nameKo": "마력 물약", "categoryKo": "회복", "slug": "Mana_Tonic"},
    "7156": {"nameKo": "갈증 이슬", "categoryKo": "지속", "slug": "Thirst_Dew"},
    "7169": {"nameKo": "복합 물약", "categoryKo": "회복", "slug": "Compound_Tonic"},
    "7614": {"nameKo": "신진대사 촉진", "categoryKo": "보조", "slug": "Hyper_Metabolism"},
    "7615": {"nameKo": "구급", "categoryKo": "보조", "slug": "Emergency_Aid"},
    "7616": {"nameKo": "약성 축적", "categoryKo": "보조", "slug": "Medicinal_Buildup"},
    "10290": {"nameKo": "블러드 러스트 나침반", "categoryKo": "나침반", "slug": "Pig_Iron_Scalpel"},
    "10291": {"nameKo": "도살자의 블러드 러스트 나침반", "categoryKo": "나침반", "slug": "Sterling_Silver_Scalpel"},
    "10292": {"nameKo": "협진의 블러드 러스트 나침반", "categoryKo": "나침반", "slug": "Titanium-Alloy_Scalpel"},
    "112149": {"nameKo": "사르투야", "categoryKo": "레전드", "slug": "Sarituya"},
    "112150": {"nameKo": "아르슬란", "categoryKo": "레전드", "slug": "Aslan"},
    "112255": {"nameKo": "상실의 포옹", "categoryKo": "레전드", "slug": "Trauma's_Embrace"},
    "350505": {"nameKo": "천명: 족쇄", "categoryKo": "운명", "slug": "Kismet:_Shackles"},
    "360607": {"nameKo": "제노 프리즘: 행운과 재앙", "categoryKo": "프리즘", "slug": "Ethereal_Prism:_Fortune's_Flip"},
    "360617": {"nameKo": "제노 프리즘: 단호한 결단", "categoryKo": "프리즘", "slug": "Ethereal_Prism:_Guaranteed_Reaping"},
    "360625": {"nameKo": "제노 프리즘: 유유상종", "categoryKo": "프리즘", "slug": "Ethereal_Prism:_Tainted_Flesh"},
    "381040": {"nameKo": "완벽한 척추: 비틀림", "categoryKo": "특수 장기-메인 옵션", "slug": "Flawless_Spine:_Torqued"},
    "382028": {"nameKo": "장기: 발 없는 새의 설움", "categoryKo": "핵심 장기", "slug": "Organ:_Legless_Bird's_Lament"},
    "382101": {"nameKo": "장기: 고문의 시작", "categoryKo": "핵심 장기", "slug": "Organ:_Torturer's_Touch"},
    "382109": {"nameKo": "장기: 흉악한 신의 손", "categoryKo": "핵심 장기", "slug": "Organ:_Death's_Touch"},
    "382111": {"nameKo": "장기: 군마의 손", "categoryKo": "핵심 장기", "slug": "Organ:_Demons'_Touch"},
    "382198": {"nameKo": "장기: 진리", "categoryKo": "핵심 장기", "slug": "Organ:_Truth"},
}


@dataclass
class Candidate:
    slug: str | None = None
    config_base_id: str | None = None
    name_ko_hint: str | None = None
    name_zh: str | None = None
    category_ko_hint: str | None = None
    category_zh: str | None = None
    icon: str | None = None
    source_url: str | None = None
    source_kind: str = "tlidb"
    reward_zh: str | None = None
    base_item_id: str | None = None
    autocomplete_value: str | None = None


@dataclass
class ItemRecord:
    config_base_id: str
    name_ko: str | None = None
    name_zh: str | None = None
    category_ko: str | None = None
    category_zh: str | None = None
    slug: str | None = None
    icon: str | None = None
    icon_alt: str | None = None
    availability: str | None = None
    seasons: list[str] = field(default_factory=list)
    url_ko: str | None = None
    url_source: str | None = None
    reward_zh: str | None = None
    base_item_id: str | None = None
    sources: set[str] = field(default_factory=set)


def fetch_bytes(url: str, cache_dir: Path, refresh: bool, offline: bool) -> bytes:
    cache_dir.mkdir(parents=True, exist_ok=True)
    cache_key = hashlib.sha1(url.encode("utf-8")).hexdigest()
    cache_path = cache_dir / cache_key
    if cache_path.exists() and not refresh:
        return cache_path.read_bytes()
    if offline:
        raise RuntimeError(f"Cache miss in offline mode: {url}")

    request = urllib.request.Request(url, headers={"User-Agent": "TorchOverlayDataBuilder/0.1"})
    with urllib.request.urlopen(request, timeout=30) as response:
        data = response.read()
    cache_path.write_bytes(data)
    return data


def fetch_text(url: str, cache_dir: Path, refresh: bool, offline: bool) -> str:
    raw = fetch_bytes(url, cache_dir, refresh, offline)
    return raw.decode("utf-8", errors="replace")


def normalize_space(value: str) -> str:
    value = html.unescape(value)
    value = re.sub(r"<[^>]+>", "", value, flags=re.DOTALL)
    value = value.replace("\\n", " ")
    value = re.sub(r"\s+", " ", value)
    return value.strip()


def normalize_lookup_key(value: str | None) -> str:
    if not value:
        return ""
    return normalize_space(urllib.parse.unquote(value)).casefold()


def slug_from_url(value: str | None) -> str | None:
    if not value:
        return None
    parsed = urllib.parse.urlparse(value)
    path = parsed.path if parsed.scheme else value.split("?", 1)[0]
    path = path.strip("/")
    parts = path.split("/")
    if parts and parts[0] in {"cn", "ko", "en", "tw", "ja"}:
        parts = parts[1:]
    if not parts or not parts[-1]:
        return None
    return urllib.parse.unquote(parts[-1])


def canonical_slug(value: str | None) -> str | None:
    key = normalize_lookup_key(value)
    return key or None


def detail_url_from_slug(slug: str) -> str:
    return f"{TLIDB_KO_BASE}/{urllib.parse.quote(slug, safe='_-')}"


def extract_id_from_href(href: str) -> str | None:
    parsed = urllib.parse.urlparse(href)
    query = urllib.parse.parse_qs(parsed.query)
    for values in query.values():
        for value in values:
            match = re.search(r"(?:ItemBase|ItemGold|Skill|Item)/(\d+)", value)
            if match:
                return match.group(1)
    match = re.search(r"(?:ItemBase|ItemGold|Skill|Item)%2F(\d+)", href, flags=re.IGNORECASE)
    if match:
        return match.group(1)
    return None


def load_seed_rt_data(path: Path) -> list[Candidate]:
    raw_items = json.loads(path.read_text(encoding="utf-8"))
    candidates: list[Candidate] = []
    for item in raw_items:
        config_base_id = str(item.get("id", "")).strip()
        if not config_base_id:
            continue
        candidates.append(
            Candidate(
                slug=slug_from_url(item.get("url")),
                config_base_id=config_base_id,
                name_zh=item.get("name") or None,
                category_zh=item.get("type") or None,
                icon=item.get("img") or None,
                source_url=item.get("url") or None,
                source_kind="public-tracker-rt-data",
                reward_zh=item.get("reward") or None,
                base_item_id=item.get("baseItemId") or None,
            )
        )
    return candidates


def load_autocomplete(lang: str, cache_dir: Path, version: str, refresh: bool, offline: bool) -> list[dict[str, str]]:
    url = f"{TLIDB_BASE}/i18n/autocomplete_{lang}.json?_={version}"
    return json.loads(fetch_text(url, cache_dir, refresh, offline))


def index_autocomplete(entries: list[dict[str, str]]) -> tuple[dict[str, list[dict[str, str]]], dict[str, list[dict[str, str]]]]:
    by_value: dict[str, list[dict[str, str]]] = {}
    by_label: dict[str, list[dict[str, str]]] = {}
    for entry in entries:
        value_key = normalize_lookup_key(entry.get("value"))
        label_key = normalize_lookup_key(entry.get("label"))
        if value_key:
            by_value.setdefault(value_key, []).append(entry)
        if label_key:
            by_label.setdefault(label_key, []).append(entry)
    return by_value, by_label


def enrich_candidates_from_autocomplete(
    candidates: list[Candidate],
    ko_by_value: dict[str, list[dict[str, str]]],
    cn_by_label: dict[str, list[dict[str, str]]],
) -> int:
    matched = 0
    for candidate in candidates:
        ko_entry = None
        if candidate.slug:
            ko_entries = ko_by_value.get(normalize_lookup_key(candidate.slug), [])
            if ko_entries:
                ko_entry = ko_entries[0]

        if not ko_entry and candidate.name_zh:
            cn_entries = cn_by_label.get(normalize_lookup_key(candidate.name_zh), [])
            for cn_entry in cn_entries:
                ko_entries = ko_by_value.get(normalize_lookup_key(cn_entry.get("value")), [])
                if ko_entries:
                    ko_entry = ko_entries[0]
                    candidate.slug = candidate.slug or cn_entry.get("value")
                    candidate.autocomplete_value = cn_entry.get("value")
                    break

        if not ko_entry:
            continue

        candidate.name_ko_hint = candidate.name_ko_hint or ko_entry.get("label")
        candidate.category_ko_hint = candidate.category_ko_hint or ko_entry.get("desc")
        candidate.slug = candidate.slug or ko_entry.get("value")
        candidate.autocomplete_value = candidate.autocomplete_value or ko_entry.get("value")
        if "tlidb-autocomplete" not in candidate.source_kind:
            candidate.source_kind = f"{candidate.source_kind},tlidb-autocomplete"
        matched += 1
    return matched


def autocomplete_seed_candidates(entries: list[dict[str, str]]) -> list[Candidate]:
    candidates: list[Candidate] = []

    for entry in entries:
        desc = entry.get("desc") or ""
        if not any(pattern in desc for pattern in AUTOCOMPLETE_SEED_DESC_PATTERNS):
            continue

        slug = entry.get("value")
        name_ko = entry.get("label")
        if not slug or not name_ko:
            continue

        candidates.append(
            Candidate(
                slug=slug,
                name_ko_hint=name_ko,
                category_ko_hint=desc,
                source_url=detail_url_from_slug(slug),
                source_kind="tlidb-autocomplete-seed",
            )
        )

    return candidates


def apply_curated_fallbacks(candidates: list[Candidate]) -> int:
    applied = 0
    for candidate in candidates:
        if not candidate.config_base_id:
            continue
        fallback = CURATED_KO_FALLBACKS.get(candidate.config_base_id)
        if not fallback:
            continue
        changed = False
        if not candidate.name_ko_hint:
            candidate.name_ko_hint = fallback["nameKo"]
            changed = True
        if not candidate.category_ko_hint:
            candidate.category_ko_hint = fallback["categoryKo"]
            changed = True
        if not candidate.slug:
            candidate.slug = fallback["slug"]
            changed = True
        if "tlidb-curated-fallback" not in candidate.source_kind:
            candidate.source_kind = f"{candidate.source_kind},tlidb-curated-fallback"
        if changed:
            applied += 1
    return applied


def extract_rt_data_from_asar(asar_path: Path) -> list[Candidate]:
    npx = shutil.which("npx")
    if not npx:
        raise RuntimeError("npx is required to extract Rt-data.json from app.asar")

    asar_path = asar_path.resolve()
    with tempfile.TemporaryDirectory(prefix="torch-overlay-asar-") as temp_dir:
        subprocess.run(
            [npx, "--yes", "asar", "extract-file", str(asar_path), "Rt-data.json"],
            cwd=temp_dir,
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        rt_data_path = Path(temp_dir) / "Rt-data.json"
        if not rt_data_path.exists():
            raise RuntimeError("asar extraction did not produce Rt-data.json")
        return load_seed_rt_data(rt_data_path)


def load_public_tracker_seed(args: argparse.Namespace, cache_dir: Path) -> list[Candidate]:
    if args.seed_rt_data:
        return load_seed_rt_data(Path(args.seed_rt_data))
    if args.public_tracker_asar:
        return extract_rt_data_from_asar(Path(args.public_tracker_asar))
    if not args.use_public_tracker:
        return []

    asar_path = cache_dir / "public-tracker-app.asar"
    if not asar_path.exists() or args.refresh:
        data = fetch_bytes(args.public_tracker_asar_url, cache_dir, args.refresh, args.offline)
        asar_path.write_bytes(data)
    return extract_rt_data_from_asar(asar_path)


def parse_list_candidates(page_html: str, page_url: str) -> list[Candidate]:
    candidates: list[Candidate] = []
    pattern = re.compile(
        r"<a\b(?=[^>]*\bdata-hover=)(?=[^>]*\bhref=)([^>]*)>(.*?)</a>",
        flags=re.DOTALL | re.IGNORECASE,
    )
    attr_pattern = re.compile(r"([a-zA-Z0-9_-]+)\s*=\s*(['\"])(.*?)\2", flags=re.DOTALL)

    for match in pattern.finditer(page_html):
        attrs = {m.group(1).lower(): html.unescape(m.group(3)) for m in attr_pattern.finditer(match.group(1))}
        href = attrs.get("href")
        if not href:
            continue
        name_hint = normalize_space(match.group(2))
        if not name_hint:
            continue
        config_base_id = extract_id_from_href(href) or extract_id_from_href(attrs.get("data-hover", ""))
        slug = slug_from_url(href)
        if slug in {"ko", "cn", "en"}:
            slug = None
        icon = None
        icon_match = re.search(r"<img\b[^>]*\bsrc\s*=\s*(['\"])(.*?)\1", match.group(2), flags=re.DOTALL)
        if icon_match:
            icon = html.unescape(icon_match.group(2))
        candidates.append(
            Candidate(
                slug=slug,
                config_base_id=config_base_id,
                name_ko_hint=name_hint,
                icon=icon,
                source_url=page_url,
                source_kind="tlidb-list",
            )
        )
    return candidates


def parse_detail_page(page_html: str) -> dict[str, Any]:
    title_match = re.search(r'<meta\s+property="og:title"\s+content="([^"]+)"', page_html, flags=re.IGNORECASE)
    if not title_match:
        title_match = re.search(r"<h1[^>]*>(.*?)</h1>", page_html, flags=re.DOTALL | re.IGNORECASE)

    id_match = re.search(r"\bid:\s*(\d+)\b", page_html)
    icon_match = re.search(
        r'<img\s+src="([^"]+)"\s+alt="([^"]*)"\s+class="[^"]*\bui_item_base\b',
        page_html,
        flags=re.DOTALL | re.IGNORECASE,
    )
    if not icon_match:
        icon_match = re.search(r'<img\s+src="([^"]+)"\s+alt="([^"]*)"', page_html, flags=re.DOTALL | re.IGNORECASE)

    tags = [
        normalize_space(m.group(1))
        for m in re.finditer(r'<span\b[^>]*\btag\b[^>]*>(.*?)</span>', page_html, flags=re.DOTALL | re.IGNORECASE)
    ]
    seasons = [
        normalize_space(m.group(1))
        for m in re.finditer(r'<div\s+class="item_ver">(.*?)</div>', page_html, flags=re.DOTALL | re.IGNORECASE)
    ]

    return {
        "configBaseId": id_match.group(1) if id_match else None,
        "nameKo": normalize_space(title_match.group(1)) if title_match else None,
        "icon": html.unescape(icon_match.group(1)) if icon_match else None,
        "iconAlt": html.unescape(icon_match.group(2)) if icon_match else None,
        "categoryKo": tags[0] if tags else None,
        "seasons": seasons,
    }


def merge_record(records: dict[str, ItemRecord], candidate: Candidate, detail: dict[str, Any] | None = None) -> None:
    config_base_id = (detail or {}).get("configBaseId") or candidate.config_base_id
    if not config_base_id:
        return

    record = records.get(config_base_id)
    if not record:
        record = ItemRecord(config_base_id=config_base_id)
        records[config_base_id] = record

    if detail:
        record.name_ko = detail.get("nameKo") or record.name_ko
        record.category_ko = detail.get("categoryKo") or record.category_ko
        record.icon = detail.get("icon") or record.icon
        record.icon_alt = detail.get("iconAlt") or record.icon_alt
        for season in detail.get("seasons") or []:
            if season and season not in record.seasons:
                record.seasons.append(season)
        if detail.get("seasons"):
            record.availability = "season" if any("시즌" in season for season in detail["seasons"]) else record.availability

    record.name_ko = record.name_ko or candidate.name_ko_hint
    record.category_ko = record.category_ko or candidate.category_ko_hint
    record.name_zh = record.name_zh or candidate.name_zh
    record.category_zh = record.category_zh or candidate.category_zh
    record.slug = record.slug or candidate.slug
    record.icon = record.icon or candidate.icon
    record.url_source = record.url_source or candidate.source_url
    record.reward_zh = record.reward_zh or candidate.reward_zh
    record.base_item_id = record.base_item_id or candidate.base_item_id
    if record.slug:
        record.url_ko = detail_url_from_slug(record.slug)
    for source in candidate.source_kind.split(","):
        if source:
            record.sources.add(source)


def merge_candidate(existing: Candidate, incoming: Candidate) -> Candidate:
    existing.slug = existing.slug or incoming.slug
    existing.config_base_id = existing.config_base_id or incoming.config_base_id
    existing.name_ko_hint = existing.name_ko_hint or incoming.name_ko_hint
    existing.name_zh = existing.name_zh or incoming.name_zh
    existing.category_ko_hint = existing.category_ko_hint or incoming.category_ko_hint
    existing.category_zh = existing.category_zh or incoming.category_zh
    existing.icon = existing.icon or incoming.icon
    existing.source_url = existing.source_url or incoming.source_url
    existing.reward_zh = existing.reward_zh or incoming.reward_zh
    existing.base_item_id = existing.base_item_id or incoming.base_item_id
    existing.autocomplete_value = existing.autocomplete_value or incoming.autocomplete_value
    if existing.source_kind != incoming.source_kind:
        existing.source_kind = f"{existing.source_kind},{incoming.source_kind}"
    return existing


def build_snapshot(args: argparse.Namespace) -> dict[str, Any]:
    cache_dir = Path(args.cache_dir)
    http_cache_dir = cache_dir / "http"
    autocomplete_cache_dir = cache_dir / "autocomplete"
    candidates = load_public_tracker_seed(args, cache_dir)
    source_counts: dict[str, int] = {"publicTrackerSeed": len(candidates)}
    unresolved: list[dict[str, Any]] = []

    autocomplete_match_count = 0
    autocomplete_seed_count = 0
    if not args.no_autocomplete:
        ko_autocomplete = load_autocomplete("ko", autocomplete_cache_dir, args.autocomplete_version, args.refresh, args.offline)
        cn_autocomplete = load_autocomplete("cn", autocomplete_cache_dir, args.autocomplete_version, args.refresh, args.offline)
        ko_by_value, _ = index_autocomplete(ko_autocomplete)
        _, cn_by_label = index_autocomplete(cn_autocomplete)
        autocomplete_match_count = enrich_candidates_from_autocomplete(candidates, ko_by_value, cn_by_label)
        if args.seed_autocomplete_items:
            autocomplete_candidates = autocomplete_seed_candidates(ko_autocomplete)
            autocomplete_seed_count = len(autocomplete_candidates)
            candidates.extend(autocomplete_candidates)
        source_counts["autocompleteKo"] = len(ko_autocomplete)
        source_counts["autocompleteCn"] = len(cn_autocomplete)
        source_counts["autocompleteSeed"] = autocomplete_seed_count

    curated_fallback_count = apply_curated_fallbacks(candidates)
    source_counts["curatedFallbacks"] = curated_fallback_count

    if args.include_start_pages:
        for path in args.start_path:
            page_url = path if path.startswith("http") else f"https://tlidb.com{path}"
            try:
                page_html = fetch_text(page_url, http_cache_dir, args.refresh, args.offline)
                page_candidates = parse_list_candidates(page_html, page_url)
                source_counts[f"list:{path}"] = len(page_candidates)
                candidates.extend(page_candidates)
            except Exception as error:  # noqa: BLE001 - data update should continue collecting failures.
                source_counts[f"list:{path}"] = 0
                unresolved.append({"type": "listFetchFailed", "url": page_url, "error": str(error)})

    slug_to_seed_id: dict[str, str] = {
        canonical_slug(candidate.slug): candidate.config_base_id
        for candidate in candidates
        if candidate.slug and candidate.config_base_id and canonical_slug(candidate.slug)
    }
    slug_to_ko_hint: dict[str, str] = {
        canonical_slug(candidate.slug): candidate.name_ko_hint
        for candidate in candidates
        if candidate.slug and candidate.name_ko_hint and canonical_slug(candidate.slug)
    }
    for candidate in candidates:
        if candidate.slug and not candidate.config_base_id:
            candidate.config_base_id = slug_to_seed_id.get(canonical_slug(candidate.slug))
        if candidate.slug and not candidate.name_ko_hint:
            candidate.name_ko_hint = slug_to_ko_hint.get(canonical_slug(candidate.slug))

    candidate_map: dict[tuple[str | None, str | None], Candidate] = {}
    for candidate in candidates:
        key = (candidate.config_base_id, candidate.slug)
        if key in candidate_map:
            merge_candidate(candidate_map[key], candidate)
        else:
            candidate_map[key] = candidate
    unique_candidates = list(candidate_map.values())

    records: dict[str, ItemRecord] = {}
    detail_count = 0
    mismatch_count = 0

    for index, candidate in enumerate(unique_candidates, start=1):
        detail: dict[str, Any] | None = None
        needs_detail = bool(candidate.slug and not (candidate.config_base_id and candidate.name_ko_hint))
        if args.resolve_details and needs_detail and (args.limit <= 0 or detail_count < args.limit):
            detail_count += 1
            try:
                url = detail_url_from_slug(candidate.slug)
                page_html = fetch_text(url, http_cache_dir, args.refresh, args.offline)
                detail = parse_detail_page(page_html)
                if candidate.config_base_id and detail.get("configBaseId") and candidate.config_base_id != detail["configBaseId"]:
                    mismatch_count += 1
                    unresolved.append(
                        {
                            "type": "idMismatch",
                            "seedId": candidate.config_base_id,
                            "detailId": detail["configBaseId"],
                            "slug": candidate.slug,
                            "source": candidate.source_kind,
                        }
                    )
                if args.delay > 0:
                    time.sleep(args.delay)
            except Exception as error:  # noqa: BLE001 - data update should continue collecting failures.
                unresolved.append(
                    {
                        "type": "detailFetchFailed",
                        "id": candidate.config_base_id,
                        "slug": candidate.slug,
                        "source": candidate.source_kind,
                        "error": str(error),
                    }
                )
        merge_record(records, candidate, detail)

        if args.progress and index % args.progress == 0:
            print(f"processed={index} detailFetched={detail_count} records={len(records)}", file=sys.stderr)

    for candidate in unique_candidates:
        if candidate.config_base_id and candidate.config_base_id not in records:
            unresolved.append(
                {
                    "type": "missingRecord",
                    "id": candidate.config_base_id,
                    "nameZh": candidate.name_zh,
                    "source": candidate.source_kind,
                }
            )

    items_by_id: dict[str, Any] = {}
    missing_korean_names: list[dict[str, Any]] = []
    for config_base_id, record in sorted(records.items(), key=lambda item: int(item[0])):
        item = {
            "nameKo": record.name_ko,
            "nameZh": record.name_zh,
            "categoryKo": record.category_ko,
            "categoryZh": record.category_zh,
            "slug": record.slug,
            "icon": record.icon,
            "iconAlt": record.icon_alt,
            "availability": record.availability,
            "seasons": record.seasons,
            "urlKo": record.url_ko,
            "urlSource": record.url_source,
            "rewardZh": record.reward_zh,
            "baseItemId": record.base_item_id,
            "sources": sorted(record.sources),
        }
        items_by_id[config_base_id] = {key: value for key, value in item.items() if value not in (None, "", [])}
        if not record.name_ko:
            missing_korean_names.append(
                {
                    "id": config_base_id,
                    "slug": record.slug,
                    "nameZh": record.name_zh,
                    "categoryZh": record.category_zh,
                }
            )

    return {
        "schemaVersion": 1,
        "generatedAt": datetime.now(timezone.utc).isoformat(),
        "language": "ko",
        "sources": {
            "tlidb": f"{TLIDB_KO_BASE}/",
            "publicTracker": "https://github.com/Giboork/TLI-tracker-translated",
            "publicTrackerAsar": args.public_tracker_asar_url if args.use_public_tracker else None,
        },
        "stats": {
            "candidateCount": len(unique_candidates),
            "detailFetchCount": detail_count,
            "autocompleteMatchCount": autocomplete_match_count,
            "itemCount": len(items_by_id),
            "missingKoreanNameCount": len(missing_korean_names),
            "idMismatchCount": mismatch_count,
            "sourceCounts": source_counts,
        },
        "itemsByConfigBaseId": items_by_id,
        "missingKoreanNames": missing_korean_names,
        "unresolved": unresolved,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", default="data/offline/items.ko.json")
    parser.add_argument("--cache-dir", default=".cache/tlidb-items")
    parser.add_argument("--seed-rt-data", default=None, help="Path to an extracted Rt-data.json seed.")
    parser.add_argument("--public-tracker-asar", default=None, help="Path to an extracted public tracker app.asar.")
    parser.add_argument("--use-public-tracker", action="store_true", help="Download app.asar from the public tracker and extract Rt-data.json.")
    parser.add_argument("--public-tracker-asar-url", default=PUBLIC_TRACKER_ASAR_URL)
    parser.add_argument("--autocomplete-version", default=TLIDB_ASSET_VERSION)
    parser.add_argument("--no-autocomplete", action="store_true", help="Do not use TLIDB autocomplete JSON.")
    parser.add_argument(
        "--seed-autocomplete-items",
        action="store_true",
        help="Also seed known item material categories from TLIDB autocomplete JSON.",
    )
    parser.add_argument("--include-start-pages", action="store_true", help="Also discover item links from TLIDB list pages.")
    parser.add_argument("--start-path", action="append", default=DEFAULT_START_PATHS)
    parser.add_argument("--resolve-details", action="store_true", help="Fetch TLIDB detail pages for unresolved items.")
    parser.add_argument("--limit", type=int, default=0, help="Limit detail page fetches. 0 means no limit.")
    parser.add_argument("--delay", type=float, default=0.5, help="Delay between detail page requests.")
    parser.add_argument("--refresh", action="store_true", help="Ignore cached HTTP responses.")
    parser.add_argument("--offline", action="store_true", help="Only use cached HTTP responses.")
    parser.add_argument("--progress", type=int, default=100)
    args = parser.parse_args()

    if not args.seed_rt_data and not args.public_tracker_asar and not args.use_public_tracker and not args.include_start_pages:
        parser.error("Provide --use-public-tracker, --seed-rt-data, --public-tracker-asar, or --include-start-pages.")

    snapshot = build_snapshot(args)
    out_path = Path(args.out)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(
        json.dumps(snapshot, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )

    stats = snapshot["stats"]
    print(f"Wrote {out_path}")
    print(f"items={stats['itemCount']}")
    print(f"candidates={stats['candidateCount']}")
    print(f"detailFetches={stats['detailFetchCount']}")
    print(f"missingKoreanNames={stats['missingKoreanNameCount']}")
    print(f"idMismatches={stats['idMismatchCount']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
