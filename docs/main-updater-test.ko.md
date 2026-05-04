# Main 업데이트 테스트

자동 업데이트는 GitHub의 최신 stable release만 사용한다.

```text
endpoint = https://github.com/bbtarzan12/torch-overlay/releases/latest/download/latest.json
workflow = Release
release tag = v{version}
```

## 절차

1. 로컬에서 `npm run verify`와 `npm run smoke:web`를 통과시킨다.
2. 변경분을 `main` 브랜치에 커밋하고 푸시한다.
3. GitHub Actions에서 `Release` workflow를 수동 실행한다.
4. workflow가 Windows installer, signature, `latest.json`을 최신 stable release에 업로드한다.
5. 설치된 앱은 시작 시 위 endpoint를 확인하고 새 버전이면 자동 설치 후 재시작한다.

## 정책

- 로컬 `file://` updater는 사용하지 않는다.
- localhost updater 서버는 사용하지 않는다.
- 별도 update-test 앱은 사용하지 않는다.
- QA는 로컬 설치본으로 하고, 배포는 `main` 브랜치의 stable release만 사용한다.
