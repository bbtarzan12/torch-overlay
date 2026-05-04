#!/usr/bin/env python3
"""Build an offline Korean zone map for the TLI tracker.

Runtime should not depend on TLIDB or GitHub. Run this script during data
updates, commit/package the generated JSON, and let the app read that file.
"""

from __future__ import annotations

import argparse
import ast
import html
import json
import re
import sys
import urllib.parse
import urllib.request
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


TLIDB_BASE = "https://tlidb.com/ko"
TITRACK_ZONES_URL = (
    "https://raw.githubusercontent.com/astockman99/TITrack/"
    "master/src/titrack/data/zones.py"
)

REGION_SLUGS = [
    "Blistering_Lava_Sea",
    "Glacial_Abyss",
    "Steel_Forge",
    "Thunder_Wastes",
    "Voidlands",
    "Deep_Space",
]

REGION_EN_TO_SLUG = {
    "Blistering Lava Sea": "Blistering_Lava_Sea",
    "Glacial Abyss": "Glacial_Abyss",
    "Steel Forge": "Steel_Forge",
    "Thunder Wastes": "Thunder_Wastes",
    "Voidlands": "Voidlands",
    "Deep Space": "Deep_Space",
}

SPECIAL_KO_NAMES = {
    "Hideout - Ember's Rest": "은신처 - 불꽃의 안식처",
    "Hideout - Sacred Court Manor": "은신처 - 성스러운 정원 저택",
    "Cloud Oasis (Sandlord)": "구름 오아시스",
    "Rift of Dimensions": "차원의 균열",
    "Secret Realm - Invaluable Time": "비경 - 귀중한 시간",
    "Secret Realm - Sea of Rites": "비경 - 의식의 바다",
    "Secret Realm - Unholy Pedestal": "비경 - 불경한 받침대",
    "Secret Realm - Abyssal Vault": "비경 - 심연의 보물고",
    "Supreme Showdown": "최고 대결",
    "Fateful Contest": "운명의 대결",
    "Mistville": "안개 도시",
    "Void Sea Terminal": "허공의 바다 터미널",
    "Ruins of Aeterna: Boundless": "이터나 유적: 무한",
    "The Frozen Canvas": "얼어붙은 화폭",
    "Vorax - Shelly's Operating Theater": "보락스 - 셀리의 수술실",
    "Rusted Abyss": "녹슨 심연",
    "Path of the Brave": "용자의 길",
    "Trial of Divinity": "신성의 시련",
    "Quicksand Treasure Stash (Sandlord)": "유사 보물 창고",
    "Demiman Village": "야인 마을",
    "Grimwind Woods": "비극의 숲",
}

AREA_SLUG_ALIASES = {
    ("Blistering Lava Sea", "Dragonrest Cavern"): "Dragonrest_Canyon",
}


@dataclass(frozen=True)
class Area:
    slug: str
    name_ko: str
    region_slug: str
    region_ko: str


def fetch_text(url: str) -> str:
    request = urllib.request.Request(url, headers={"User-Agent": "TLI-KR-Tracker/0.1"})
    with urllib.request.urlopen(request, timeout=30) as response:
        raw = response.read()
    return raw.decode("utf-8", errors="replace")


def normalize_key(value: str) -> str:
    decoded = urllib.parse.unquote(value)
    decoded = decoded.replace("_", " ")
    decoded = decoded.replace(":", " ")
    decoded = decoded.replace("-", " ")
    decoded = re.sub(r"\s+", " ", decoded)
    decoded = decoded.strip().casefold()
    decoded = re.sub(r"[^0-9a-z가-힣']+", "", decoded)
    return decoded


def slug_from_href(href: str) -> str:
    href = href.split("#", 1)[0].split("?", 1)[0].strip("/")
    return urllib.parse.unquote(href)


def extract_region_name(page_html: str, fallback_slug: str) -> str:
    match = re.search(r'<div class="card-header">([^<]+?)\s+-\s+Area\s+/', page_html)
    if match:
        return html.unescape(match.group(1)).strip()

    match = re.search(r'data-bs-target="#[^"]+-Area">([^<]+?)\s+-\s+Area\s+/', page_html)
    if match:
        return html.unescape(match.group(1)).strip()

    return fallback_slug.replace("_", " ")


