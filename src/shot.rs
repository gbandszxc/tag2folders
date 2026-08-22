//! 截图取证模块(T1 视觉证据;仅 macOS,正常启动路径零开销)。
//!
//! 通过环境变量启用:
//!
//! ```sh
//! T2F_SHOT_STATES="empty,scan,preview,preview_tree,progress" \
//! T2F_SHOT_DIR="shots" \
//! cargo run
//! ```
//!
//! 流程:窗口打开后按列表逐个把 [`AppShell`] 置为演示态(见
//! `AppShell::setup_shot_state`)→ 等待渲染稳定 → 用 CoreGraphics
//! `CGWindowListCreateImage` 截取**本进程**窗口(截取自有窗口不需要
//! 屏幕录制权限)→ 解出像素缓冲 → 以 PNG 落盘 → 全部完成后退出。
//!
//! 为什么不用 gpui 测试框架的离屏 `draw`:gpui 0.2.2 的 `Scene`/
//! `rendered_frame` 是 `pub(crate)`,`VisualTestContext::draw` 只做
//! layout/prepaint/paint,不产出可读的逐像素缓冲(见 docs/T1_FINDINGS.md)。
//! 真窗口 + 自窗口截取是唯一能拿到忠实像素的路线。

#[cfg(target_os = "macos")]
use std::ffi::c_void;
use std::path::Path;
use std::time::Duration;

use gpui::{App, AppContext, Entity};
#[cfg(target_os = "macos")]
use image::RgbaImage;

use crate::app::AppShell;

/// 状态列表环境变量(逗号分隔)。
pub const ENV_STATES: &str = "T2F_SHOT_STATES";
/// 输出目录环境变量(默认 `shots`)。
pub const ENV_DIR: &str = "T2F_SHOT_DIR";

/// 每个状态注入后等待渲染的时长(首帧 + 字体/图标异步加载余量)。
const SETTLE_MS: u64 = 1200;

/// 等外部激活器(取证脚本)把窗口拉起的固定时长。
const WAIT_ON_SCREEN_MS: u64 = 8_000;

/// 若设置了 [`ENV_STATES`] 则启动取证会话;否则立即返回(正常启动路径)。
pub fn maybe_run_shot_session(
    cx: &mut App,
    window: &gpui::WindowHandle<gpui_component::Root>,
    shell: Entity<AppShell>,
) {
    let Ok(states_var) = std::env::var(ENV_STATES) else {
        return;
    };
    let states: Vec<String> = states_var
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if states.is_empty() {
        return;
    }
    let out_dir = std::env::var(ENV_DIR).unwrap_or_else(|_| "shots".to_string());
    if let Err(err) = std::fs::create_dir_all(&out_dir) {
        eprintln!("[shot] 无法创建输出目录 {out_dir}: {err}");
        return;
    }
    eprintln!("[shot] 取证模式: {states:?} → {out_dir}");

    let any_handle = gpui::AnyWindowHandle::from(*window);
    cx.spawn(async move |cx| {
        // 等待首帧与字体/图标资源就绪
        cx.background_executor()
            .timer(Duration::from_millis(SETTLE_MS))
            .await;
        // macOS 26 上 CLI 启动的进程窗口默认 ordered-out:自激活/
        // orderFrontRegardless/setLevel 实测均无效,且进程自身的 CG 窗口
        // 枚举会被 redact 掉 bounds,无法自查可见性(详见 T1_FINDINGS.md)。
        // 取证脚本约定在启动后 ~5s 用外部激活器把本进程拉起,
        // 这里多等一拍再开始逐状态截取。
        cx.background_executor()
            .timer(Duration::from_millis(WAIT_ON_SCREEN_MS))
            .await;
        for state in &states {
            let injected = cx
                .update_window(any_handle, |_root, window, cx| {
                    shell.update(cx, |this, cx| this.setup_shot_state(state, window, cx))
                })
                .unwrap_or(false);
            if !injected {
                eprintln!("[shot] 未知状态(跳过): {state}");
                continue;
            }
            cx.background_executor()
                .timer(Duration::from_millis(SETTLE_MS))
                .await;
            let path = Path::new(&out_dir).join(format!("step-{state}.png"));
            match capture_window_png(&path) {
                Ok((w, h)) => eprintln!("[shot] 已保存 {} ({w}x{h})", path.display()),
                Err(err) => eprintln!("[shot] 截取 {state} 失败: {err}"),
            }
        }
        eprintln!("[shot] 取证会话结束");
        let _ = cx.update(|cx| cx.quit());
    })
    .detach();
}

