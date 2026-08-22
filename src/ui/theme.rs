//! 设计 token(全部数值照抄 docs/SOURCE_SPEC.md 第 6 章,未做任何四舍五入/自创)。
//!
//! 颜色注意:**amber 系采用"运行时生效值"**(源 index.css 中 `--amber-*` 被声明两次,
//! 后声明覆盖;见 SPEC 文首陷阱表)。例如 `--amber-500` 生效值为 `#f59e0b`,
//! 而非 DESIGN.md 宣称的 `#FFAE00`。
//!
//! 常量命名对应 CSS 变量名:`--slate-50` → [`SLATE_50`]、`--bg-app` → [`BG_APP`]。
//! 颜色统一用 `Rgba`(hex 低 2 位为 alpha),按 gpui 约定可 `impl Into<Hsla>` 直接传给
//! `.bg()` / `.text_color()` / `.border_color()`。

#![allow(dead_code)] // token 表/图标枚举/服务辅助为后续页面 agent 预留,当前未全部使用

use gpui::{BoxShadow, Hsla, Pixels, Rgba, point, px};

/// 编译期把 0xRRGGBBAA 字面量转为 `Rgba` 常量(等价 `rgba()` 但可用于 const)。
macro_rules! color {
    ($name:ident = $hex:expr) => {
        pub const $name: Rgba = Rgba {
            r: ((($hex as u32) >> 24) & 0xff) as f32 / 255.0,
            g: ((($hex as u32) >> 16) & 0xff) as f32 / 255.0,
            b: ((($hex as u32) >> 8) & 0xff) as f32 / 255.0,
            a: (($hex as u32) & 0xff) as f32 / 255.0,
        };
    };
}

// ── 6.1 中性色(Slate)────────────────────────────────────────────────────────

color!(SLATE_50 = 0xf8fafcff);
color!(SLATE_100 = 0xf1f5f9ff);
color!(SLATE_200 = 0xe2e8f0ff);
color!(SLATE_300 = 0xcbd5e1ff);
color!(SLATE_400 = 0x94a3b8ff);
color!(SLATE_500 = 0x64748bff);
color!(SLATE_600 = 0x475569ff);
color!(SLATE_700 = 0x334155ff);
color!(SLATE_800 = 0x1e293bff);
color!(SLATE_900 = 0x0f172aff);
color!(SLATE_950 = 0x020617ff);

// ── 6.2 琥珀色(amber,运行时生效值)─────────────────────────────────────────
// 注意:--amber-950 不存在(PLACEHOLDER 文字色陷阱见 SPEC 7.9,等效 #0f172a)。

color!(AMBER_50 = 0xfffbefff);
color!(AMBER_100 = 0xfef3c7ff);
color!(AMBER_200 = 0xfde68aff);
color!(AMBER_300 = 0xffdc80ff);
color!(AMBER_400 = 0xffc533ff);
color!(AMBER_500 = 0xf59e0bff); // 品牌主色生效值(被第二次声明覆盖)
color!(AMBER_600 = 0xd97706ff);
color!(AMBER_700 = 0xb45309ff);
color!(AMBER_800 = 0xb36900ff);
color!(AMBER_900 = 0x7d4600ff);

// ── 6.3 语义色───────────────────────────────────────────────────────────────

color!(EMERALD_50 = 0xecfdf5ff);
color!(EMERALD_100 = 0xd1fae5ff);
color!(EMERALD_200 = 0xa7f3d0ff);
color!(EMERALD_500 = 0x10b981ff);
color!(EMERALD_600 = 0x059669ff);
color!(EMERALD_700 = 0x047857ff);

color!(ROSE_50 = 0xfff1f2ff);
color!(ROSE_100 = 0xffe4e6ff);
color!(ROSE_200 = 0xfecdd3ff);
color!(ROSE_500 = 0xf43f5eff);
color!(ROSE_600 = 0xe11d48ff);
color!(ROSE_700 = 0xbe123cff);

color!(SKY_50 = 0xf0f9ffff);
color!(SKY_100 = 0xe0f2feff);
color!(SKY_200 = 0xbae6fdff);
color!(SKY_300 = 0xbae6fdff); // 源定义与 200 同值,用于日志正文色
color!(SKY_500 = 0x0ea5e9ff);
color!(SKY_600 = 0x0284c7ff);
color!(SKY_700 = 0x0369a1ff);

// ── 6.4 功能映射─────────────────────────────────────────────────────────────

