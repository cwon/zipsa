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

위 링크는 항상 최신 정식 릴리스를 가리킵니다. 현재 설치 파일은 macOS 공증과 Windows 코드 서명이 적용되지 않아 운영체제에서 확인 메시지가 표시될 수 있습니다.

## 주요 기능

- 공간 안에 하위 공간과 아이템을 담아 관리
- 아이템의 사진·수량·유통기한·구매정보 기록
- 공간·아이템별 할 일, 일회성·날짜 기준·완료일 기준 반복 일정
- 할 일 알림과 완료 이력, 전체 검색, 로컬 백업·복원
- 최신 버전 확인과 GitHub 다운로드 페이지 연결

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

버그 제보와 기능 제안은 [Issues](https://github.com/cwon/zipsa/issues)에 남겨 주세요. PR도 환영합니다.

## 라이선스

[MIT](LICENSE) · Copyright (c) 2026 cwon
