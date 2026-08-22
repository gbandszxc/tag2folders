#!/usr/bin/env bash
# tag2folders macOS 打包脚本:release 二进制 → .app bundle → DMG
#
# 用法:
#   scripts/build-dmg.sh                  # cargo build --release 后打包
#   T2F_SKIP_BUILD=1 scripts/build-dmg.sh # 复用 target/release 已有二进制
#   T2F_BIN=/path/to/tag2folders scripts/build-dmg.sh  # 用指定二进制打包
#
# 产物:target/dmg/tag2folders_<version>_<arch>.dmg(版本取自 Cargo.toml,架构取自 rustc host)
# 详见 docs/PACKAGING.md。
#
# 仅依赖 macOS 系统自带工具:cargo / sips / iconutil / codesign / hdiutil。
set -euo pipefail

cd "$(dirname "$0")/.."
repo_root=$(pwd)

APP_NAME="tag2folders"
BUNDLE_ID="com.gbandszxc.tag2folders"
MIN_MACOS="13.0"
DMG_DIR="$repo_root/target/dmg"

VERSION=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)
if [[ -z "$VERSION" ]]; then echo "error: 无法从 Cargo.toml 解析版本" >&2; exit 1; fi
ARCH=$(rustc -vV | sed -n 's/^host: //p')

# 1) 二进制
BIN="${T2F_BIN:-$repo_root/target/release/$APP_NAME}"
if [[ -z "${T2F_BIN:-}" && -z "${T2F_SKIP_BUILD:-}" ]]; then
    cargo build --release
fi
if [[ ! -x "$BIN" ]]; then echo "error: 二进制不存在或不可执行: $BIN" >&2; exit 1; fi
echo "==> 二进制: $BIN ($(file -b "$BIN" | cut -d, -f1-2))"

# 2) 图标(优先用缓存 icns;否则从 assets/app-icon.png 生成)
#    图标源 = 原项目 tag2folders(Tauri 版)docs/icon/raw.png 的副本,与重构前完全一致
ICON_SRC="$repo_root/assets/app-icon.png"
ICON_CACHE="$repo_root/assets/AppIcon.icns"
build_icns() {
    command -v iconutil >/dev/null || return 1
    local iconset; iconset=$(mktemp -d)/AppIcon.iconset
    mkdir -p "$iconset"
    if python3 -c "import PIL" >/dev/null 2>&1; then
        # PIL Lanczos 与原项目 scripts/generate_icons.py 同算法,产物与原项目 icns 像素级一致
        python3 - "$ICON_SRC" "$iconset" <<'PY'
import sys
from PIL import Image
src, iconset = sys.argv[1], sys.argv[2]
img = Image.open(src).convert("RGBA")
for s in (16, 32, 128, 256, 512):
    img.resize((s, s), Image.Resampling.LANCZOS).save(f"{iconset}/icon_{s}x{s}.png")
    img.resize((s * 2, s * 2), Image.Resampling.LANCZOS).save(f"{iconset}/icon_{s}x{s}@2x.png")
img.resize((1024, 1024), Image.Resampling.LANCZOS).save(f"{iconset}/icon_512x512@2x.png")
PY
    else
        # 兜底:sips(重采样质量略低于 Lanczos,图标内容仍一致)
        for size in 16 32 128 256 512; do
            sips -z "$size" "$size" "$ICON_SRC" --out "$iconset/icon_${size}x${size}.png" >/dev/null
            sips -z "$((size * 2))" "$((size * 2))" "$ICON_SRC" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
        done
        sips -z 1024 1024 "$ICON_SRC" --out "$iconset/icon_512x512@2x.png" >/dev/null
    fi
    iconutil -c icns "$iconset" -o "$ICON_CACHE"
}
if [[ ! -f "$ICON_CACHE" ]]; then
    if build_icns; then echo "==> 已生成图标缓存: assets/AppIcon.icns"
    else echo "==> 警告: 缺少 ImageMagick 且无图标缓存,跳过应用图标"; fi
fi

# 3) 组装 .app
APP="$DMG_DIR/staging/$APP_NAME.app"
rm -rf "$DMG_DIR/staging"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/$APP_NAME"
chmod +x "$APP/Contents/MacOS/$APP_NAME"
if [[ -f "$ICON_CACHE" ]]; then cp "$ICON_CACHE" "$APP/Contents/Resources/AppIcon.icns"; fi

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>zh_CN</string>
    <key>CFBundleExecutable</key>
    <string>$APP_NAME</string>
    <key>CFBundleIconFile</key>
    <string>AppIcon</string>
    <key>CFBundleIdentifier</key>
    <string>$BUNDLE_ID</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>$APP_NAME</string>
    <key>CFBundleDisplayName</key>
    <string>$APP_NAME</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>$VERSION</string>
    <key>CFBundleVersion</key>
    <string>$VERSION</string>
    <key>CFBundleSupportedPlatforms</key>
    <array><string>MacOSX</string></array>
    <key>LSApplicationCategoryType</key>
    <string>public.app-category.utilities</string>
    <key>LSMinimumSystemVersion</key>
    <string>$MIN_MACOS</string>
    <key>LSRequiresNativeExecution</key>
    <true/>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSSupportsAutomaticGraphicsSwitching</key>
    <true/>
</dict>
</plist>
PLIST

# arm64 二进制必须带签名才能运行;cargo 链接器默认 ad-hoc 签名,
# 组包后重签一次并校验,确保拷贝过程中签名未被破坏
codesign --force --sign - "$APP"
codesign --verify --strict "$APP"

# 4) DMG(拖入 Applications 安装)
ln -s /Applications "$DMG_DIR/staging/Applications"
DMG="$DMG_DIR/${APP_NAME}_${VERSION}_${ARCH}.dmg"
rm -f "$DMG"
hdiutil create -volname "$APP_NAME" -srcfolder "$DMG_DIR/staging" -ov -format UDZO "$DMG" -quiet
hdiutil verify "$DMG" -quiet && echo "==> DMG 校验通过"

echo "==> 完成: $DMG ($(du -h "$DMG" | cut -f1))"
