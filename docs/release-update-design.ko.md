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
고정 채널: beta
```

초기 버전은 Windows x64만 공식 지원한다. Linux/macOS는 필요할 때 빌드 산출물과 updater platform key를 추가한다.

## 기본 정책

- 앱 시작 후 업데이트를 1회 확인한다.
- 새 버전이 있으면 자동으로 다운로드와 설치를 진행한다.
- 설치 후 앱을 재시작한다.
- 업데이트 실패는 트래커 사용을 막지 않는다.

오버레이 앱은 게임 위에 떠 있으므로 업데이트 UI는 bar 안의 작은 상태 표시만 사용한다. 강제 팝업이나 큰 모달은 사용하지 않는다.

## UX 반영

상단 bar에는 평소 업데이트 정보를 숨긴다. 업데이트가 진행 중이거나 오류가 있을 때만 우측 제어 영역에 작은 상태를 표시한다.

```text
다운로드 중: 42%
설치 중: 설치 중
최신 상태: 표시 안 함
오류 상태: bar에 짧게 표시
```

## 업데이트 확인 시점

자동 확인은 앱 시작 시 1회 실행한다.

```text
앱 시작
Tauri updater check
update가 있으면 downloadAndInstall
relaunch
```

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
        "https://github.com/bbtarzan12/torch-overlay/releases/download/beta/latest.json"
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

## GitHub Beta Release 산출물

고정 `beta` release에는 최소 다음 파일이 올라가야 한다.

```text
Torch.Overlay_x.y.z_x64-setup.exe
Torch.Overlay_x.y.z_x64-setup.exe.sig
latest.json
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
      "url": "https://github.com/bbtarzan12/torch-overlay/releases/download/beta/Torch%20Overlay_0.1.2-beta.1_x64-setup.exe"
    }
  }
}
```

파일명은 최종 앱 이름이 정해지면 고정한다. URL과 `latest.json`의 파일명 규칙이 CI에서 일관되게 생성되어야 한다.

## GitHub Actions 배포 흐름

릴리즈는 수동 `Beta` workflow로만 갱신한다.

```text
1. GitHub Actions `Beta` workflow 수동 실행
2. workflow에서 `0.1.2-beta.{run_number}` 형식의 버전 생성
3. Windows installer와 updater signature 생성
4. latest.json 생성
5. `beta` tag를 현재 커밋으로 이동
6. GitHub Release `beta`에 산출물 덮어쓰기
```

초기에는 stable/beta 채널을 나누지 않는다. 앱의 updater endpoint는 `beta` release 하나로 고정한다.

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
updates.last_checked_at = 2026-05-04T00:00:00Z
```

업데이트 채널 선택, 특정 버전 건너뛰기, 자동 확인 on/off는 초기 구현에서 제외한다.

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
1. GitHub Releases beta 배포
2. Tauri updater 설정
3. 시작 후 자동 확인
4. 자동 다운로드와 설치
5. 설치 후 재시작
```

후순위:

```text
stable 채널 분리
특정 버전 건너뛰기
릴리즈 노트 상세 보기
업데이트 실패 진단 로그 내보내기
```

초기 목표는 유저가 GitHub 페이지를 직접 다시 방문하지 않아도 안정적으로 최신 버전을 받을 수 있게 하는 것이다.
