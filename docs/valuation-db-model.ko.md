# 가치 평가 DB 모델

이 문서는 결정 트래커가 `최초의 불꽃 결정` 직접 드랍뿐 아니라 다른 아이템의 추정 결정 가치까지 계산하기 위한 DB 설계를 정리한다.

## 목표

유저가 인게임에서 가격조회를 누르면 `XchgSearchPrice` 로그를 파싱해 시세를 캐싱한다. 이후 같은 아이템 또는 같은 거래소 검색 키에 매칭되는 과거 드랍 아이템의 가치도 현재 캐시 가격으로 다시 평가한다.

핵심 원칙:

```text
원본 이벤트는 불변으로 저장한다.
시세 관측값은 시간순으로 누적 저장한다.
런 가치 합계는 저장 원본이 아니라 파생값으로 계산한다.
```

## 저장소 선택

초기 구현은 SQLite를 사용한다.

```text
원본 로그 보존: SQLite row + 필요 시 raw line 일부
조회/집계: SQLite index
런 가치 재계산: SQL view 또는 materialized cache
앱 설정: SQLite settings 또는 별도 json
```

JSONL은 append-only 이벤트 보관에는 좋지만, 가격 캐시가 갱신될 때 과거 런 전체를 재평가하기 어렵다. 이 기능을 넣는 순간 SQLite가 더 적합하다.

## 핵심 테이블

### runs

런 단위 원본 정보다.

```sql
CREATE TABLE runs (
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
```

### loot_events

런 중 획득한 아이템 원본 이벤트다. 결정 직접 드랍도 여기에 저장한다.

```sql
CREATE TABLE loot_events (
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

CREATE INDEX idx_loot_events_run ON loot_events(run_id);
CREATE INDEX idx_loot_events_base ON loot_events(config_base_id);
CREATE INDEX idx_loot_events_market_key ON loot_events(market_key_id);
```

`quantity`는 `BagNum` 자체가 아니라 이전 수량과 비교해 계산한 delta다.

### inventory_events

런 수익과 별개로 마을 수령, 거래소 구매, 제작 소비 같은 전체 결정 변화를 기록한다.

```sql
CREATE TABLE inventory_events (
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

CREATE INDEX idx_inventory_events_base ON inventory_events(config_base_id);
CREATE INDEX idx_inventory_events_reason ON inventory_events(reason);
```

예시 `reason`:

```text
run_drop
town_income_xchg_receive
spend_xchg_buy
spend_craft
ignored_sync
```

### market_keys

시세 캐시의 기준 키다. 같은 아이템이라도 장비 옵션/레벨/검색 필터가 다르면 다른 키로 취급한다.

```sql
CREATE TABLE market_keys (
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

CREATE INDEX idx_market_keys_base ON market_keys(config_base_id);
```

`canonical_json`은 `XchgSearchPrice` 요청 필터를 정규화한 JSON이다. 정렬 가능한 구조로 만들어야 같은 검색 조건이 항상 같은 `key_hash`가 된다.

권장 key 타입:

```text
currency        100300 같은 고정 화폐
stackable       재료, 카드, 화석, 연료 등 수량형 아이템
unique_item     고유 장비/레전드 장비의 base/gold id 기준
gear_query      옵션 필터까지 포함한 장비 검색 조건
unknown         아직 분류 안 됨
```

### price_observations

유저가 가격조회/거래소 검색을 했을 때 얻은 원본 시세 관측값이다.

```sql
CREATE TABLE price_observations (
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

CREATE INDEX idx_price_observations_key_time
  ON price_observations(market_key_id, observed_at DESC);
```

`currency_base_id=100300`이면 결정 기준 가격이다. `100200`이면 가루 기준 가격이므로 결정 환산율이 없으면 수익 계산에 쓰지 않는다.

### price_estimates

현재 UI에서 사용할 최신 평가 가격이다. `price_observations`에서 파생되지만, 빠른 조회를 위해 별도 캐시로 둔다.

