<p align="center">
  <img src="src-tauri/icons/128x128@2x.png" width="128" alt="PalworldModManager icon" />
</p>

# PalworldModManager

> Palworld용 macOS(Apple Silicon) 네이티브 모드 매니저. 게임을 탐지해 UE4SS 로더를 주입하고, 모드 가져오기 · 설치 · 활성/비활성 · 업데이트 · 프로필을 한 앱에서 관리한다.

<p align="center">
  <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/License-MIT-green.svg"></a>
  <img alt="Version" src="https://img.shields.io/badge/version-0.2.0-orange.svg">
  <img alt="Desktop" src="https://img.shields.io/badge/desktop-Apple%20Silicon-lightgrey.svg">
  <img alt="UE4SS" src="https://img.shields.io/badge/UE4SS-v0.2.0%20bundled-blue.svg">
</p>

<p align="center"><a href="README.md">English</a></p>

---

## 요구 사항

- macOS, Apple Silicon(arm64)
- Palworld(Mac App Store / 샌드박스 빌드) 설치
- 모드는 사용자가 직접 준비(이 앱은 배포처가 아님)

## 설치

1. 릴리즈에서 `PalworldModManager.app`(또는 DMG)을 내려받아 응용 프로그램으로 옮긴다.
2. 앱은 ad-hoc 서명(미공증)이라 최초 실행이 Gatekeeper에 막힌다. 한 번만 아래로 허용하면 이후엔 정상 실행된다.
   - 앱 실행 시도 → 차단 안내가 뜬다.
   - 시스템 설정 > 개인정보 보호 및 보안 으로 이동한다.
   - "'PalworldModManager'이(가) 차단되었습니다" 옆의 **그래도 열기** 를 누른다.
3. 실행하면 게임을 자동 탐지한다. 못 찾으면 앱에서 게임 실행 파일 경로를 직접 지정한다.

## 모드 사용

- 모드는 압축을 푼 뒤 원본 폴더 구조(`LogicMods` / `~mods` / `Scripts`)를 그대로 임포트한다. 이 구조로 배치 위치를 판별한다.
- 되도록 **IoStore**(`.pak` + `.utoc` + `.ucas`)로 쿡된 모드를 받아서 쓴다. GamePass 버전 모드가 보통 이 형식이다. 단일 레거시 `.pak`도 임포트 시 자동 변환하지만, IoStore 원본이 더 안정적이다.
- 자동 업데이트는 매니페스트에 `updateURL`이 있는 모드만 확인한다. 대부분의 배포 모드에는 없으므로, 재임포트로 갱신한다.
- 게임 업데이트 후 첫 Play에서 폴더 권한 설정을 위해 관리자 암호를 다시 요청할 수 있다.

## 제약

- macOS Apple Silicon(arm64) 전용.
- Windows에서 쿡된 일부 코스메틱 pak은 셰이더 포맷 차이로 외형이 표시되지 않을 수 있다(모드 제작 방식에 따른 한계, 매니저가 아님).
- 매니저가 업데이트(앱 자체·UE4SS 런타임·모드)를 확인해 패널로 알려주지만, 앱 자체는 ad-hoc 서명이라 자동 설치는 못 한다 — 릴리즈 페이지로 안내하므로 앱은 재다운로드로 갱신한다.

## 라이선스

[MIT](LICENSE) © h-taek
