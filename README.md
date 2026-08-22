# Tag2Folders — GPUI 版

> 源项目 `tag2folders`（Tauri 2 + React）的 GPUI 纯 Rust 重写，样式/功能 1:1（见 `docs/SOURCE_SPEC.md`），窗口 `1100×750` 向导式 三步：扫描 → 预览 → 执行。

## 结构

```
.
├── Cargo.toml              # lib(tag2folders_lib) + bin(tag2folders)，gpui 0.2.2 + gpui-component 0.5.1
├── src/
│   ├── main.rs             # Application + Window + Root，shot 句柄外带
│   ├── lib.rs              # 纯逻辑库（无 gpui）
│   ├── app.rs              # AppShell + Scan/Preview/Progress 三页 + 状态机 + 弹窗
│   ├── shot.rs             # 截图取证（T2F_SHOT_*，ScreenCaptureKit）
│   ├── core/               # 1:1 平移：metadata/scanner/template/preview/organizer/path_util/path_security
│   ├── task.rs             # 快照注册表（TTL 300s/容量32，轮询）
│   ├── service.rs          # 纯函数服务层（scan/preview/organize/browse）
│   └── ui/
│       ├── theme.rs        # 设计 token（amber/slate 等）
│       ├── icon.rs         # 32 图标（currentColor 遮罩，.text_color 上色）
│       ├── assets.rs       # AssetSource（include_str! + fs 回退）
│       ├── dir_picker.rs   # 目录选择（原生→内置模态降级）
│       ├── service.rs      # run_service 后台线程 helpers
│       └── components/{button,badge,card,alert_bar,progress_bar,step_nav,modal}.rs
├── assets/
│   ├── icons/*.svg         # 33 图标（含 check/circle-x）
│   ├── AppIcon.icns        # .app 图标
│   └── app-icon.svg        # 源
├── scripts/build-dmg.sh    # hdiutil/iconutil/codesign → .app → .dmg（不引 cargo-bundle）
├── docs/
│   ├── SOURCE_SPEC.md      # 源项目全量规格（唯一 UI 依据）
│   ├── GPUI_NOTES.md       # gpui 选型/坑位
│   ├── UI_INTEGRATION.md   # 加页/调服务/弹窗 指南
│   ├── PORT_NOTES.md       # 移植分歧（含 runtime_shaders）
│   ├── KNOWN_DIFFERENCES.md# 有意差异
│   └── PACKAGING.md        # 打包说明
└── shots/                  # 取证输出（git 忽略）
```

## 开发速查（macOS arm64，Rust ≥1.85，Command Line Tools 即可）

> `runtime_shaders` 已开，构建无需完整 Xcode；装后可删该 feature 回构建期编译。

### 启停

```sh
cargo run                          # 调试启动（1100×750，Dock 见 Tag2Folders）
cargo run --release                # 优化版
./target/debug/tag2folders         # 直接跑产物（单文件，assets 已内嵌）
./target/release/tag2folders       # 发版产物

# 后台启停（omp hub，日志走 hub logs）
hub start tag2folders -- ./target/debug/tag2folders
hub logs tag2folders
hub stop tag2folders
pkill -f tag2folders; sleep 0.5    # 兜底杀

# 激活已跑实例
osascript -e 'tell application "Tag2Folders" to activate'
```

### 调试/日志

```sh
cargo build                        # 2-5s 增量，4s 全量
cargo test                         # lib 67 + bin 13
cargo test --lib -- --nocapture    # 单测打印
cargo clippy --all-targets 2>&1 | tail
cargo run 2>&1 | tee /tmp/t2f.log  # eprintln! 走 stderr，Finder 启动看 Console.app
log show --predicate 'process == "tag2folders"' --last 10s | grep exit
cat /tmp/t2f_exit.log 2>/dev/null  # 退出链路调试残留（已清理）

# 视口/滚动：外层 relative+size_full+overflow_hidden 约束 → workspace flex_col+min_h0+overflow_y_scroll
# 图标：svg 遮罩，颜色必须 .text_color() 设在 svg 自身，不继承父
```

### 取证截图（真窗口像素）

```sh
mkdir -p /tmp/t2f-shots/music /tmp/t2f-shots/target
T2F_SHOT_STATES="empty,scan,preview,preview_tree,progress" \
T2F_SHOT_DIR="shots" cargo run
# 或
T2F_SHOT_STATES="empty,scan,preview,preview_tree,progress" \
T2F_SHOT_DIR="/tmp/t2f-shots" ./target/debug/tag2folders
ls -lh shots/ /tmp/t2f-shots/
```

原理：`shot.rs` 用 ScreenCaptureKit 截自有窗口，无需屏幕录制权限；`WAIT_ON_SCREEN 8s + SETTLE 1.2s`。

### 发版

```sh
cargo build --release && strip target/release/tag2folders
# 产物：target/release/tag2folders（~30M stripped，单文件可分发 zip）

# .app + .dmg（macOS 宿主，含图标/签名可选项）
bash scripts/build-dmg.sh              # 产 dist/Tag2Folders.app + Tag2Folders.dmg
hdiutil create -volname Tag2Folders -srcfolder dist/Tag2Folders.app -ov dist/Tag2Folders.dmg  # 裸 dmg
# 签名公证（分发给他人再做）
codesign --sign "Developer ID" --deep dist/Tag2Folders.app
xcrun notarytool submit dist/Tag2Folders.dmg --wait
```

Windows `.msi` 需 Win 宿主：`cargo install cargo-wix && cargo wix`（WiX Toolset）。

### 常用排查

```sh
git status --short; git diff --stat HEAD; git log --oneline -7
lsof -i :5173 2>/dev/null | head              # 源 Tauri 前端 vite 残留
ps aux | grep tag2folders | grep -v grep
xcode-select -p; xcrun -f metal 2>&1 | head  # 验证 Metal 工具链
```

`docs/` 为唯一规格源，改 UI 前必读 `SOURCE_SPEC.md` 对应章节 + `UI_INTEGRATION.md` 7 条 gpui 踩坑。