```sql
CREATE TABLE price_estimates (
  market_key_id INTEGER PRIMARY KEY REFERENCES market_keys(id),
  price_in_crystal REAL NOT NULL,
  source_observation_id INTEGER REFERENCES price_observations(id),
  confidence TEXT NOT NULL,
  observed_at TEXT NOT NULL,
  expires_at TEXT,
  estimator_version INTEGER NOT NULL
);
```

권장 `confidence`:

```text
fixed       100300 결정 자체
fresh       최근 직접 가격조회
stale       오래된 가격조회
converted   100200 등 다른 화폐에서 환산
manual      유저 수동 입력
unknown     평가 불가
```

### run_value_cache

런 가치 합계를 빠르게 보여주기 위한 캐시다. 원본 진실이 아니라 언제든 재생성 가능한 파생 데이터로 취급한다.

```sql
CREATE TABLE run_value_cache (
  run_id INTEGER PRIMARY KEY REFERENCES runs(id),
  direct_crystal REAL NOT NULL DEFAULT 0,
  estimated_item_value REAL NOT NULL DEFAULT 0,
  total_estimated_value REAL NOT NULL DEFAULT 0,
  unpriced_item_count INTEGER NOT NULL DEFAULT 0,
  valuation_version INTEGER NOT NULL,
  calculated_at TEXT NOT NULL
);
```

가격 캐시가 갱신되면 관련 `market_key_id`를 가진 모든 `loot_events`의 런을 찾아 `run_value_cache`를 무효화하거나 재계산한다.

## 가격조회 적용 흐름

1. `XchgSearchPrice SendMessage`를 `SynId` 기준으로 임시 저장한다.
2. 같은 `SynId`의 `RecvMessage`를 받으면 요청 필터와 응답 가격을 결합한다.
3. 요청 필터를 정규화해 `market_keys.key_hash`를 만든다.
4. `price_observations`에 원본 가격 샘플을 추가한다.
5. `selected_price`를 계산해 `price_estimates`를 갱신한다.
6. 같은 `market_key_id`를 가진 과거 `loot_events`의 런 캐시를 무효화한다.
7. UI는 다음 렌더 시 최신 `price_estimates` 기준으로 과거 런 가치를 다시 보여준다.

## 평가 계산

결정 직접 드랍:

```text
config_base_id == 100300
value = quantity
confidence = fixed
```

가격 캐시가 있는 아이템:

```text
value = quantity * price_estimates.price_in_crystal
confidence = price_estimates.confidence
```

가격 캐시가 없는 아이템:

```text
value = 0
unpriced_item_count += 1
```

## 과거 런 재평가 방식

과거 런의 가치를 “드랍 당시 가격”으로 고정하지 않는다. 기본 UI는 항상 최신 캐시 기준 `현재 평가액`을 보여준다.

필요하면 나중에 다음 값을 추가로 제공한다.

```text
value_at_drop_time   드랍 당시 알고 있던 가격
value_now            현재 캐시 기준 가격
value_delta          가격 변동에 따른 차이
```

초기 버전에서는 `value_now`만 구현한다.

## 장비 가격 처리

장비는 단순 `config_base_id`만으로 평가하면 오차가 크다. 같은 베이스라도 레벨, 희귀도, 고유 옵션, affix 필터에 따라 가격이 크게 달라진다.

따라서 장비는 보수적으로 처리한다.

```text
1. 유저가 해당 장비 또는 동일 검색 조건으로 가격조회한 경우에만 market_key를 부여한다.
2. 옵션 필터가 없는 일반 base 가격은 참고 가격으로만 사용한다.
3. exact gear query가 없는 장비는 기본적으로 미평가 아이템으로 둔다.
```

재료/소모품은 `config_base_id` 기반 캐싱을 우선 적용해도 된다.

## UI 반영

상단 바의 기본 결정/h는 확정 수익 중심으로 유지한다.

```text
런 +14 결정 · 추정 +6.2 · 총 +20.2
```

추정값을 넣을 경우 확정값과 분리해서 보여준다.

상세 패널에는 다음을 보여줄 수 있다.

```text
확정 결정
추정 아이템 가치
미평가 아이템 수
최근 가격조회 시각
가격 캐시 신뢰도
```

이렇게 해야 유저가 “실제로 주운 결정”과 “시세 기반 추정 수익”을 혼동하지 않는다.