// ── CoreGraphics FFI(仅用到 C 符号,不引 objc 绑定 crate)───────────────────

#[cfg(target_os = "macos")]
mod cg {
    #![allow(non_snake_case, dead_code)]

    use std::ffi::c_void;

    pub type CFStringRef = *const c_void;
    pub type CFArrayRef = *const c_void;
    pub type CFDictionaryRef = *const c_void;
    pub type CFNumberRef = *const c_void;
    pub type CFDataRef = *const c_void;
    pub type CGDataProviderRef = *const c_void;
    pub type CGImageRef = *const c_void;

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct CGPoint {
        pub x: f64,
        pub y: f64,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct CGSize {
        pub width: f64,
        pub height: f64,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct CGRect {
        pub origin: CGPoint,
        pub size: CGSize,
    }

    // CGWindowListOption
    pub const K_CG_WINDOW_LIST_OPTION_ON_SCREEN_ONLY: u32 = 1 << 0;
    // CFNumberType
    pub const K_CF_NUMBER_SINT64_TYPE: isize = 4;
    pub const K_CF_NUMBER_FLOAT64_TYPE: isize = 6;
    // CFStringEncoding
    pub const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
    // CGBitmapInfo
    pub const K_CG_BITMAP_ALPHA_INFO_MASK: u32 = 0x1F;
    pub const K_CG_BITMAP_BYTE_ORDER_MASK: u32 = 0xF << 12;
    pub const K_CG_BITMAP_BYTE_ORDER_32_LITTLE: u32 = 2 << 12;
    pub const K_CG_IMAGE_ALPHA_NONE: u32 = 0;

    extern "C" {
        pub static kCGWindowNumber: CFStringRef;
        pub static kCGWindowOwnerPID: CFStringRef;
        pub static kCGWindowBounds: CFStringRef;

        pub fn CGWindowListCopyWindowInfo(option: u32, relativeToWindow: u32) -> CFArrayRef;

        pub fn CGImageGetWidth(image: CGImageRef) -> usize;
        pub fn CGImageGetHeight(image: CGImageRef) -> usize;
        pub fn CGImageGetBitsPerPixel(image: CGImageRef) -> usize;
        pub fn CGImageGetBytesPerRow(image: CGImageRef) -> usize;
        pub fn CGImageGetBitmapInfo(image: CGImageRef) -> u32;
        pub fn CGImageGetDataProvider(image: CGImageRef) -> CGDataProviderRef;
        pub fn CGDataProviderCopyData(provider: CGDataProviderRef) -> CFDataRef;

        pub fn CFArrayGetCount(array: CFArrayRef) -> isize;
        pub fn CFArrayGetValueAtIndex(array: CFArrayRef, idx: isize) -> *const c_void;
        pub fn CFDictionaryGetValue(dict: CFDictionaryRef, key: CFStringRef) -> *const c_void;
        pub fn CFNumberGetValue(
            number: CFNumberRef,
            theType: isize,
            valuePtr: *mut c_void,
        ) -> bool;
        pub fn CFDataGetBytePtr(data: CFDataRef) -> *const u8;
        pub fn CFDataGetLength(data: CFDataRef) -> isize;
        pub fn CFStringCreateWithCString(
            alloc: *const c_void,
            c_str: *const u8,
            encoding: u32,
        ) -> CFStringRef;
        pub fn CFRetain(cf: *const c_void);
        pub fn CFRelease(cf: *const c_void);
    }
}

#[cfg(target_os = "macos")]
/// 从 CFDictionary 读 CFNumber 为 f64。
#[allow(dead_code)]
unsafe fn dict_f64(dict: cg::CFDictionaryRef, key: &str) -> Option<f64> {
    let c_key = key.as_bytes();
    let cf_key = cg::CFStringCreateWithCString(
        std::ptr::null(),
        c_key.as_ptr(),
        cg::K_CF_STRING_ENCODING_UTF8,
    );
    if cf_key.is_null() {
        return None;
    }
    let num = cg::CFDictionaryGetValue(dict, cf_key) as cg::CFNumberRef;
    let mut out = 0f64;
    let ok = !num.is_null()
        && cg::CFNumberGetValue(
            num,
            cg::K_CF_NUMBER_FLOAT64_TYPE,
            &mut out as *mut f64 as *mut c_void,
        );
    cg::CFRelease(cf_key);
    ok.then_some(out)
}

/// 找本进程主窗口的 CGWindowID。
/// 仅读窗口元数据(windowNumber/PID,不截内容),不需要屏幕录制权限。
/// - 用 option 0(含离屏)枚举:实测 macOS 26 上进程自身的 onScreenOnly
///   枚举永远不含自己的窗口(外部进程观察则包含);
/// - 自身窗口的 bounds 字段会被 redact,因此**不能**按尺寸挑——
///   CGWindowListCopyWindowInfo 返回按前后(z)排序,取第一个本进程窗口
///   即最前面的(激活后的主窗口)。
#[cfg(target_os = "macos")]
unsafe fn find_our_window() -> Option<u32> {
    let list = cg::CGWindowListCopyWindowInfo(0, 0);
    if list.is_null() {
        eprintln!("[shot] CGWindowListCopyWindowInfo 返回 null");
        return None;
    }
    let count = cg::CFArrayGetCount(list);
    let mut result = None;
    for i in 0..count {
        let dict = cg::CFArrayGetValueAtIndex(list, i) as cg::CFDictionaryRef;
        if dict.is_null() {
            continue;
        }
        let pid_num = cg::CFDictionaryGetValue(dict, cg::kCGWindowOwnerPID);
        if pid_num.is_null() {
            continue;
        }
        let mut pid: i64 = 0;
        if !cg::CFNumberGetValue(
            pid_num as cg::CFNumberRef,
            cg::K_CF_NUMBER_SINT64_TYPE,
            &mut pid as *mut i64 as *mut c_void,
        ) || pid as u32 != std::process::id()
        {
            continue;
        }
        let wid_num = cg::CFDictionaryGetValue(dict, cg::kCGWindowNumber);
        let mut wid: i64 = 0;
        if wid_num.is_null()
            || !cg::CFNumberGetValue(
                wid_num as cg::CFNumberRef,
                cg::K_CF_NUMBER_SINT64_TYPE,
                &mut wid as *mut i64 as *mut c_void,
            )
        {
            continue;
        }
        if std::env::var("T2F_SHOT_DEBUG").is_ok() {
            eprintln!("[shot] 自有窗口(第 {i} 项)windowNumber={wid}");
        }
        result = Some(wid as u32);
        break; // 第一个 = 最前
    }
    cg::CFRelease(list);
    result
}

// ── ScreenCaptureKit 截取(自有窗口免 TCC 屏幕录制权限)──────────────────────
//
// 背景调研结论(2026-08,macOS 26 / darwin 25):
// - `CGWindowListCreateImage` 头文件已 obsolete,运行时恒返回 NULL(dlsym 可
//   解析但实测无输出);
// - `screencapture -l<id>` 需要宿主终端的屏幕录制权限(TCC),CLI 环境被拒;
// - ScreenCaptureKit 的 `getCurrentProcessShareableContent` 官方注释明确:
//   "available to capture by current process without user consent via TCC",
//   截自有窗口无需任何权限 —— 本模块即走此路。

#[cfg(target_os = "macos")]
mod sck {
    #![allow(non_snake_case, non_camel_case_types, dead_code, clashing_extern_declarations)]

    use std::ffi::c_void;

    pub type id = *mut c_void;
    pub type Sel = *const c_void;

    #[link(name = "objc")]
    extern "C" {
        pub fn objc_getClass(name: *const u8) -> id;
        pub fn sel_registerName(name: *const u8) -> Sel;

        #[link_name = "objc_msgSend"]
        pub fn msg_id_id(receiver: id, sel: Sel, arg: id) -> id;
        #[link_name = "objc_msgSend"]
        pub fn msg_id_id_id(receiver: id, sel: Sel, a: id, b: id) -> id;
        #[link_name = "objc_msgSend"]
        pub fn msg_id(receiver: id, sel: Sel) -> id;
        #[link_name = "objc_msgSend"]
        pub fn msg_u32(receiver: id, sel: Sel) -> u32;
        #[link_name = "objc_msgSend"]
        pub fn msg_void_ptr(receiver: id, sel: Sel, arg: *const c_void);
        #[link_name = "objc_msgSend"]
        pub fn msg_id_id_ptr(receiver: id, sel: Sel, a: id, b: id, block: *const c_void);
        #[link_name = "objc_msgSend"]
        pub fn msg_activate(receiver: id, sel: Sel, options: u64) -> bool;
        #[link_name = "objc_msgSend"]
        pub fn msg_set_level(receiver: id, sel: Sel, level: i64);
        #[link_name = "objc_msgSend"]
        pub fn msg_void(receiver: id, sel: Sel);
    }

    // 让链接器带上 ScreenCaptureKit(类符号在运行时经 objc_getClass 解析;
    // 锚定该框架导出的真实符号 SCStreamErrorDomain)
    #[link(name = "ScreenCaptureKit", kind = "framework")]
    extern "C" {
        static SCStreamErrorDomain: *const c_void;
    }

    pub fn framework_anchor() -> *const c_void {
        unsafe { SCStreamErrorDomain }
    }
}

#[cfg(target_os = "macos")]
use std::sync::mpsc;

/// 调用一个以 (obj, err) 回调的 async Objective-C 方法并阻塞等待结果。
///
/// Block 用依赖树内已有的 `block` crate 构造(真·堆块,含 copy/dispose
/// 助手;手写全局块会破坏 ScreenCaptureKit 内部的块簿记,实测导致
/// `_Block_release` 释放已捕获对象时崩溃,见 T1_FINDINGS.md)。
#[cfg(target_os = "macos")]
unsafe fn sck_call_async(
    receiver: sck::id,
    selector: &str,
    args: &[sck::id],
    timeout: std::time::Duration,
) -> Result<(*mut std::ffi::c_void, *mut std::ffi::c_void), String> {
    let (tx, rx) = mpsc::channel();
    let block = block::ConcreteBlock::new(move |result: sck::id, error: sck::id| {
        // 回调在后台队列的 autorelease pool 内运行,池排空后对象即悬垂;
        // 先 retain 再跨线程送回主线程
        if !result.is_null() {
            cg::CFRetain(result);
        }
        if !error.is_null() {
            cg::CFRetain(error);
        }
        let _ = tx.send((result, error));
    });
    let rc_block: block::RcBlock<(sck::id, sck::id), ()> = block.copy();
    let block_ptr: *const std::ffi::c_void =
        (&*rc_block) as *const block::Block<(sck::id, sck::id), ()> as *const std::ffi::c_void;

    let sel = sck_sel(selector);
    match args.len() {
        0 => sck::msg_void_ptr(receiver, sel, block_ptr),
        2 => sck::msg_id_id_ptr(receiver, sel, args[0], args[1], block_ptr),
        n => return Err(format!("未支持的参数个数 {n}")),
    }
    // rc_block 在等待期间保持存活;返回即释放(Block_release)
    rx.recv_timeout(timeout)
        .map_err(|_| format!("等待 {selector} 回调超时"))
}

/// 截取本进程主窗口并写 PNG。返回 (宽, 高)(像素)。
#[cfg(target_os = "macos")]
pub fn capture_window_png(path: &Path) -> Result<(u32, u32), String> {
    unsafe {
        // 触发 ScreenCaptureKit 链接锚点
        let _ = sck::framework_anchor();
        eprintln!("[shot] step1: 获取 SCShareableContent…");

        // 1) SCShareableContent(当前进程;官方注释:免 TCC 屏幕录制权限)
        let content_cls = sck_cls("SCShareableContent")?;
        eprintln!("[shot] step1 class={:?}", content_cls);
        let (content, err) = sck_call_async(
            content_cls,
            "getCurrentProcessShareableContentWithCompletionHandler:",
            &[],
            std::time::Duration::from_secs(10),
        )?;
        eprintln!("[shot] step1 done: content={:?} err={:?}", content, err);
        if content.is_null() {
            return Err(format!("获取 ShareableContent 失败(err={err:?})"));
        }

        // 2) 取窗口:优先按 CGWindowID 匹配(仅读 windowNumber 元数据;
        // 注:macOS 26 上自身进程的窗口枚举会被 redact 掉 bounds,所以
        // find_our_window 只比较编号,不比较尺寸);失败则退化为首个窗口。
        eprintln!("[shot] step2: 取 windows 属性…");
        let windows = sck::msg_id(content, sck_sel("windows"));
        eprintln!("[shot] step2: windows={:?}", windows);
        if windows.is_null() {
            return Err("ShareableContent.windows 为空".to_string());
        }
        let count = cg::CFArrayGetCount(windows);
        eprintln!("[shot] step2: count={count}");
        let mut target: sck::id = std::ptr::null_mut();
        eprintln!("[shot] step2: 开始 find_our_window…");
        let own = find_our_window();
        eprintln!("[shot] step2: find_our_window={own:?}");
        if let Some(window_id) = own {
            for i in 0..count {
                let w = cg::CFArrayGetValueAtIndex(windows, i);
                if w.is_null() {
                    continue;
                }
                eprintln!("[shot] step2: win[{i}] 对象读取…");
                let wid = sck::msg_u32(w as sck::id, sck_sel("windowID"));
                eprintln!("[shot] step2: win[{i}] windowID={wid}");
                if wid == window_id {
                    target = w as sck::id;
                    break;
                }
            }
        }
        if target.is_null() {
            // 退化:窗口编号匹配不上(如 CG 枚举被 redact)→ 取第一个窗口
            if count == 0 {
                return Err("ShareableContent.windows 为空数组".to_string());
            }
            target = cg::CFArrayGetValueAtIndex(windows, 0) as sck::id;
            if std::env::var("T2F_SHOT_DEBUG").is_ok() {
                eprintln!("[shot] windowID 匹配失败,退化取 SCShareableContent 第一个窗口");
            }
        }

        eprintln!("[shot] step3: target={:?}", target);
        // 3) SCContentFilter(desktopIndependentWindow)
        let filter = sck::msg_id_id(
            sck::msg_id(sck_cls("SCContentFilter")?, sck_sel("alloc")),
            sck_sel("initWithDesktopIndependentWindow:"),
            target,
        );
        if filter.is_null() {
            return Err("创建 SCContentFilter 失败".to_string());
        }

        // 4) SCStreamConfiguration(默认值 = 窗口原始尺寸)
        let config = sck::msg_id(
            sck::msg_id(sck_cls("SCStreamConfiguration")?, sck_sel("alloc")),
            sck_sel("init"),
        );
        if config.is_null() {
            return Err("创建 SCStreamConfiguration 失败".to_string());
        }

        eprintln!("[shot] step5: captureImage…");
        // 5) SCScreenshotManager.captureImageWithFilter:configuration:completionHandler:
        let (image, err) = sck_call_async(
            sck_cls("SCScreenshotManager")?,
            "captureImageWithFilter:configuration:completionHandler:",
            &[filter, config],
            std::time::Duration::from_secs(10),
        )?;
        eprintln!("[shot] step5 done: image={:?} err={:?}", image, err);
        if image.is_null() {
            return Err(format!("captureImage 返回空(err={err:?})"));
        }
        let result = decode_and_save(image as cg::CGImageRef, path);
        cg::CFRelease(image);
        result
    }
}

#[cfg(target_os = "macos")]
unsafe fn sck_cls(name: &str) -> Result<sck::id, String> {
    let mut c = name.as_bytes().to_vec();
    c.push(0);
    let cls = sck::objc_getClass(c.as_ptr());
    if cls.is_null() {
        Err(format!("ObjC 类不存在: {name}(未链接 ScreenCaptureKit?)"))
    } else {
        Ok(cls)
    }
}

#[cfg(target_os = "macos")]
unsafe fn sck_sel(name: &str) -> sck::Sel {
    let mut c = name.as_bytes().to_vec();
    c.push(0);
    sck::sel_registerName(c.as_ptr())
}

#[cfg(target_os = "macos")]
#[allow(dead_code)]
/// AppKit:让主窗口真正上屏。
/// CLI 启动的进程在 macOS 14+ 不允许自抢前台(activateIgnoringOtherApps
/// 被系统忽略),但 `orderFrontRegardless` 不受激活状态约束,一定把窗口
/// 排到窗口服务器上(必要时再抬 Floating 层兜底)。
fn ensure_window_on_screen() {
    unsafe {
        let debug = std::env::var("T2F_SHOT_DEBUG").is_ok();
        let cls = sck::objc_getClass(b"NSApplication\0".as_ptr());
        if cls.is_null() {
            eprintln!("[shot] NSApplication 类缺失");
            return;
        }
        let app = sck::msg_id(cls, sck_sel("sharedApplication"));
        if app.is_null() {
            eprintln!("[shot] sharedApplication 为空");
            return;
        }

        // 1) 最好努力的自激活(多数情况被系统忽略)
        if let Ok(activated) = std::panic::catch_unwind(|| {
            let cls = sck::objc_getClass(b"NSRunningApplication\0".as_ptr());
            let cur = sck::msg_id(cls, sck_sel("currentApplication"));
            !cur.is_null() && sck::msg_activate(cur, sck_sel("activateWithOptions:"), 1 << 1)
        }) {
            if debug {
                eprintln!("[shot] 自激活 activateWithOptions → {activated}");
            }
        }

        // 2) orderFrontRegardless:对 key/main/全部窗口都试一遍
        let _ = std::panic::catch_unwind(|| {
            let candidates = [
                sck::msg_id(app, sck_sel("keyWindow")),
                sck::msg_id(app, sck_sel("mainWindow")),
            ];
            for win in candidates {
                if !win.is_null() {
                    sck::msg_void(win, sck_sel("orderFrontRegardless"));
                }
            }
            let all = sck::msg_id(app, sck_sel("windows"));
            if !all.is_null() {
                let count = cg::CFArrayGetCount(all);
                if debug {
                    eprintln!("[shot] NSApp.windows 数量: {count}");
                }
                for i in 0..count {
                    let w = cg::CFArrayGetValueAtIndex(all, i);
                    if !w.is_null() {
                        sck::msg_void(w as sck::id, sck_sel("orderFrontRegardless"));
                    }
                }
            }
        });

        // 3) 仍不在屏上 → 抬 Floating 层
        let on_screen = std::panic::catch_unwind(|| find_our_window().is_some())
            .unwrap_or(false);
        if !on_screen {
            if debug {
                eprintln!("[shot] orderFrontRegardless 后仍未上屏,尝试 setLevel(3)");
            }
            let _ = std::panic::catch_unwind(|| {
                let all = sck::msg_id(app, sck_sel("windows"));
                if !all.is_null() {
                    let count = cg::CFArrayGetCount(all);
                    for i in 0..count {
                        let w = cg::CFArrayGetValueAtIndex(all, i);
                        if !w.is_null() {
                            sck::msg_set_level(w as sck::id, sck_sel("setLevel:"), 3);
                        }
                    }
                }
            });
        } else if debug {
            eprintln!("[shot] orderFrontRegardless 后已上屏");
        }
    }
}

/// 解码 CGImage 像素 → RGBA → PNG。
#[cfg(target_os = "macos")]
unsafe fn decode_and_save(image: cg::CGImageRef, path: &Path) -> Result<(u32, u32), String> {
    let width = cg::CGImageGetWidth(image) as u32;
    let height = cg::CGImageGetHeight(image) as u32;
    let bpp = cg::CGImageGetBitsPerPixel(image);
    let bytes_per_row = cg::CGImageGetBytesPerRow(image);
    if bpp != 32 {
        return Err(format!("意外的位深 {bpp}(预期 32)"));
    }

    let info = cg::CGImageGetBitmapInfo(image);
    let alpha_info = info & cg::K_CG_BITMAP_ALPHA_INFO_MASK;
    let byte_order = info & cg::K_CG_BITMAP_BYTE_ORDER_MASK;
    // 内存中 R/G/B/A 的字节下标(见 CGBitmapInfo 语义推导,T1_FINDINGS.md)
    let (ri, gi, bi, ai) = match (byte_order, alpha_info) {
        (cg::K_CG_BITMAP_BYTE_ORDER_32_LITTLE, 1 | 2) => (2, 1, 0, 3), // BGRA
        (cg::K_CG_BITMAP_BYTE_ORDER_32_LITTLE, 3 | 4) => (3, 2, 1, 0), // ABGR
        (_, 1 | 2) => (1, 2, 3, 0),                                    // ARGB(大端)
        _ => (0, 1, 2, 3),                                             // RGBA(大端)
    };

    let provider = cg::CGImageGetDataProvider(image);
    let data = cg::CGDataProviderCopyData(provider);
    if data.is_null() {
        return Err("CGDataProviderCopyData 失败".to_string());
    }
    let len = cg::CFDataGetLength(data) as usize;
    let ptr = cg::CFDataGetBytePtr(data);
    if ptr.is_null() || len < bytes_per_row * height as usize {
        cg::CFRelease(data);
        return Err("像素数据为空或长度不足".to_string());
    }

    let no_alpha = alpha_info == cg::K_CG_IMAGE_ALPHA_NONE;
    let mut rgba = Vec::with_capacity((width as usize) * (height as usize) * 4);
    let slice = std::slice::from_raw_parts(ptr, len);
    for y in 0..height as usize {
        let row = &slice[y * bytes_per_row..y * bytes_per_row + width as usize * 4];
        for px in row.chunks_exact(4) {
            let a = if no_alpha { 255 } else { px[ai] };
            if a == 0 {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            } else if a == 255 || no_alpha {
                rgba.extend_from_slice(&[px[ri], px[gi], px[bi], a]);
            } else {
                // 预乘 alpha 还原为直通
                let f = 255.0 / a as f32;
                let r = ((px[ri] as f32) * f).round().min(255.0) as u8;
                let g = ((px[gi] as f32) * f).round().min(255.0) as u8;
                let b = ((px[bi] as f32) * f).round().min(255.0) as u8;
                rgba.extend_from_slice(&[r, g, b, a]);
            }
        }
    }
    cg::CFRelease(data);

    let img = RgbaImage::from_raw(width, height, rgba)
        .ok_or_else(|| "像素缓冲尺寸不匹配".to_string())?;
    img.save(path)
        .map_err(|err| format!("写 PNG 失败: {err}"))?;
    Ok((width, height))
}

// Windows/Linux:截图取证依赖 ScreenCaptureKit/CoreGraphics,不可用
#[cfg(not(target_os = "macos"))]
pub fn capture_window_png(_path: &Path) -> Result<(u32, u32), String> {
    Err("截图取证仅支持 macOS".to_string())
}
