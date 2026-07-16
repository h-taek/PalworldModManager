#!/usr/bin/env bash
# sign-and-package-dmg.sh — 빌드된 .app을 제대로 ad-hoc 서명하고 DMG를 다시 만든다.
#
# 왜 필요한가:
#   tauri.conf.json 의 signingIdentity=None 이면 Tauri 는 codesign 을 아예 돌리지 않는다.
#   그러면 링커가 실행파일 하나에만 임시서명(linker-signed)을 남길 뿐, 번들 전체를 봉인하는
#   _CodeSignature/CodeResources 가 없어서 macOS 가 "code has no resources but signature
#   indicates they must be present" 로 실행을 막는다(= "손상되어 열 수 없습니다", 그래도 열기 불가).
#   → 여기서 xattr 정리 후 codesign --deep --force -s - 로 번들 전체를 봉인해 정상 ad-hoc 로 만든다.
#
#   이 프로젝트는 iCloud Drive 안에 있어 파일마다 iCloud 확장속성이 붙는다. codesign 은
#   "resource fork, Finder information, or similar detritus not allowed" 로 거부하므로
#   서명 전에 반드시 xattr -cr 로 확장속성을 털어야 한다.
#
# 사용:
#   npm run tauri build   # 먼저 앱+DMG 생성(이때 DMG 안 앱은 서명이 깨져 있음)
#   tools/sign-and-package-dmg.sh   # 이 스크립트로 서명 고치고 DMG 재생성
#   gh release create vX.Y.Z <생성된 DMG>
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT="$(pwd)"
APP="$ROOT/src-tauri/target/release/bundle/macos/PalworldModManager.app"
DMG_DIR="$ROOT/src-tauri/target/release/bundle/dmg"

VERSION="$(grep -m1 '"version"' src-tauri/tauri.conf.json | sed -E 's/.*: *"([^"]+)".*/\1/')"
OUT="$DMG_DIR/PalworldModManager_${VERSION}_aarch64.dmg"

[ -d "$APP" ] || { echo "ERROR: 빌드된 앱 없음 ($APP). 먼저 'npm run tauri build' 실행."; exit 1; }

echo "==> 앱 확장속성 정리(iCloud xattr 제거)"
xattr -cr "$APP"

echo "==> 번들 전체 ad-hoc 재서명"
codesign --deep --force -s - --timestamp=none "$APP"

echo "==> 서명 검증"
codesign --verify --deep --strict --verbose=2 "$APP"

echo "==> DMG 재생성"
STAGING="$(mktemp -d)"
trap 'rm -rf "$STAGING"' EXIT
cp -R "$APP" "$STAGING/PalworldModManager.app"
ln -s /Applications "$STAGING/Applications"
xattr -cr "$STAGING/PalworldModManager.app"
codesign --verify --deep --strict "$STAGING/PalworldModManager.app"
rm -f "$OUT"
hdiutil create -volname "PalworldModManager" -srcfolder "$STAGING" -ov -format UDZO "$OUT"

echo ""
echo "==> 완료: $OUT"
echo "    (DMG 안 앱이 codesign --verify 통과 = 미공증이지만 '그래도 열기'로 실행 가능)"
