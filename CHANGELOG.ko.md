# Changelog

<p align="center"><a href="CHANGELOG.md">English</a></p>

이 프로젝트의 주요 변경 사항을 기록한다. 버전은 매니저 앱 버전(`src-tauri/tauri.conf.json`)을 따르며, 번들되는 UE4SS 런타임 버전과는 별개다.

## [0.1.1] - 2026-07-05

### 수정

- 로그가 많은 모드에서 게임 실행이 검은화면/"응답 없음"으로 멈추던 문제. 매니저가 게임을 상속된 stdio로 실행해, 게임 로그가 아무도 읽지 않는 파이프 버퍼(약 64KB)를 채우는 순간 `write()`가 블로킹되어 게임이 얼어붙었다(수다스러운 모드, 예: MinimapWidget을 켜면 재현). 이제 게임의 stdout/stderr를 로그 파일(`~/Library/Caches/ue4ss-mac/palworld-launch.log`, 터미널 런처와 동일 위치)로 리다이렉트해, 로그량이 아무리 많아도 게임이 멈추지 않는다.

### 변경

- 번들 UE4SS 런타임을 **v0.2.1**로 갱신(기존 v0.2.0). FName 세터 가드를 고쳐 블루프린트 로직 모드(예: ModConfigMenu)가 `GetAsset`에서 실패하지 않고 실제로 로드된다. 매니저 앱 버전과 번들 UE4SS 버전은 계속 별개다.

## [0.1.0] - 2026-07-05

첫 공개 릴리즈. macOS(Apple Silicon) 네이티브 Palworld 모드 매니저.

### 추가

- 게임 자동 탐지 후 UE4SS 로더를 DYLD 주입으로 실행(Play).
- 모드 가져오기: Lua 모드와 pak 모드 모두 지원. 단일 레거시 pak은 가져올 때 IoStore 3종(.pak/.ucas/.utoc)으로 변환.
- 모드 활성/비활성/회수와 프로필(작업셋) 관리. 회수 시 매니저가 배치한 파일만 삭제하고 사용자가 직접 둔 파일은 보존.
- 번들 직접 스테이징: 활성 모드를 게임 앱 번들의 세 폴더(`Content/Paks/~mods`, `Content/Paks/LogicMods`, `Binaries/Win64/Mods`)로 Play 시점에 분산 배치. 최초 1회 관리자 권한으로 폴더 소유권을 사용자로 지정.
- UE4SS 런타임 자동 업데이트: GitHub 릴리즈를 조회해 dylib과 Lua 인프라(BPModLoaderMod, shared), settings를 런타임 폴더 단위로 통째 갱신. 번들 v0.2.0 포함.
- 모드 자동 업데이트: 매니페스트에 `updateURL`이 있는 모드에 한해 갱신 확인(opt-in).
- ModConfig 설정 파일 처리: `.modconfig.json` 원본을 쓰기 가능한 컨테이너에 두고 읽기 전용 번들에는 심링크만 배치해, 인게임 설정 저장이 정상 동작하도록 우회. 컨테이너의 사용자 저장값은 회수·재배포 시에도 보존.

### 알려진 제약

- macOS Apple Silicon(arm64) 전용.
- 앱은 ad-hoc 서명(미공증)이라 최초 실행 시 시스템 설정 > 개인정보 보호 및 보안 > "확인 없이 열기"로 허용해야 함.
- Windows에서 쿡된 일부 코스메틱 pak은 셰이더 포맷 차이로 외형이 표시되지 않을 수 있음(모드 제작 방식에 따른 한계).
- 매니저 앱 자체의 자동 업데이트는 없음. 새 버전은 재다운로드로 교체.
