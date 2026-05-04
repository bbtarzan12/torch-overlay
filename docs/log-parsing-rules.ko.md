# Torchlight Infinite 로그 파싱 규칙

이 문서는 `UE_game.log` 기반 인게임 결정 트래커 구현에 필요한 맵/난이도 식별 규칙을 정리한다.

## 로그 위치

기본 확인 경로:

```text
D:\SteamLibrary\steamapps\common\Torchlight Infinite\UE_game\TorchLight\Saved\Logs\UE_game.log
```

## 주요 로그 신호

맵 진입 시 다음 로그를 조합한다.

```text
MysteryItemMgr@GetBeaconNumByAreaId  AreaId == ... AreaLv = ...
MysteryAreaModel@UpdateMysteryMapDataList AreaId == ... AreaLv == ...
MysteryCardMgr@GetCardDataByMapOrder AreaId = ... MapOrder ...
LevelMgr@ LevelUid, LevelType, LevelId = ...
Loading@ BeginLoadingScreen MapName = ...
Loading@ OnLoadingComplete notify Lua! LoadMapName = ...
SceneLevelMgr@ OpenMainWorld END! InMainLevelPath = ...
```

파싱 우선순위:

1. 난이도는 `AreaLv`를 1순위로 사용한다.
2. `AreaLv`가 없으면 `LevelUid` prefix를 보조 신호로 사용한다.
3. `MapOrder`는 난이도가 아니므로 난이도 판정에 사용하지 않는다.
4. 맵명은 `MapName` 또는 `LoadMapName`의 마지막 경로 조각에서 추출한다.

## 난이도 매핑

현재 확인된 구현 기준 매핑은 다음과 같다.

```text
AreaLv 4  -> 5단계
AreaLv 5  -> 6단계
AreaLv 6  -> 7-0
AreaLv 7  -> 7-1
AreaLv 8  -> 7-2
AreaLv 9  -> 8-0
AreaLv 10 -> 8-1
AreaLv 11 -> 8-2
AreaLv 12 -> 아득한 8단계
AreaLv 13 -> 딥 스페이스
```

직접 로그로 확인된 값:

```text
AreaLv 4  -> 5단계
AreaLv 6  -> 7-0
AreaLv 7  -> 7-1
AreaLv 8  -> 7-2
AreaLv 9  -> 8-0
AreaLv 12 -> 아득한 8단계
AreaLv 13 -> 딥 스페이스
```

구조상 확정으로 보는 값:

```text
AreaLv 5  -> 6단계
AreaLv 10 -> 8-1
AreaLv 11 -> 8-2
```

## LevelUid 보조 규칙

이계 맵의 `LevelUid`는 `10{AreaLv}...` 형태로 관찰된다.

```text
104.... -> AreaLv 4  -> 5단계
106.... -> AreaLv 6  -> 7-0
107.... -> AreaLv 7  -> 7-1
108.... -> AreaLv 8  -> 7-2
109.... -> AreaLv 9  -> 8-0
112.... -> AreaLv 12 -> 아득한 8단계
113.... -> AreaLv 13 -> 딥 스페이스
```

`AreaLv`가 같은 구간에 존재하면 `LevelUid`보다 `AreaLv`를 우선한다.

## 맵명 매핑

`MapName`에서 마지막 경로 조각을 맵 코드로 사용한다.

```text
/Game/Art/Maps/01SD/SD_ZhongXiGaoQiang200/SD_ZhongXiGaoQiang200
```

위 예시는 `SD_ZhongXiGaoQiang200`을 추출한 뒤, 끝의 숫자 suffix를 제거해 `SD_ZhongXiGaoQiang`으로 정규화한다.

정규화된 코드를 `data/offline/maps.ko.json`의 `zonesByInternalCode`에 매칭한다.

```text
SD_ZhongXiGaoQiang200 -> SD_ZhongXiGaoQiang -> 종식의 벽
DD_TanXiZhiQiang000   -> DD_TanXiZhiQiang   -> 슬픈 가락의 장벽
YL_BeiFengLinDi201    -> YL_BeiFengLinDi    -> 비극의 숲
SQ_BianChuiZhiDi200   -> SQ_BianChuiZhiDi   -> 황야의 들판
JH_MengZhongShengDi000 -> JH_MengZhongShengDi -> 잔잔한 빛의 강당
SD_DaHuangZhiYe200    -> SD_DaHuangZhiYe    -> 끝없는 광야
```