color!(BG_APP = 0xf8fafcff); // = --slate-50
color!(BG_SURFACE = 0xffffffff); // 白
color!(BG_SUBTLE = 0xf1f5f9ff); // = --slate-100
color!(BG_MUTED = 0xe2e8f0ff); // = --slate-200
color!(BORDER_SUBTLE = 0xe2e8f0ff); // = --slate-200
color!(BORDER_DEFAULT = 0xcbd5e1ff); // = --slate-300
color!(BORDER_FOCUS = 0xffae00ff); // 硬编码,非 --amber-500
color!(TEXT_PRIMARY = 0x0f172aff); // = --slate-900
color!(TEXT_SECONDARY = 0x334155ff); // = --slate-700
color!(TEXT_MUTED = 0x64748bff); // = --slate-500
color!(TEXT_TERTIARY = 0x94a3b8ff); // = --slate-400
color!(TEXT_ON_PRIMARY = 0xffffffff);

/// `--bg-overlay`: rgba(15, 23, 42, 0.55)
pub const BG_OVERLAY: Hsla = Hsla {
    h: 0.5861111,
    s: 0.1904762,
    l: 0.1098039,
    a: 0.55,
};

/// 未定义变量陷阱的等效值(SPEC 7.9):`var(--amber-950)` / `var(--rose-800)` /
/// `var(--sky-800)` 均继承 `--text-primary` = #0f172a。
pub const INHERITED_TEXT: Rgba = SLATE_900;

/// 输入框聚焦光晕:`box-shadow: 0 0 0 3px rgba(255, 174, 0, 0.2)`(硬编码,非 token)
pub const INPUT_FOCUS_GLOW: Hsla = Hsla {
    h: 0.0972222,
    s: 1.0,
    l: 0.5,
    a: 0.2,
};

// ── 6.6 圆角─────────────────────────────────────────────────────────────────

pub const RADIUS_XS: Pixels = px(4.0);
pub const RADIUS_SM: Pixels = px(6.0);
pub const RADIUS_MD: Pixels = px(8.0);
pub const RADIUS_LG: Pixels = px(12.0);
pub const RADIUS_XL: Pixels = px(16.0);
/// `--radius-full: 9999px`;gpui 画圆角传该值即可得到胶囊/圆形
pub const RADIUS_FULL: Pixels = px(9999.0);

// ── 6.7 阴影(token)──────────────────────────────────────────────────────────
// 多层阴影按 CSS 书写顺序排列;spread 为负数照抄。
// 颜色 rgba(15,23,42,x) 即 #0f172a 加透明度。

const SHADOW_INK: Hsla = Hsla {
    h: 0.5861111,
    s: 0.1904762,
    l: 0.1098039,
    a: 1.0,
};

fn ink(alpha: f32) -> Hsla {
    Hsla { a: alpha, ..SHADOW_INK }
}

/// `--shadow-xs: 0 1px 2px 0 rgba(15,23,42,0.05)`
pub fn shadow_xs() -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: ink(0.05),
        offset: point(px(0.0), px(1.0)),
        blur_radius: px(2.0),
        spread_radius: px(0.0),
    }]
}

/// `--shadow-sm: 0 1px 3px 0 rgba(15,23,42,0.08), 0 1px 2px -1px rgba(15,23,42,0.08)`
pub fn shadow_sm() -> Vec<BoxShadow> {
    vec![
        BoxShadow {
            color: ink(0.08),
            offset: point(px(0.0), px(1.0)),
            blur_radius: px(3.0),
            spread_radius: px(0.0),
        },
        BoxShadow {
            color: ink(0.08),
            offset: point(px(0.0), px(1.0)),
            blur_radius: px(2.0),
            spread_radius: px(-1.0),
        },
    ]
}

/// `--shadow-md: 0 4px 6px -1px rgba(15,23,42,0.08), 0 2px 4px -2px rgba(15,23,42,0.06)`
pub fn shadow_md() -> Vec<BoxShadow> {
    vec![
        BoxShadow {
            color: ink(0.08),
            offset: point(px(0.0), px(4.0)),
            blur_radius: px(6.0),
            spread_radius: px(-1.0),
        },
        BoxShadow {
            color: ink(0.06),
            offset: point(px(0.0), px(2.0)),
            blur_radius: px(4.0),
            spread_radius: px(-2.0),
        },
    ]
}

