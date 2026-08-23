# 开发手册（MANUAL）

> 日常开发的工作命令与排查经验。规格基准见 `SPEC.md`，UI 组装见 `UI_GUIDE.md`，打包细节见 `PACKAGING.md`。

## 启停

```sh
cargo run                          # 调试启动（1100×750，Dock 见 Tag2Folders）
./target/release/tag2folders       # 直接跑产物（单文件，assets 已内嵌）

# 后台启停（omp hub，日志走 hub logs）
hub start tag2folders -- ./target/debug/tag2folders
hub logs tag2folders
hub stop tag2folders
pkill -f tag2folders; sleep 0.5    # 兜底杀

# 激活已跑实例
osascript -e 'tell application "Tag2Folders" to activate'
```

## 调试与日志

```sh
cargo build                        # 2-5s 增量，4s 全量
cargo test                         # lib 67 + bin 13
cargo test --lib -- --nocapture    # 单测打印
cargo clippy --all-targets 2>&1 | tail
cargo run 2>&1 | tee /tmp/t2f.log  # eprintln! 走 stderr；Finder 启动看 Console.app
log show --predicate 'process == "tag2folders"' --last 10s | grep exit
```

## 发版

```sh
cargo build --release && strip target/release/tag2folders
# 产物：target/release/tag2folders（~30M stripped，单文件可分发 zip）

bash scripts/build-dmg.sh          # .app + .dmg；MSI/签名/公证详见 docs/PACKAGING.md
```

## 常用排查

```sh
git status --short; git diff --stat HEAD; git log --oneline -7
ps aux | grep tag2folders | grep -v grep
xcode-select -p; xcrun -f metal 2>&1 | head  # Metal 工具链诊断（runtime_shaders 下仅参考）
```