## 딥 스페이스 예외

딥 스페이스는 `MapName` 경로에 `Deep_Space`가 들어가지 않는다.

관찰된 딥 스페이스 샘플:

```text
AreaId = 2000
AreaLv = 13
LevelUid = 1132004
MapName = /Game/Art/Maps/01SD/SD_DaHuangZhiYe200/SD_DaHuangZhiYe200
```

따라서 딥 스페이스 판정은 다음 신호를 우선한다.

```text
AreaId == 2000
AreaLv == 13
LevelUid prefix == 113
maps.ko.json 매칭 결과 regionSlug == Deep_Space
```

`MapName contains Deep_Space` 방식은 사용하면 안 된다.

## 결정 수량 파싱

트래커가 집계할 대상은 `최초의 불꽃 결정`이다.

```text
최초의 불꽃 결정: ConfigBaseId 100300
최초의 불꽃 가루: ConfigBaseId 100200
```

`100200`은 결정이 아니라 가루이므로 결정/h 계산에 포함하지 않는다.

결정 수량 변화는 다음 로그에서 확인한다.

```text
ItemChange@ Update Id=100300_... BagNum=N in PageId=102 SlotId=0
BagMgr@:Modfy BagItem PageId = 102 SlotId = 0 ConfigBaseId = 100300 Num = N
```

`BagNum`/`Num`은 이번 획득량이 아니라 현재 보유 수량이다. 따라서 같은 캐릭터/세션 내 이전 `100300` 수량과의 차이(`delta`)를 실제 변화량으로 계산한다.

집계 규칙:

1. 현재 컨텍스트가 이계 런이고 `ProtoName=PickItems`이며 `delta > 0`인 `100300`만 런 수익에 더한다.
2. `LevelUid=111000`, `LevelType=0`, `+checkType [MainCity]`, 또는 마을 `LoadMapName` 상태에서 발생한 `100300` 변화는 런 수익에 더하지 않는다.
3. `ResetItemsLayout`은 인벤토리 재배치/초기화 신호이므로 수익으로 처리하지 않는다.
4. `XchgBuy`는 거래소 구매로 인한 결정 감소다.
5. `XchgReceive`는 거래소 정산/수령으로 인한 결정 증가다.
6. `Push2`는 제작, 메모리 카드, 보상 수령 등 UI/시스템 액션에서도 발생하므로 현재 위치가 마을이면 런 수익에서 제외한다.

현재 확인한 로그에서 거래소 관련 변화는 다음 형태다.

```text
ItemChange@ ProtoName=XchgBuy start
ItemChange@ Update Id=100300_... BagNum=131 in PageId=102 SlotId=0
BagMgr@:Modfy BagItem PageId = 102 SlotId = 0 ConfigBaseId = 100300 Num = 131
ItemChange@ ProtoName=XchgBuy end

ItemChange@ ProtoName=XchgReceive start
ItemChange@ Update Id=100300_... BagNum=102 in PageId=102 SlotId=0
BagMgr@:Modfy BagItem PageId = 102 SlotId = 0 ConfigBaseId = 100300 Num = 102
ItemChange@ ProtoName=XchgReceive end
```

거래소, 제작, 창고, 메모리 카드, 로그인 직후 인벤토리 동기화는 모두 마을 컨텍스트에서 발생할 수 있다. 따라서 단순히 `100300`의 양수 delta를 모두 더하면 트래커가 과대 집계된다. 반드시 런 상태와 `PickItems`를 함께 확인한다.

## 결정 소비 파싱

소비는 `100300`의 `delta < 0`으로 알 수 있다. 단, 모든 음수 delta가 실제 소비는 아니다. 로그인, 캐릭터 전환, `Reset PageId=102`, `Spv3Open`, `ResetItemsLayout`처럼 인벤토리를 다시 구성하는 구간은 기준 수량만 갱신하고 소비로 기록하지 않는다.

소비로 인정하는 대표 케이스:

```text
ProtoName=XchgBuy
```

거래소 구매다. 현재 로그에서 확인된 `XchgBuy` 소비는 `3 + 7 + 20 + 22 + 20 = 72` 결정이다.

```text
ProtoName=Push2
nearby UI = ForgeConsole_ForgeBtn / S8Forge / ChooseForgeItem
```