def extract_areas(page_html: str, region_slug: str, region_ko: str) -> list[Area]:
    section_match = re.search(
        r'<div id="[^"]+-Area" class="tab-pane[^"]*">(.*?)</tbody>',
        page_html,
        flags=re.DOTALL,
    )
    if not section_match:
        return []

    section = section_match.group(1)
    areas: list[Area] = []
    seen: set[str] = set()
    for href, label in re.findall(r'<a href="([^"]+)">([^<]+)</a>', section):
        slug = slug_from_href(href)
        if not slug or slug in seen:
            continue
        seen.add(slug)
        areas.append(
            Area(
                slug=slug,
                name_ko=html.unescape(label).strip(),
                region_slug=region_slug,
                region_ko=region_ko,
            )
        )
    return areas


def parse_titrack_constants(source: str) -> dict[str, Any]:
    module = ast.parse(source)
    constants: dict[str, Any] = {}
    for node in module.body:
        if not isinstance(node, ast.Assign):
            continue
        for target in node.targets:
            if isinstance(target, ast.Name) and target.id in {
                "ZONE_NAMES",
                "AMBIGUOUS_ZONES",
                "LEVEL_ID_ZONES",
            }:
                constants[target.id] = ast.literal_eval(node.value)
    missing = {"ZONE_NAMES", "AMBIGUOUS_ZONES", "LEVEL_ID_ZONES"} - constants.keys()
    if missing:
        raise RuntimeError(f"Missing constants from TITrack zones.py: {sorted(missing)}")
    return constants


def split_display_name(display_en: str) -> tuple[str | None, str]:
    if " - " not in display_en:
        return None, display_en
    region_en, area_en = display_en.split(" - ", 1)
    return region_en.strip(), area_en.strip()


def build_area_indexes(regions: list[dict[str, Any]]) -> tuple[dict[str, Area], dict[tuple[str, str], Area]]:
    by_slug: dict[str, Area] = {}
    by_region_and_name: dict[tuple[str, str], Area] = {}
    for region in regions:
        for raw_area in region["areas"]:
            area = Area(
                slug=raw_area["slug"],
                name_ko=raw_area["nameKo"],
                region_slug=region["slug"],
                region_ko=region["nameKo"],
            )
            by_slug[area.slug] = area
            by_region_and_name[(region["slug"], normalize_key(area.slug))] = area
    return by_slug, by_region_and_name


def enrich_display_name(
    display_en: str,
    regions_by_slug: dict[str, dict[str, Any]],
    areas_by_region_and_name: dict[tuple[str, str], Area],
) -> dict[str, Any]:
    region_en, area_en = split_display_name(display_en)
    if display_en in SPECIAL_KO_NAMES:
        return {"nameKo": SPECIAL_KO_NAMES[display_en], "regionSlug": None, "areaSlug": None}

    if region_en is None:
        return {"nameKo": SPECIAL_KO_NAMES.get(display_en, display_en), "regionSlug": None, "areaSlug": None}

    region_slug = REGION_EN_TO_SLUG.get(region_en)
    if not region_slug:
        return {"nameKo": SPECIAL_KO_NAMES.get(display_en, display_en), "regionSlug": None, "areaSlug": None}

    region = regions_by_slug.get(region_slug)
    region_ko = region["nameKo"] if region else region_en
    area_slug_alias = AREA_SLUG_ALIASES.get((region_en, area_en))
    area = None
    if area_slug_alias:
        area = areas_by_region_and_name.get((region_slug, normalize_key(area_slug_alias)))
    if not area:
        area = areas_by_region_and_name.get((region_slug, normalize_key(area_en)))
    if area:
        return {
            "nameKo": area.name_ko,
            "regionKo": region_ko,
            "regionSlug": region_slug,
            "areaSlug": area.slug,
        }

    return {
        "nameKo": f"{region_ko} - {area_en}",
        "regionKo": region_ko,
        "regionSlug": region_slug,
        "areaSlug": None,
    }


