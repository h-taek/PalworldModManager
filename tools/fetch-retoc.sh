#!/usr/bin/env bash
# retoc(단일 pak -> IoStore 3종 변환) aarch64 바이너리를 resources/로 조달한다.
# 번들 핀 고정: 검증된 버전만 사용(자동업데이트 안 함). libUE4SS.dylib와 같은 provisioning 방식
# — 바이너리는 gitignore, 이 스크립트가 다운로드+sha256 검증. 빌드/번들 전에 실행.
set -euo pipefail

REPO="trumank/retoc"
TAG="v0.1.5"
ASSET="retoc_cli-aarch64-apple-darwin.tar.xz"
# 릴리즈 자산 sha256(핀). 값이 바뀌면 릴리즈가 변조됐거나 태그를 올린 것 — 수동 확인 필요.
EXPECTED_SHA="07d490455012c4851a20df07380fe3bf470ed49f20c465a8af327328e735b935"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="$ROOT/src-tauri/resources/retoc"
VERSION_FILE="$ROOT/src-tauri/resources/retoc-version.txt"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "[fetch-retoc] $REPO $TAG ($ASSET)"
gh release download "$TAG" --repo "$REPO" --pattern "$ASSET" --dir "$TMP" --clobber

GOT_SHA="$(shasum -a 256 "$TMP/$ASSET" | awk '{print $1}')"
if [ "$GOT_SHA" != "$EXPECTED_SHA" ]; then
  echo "[fetch-retoc] sha256 불일치! expected=$EXPECTED_SHA got=$GOT_SHA" >&2
  exit 1
fi
echo "[fetch-retoc] sha256 OK"

tar -xf "$TMP/$ASSET" -C "$TMP"
BIN="$(find "$TMP" -type f -name retoc -perm -u+x | head -1)"
[ -n "$BIN" ] || { echo "[fetch-retoc] 추출물에서 retoc 바이너리를 못 찾음" >&2; exit 1; }

mkdir -p "$(dirname "$DEST")"
cp "$BIN" "$DEST"
chmod +x "$DEST"
xattr -d com.apple.quarantine "$DEST" 2>/dev/null || true
printf '%s' "$TAG" > "$VERSION_FILE"
echo "[fetch-retoc] 배치 완료: $DEST ($TAG)"
