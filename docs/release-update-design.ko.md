# 배포 및 자동 업데이트 설계

Torch Overlay는 GitHub의 최신 stable release를 자동 업데이트 경로로 사용한다.

```text
배포 위치: GitHub Releases
업데이트 방식: Tauri updater
업데이트 메타데이터: latest.json
업데이트 검증: Tauri 서명 검증
대상 플랫폼: Windows x64
업데이트 endpoint: https://github.com/bbtarzan12/torch-overlay/releases/latest/download/latest.json
```

## 정책

- 앱 시작 후 업데이트를 1회 확인한다.
- 새 버전이 있으면 자동 다운로드와 설치를 진행한다.
- 설치 후 앱을 재시작한다.
- 업데이트 실패는 트래커 사용을 막지 않는다.
- 로컬 테스트는 로컬 설치본으로 수행하고, 배포는 `main` 브랜치에서 수동 `Release` workflow로만 수행한다.

## Release 산출물

최신 stable release에는 다음 파일이 필요하다.

```text
Torch.Overlay_{version}_x64-setup.exe
Torch.Overlay_{version}_x64-setup.exe.sig
latest.json
```

`latest.json`은 Tauri updater가 읽는 메타데이터이며, Windows installer URL과 signature를 포함한다.

```json
{
  "version": "0.1.3",
  "notes": "Torch Overlay release 0.1.3.",
  "pub_date": "2026-05-04T00:00:00Z",
  "platforms": {
    "windows-x86_64-nsis": {
      "signature": "SIGNATURE_CONTENT",
      "url": "https://github.com/bbtarzan12/torch-overlay/releases/download/v0.1.3/Torch.Overlay_0.1.3_x64-setup.exe"
    },
    "windows-x86_64": {
      "signature": "SIGNATURE_CONTENT",
      "url": "https://github.com/bbtarzan12/torch-overlay/releases/download/v0.1.3/Torch.Overlay_0.1.3_x64-setup.exe"
    }
  }
}
```

## GitHub Actions 흐름

```text
1. 로컬 QA 완료
2. version을 stable semver로 올림
3. main 브랜치에 커밋/푸시
4. GitHub Actions `Release` workflow 수동 실행
5. workflow에서 verify, signed build, latest.json 생성
6. `v{version}` GitHub Release에 산출물 업로드
```

`main`에 push해도 자동 배포하지 않는다. 실제 배포는 사용자가 QA 후 `workflow_dispatch`로 명시 실행한다.

## 서명 키

Tauri updater는 서명 검증을 통과해야 설치된다.

```text
공개키: src-tauri/tauri.conf.json
개인키: GitHub Actions secret TAURI_SIGNING_PRIVATE_KEY
개인키 비밀번호: GitHub Actions secret TAURI_SIGNING_PRIVATE_KEY_PASSWORD
```

개인키를 잃어버리면 기존 설치 유저에게 정상 업데이트를 배포하기 어렵다. GitHub secret 외 오프라인 백업을 유지한다.
