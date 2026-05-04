# Beta 업데이트 테스트

자동 업데이트는 GitHub Release의 고정 `beta` release 하나만 사용한다.

```text
endpoint = https://github.com/bbtarzan12/torch-overlay/releases/download/beta/latest.json
release tag = beta
workflow = Beta
```

## 절차

1. GitHub Actions에서 `Beta` workflow를 수동 실행한다.
2. workflow가 `beta` tag를 현재 커밋으로 이동한다.
3. workflow가 Windows installer, signature, `latest.json`을 `beta` release에 덮어쓴다.
4. 설치된 앱은 시작 시 위 endpoint를 확인하고 새 beta 버전이면 자동 설치 후 재시작한다.

## 정책

- 로컬 `file://` updater는 사용하지 않는다.
- localhost updater 서버는 사용하지 않는다.
- 별도 update-test 앱은 사용하지 않는다.
- beta도 실제 Tauri updater plugin, 실제 GitHub 다운로드 URL, 실제 서명 검증을 사용한다.

## 버전

`Beta` workflow는 `package.json`의 기본 semver에서 patch를 하나 올린 뒤 `-beta.{GITHUB_RUN_NUMBER}`를 붙인다.

예시:

```text
package.json = 0.1.1
beta build = 0.1.2-beta.123
```

다음 beta build는 더 큰 prerelease 번호가 되므로 기존 설치본에서 업데이트 감지가 가능하다.
