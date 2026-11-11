# 집사 · Zipsa

집·사무실의 공간, 아이템, 할 일을 연결해 관리하는 데스크톱 앱입니다.
계정 없이 사용하며, 사진을 포함한 모든 생활 정보는 내 컴퓨터에 저장합니다.

**macOS 13 이상(Apple Silicon) · Windows 10 이상(x64)** — 현재 초기 개발 버전입니다.

## 다운로드

**[최신 버전 받기](https://github.com/cwon/zipsa/releases/latest)** · [이전 버전](https://github.com/cwon/zipsa/releases)

| 운영체제              | 설치 파일                                                                                    |
| --------------------- | -------------------------------------------------------------------------------------------- |
| macOS · Apple Silicon | [DMG 다운로드](https://github.com/cwon/zipsa/releases/latest/download/Zipsa-macOS-arm64.dmg) |
| Windows · x64         | [EXE 다운로드](https://github.com/cwon/zipsa/releases/latest/download/Zipsa-Windows-x64.exe) |

위 링크는 항상 최신 정식 릴리스를 가리킵니다.

## 처음 실행할 때

현재 설치 파일에는 Apple 공증과 Windows 코드 서명이 없습니다. 그래서 처음 실행할 때 운영체제가 경고를 표시합니다. 한 번 허용하면 다음부터는 바로 열립니다.

### macOS

DMG를 열고 Zipsa를 응용 프로그램 폴더로 옮긴 뒤 실행합니다. Apple이 악성 코드 여부를 확인할 수 없다는 경고가 표시되면 다음과 같이 허용합니다.

- **macOS 13·14:** Finder의 응용 프로그램 폴더에서 Zipsa를 Control-클릭하고 **열기**를 선택합니다. 경고 창에서 다시 **열기**를 누릅니다.
- **macOS 15 이상:** 경고 창을 닫고 **시스템 설정 > 개인정보 보호 및 보안**으로 이동합니다. 보안 항목의 **그래도 열기**를 누르고 암호를 입력합니다.

v0.1.0 설치 파일은 “손상되었기 때문에 열 수 없습니다”라는 메시지가 표시되며 열리지 않습니다. 이후 버전에서는 이 메시지가 표시되지 않습니다. v0.1.0을 사용하려면 터미널에서 다음 명령을 실행한 뒤 다시 엽니다.

```sh
xattr -dr com.apple.quarantine /Applications/Zipsa.app
```

### Windows

“Windows의 PC 보호” 창이 표시되면 **추가 정보**를 누른 뒤 **실행**을 누릅니다.

## 주요 기능

- 공간 안에 하위 공간과 아이템을 담아 관리
- 아이템의 사진 여러 장·수량·유통기한·구매정보·보증 만료일 기록
- 영수증·보증서·설명서를 아이템에 첨부해 함께 보관
- 공간·아이템별 할 일, 일회성·날짜 기준·완료일 기준 반복 일정
- 할 일별 미리 알림과 대형 월간 달력, 날짜별 일정 확인·완료·미루기·수정
- 할 일 알림과 완료 이력, 전체 검색, 로컬 백업·복원
- 내용이 바뀐 날 하루 한 번 자동 백업(최근 7개 보관)
- 앱 시작 시 새 버전 알림, 수동 버전 확인과 GitHub 다운로드 페이지 연결
- 앱 설정에서 Google 설문지로 의견 보내기

[공간·아이템·할 일의 관계 한눈에 보기](assets/community/zipsa-concepts-v1.png)

## 실행

Node.js 24 LTS, Rust 1.90 이상과 [Tauri 개발 환경](https://v2.tauri.app/start/prerequisites/)이 필요합니다.

```sh
git clone https://github.com/cwon/zipsa.git
cd zipsa
npm ci
npm run desktop
```

테스트와 배포 설정은 [개발 문서](docs/DEVELOPMENT.md)를 참고하세요.

## 참여

불편한 점과 기능 제안은 [의견 보내기](https://docs.google.com/forms/d/e/1FAIpQLSfeuuXiUu_BCWlc4_tnlmNma9HAtdgCO2Yxhy1kQfBDYvP4Cg/viewform)에 남겨 주세요. 앱 설정에서도 열 수 있습니다.
공개 버그 제보는 [Issues](https://github.com/cwon/zipsa/issues), 코드 기여는 PR을 이용해 주세요.

## 라이선스

[MIT](LICENSE) · Copyright (c) 2026 cwon

화면 글꼴로 [Pretendard](https://github.com/orioncactus/pretendard)를 앱에 포함합니다. Pretendard는 [SIL Open Font License 1.1](https://openfontlicense.org)을 따릅니다.
