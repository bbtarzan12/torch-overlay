# 오프라인 아이템 데이터

아이템 이름 매핑은 런타임 DB가 아니라 `data/offline/items.ko.json` 정적 스냅샷으로 관리한다.

## 원칙

- 게임 로그의 원본 키는 `ConfigBaseId`다.
- 유저 DB에는 획득 이벤트와 가격 캐시만 저장한다.
- 아이템 이름은 표시 시점에 오프라인 매핑을 조회한다.
- 매핑 파일 갱신 실패가 유저 DB를 변경하거나 손상시키면 안 된다.

## 생성 방식

`TLI-tracker-translated`의 `resources/app.asar` 안에는 `Rt-data.json`이 있으며, 현재 공개 트래커가 사용하는 1811개 항목의 ID/타입/이미지/일부 TLIDB slug 후보가 들어 있다. 이 데이터는 중국어 이름이므로 표시 원본으로 쓰지 않는다.

생성 스크립트는 공개 트래커 데이터를 ID seed로 사용하고, 한국어 이름은 TLIDB의 autocomplete JSON에서 읽는다.

```text
https://tlidb.com/i18n/autocomplete_ko.json
https://tlidb.com/i18n/autocomplete_cn.json
```

매칭 방식:

```text
Rt-data.json name(중국어) -> autocomplete_cn.label -> autocomplete value -> autocomplete_ko.label
Rt-data.json url slug -> autocomplete_ko.value -> autocomplete_ko.label
```

TLIDB autocomplete와 목록 페이지에서 바로 연결되지 않는 항목은 `scripts/build_offline_items.py`의
`CURATED_KO_FALLBACKS`에 최소 보강 테이블로 둔다. 현재 보강 대상은 25개이며, 기본 생성 결과의
`missingKoreanNameCount`는 0이어야 한다.

```powershell
npm run data:items
```

빠른 검증은 동일한 방식으로 `artifacts/items.sample.ko.json`에 출력한다.

```powershell
npm run data:items:sample
```

## 출력 구조

```json
{
  "schemaVersion": 1,
  "language": "ko",
  "itemsByConfigBaseId": {
    "100300": {
      "nameKo": "최초의 불꽃 결정",
      "nameZh": "初火源质",
      "categoryKo": "연료",
      "categoryZh": "燃料",
      "slug": "Flame_Elementium"
    }
  }
}
```

## 캐시

크롤링 결과는 `.cache/tlidb-items`에 저장한다. 기본 생성은 GitHub의 공개 트래커 asar 1회와 TLIDB autocomplete JSON 2개만 받는다.

강제로 새로 받으려면 다음처럼 실행한다.

```powershell
python scripts/build_offline_items.py --use-public-tracker --refresh
```

상세 페이지 보강이 꼭 필요할 때만 `--resolve-details`를 사용한다.
