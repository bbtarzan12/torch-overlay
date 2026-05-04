# 로컬 업데이트 테스트

이 절차는 실제 릴리즈와 충돌하지 않도록 별도 제품명과 식별자를 사용한다.

- 제품명: `Torch Overlay Update Test`
- 식별자: `kr.tli.torch-overlay.update-test`
- 기본 endpoint: `https://localhost:7531/latest.json`
- 테스트 키/아티팩트 위치: `artifacts/update-test`

## 절차

1. 업데이트 테스트 아티팩트를 만든다.

```powershell
npm run update:test:build
```

2. 로컬 업데이트 서버를 켠다. 이 터미널은 유지한다.

```powershell
npm run update:test:server
```

3. 다른 터미널에서 old 버전을 설치하고 실행한다.

```powershell
npm run update:test:install-old
```

4. 앱에서 `상세` 오른쪽의 `업데이트` 버튼을 누른다.

예상 흐름:

- 앱은 `0.1.1`로 실행된다.
- 로컬 서버의 `latest.json`은 `0.1.2`를 제공한다.
- 버튼은 업데이트 발견 후 다운로드/설치 상태로 바뀐다.
- 설치 후 `C:\Users\<user>\AppData\Local\Torch Overlay Update Test\torch-overlay.exe`가 갱신된다.

## 주의

- `artifacts/update-test/keys/update-test.key`는 테스트 전용 개인키다.
- `artifacts/update-test/certs/localhost.cer`는 `CurrentUser\Root`에 등록되는 테스트 전용 로컬 HTTPS 인증서다.
- 이 키는 실제 배포에 사용하지 않는다.
- `artifacts/`는 `.gitignore` 대상이므로 생성된 키와 설치파일은 커밋하지 않는다.