/// `--shadow-lg: 0 10px 15px -3px rgba(15,23,42,0.08), 0 4px 6px -4px rgba(15,23,42,0.04)`
pub fn shadow_lg() -> Vec<BoxShadow> {
    vec![
        BoxShadow {
            color: ink(0.08),
            offset: point(px(0.0), px(10.0)),
            blur_radius: px(15.0),
            spread_radius: px(-3.0),
        },
        BoxShadow {
            color: ink(0.04),
            offset: point(px(0.0), px(4.0)),
            blur_radius: px(6.0),
            spread_radius: px(-4.0),
        },
    ]
}

/// `--shadow-xl: 0 20px 25px -5px rgba(15,23,42,0.1), 0 8px 10px -6px rgba(15,23,42,0.06)`
pub fn shadow_xl() -> Vec<BoxShadow> {
    vec![
        BoxShadow {
            color: ink(0.1),
            offset: point(px(0.0), px(20.0)),
            blur_radius: px(25.0),
            spread_radius: px(-5.0),
        },
        BoxShadow {
            color: ink(0.06),
            offset: point(px(0.0), px(8.0)),
            blur_radius: px(10.0),
            spread_radius: px(-6.0),
        },
    ]
}

// 组件内硬编码阴影(SPEC 6.7 末段):

/// 主按钮常态 `0 1px 2px rgba(0,0,0,0.05)`
pub fn shadow_primary_btn() -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: gpui::black().opacity(0.05),
        offset: point(px(0.0), px(1.0)),
        blur_radius: px(2.0),
        spread_radius: px(0.0),
    }]
}

/// 主按钮悬浮 `0 2px 4px rgba(0,0,0,0.08)`
pub fn shadow_primary_btn_hover() -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: gpui::black().opacity(0.08),
        offset: point(px(0.0), px(2.0)),
        blur_radius: px(4.0),
        spread_radius: px(0.0),
    }]
}

/// 品牌方块 `0 1px 3px rgba(0,0,0,0.1)`
pub fn shadow_brand_tile() -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: gpui::black().opacity(0.1),
        offset: point(px(0.0), px(1.0)),
        blur_radius: px(3.0),
        spread_radius: px(0.0),
    }]
}

/// 激活步骤瓦片 `0 1px 4px rgba(217,133,0,0.25)`
pub fn shadow_step_active() -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: Hsla {
            h: 0.0888889, // #d98500
            s: 1.0,
            l: 0.4294118,
            a: 0.25,
        },
        offset: point(px(0.0), px(1.0)),
        blur_radius: px(4.0),
        spread_radius: px(0.0),
    }]
}

/// ConfirmModal 卡片 `0 12px 36px rgba(15,23,42,0.16)`
pub fn shadow_confirm_modal() -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: ink(0.16),
        offset: point(px(0.0), px(12.0)),
        blur_radius: px(36.0),
        spread_radius: px(0.0),
    }]
}

/// 底部 sticky 导航条 `0 -6px 16px rgba(15,23,42,0.05)`(负 y 偏移)
pub fn shadow_sticky_bar() -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: ink(0.05),
        offset: point(px(0.0), px(-6.0)),
        blur_radius: px(16.0),
        spread_radius: px(0.0),
    }]
}

// ── 6.5 字体 / 6.8 过渡与动画时长───────────────────────────────────────────

/// UI 文本字体(macOS 系统字体,源 --font-sans 链中的 CJK 主力)
pub const FONT_SANS: &str = "PingFang SC";
/// 等宽字体(路径/日志/模板)
pub const FONT_MONO: &str = "Menlo";
/// html 基准字号 14px
pub const FONT_SIZE_BASE: Pixels = px(14.0);
/// 基准行高 1.5(相对值,用于 `.line_height(relative(1.5))`)
pub const LINE_HEIGHT_BASE: f32 = 1.5;

/// `--transition-fast: 150ms`(gpui 无 CSS 过渡;此值用于需要动画时的时长参考)
pub const DURATION_FAST_MS: u64 = 150;
/// `--transition-base: 200ms`
pub const DURATION_BASE_MS: u64 = 200;
/// `--transition-smooth: 300ms`
pub const DURATION_SMOOTH_MS: u64 = 300;
/// animate-spin:1s 线性无限
pub const DURATION_SPIN_MS: u64 = 1000;
/// animate-pulse:2s(透明度 1↔0.6)
pub const DURATION_PULSE_MS: u64 = 2000;
/// 进度条填充过渡 250ms
pub const DURATION_PROGRESS_MS: u64 = 250;
/// fadeIn(遮罩)150ms / scaleUp(模态内容)200ms / ConfirmModal 180ms / 页面切换 220ms
pub const DURATION_FADE_IN_MS: u64 = 150;
pub const DURATION_MODAL_SCALE_MS: u64 = 200;
pub const DURATION_CONFIRM_SCALE_MS: u64 = 180;
pub const DURATION_PAGE_ENTER_MS: u64 = 220;