제작 또는 옵션 변경이다. `Push2` 자체는 범용 push이므로 단독으로는 원인을 확정하지 않는다. 주변 UI 로그에 `ForgeConsole_ForgeBtn`, `S8Forge`, `ForgeAffixBar`, `ChooseForgeItem` 등이 있을 때 제작 소비로 분류한다. 현재 로그에서 확인된 제작 소비는 `41` 결정이다.

소비로 처리하지 않는 대표 케이스:

```text
ProtoName=Spv3Open
LoginScene / LoadingCtrl 근처
Reset PageId=102 이후 대량 Add/Update
```

현재 로그에는 `Spv3Open`에서 `347 -> 17`로 보이는 `-330` 변화가 있다. 이 구간은 `LoginScene`/`LoadingCtrl` 근처의 인벤토리 재동기화이므로 소비가 아니라 baseline 재설정으로 처리한다.

구현상 권장 상태는 다음과 같다.

```text
gross_drop  = 런 중 PickItems 양수 delta
town_income = XchgReceive 등 마을 양수 delta
spend_trade = XchgBuy 음수 delta
spend_craft = Forge UI 근처 Push2 음수 delta
ignored_sync = Reset/Spv3Open/LoginScene 동기화 delta
```

런 효율 UI의 기본 결정/h는 `gross_drop`만 사용한다. 소비를 보여줄 경우에는 별도 상세 패널에서 `거래소 구매`, `제작 소비`, `마을 수령`처럼 분리한다.

## 시세 로그 파싱

아이템 시세는 거래소 가격조회 로그에서 일부 확인할 수 있다.

```text
TLNetGame: ----Socket SendMessage STT----XchgSearchPrice----SynId = 57951
+typ3 [63]
+filters+1+key [4]
|       | +refer [5080]
TLNetGame: ----Socket SendMessage End----

TLNetGame: ----Socket RecvMessage STT----XchgSearchPrice----SynId = 57951
+prices+1+unitPrices+1 [0.019957631843015]
|      | |          +2 [0.02000200020002]
|      | +currency [100300]
+errCode
+itemGoldId [5080]
TLNetGame: ----Socket RecvMessage End----
```

파싱 규칙:

1. `SynId`로 `SendMessage` 요청과 `RecvMessage` 응답을 매칭한다.
2. 요청의 `typ3`는 거래소 카테고리다.
3. 단순 재료/소모품은 보통 `filters ... key [4]`의 `refer`가 아이템 `ConfigBaseId`다.
4. 장비/레전드/고유 아이템 가격조회는 `key [16]` 또는 `key [24]`와 `refer`, `itemGoldId`, 옵션 필터가 함께 쓰인다.
5. 응답의 `unitPrices`는 거래소 검색 결과의 가격 샘플 배열이다.
6. `currency [100300]`이면 결정 기준 가격이다.
7. `currency [100200]`이면 가루 기준 가격이므로 결정 가치로 환산하려면 별도 환율이 필요하다.

주의할 점:

```text
XchgSearchPrice는 드랍 시 자동으로 생성되는 로그가 아니다.
TradeQueryButton, TipSearchPriceItem, AuctionHouse 검색 등 유저가 가격조회를 실행했을 때 생성된다.
```

따라서 로그만으로 모든 드랍 아이템의 실시간 시세를 자동 계산할 수는 없다. 구현은 다음 중 하나를 선택해야 한다.

```text
1. 로그에서 관찰된 XchgSearchPrice 결과를 로컬 시세 캐시로 저장한다.
2. tlidb/거래소성 외부 데이터에서 기본 시세 테이블을 별도로 구축한다.
3. 가격 미확인 아이템은 결정 수익에 포함하지 않고, UI에서 "미평가 아이템"으로 분리한다.
```

초기 버전에서는 결정 드랍(`100300`)은 확정 수익으로 계산하고, 다른 아이템은 `시세 캐시가 있는 경우에만 추정 결정 가치`로 더하는 방식이 안전하다.

## 구현 결론

결정 트래커의 상단 바에는 다음 형태로 표시한다.

```text
{맵명} {난이도} {시간} | 현재 런 +N 결정 | 결정/h X/h | 세션 Y 결정 · HH:MM:SS · 평균 Z/h | 초기화
```

난이도와 맵명은 별도 테이블로 관리한다.

```text
data/offline/difficulty.ko.json
data/offline/maps.ko.json
```