def build_snapshot() -> dict[str, Any]:
    regions: list[dict[str, Any]] = []
    for region_slug in REGION_SLUGS:
        page_html = fetch_text(f"{TLIDB_BASE}/{urllib.parse.quote(region_slug)}")
        region_ko = extract_region_name(page_html, region_slug)
        areas = extract_areas(page_html, region_slug, region_ko)
        regions.append(
            {
                "slug": region_slug,
                "nameKo": region_ko,
                "areas": [
                    {"slug": area.slug, "nameKo": area.name_ko}
                    for area in areas
                ],
            }
        )

    regions_by_slug = {region["slug"]: region for region in regions}
    _, areas_by_region_and_name = build_area_indexes(regions)

    titrack_source = fetch_text(TITRACK_ZONES_URL)
    titrack = parse_titrack_constants(titrack_source)

    zones_by_internal_code: dict[str, Any] = {}
    unresolved: list[dict[str, str]] = []
    for internal_code, display_en in titrack["ZONE_NAMES"].items():
        enriched = enrich_display_name(display_en, regions_by_slug, areas_by_region_and_name)
        if enriched.get("nameKo") == display_en or (
            enriched.get("regionSlug") is not None and enriched.get("areaSlug") is None
        ):
            unresolved.append({"type": "zone", "key": internal_code, "displayEn": display_en})
        zones_by_internal_code[internal_code] = {
            "nameKo": enriched["nameKo"],
            "displayEn": display_en,
            "regionKo": enriched.get("regionKo"),
            "regionSlug": enriched.get("regionSlug"),
            "areaSlug": enriched.get("areaSlug"),
        }

    ambiguous_zones: dict[str, dict[str, Any]] = {}
    for internal_code, suffix_map in titrack["AMBIGUOUS_ZONES"].items():
        ambiguous_zones[internal_code] = {}
        for suffix, display_en in suffix_map.items():
            enriched = enrich_display_name(display_en, regions_by_slug, areas_by_region_and_name)
            ambiguous_zones[internal_code][str(suffix)] = {
                "nameKo": enriched["nameKo"],
                "displayEn": display_en,
                "regionKo": enriched.get("regionKo"),
                "regionSlug": enriched.get("regionSlug"),
                "areaSlug": enriched.get("areaSlug"),
            }

    zones_by_level_id: dict[str, Any] = {}
    for level_id, display_en in titrack["LEVEL_ID_ZONES"].items():
        enriched = enrich_display_name(display_en, regions_by_slug, areas_by_region_and_name)
        zones_by_level_id[str(level_id)] = {
            "nameKo": enriched["nameKo"],
            "displayEn": display_en,
            "regionKo": enriched.get("regionKo"),
            "regionSlug": enriched.get("regionSlug"),
            "areaSlug": enriched.get("areaSlug"),
        }

    return {
        "schemaVersion": 1,
        "generatedAt": datetime.now(timezone.utc).isoformat(),
        "language": "ko",
        "sources": {
            "tlidb": f"{TLIDB_BASE}/",
            "titrackZones": TITRACK_ZONES_URL,
            "titrackLicense": "MIT",
        },
        "regions": regions,
        "zonesByInternalCode": zones_by_internal_code,
        "ambiguousZonesByInternalCode": ambiguous_zones,
        "zonesByLevelId": zones_by_level_id,
        "unresolved": unresolved,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--out",
        default="data/offline/maps.ko.json",
        help="Path to write the offline map snapshot.",
    )
    args = parser.parse_args()

    out_path = Path(args.out)
    snapshot = build_snapshot()
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(
        json.dumps(snapshot, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )

    print(f"Wrote {out_path}")
    print(f"regions={len(snapshot['regions'])}")
    print(f"zonesByInternalCode={len(snapshot['zonesByInternalCode'])}")
    print(f"zonesByLevelId={len(snapshot['zonesByLevelId'])}")
    print(f"unresolved={len(snapshot['unresolved'])}")
    if snapshot["unresolved"]:
        for item in snapshot["unresolved"][:10]:
            print(f"unresolved: {item['key']} -> {item['displayEn']}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
