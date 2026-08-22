//! Windows:把 assets/app.ico 嵌入 exe 资源段(资源管理器/任务栏/窗口图标)。
//! 文件缺失时静默跳过,不影响 macOS/Linux 构建;ico 由 scripts/build-msi.ps1 重建。

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::path::Path::new("assets/app.ico").exists()
    {
        winresource::WindowsResource::new()
            .set_icon("assets/app.ico")
            .compile()
            .expect("嵌入 app.ico 失败");
        println!("cargo:rerun-if-changed=assets/app.ico");
    }
}
