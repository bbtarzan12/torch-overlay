# 로컬 테스트 및 디버깅 파이프라인

이 문서는 Torch Overlay가 “빌드는 성공했지만 설치 후 빈 창만 뜨는” 문제를 다시 만들지 않기 위한 로컬 검증 절차를 정리한다.

## 원칙

검증은 4단계로 나눈다.

```text
1. 정적 검증: 타입, 포맷, Rust 테스트
2. 웹 smoke: dist를 브라우저에서 실제 실행하고 DOM/스크린샷 확인
3. Tauri dev: WebView 런타임에서 로그와 DevTools로 확인
4. Release 배포: 실제 GitHub updater 경로로 설치본 확인
```

`npm run build`만 통과해도 런타임 오류로 화면이 비어 있을 수 있다. 따라서 DOM이 실제로 렌더링됐는지 확인하는 smoke test를 필수로 둔다.

## 1단계: 정적 검증

```powershell
npm run verify
```

실행 내용:

```text
svelte-check
vite build
cargo fmt --check
cargo test
```

이 단계는 컴파일 오류와 parser 단위 테스트를 잡는다. 하지만 WebView에서 실제 화면이 뜨는지는 보장하지 않는다.

## 2단계: 웹 smoke

```powershell
npm run smoke:web
```

이 단계는 다음을 자동으로 수행한다.

```text
1. npm run build
2. vite preview 실행
3. Chrome 또는 Edge headless 실행
4. 실제 dist 페이지 로드
5. .tracker-bar 존재 여부 확인
6. bar 크기 확인
7. "결정" 텍스트 렌더링 확인
8. JS runtime exception 감지
9. 스크린샷 저장
```

결과 스크린샷:

```text
artifacts/smoke/web-smoke.png
```

이 단계에서 실패하면 설치본을 만들면 안 된다.

## 3단계: Tauri dev 디버깅

기본 실행:

```powershell
npm run debug:tauri
```

DevTools를 자동으로 열고 싶을 때:

```powershell
.\scripts\dev_tauri.ps1 -OpenDevTools
```

이 단계에서는 다음을 확인한다.

```text
WebView 콘솔 오류
Tauri invoke 오류
updater plugin permission 오류
투명창에서 UI가 실제로 보이는지
로그 파일 접근 오류
```

빈 창이 뜨면 DevTools Console에서 첫 오류를 확인한다. 앱 시작 중 fatal error가 발생하면 화면에 `Torch Overlay failed to start` 패널이 보여야 한다.

## 4단계: Release 배포 빌드

자동 업데이트 검증은 로컬 전용 updater가 아니라 GitHub Actions의 `Release` workflow에서 수행한다.

```text
endpoint = https://github.com/bbtarzan12/torch-overlay/releases/latest/download/latest.json
release tag = v{version}
```

이 경로는 앱 런타임에서 사용하는 유일한 자동 업데이트 경로다. `file://` updater, localhost updater 서버, 별도 update-test 앱은 사용하지 않는다.

기대 산출물:

```text
GitHub latest Release latest.json
GitHub latest Release Torch Overlay_{version}_x64-setup.exe
GitHub latest Release Torch Overlay_{version}_x64-setup.exe.sig
```

로컬에서는 `npm run verify`, `npm run smoke:web`, `npm run debug:tauri`, 로컬 설치본 확인까지 수행한다. 자동 업데이트 배포는 Release workflow 산출물로만 갱신한다.

## 설치본 확인

로컬 설치 전 확인 순서:

```text
1. npm run verify
2. npm run smoke:web
3. .\scripts\dev_tauri.ps1 -OpenDevTools
4. 로컬 설치본 빌드와 설치 확인
5. GitHub Actions `Release` workflow 실행
6. 새 버전 설치본에서 앱 시작 시 자동 업데이트 확인
```

설치본이 빈 창이면 다음 순서로 본다.

```text
1. 화면에 fatal error 패널이 있는지 확인
2. Tauri dev에서 같은 현상이 재현되는지 확인
3. npm run smoke:web 결과가 통과했는지 확인
4. updater/permission 문제인지 WebView 렌더링 문제인지 분리
```

## 이번 빈 창의 직접 원인

Svelte 5에서는 `new App({ target })` 방식이 더 이상 유효하지 않다. 이 방식은 빌드는 통과할 수 있지만 런타임에서 앱 mount가 실패해 빈 화면이 된다.

수정 기준:

```ts
import { mount } from "svelte";

const app = mount(App, { target });
```

이 문제는 `npm run smoke:web`에서 `.tracker-bar`가 렌더링되지 않는 실패로 잡혀야 한다.

## 릴리즈 전 필수 체크리스트

릴리즈 태그를 만들기 전에 다음이 모두 통과해야 한다.

```text
npm run verify
npm run smoke:web
```

UI를 수정한 경우에는 `artifacts/smoke/web-smoke.png`를 직접 열어 bar와 상세 패널이 의도대로 보이는지도 확인한다.

## 릴리즈 게이트

GitHub Release는 `Release` workflow를 수동 실행할 때만 갱신한다. QA 승인 없이 release workflow를 실행하지 않는다.

Release 절차:

```powershell
gh workflow run Release --repo bbtarzan12/torch-overlay
gh run watch --repo bbtarzan12/torch-overlay --exit-status
```

이렇게 분리하면 main에 코드를 push해도 배포가 되지 않는다. 실제 설치본 갱신은 `workflow_dispatch`로만 실행한다.
