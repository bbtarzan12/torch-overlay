# 오버레이 입력 처리 규칙

## 목표

트래커는 게임 화면 위에 떠 있지만 게임 조작을 방해하면 안 된다.

- 고정 상태에서는 클릭 가능한 UI만 입력을 받는다.
- 고정 상태에서 그 외 bar, 상세 패널 배경, 표/그래프 영역은 게임으로 클릭이 통과해야 한다.
- 고정 해제 상태에서는 bar를 드래그해서 위치를 옮길 수 있다.

## 상태별 동작

### 고정 상태

- pin 버튼은 클릭 가능하다.
- 투명도 슬라이더는 클릭/드래그 가능하다.
- 초기화 버튼은 클릭 가능하다.
- 상세 버튼은 클릭 가능하다.
- 그래프 토글 버튼은 클릭 가능하다.
- 위 항목을 제외한 영역은 click-through 처리한다.
- bar 자체는 드래그되지 않는다.

### 고정 해제 상태

- 전체 오버레이 입력을 받는다.
- pin, 투명도, 초기화, 상세, 그래프 토글은 그대로 클릭 가능하다.
- 버튼/슬라이더가 아닌 bar 영역은 `data-tauri-drag-region`으로 지정해서 창 이동에 사용한다.
- 다시 pin을 켜면 drag region을 제거하고 click-through 정책으로 돌아간다.

## 구현 기준

CSS `pointer-events: none`만으로는 충분하지 않다. WebView가 OS 레벨에서 마우스 입력을 먼저 받을 수 있으므로 실제 앱에서는 Rust 쪽에서 native click-through를 제어한다.

권장 구현은 다음과 같다.

1. frontend는 클릭 가능한 DOM 요소들의 `getBoundingClientRect()`를 Rust로 전달한다.
2. Rust는 현재 마우스 위치가 clickable rect 안에 있는지 판단한다.
3. 고정 상태에서 마우스가 clickable rect 밖이면 window click-through를 켠다.
4. 고정 상태에서 마우스가 clickable rect 안이면 window click-through를 끈다.
5. 고정 해제 상태에서는 window click-through를 끄고 drag region을 활성화한다.

Tauri API의 `setIgnoreCursorEvents(true)`는 창 전체를 무시하므로, 이 기능만 단독으로 쓰면 버튼도 클릭할 수 없어진다. 따라서 clickable rect 기반으로 켜고 끄는 레이어가 필요하다.

## Mockup 반영

mockup에서는 의도를 표현하기 위해 다음 CSS를 사용한다.

- `.tracker-shell[data-position-locked="true"] { pointer-events: none; }`
- 실제 클릭 가능한 버튼/슬라이더만 `pointer-events: auto`
- `.tracker-shell[data-position-locked="false"] { pointer-events: auto; }`

이는 브라우저 mockup용 표현이고, 실제 앱에서는 Rust native click-through 처리가 최종 기준이다.
