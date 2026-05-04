# 배포 및 자동 업데이트 설계

이 문서는 Torch Overlay를 GitHub로 배포하고, 설치된 앱이 자동으로 새 버전을 감지해 업데이트하는 방식을 정리한다.

## 목표

유저가 매번 새 버전을 직접 찾아 설치하지 않아도 되게 한다.

```text
배포 위치: GitHub Releases
업데이트 방식: Tauri updater
업데이트 메타데이터: latest.json
업데이트 검증: Tauri 서명 검증
대상 플랫폼: Windows 우선
```

초기 버전은 Windows x64만 공식 지원한다. Linux/macOS는 필요할 때 빌드 산출물과 updater platform key를 추가한다.

## 기본 정책

- 앱 시작 후 일정 시간 뒤 업데이트를 확인한다.
- 게임 중 방해를 줄이기 위해 자동 설치는 하지 않는다.
- 새 버전이 있으면 bar 또는 상세 패널에 작게 표시한다.
- 유저가 `업데이트`를 누르면 다운로드와 설치를 진행한다.
- 설치 후 재시작이 필요하면 명확히 표시한다.
- 업데이트 실패는 트래커 사용을 막지 않는다.

오버레이 앱은 게임 위에 떠 있으므로 업데이트 UI도 작아야 한다. 강제 팝업이나 큰 모달은 사용하지 않는다.

## UX 반영

상단 bar에는 평소 업데이트 정보를 숨긴다. 새 버전이 있을 때만 우측 제어 영역에 작은 버튼을 추가한다.

```text
업데이트 있음: 업데이트
다운로드 중: 42%
설치 준비됨: 재시작
최신 상태: 표시 안 함
오류 상태: 상세 패널에만 표시
```

상세 패널의 설정 영역에는 다음 항목을 둔다.

```text
현재 버전
최신 버전
릴리즈 노트 요약
업데이트 확인 버튼
자동 확인 on/off
마지막 확인 시각
```

초기 구현에서는 릴리즈 노트를 길게 보여주지 않는다. GitHub Release의 주요 변경점 몇 줄만 표시하고, 필요하면 브라우저로 릴리즈 페이지를 열 수 있게 한다.

## 업데이트 확인 시점

자동 확인은 다음 조건에서만 실행한다.

```text
앱 시작 후 10초 뒤 1회
마지막 확인 후 6시간 이상 지난 경우
유저가 수동으로 업데이트 확인을 누른 경우
```

런 진행 중이어도 확인 요청 자체는 가능하다. 다만 다운로드와 설치는 유저가 명시적으로 누를 때만 실행한다.

## Tauri updater 구성

Tauri 2의 updater plugin을 사용한다.

`tauri.conf.json` 기준 구성:

```json
{
  "bundle": {
    "createUpdaterArtifacts": true
  },
  "plugins": {
    "updater": {
      "pubkey": "PUBLIC_KEY_CONTENT",
      "endpoints": [
        "https://github.com/OWNER/REPO/releases/latest/download/latest.json"
      ],
      "windows": {
        "installMode": "passive"
      }
    }
  }
}
```

`PUBLIC_KEY_CONTENT`는 파일 경로가 아니라 공개키 문자열 자체를 넣는다. 개인키는 절대 저장소에 커밋하지 않는다.

Windows는 `passive` 설치를 기본값으로 사용한다. 설치 진행 상황은 표시하되, 불필요한 설치 마법사 UI를 줄이기 위함이다.

## GitHub Releases 산출물

릴리즈에는 최소 다음 파일이 올라가야 한다.

```text
latest.json
Torch Overlay_x.y.z_x64-setup.exe
Torch Overlay_x.y.z_x64-setup.exe.sig
```

`latest.json`에는 Windows x64 플랫폼의 다운로드 URL과 서명이 포함된다.

예시:

```json
{
  "version": "0.1.0",
  "notes": "초기 배포",
  "pub_date": "2026-05-04T00:00:00Z",
  "platforms": {
    "windows-x86_64": {
      "signature": "SIGNATURE_CONTENT",
      "url": "https://github.com/OWNER/REPO/releases/download/v0.1.0/TLI.Tracker_0.1.0_x64-setup.exe"
    }
  }
}
```

파일명은 최종 앱 이름이 정해지면 고정한다. URL과 `latest.json`의 파일명 규칙이 CI에서 일관되게 생성되어야 한다.