/// scaleUp 同款缓动曲线 cubic-bezier(0.16, 1, 0.3, 1) 的近似实现(f32 -> f32)。
/// 用于 gpui `Animation::with_easing`。
pub fn ease_scale_up(t: f32) -> f32 {
    // 三次贝塞尔 B(t) 由参数 x 近似:用 x=t 做一次求解足够 UI 用途
    let p0 = 0.0f32;
    let p1 = 0.16f32;
    let p2 = 1.0f32;
    let p3 = 1.0f32;
    let u = t;
    let v = 1.0 - u;
    v * v * v * p0 + 3.0 * v * v * u * p1 + 3.0 * v * u * u * p2 + u * u * u * p3
}

// ── gpui-component 主题接管(把色板换成我们的设计 token)────────────────────

/// 覆盖 gpui-component 全局主题(必须在 `gpui_component::init(cx)` 之后调用一次)。
///
/// 高交互控件(Input/Checkbox 等)的颜色取自该主题;装饰性 UI 一律走本文件的
/// 常量,不经由此处。映射关系(源 token → gpui-component ThemeColor):
/// - `primary` = amber-500(#f59e0b),`primary_foreground` = slate-800(#1e293b,
///   源主按钮文字色),`primary_hover` = amber-600,`primary_active` = amber-700
/// - `background` = 白,`foreground` = text-primary,`border`/`input` = border-default
/// - `ring`(聚焦边框)= border-focus(#ffae00,硬编码值)
/// - `danger` = rose-600、`success` = emerald-600、`info` = sky-600
/// - `muted` = slate-100(禁用输入框底),`muted_foreground` = slate-400
/// - `radius` = radius-md(8px,输入框圆角)
/// - 字体:PingFang SC / Menlo,基准 14px(源 html font-size)
pub fn apply_to_gpui_component(cx: &mut gpui::App) {
    let theme = gpui_component::Theme::global_mut(cx);
    theme.font_family = FONT_SANS.into();
    theme.font_size = FONT_SIZE_BASE;
    theme.mono_font_family = FONT_MONO.into();
    theme.mono_font_size = px(12.5);
    theme.radius = RADIUS_MD;
    theme.radius_lg = RADIUS_XL;
    // 我们自绘卡片/模态,不需要组件库再给输入框默认投影
    theme.shadow = false;

    let c = &mut theme.colors;
    c.primary = AMBER_500.into();
    c.primary_hover = AMBER_600.into();
    c.primary_active = AMBER_700.into();
    c.primary_foreground = SLATE_800.into();

    c.danger = ROSE_600.into();
    c.danger_hover = ROSE_500.into();
    c.danger_active = ROSE_700.into();
    c.danger_foreground = gpui::white();

    c.success = EMERALD_600.into();
    c.success_hover = EMERALD_500.into();
    c.success_active = EMERALD_700.into();
    c.success_foreground = gpui::white();

    c.info = SKY_600.into();
    c.info_hover = SKY_500.into();
    c.info_active = SKY_700.into();
    c.info_foreground = gpui::white();

    c.accent = AMBER_500.into();
    c.accent_foreground = SLATE_800.into();

    c.background = BG_SURFACE.into();
    c.foreground = TEXT_PRIMARY.into();
    c.border = BORDER_DEFAULT.into();
    c.input = BORDER_DEFAULT.into();
    // 源输入框聚焦边框为 var(--amber-500)(#f59e0b);光晕 rgba(255,174,0,0.2)
    // 无法经组件复刻(见 KNOWN_DIFFERENCES),边框取 amber-500 保持像素一致
    c.ring = AMBER_500.into();
    c.caret = AMBER_600.into();
    c.selection = AMBER_100.into();
    c.muted = SLATE_100.into();
    c.muted_foreground = SLATE_400.into();
    c.popover = BG_SURFACE.into();
    c.popover_foreground = TEXT_PRIMARY.into();
    c.title_bar = BG_SURFACE.into();
    c.tab_active = BG_SURFACE.into();
    c.tab_active_foreground = TEXT_PRIMARY.into();
    c.tab_bar = BG_SUBTLE.into();
    c.tab_bar_segmented = BG_SUBTLE.into();
    c.progress_bar = AMBER_500.into();
}
