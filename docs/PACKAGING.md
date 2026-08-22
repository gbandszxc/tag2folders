# macOS DMG 打包

打包流程完全脚本化:`scripts/build-dmg.sh`,只依赖 macOS 系统自带工具
(`cargo` / `sips` / `iconutil` / `codesign` / `hdiutil`,另有 `python3 + PIL` 时图标质量更好),
不引入 cargo-bundle / create-dmg 等第三方打包器。

## 快速使用

```bash
scripts/build-dmg.sh                  # 常规:cargo build --release 后打包
T2F_SKIP_BUILD=1 scripts/build-dmg.sh # 跳过构建,复用 target/release/tag2folders
T2F_BIN=/path/to/tag2folders scripts/build-dmg.sh  # 用指定二进制打包(如 CI 产物)
```

产物:`target/dmg/tag2folders_<version>_<arch>.dmg`

- `<version>` 取自 `Cargo.toml` 的 `version`(当前 2.0.1);
- `<arch>` 取自 `rustc -vV` 的 host(当前 `aarch64-apple-darwin`,即 Apple Silicon 单架构)。

DMG 内含 `tag2folders.app` 与 `/Applications` 软链接,用户拖拽即完成安装。

## 环境变量

| 变量 | 作用 |
| --- | --- |
| `T2F_SKIP_BUILD=1` | 跳过 `cargo build --release`,直接用已有二进制 |
| `T2F_BIN=<path>` | 指定二进制路径(与 `T2F_SKIP_BUILD=1` 搭配) |

## 流程细节

1. **二进制**:`cargo build --release`(release profile,无 debug 符号裁剪等特殊配置)。
2. **图标**:
   - 源图 `assets/app-icon.png`,是原项目(Tauri 版)`docs/icon/raw.png` 的**逐字节副本**,
     因此图标与重构前完全一致(圆形插画风格);
   - 生成 `assets/AppIcon.icns` 缓存:有 `python3 + PIL` 时用 Lanczos 重采样
     (与原项目 `scripts/generate_icons.py` 同算法,产物与其 icns **像素级一致**,已实测 0.0000/255);
     无 PIL 时退回 `sips`(重采样质量略低,内容仍一致);
   - 缓存 `assets/AppIcon.icns` 随仓库提交,删掉后会按上述规则自动重建。
3. **.app bundle**(`Contents/` 结构):
   - `MacOS/tag2folders` — 二进制;
   - `Resources/AppIcon.icns` — 图标;
   - `Info.plist` — `CFBundleIdentifier=com.gbandszxc.tag2folders`,版本取自 Cargo.toml,
     `LSMinimumSystemVersion=13.0`,Retina(`NSHighResolutionCapable`)等键齐全。
4. **签名**:组包后 `codesign --force --sign -`(ad-hoc)并 `--verify --strict` 校验。
   arm64 二进制必须带签名才能运行,cargo 链接器默认已 ad-hoc 签名,重签是为覆盖拷贝过程并统一标识。
5. **DMG**:`hdiutil create -format UDZO`(压缩只读),完成后 `hdiutil verify` 自校验。

## 签名与分发

当前产物为 **ad-hoc 签名**:

- 自用/内部分发(直接拷贝、本机运行)没有问题;
- 通过浏览器/聊天工具分发给其他人时,Gatekeeper 会拦截"无法验证开发者"。
  正式对外分发需要 Apple Developer 账号:

```bash
# Developer ID 签名 + 公证(需证书与 App 专用密码)
codesign --force --deep --options runtime --sign "Developer ID Application: <名称> (<TeamID>)" \
    target/dmg/staging/tag2folders.app
xcrun notarytool submit <dmg> --keychain-profile <profile> --wait
xcrun stapler staple <dmg>
```

## 通用二进制(Intel + Apple Silicon)

当前只出 arm64。需要 universal2 时:

```bash
rustup target add x86_64-apple-darwin
cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin
lipo -create \
    target/aarch64-apple-darwin/release/tag2folders \
    target/x86_64-apple-darwin/release/tag2folders \
    -output /tmp/tag2folders-universal
T2F_SKIP_BUILD=1 T2F_BIN=/tmp/tag2folders-universal scripts/build-dmg.sh
```

(注意:文件名里的 arch 仍取自本机 rustc host,universal 产物建议自行改名加 `universal` 后缀。)

## 历史验证记录

- 2026-08-22:从 HEAD(8877339)构建,DMG 挂载后 bundle 结构/Info.plist/签名校验通过,
  从挂载卷直接运行 app 成功创建窗口并正常退出;
  图标与原项目 `src-tauri/icons/icon.icns` 各尺寸像素差 0.0000/255。