## GitHub Actions 배포 흐름

릴리즈는 태그 push로 만든다.

```text
1. 버전 수정: package.json, Cargo.toml, tauri.conf.json
2. 태그 생성: v0.1.0
3. GitHub Actions에서 Windows 빌드
4. Tauri updater artifact와 signature 생성
5. GitHub Release 생성 또는 갱신
6. latest.json 업로드
```

권장 태그 규칙:

```text
stable: v0.1.0
pre-release: v0.1.0-beta.1
```

초기에는 stable 채널만 사용한다. 테스트 배포가 필요해지면 `latest-beta.json`을 추가하고, 앱 설정에서 베타 업데이트를 선택한 유저만 beta endpoint를 보게 한다.

## 서명 키 관리

업데이트는 서명 검증을 통과해야 설치된다.

```text
공개키: tauri.conf.json에 포함 가능
개인키: GitHub Actions secret에 저장
개인키 비밀번호: GitHub Actions secret에 저장
```

권장 secret 이름:

```text
TAURI_SIGNING_PRIVATE_KEY
TAURI_SIGNING_PRIVATE_KEY_PASSWORD
```

개인키를 잃어버리면 기존 설치 유저에게 정상 업데이트를 배포하기 어렵다. 따라서 개인키는 GitHub secret 외에도 오프라인 백업을 하나 둔다.

## 앱 설정 저장

업데이트 설정은 SQLite `settings` 또는 별도 json에 저장한다. DB를 이미 사용하므로 초기 구현은 SQLite를 권장한다.

```sql
CREATE TABLE app_settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
```

업데이트 관련 key:

```text
updates.auto_check_enabled = true
updates.last_checked_at = 2026-05-04T00:00:00Z
updates.channel = stable
updates.skipped_version = 0.0.0
```

`skipped_version`은 유저가 특정 버전을 건너뛰는 기능을 넣을 때 사용한다. 초기 구현에서는 UI를 숨겨도 된다.

## 상태 모델

frontend는 updater 상태를 단순하게 받는다.

```text
idle
checking
available
not_available
downloading
ready_to_install
installing
error
```

상태별 UI:

```text
checking: 상세 패널에만 표시
available: bar에 업데이트 버튼 표시
downloading: bar에 진행률 표시
ready_to_install: bar에 재시작 버튼 표시
error: bar에는 표시하지 않고 상세 패널에만 표시
```

## DB 마이그레이션과 업데이트

앱 업데이트와 DB 마이그레이션은 분리해서 생각한다.

- 앱 바이너리 업데이트 후 첫 실행에서 DB schema version을 확인한다.
- 마이그레이션은 순차적으로 실행한다.
- 실패하면 앱을 종료하지 말고 오류 상태를 표시한다.
- 원본 로그와 run/loot 이벤트 테이블은 가능한 한 destructive migration을 피한다.

권장 테이블:

```sql
CREATE TABLE schema_migrations (
  version INTEGER PRIMARY KEY,
  applied_at TEXT NOT NULL
);
```

가격 캐시나 파생 캐시는 깨져도 다시 계산할 수 있다. 반대로 `runs`, `loot_events`, `inventory_events`, `price_observations` 같은 원본 데이터는 마이그레이션에서 보수적으로 다룬다.

## 실패 처리

업데이트 실패는 치명 오류로 취급하지 않는다.

```text
네트워크 실패: 다음 자동 확인 때 재시도
latest.json 오류: 상세 패널에 오류 표시
서명 검증 실패: 설치 중단, 보안 오류로 표시
다운로드 실패: 진행률 초기화, 재시도 가능
설치 실패: 앱 재시작 후 다시 확인 가능
```

서명 검증 실패는 일반 네트워크 오류와 다르게 표시한다. 이 경우 사용자가 임의 파일을 설치하지 않도록 유도하지 않는다.

## 구현 우선순위

초기 구현:

```text
1. GitHub Releases stable 배포
2. Tauri updater 설정
3. 시작 후 자동 확인
4. 수동 업데이트 버튼
5. 다운로드 진행률 표시
6. 설치 후 재시작 안내
```

후순위:

```text
beta 채널
특정 버전 건너뛰기
릴리즈 노트 상세 보기
업데이트 실패 진단 로그 내보내기
```

초기 목표는 유저가 GitHub 페이지를 직접 다시 방문하지 않아도 안정적으로 최신 버전을 받을 수 있게 하는 것이다.
