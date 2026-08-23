# Tag2Folders

基于音频元数据标签自动整理文件的桌面工具（纯 Rust + GPUI），向导式三步：**扫描 → 模板预览 → 执行整理**。

## 功能特性

- 扫描本地目录并读取音频标签，支持 mp3 / flac / ogg / m4a / wav / aac / wma / ape / opus
- 命名模板规划目标路径（`{album}/{track}. {title}.{ext}` 等 7 个占位符），生成映射表与目录树预览
- 冲突自动消解（`_1` 后缀）、批内碰撞 / 路径越界 / 权限预检；预览纯只读，不碰文件系统
- 复制 / 移动两种模式，后台任务执行 + 实时进度与日志
- macOS（DMG）与 Windows（MSI）打包脚本

## 构建

- Rust ≥ 1.85；macOS 只需 Command Line Tools（gpui 已开 `runtime_shaders` feature，无需完整 Xcode）

```sh
cargo run                # 开发运行（1100×750 单窗口）
cargo run --release      # 优化版
cargo test               # lib 67 + bin 13
cargo clippy --all-targets
```

## 项目结构

```
.
├── Cargo.toml              # lib(tag2folders_lib) + bin(tag2folders)，gpui 0.2.2 + gpui-component 0.5.1
├── src/
│   ├── main.rs             # gpui 入口：Application → Window → Root
│   ├── lib.rs              # 纯逻辑库（无 gpui）
│   ├── core/               # 业务核心：scanner/metadata/template/preview/organizer/path_security/path_util
│   ├── task.rs             # 任务快照注册表（TTL 300s / 容量 32，轮询消费）
│   ├── service.rs          # 服务层纯函数（scan/preview/organize/status/browse）
│   ├── app.rs              # AppShell + 三页 + 状态机 + 弹窗
│   ├── shot.rs             # 截图取证（T2F_SHOT_*）
│   └── ui/                 # theme / icon / assets / dir_picker / components
├── assets/                 # SVG 图标与应用图标
├── scripts/                # build-dmg.sh / build-msi.ps1 / tag2folders.wxs
└── docs/                   # SPEC / UI_GUIDE / MANUAL / PACKAGING
```

## 文档

| 文档 | 内容 |
|---|---|
| [`docs/SPEC.md`](docs/SPEC.md) | 应用规格基准线：行为、数据契约、错误文案、设计 token（改代码须同步） |
| [`docs/UI_GUIDE.md`](docs/UI_GUIDE.md) | UI 开发指南：加页 / 调服务 / 弹窗套路与 gpui 坑位 |
| [`docs/MANUAL.md`](docs/MANUAL.md) | 开发手册：日常启停、调试日志、发版与排查命令 |
| [`docs/PACKAGING.md`](docs/PACKAGING.md) | 打包说明：macOS DMG / Windows MSI |

## License

MIT
