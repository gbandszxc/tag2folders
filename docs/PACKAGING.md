# 打包

| 平台 | 脚本 | 产物 |
| --- | --- | --- |
| macOS | `scripts/build-dmg.sh` | `target/dmg/tag2folders_<version>_<arch>.dmg` |
| Windows | `scripts/build-msi.ps1` | `target/msi/tag2folders_<version>_<arch>.msi` |

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
   - 源图 `assets/app-icon.png`(圆形插画风格);
   - 生成 `assets/AppIcon.icns` 缓存:有 `python3 + PIL` 时用 Lanczos 重采样(质量更好);
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

# Windows MSI 打包

打包流程同样脚本化:`scripts/build-msi.ps1` + `scripts/tag2folders.wxs`(WiX v5 语法)。
WiX 不在 PATH 时脚本自动下载官方 wix-cli MSI 到
`%LOCALAPPDATA%\tag2folders\wix-cli\<version>` 并以 `msiexec /a` 免管理员解包使用,
仅需 .NET 运行时(无需 .NET SDK)。不引入 cargo-wix(绑死已停止支持的 WiX v3)。

```powershell
powershell -File scripts\build-msi.ps1                # 常规:cargo build --release 后打包
$env:T2F_SKIP_BUILD=1; powershell ...                 # 跳过构建,复用 target/release/tag2folders.exe
$env:T2F_BIN='D:\path\tag2folders.exe'; powershell ... # 用指定二进制打包(如 CI 产物)
$env:T2F_WIX='D:\path\wix.exe'; powershell ...         # 用指定 wix.exe
```

产物:`target\msi\tag2folders_<version>_<arch>.msi`(arch 取自 rustc host:x64 / arm64)。

## 内容

- `C:\Program Files\Tag2Folders\`:主程序 + `app.ico`;
- 开始菜单 `Tag2Folders` 快捷方式(per-machine);
- 控制面板“应用”条目(ARP)带产品图标,不可“修改”只可卸载;
- `MajorUpgrade(AllowSameVersionUpgrades)` + 固定 `UpgradeCode`:高版本覆盖升级,
  同版本号也可覆盖安装(开发期反复重打 MSI),拒绝降级。

## exe 图标

`build.rs` 用 `winresource` 把 `assets/app.ico` 嵌入 exe 资源段(资源管理器/任务栏/窗口图标),
仅 windows 目标生效。`app.ico` 缺失时 `build-msi.ps1` 会用 System.Drawing 从
`assets/app-icon.png` 重建(256px 档 PNG 压缩),但建议直接提交缓存文件
(当前仓库版由 python+PIL Lanczos 生成,与 macOS icns 管线同源)。

## 安装/卸载

- `msiexec /i ... /qn` 静默安装,文件/开始菜单/ARP 齐全;
- `msiexec /a` 管理员映像自校验(脚本内建)。

## Windows:无控制台窗口 / libpng warning

现象:双击启动自带终端窗口,持续打印 `libpng warning: iCCP: known incorrect sRGB profile`。

定位(对照实验,同 exe 同 cwd 复现 0/51 行漂移):
- 项目依赖树只有纯 Rust `png` crate(0.17/0.18),registry 源码中无该 warning 文本;
- 进程模块表显示搜狗输入法注入 DLL(PicFace64 等,内嵌 C libpng)与系统
  IconCodecService.dll —— warning 来自外部注入组件写 stderr,非本项目代码;
- 终端窗口本体是 Rust 默认 console 子系统:双击启动时 Windows 自动分配控制台。

修复:`src/main.rs` 顶部 `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]`
(仅 release,debug 保留控制台看日志)。GUI 子系统不分配控制台 → 无窗口、warning 无处显示;
显式重定向 stderr 时句柄仍继承,不影响 `shot.rs` 取证的 `eprintln!`。

验证:安装版 PE 头 `Subsystem=2(GUI)`;启动后 EnumWindows 仅 `Zed::Window` + 输入法窗口,
无 `ConsoleWindowClass`。
