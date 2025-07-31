// `#![windows_subsystem = "windows"]` 是一个特殊的属性，用于告诉编译器在 Windows 平台上构建一个“窗口”应用程序，
// 而不是“控制台”应用程序。这样，在运行程序时就不会出现一个黑色的命令行窗口。
#![windows_subsystem = "windows"]

// --- 依赖项导入 ---

use anyhow::Result;
use arboard::{Clipboard, ImageData};
use druid::piet::PietImage;
use druid::{
    AppLauncher, BoxConstraints, Color, Data, Env, Event, EventCtx, LayoutCtx, LifeCycle,
    LifeCycleCtx, Menu, MenuItem, PaintCtx, Point, Rect, RenderContext, Size, UpdateCtx, Widget,
    WindowDesc,
};
use image::{DynamicImage, GenericImageView, ImageBuffer, Rgba};
use rfd::MessageDialog;
use std::sync::Arc;
use xcap::Monitor;

// --- 自定义模块 ---
mod qrcode;
use qrcode::scan_qr_code; // 从我们自己的 `qrcode` 模块中导入二维码扫描函数。

/// AppState 结构体定义了应用程序的全部状态。
/// `druid` 框架会在状态发生变化时自动更新 UI。
/// `#[derive(Clone, Data)]` 是 `druid` 的要求，使状态可以被克隆和比较。
#[derive(Clone, Data)]
struct AppState {
    /// 存储屏幕截图的图像数据。
    /// 使用 `Arc<DynamicImage>` 是为了高效共享：
    /// - `Arc` 避免了在每次状态更新时都复制整个图像数据，只复制一个指针。
    /// - `DynamicImage` 是 `image` 库中一种通用的图像类型。
    /// `#[data(same_fn = "PartialEq::eq")]` 告诉 `druid` 如何比较这个字段是否“相同”。
    /// 这里使用指针比较，只有当 `Arc` 指向完全不同的图像时，才认为数据已更改。
    #[data(same_fn = "PartialEq::eq")]
    screenshot: Arc<DynamicImage>,

    /// 一个布尔标志，用于表示用户当前是否正在按住鼠标左键进行选择。
    is_selecting: bool,

    /// 当用户开始选择时，鼠标按下的起始位置。
    start_pos: Point,

    /// 鼠标在拖动过程中的当前位置。
    current_pos: Point,

    /// 用户完成选择后，最终确定的选区矩形。
    /// 使用 `Option` 是因为在程序启动或完成一次操作后，可能没有活动的选区。
    selection_rect: Option<Rect>,
}

impl AppState {
    /// 根据鼠标的起始点和当前点计算出当前的选区矩形。
    /// `.abs()` 确保即使用户从右下向左上拖动，也能得到一个有效的矩形（宽度和高度为正）。
    fn get_current_selection(&self) -> Rect {
        Rect::from_points(self.start_pos, self.current_pos).abs()
    }

    /// 根据最终确定的 `selection_rect` 从原始截图中裁剪出图像。
    /// 返回一个 `Option`，因为 `selection_rect` 可能为 `None`。
    fn crop_image(&self) -> Option<ImageBuffer<Rgba<u8>, Vec<u8>>> {
        // `map` 方法会在 `self.selection_rect` 是 `Some(rect)` 时执行闭包。
        self.selection_rect.map(|rect| {
            // `crop_imm` 是一个不可变裁剪操作，返回一个新的图像。
            // 坐标需要转换为 u32 类型，并确保不超出图像边界。
            self.screenshot
                .crop_imm(
                    rect.x0.max(0.0) as u32,
                    rect.y0.max(0.0) as u32,
                    rect.width() as u32,
                    rect.height() as u32,
                )
                .to_rgba8() // 将裁剪后的图像转换为 `Rgba<u8>` 格式，这是最通用的格式。
        })
    }
}

// --- 剪贴板辅助函数 ---

/// 将 `image` 库的图像缓冲区复制到系统剪贴板。
fn copy_image_to_clipboard(image: &ImageBuffer<Rgba<u8>, Vec<u8>>) -> Result<()> {
    let mut clipboard = Clipboard::new()?;
    let image_data = ImageData {
        width: image.width() as usize,
        height: image.height() as usize,
        // `.as_raw()` 获取图像的原始字节数据，`.into()` 将其转换为 `arboard` 需要的 `Cow<[u8]>` 类型。
        bytes: image.as_raw().into(),
    };
    clipboard.set_image(image_data)?;
    Ok(())
}

/// 将文本复制到系统剪贴板。
fn copy_text_to_clipboard(text: &str) -> Result<()> {
    let mut clipboard = Clipboard::new()?;
    clipboard.set_text(text.to_string())?;
    Ok(())
}

// --- 自定义 Widget ---

/// `ScreenshotWidget` 是我们应用的主界面，负责显示截图和处理用户交互。
struct ScreenshotWidget {
    /// 缓存转换后的图像，用于 `druid` 的渲染。
    /// 这是一个性能优化：避免在每一帧都将 `image::DynamicImage` 转换为 `druid` 的 `PietImage`。
    /// 转换只在首次绘制或截图本身改变时发生。
    cached_image: Option<PietImage>,

    /// 缓存上一次绘制的选择框矩形。
    /// 这是另一个性能优化，用于在鼠标移动时只重绘变化的区域（脏矩形），而不是整个屏幕。
    previous_rect: Option<Rect>,
}

impl Widget<AppState> for ScreenshotWidget {
    /// `event` 方法处理所有用户输入事件，如鼠标点击、移动、键盘按键等。
    fn event(&mut self, ctx: &mut EventCtx, event: &Event, data: &mut AppState, _env: &Env) {
        match event {
            // --- 鼠标左键按下：开始选择 ---
            Event::MouseDown(e) if e.button.is_left() => {
                data.is_selecting = true;       // 设置选择状态为 true
                data.start_pos = e.pos;         // 记录选择的起始点
                data.current_pos = e.pos;       // 当前点也设为起始点
                data.selection_rect = None;     // 清除上一次的最终选区
                
                // 缓存当前的矩形，用于下一次MouseMove事件计算脏区域
                self.previous_rect = Some(data.get_current_selection());
                ctx.request_paint(); // 请求重绘，以显示初始的选择状态
            }

            // --- 鼠标拖动：更新选择区域 ---
            Event::MouseMove(e) if data.is_selecting => {
                // `unwrap_or_else` 确保即使 `previous_rect` 为 `None` 也有一个有效的旧矩形
                let old_rect = self.previous_rect.unwrap_or_else(|| data.get_current_selection());
                data.current_pos = e.pos;
                let new_rect = data.get_current_selection();
                self.previous_rect = Some(new_rect);

                // **性能优化**: 只重绘变化的区域
                // `union` 计算包含旧矩形和新矩形的最小矩形
                // `inset` 稍微扩大一点区域，确保边框也能被完全重绘
                let dirty_region = old_rect.union(new_rect).inset(-2.0);
                ctx.request_paint_rect(dirty_region);
            }
            
            // --- 鼠标左键抬起：完成选择并显示菜单 ---
            Event::MouseUp(e) if e.button.is_left() => {
                if data.is_selecting {
                    data.is_selecting = false; // 结束选择状态

                    let selection = data.get_current_selection();
                    // 只有当选区足够大时（避免误触），才认为是有效选择
                    if selection.width() > 1.0 && selection.height() > 1.0 {
                        data.selection_rect = Some(selection); // 保存最终选区
                        ctx.show_context_menu(make_context_menu(), e.pos); // 在鼠标位置显示右键菜单
                    } else {
                        data.selection_rect = None; // 选区太小，视为无效，清除它
                    }
                    ctx.request_paint(); // 请求重绘，以移除选择框的边框，并显示最终的遮罩
                }
            }

            // --- 鼠标右键按下：直接显示菜单 ---
            // 这提供了一种快捷方式，例如对整个屏幕进行操作。
            Event::MouseDown(e) if e.button.is_right() => {
                // 如果当前没有一个已经确定的选区...
                if data.selection_rect.is_none() {
                    // ...那么我们将整个屏幕作为选区
                    let screen_rect = ctx.size().to_rect();
                    data.selection_rect = Some(screen_rect);
                    // 请求重绘，以更新视觉状态（虽然全屏选区看不出遮罩）
                    ctx.request_paint();
                }
                // 无论之前是否有选区，现在都在鼠标位置显示菜单
                ctx.show_context_menu(make_context_menu(), e.pos);
            }
            
            // --- 键盘事件：按 Escape 键退出程序 ---
            Event::KeyDown(key_event) if key_event.key == druid::keyboard_types::Key::Escape => {
                // 发送一个全局命令来关闭应用程序
                ctx.submit_command(druid::commands::QUIT_APP);
            }

            _ => {} // 忽略其他所有事件
        }
    }
    
    /// `lifecycle` 方法处理窗口生命周期事件（如窗口创建、获得/失去焦点）。这里未使用。
    fn lifecycle(
        &mut self,
        _ctx: &mut LifeCycleCtx,
        _event: &LifeCycle,
        _data: &AppState,
        _env: &Env,
    ) {}

    /// `update` 方法在 `AppState` 数据发生改变时被调用。
    fn update(&mut self, ctx: &mut UpdateCtx, old_data: &AppState, data: &AppState, _env: &Env) {
        // 使用 `Arc::ptr_eq` 高效地检查截图数据是否真的改变了。
        // 如果改变了，就清除缓存的 `PietImage`，强制在下一次 `paint` 时重新生成。
        if !Arc::ptr_eq(&old_data.screenshot, &data.screenshot) {
            self.cached_image = None;
            ctx.request_paint();
        }
    }

    /// `layout` 方法决定 widget 的大小。
    fn layout(&mut self, _ctx: &mut LayoutCtx, _bc: &BoxConstraints, data: &AppState, _env: &Env) -> Size {
        // Widget 的大小就是截图的尺寸，铺满整个窗口。
        let (w, h) = data.screenshot.dimensions();
        Size::new(w as f64, h as f64)
    }


    /// `paint` 方法负责将 widget 绘制到屏幕上。
    fn paint(&mut self, ctx: &mut PaintCtx, data: &AppState, _env: &Env) {
        let size = ctx.size();
        let full_rect = size.to_rect();

        // --- 绘制背景截图 ---
        // 检查是否有缓存的 PietImage
        if self.cached_image.is_none() {
            // 如果没有，就从 AppState 的 screenshot 创建一个新的 PietImage
            let (w, h) = data.screenshot.dimensions();
            let buf = data.screenshot.to_rgba8();
            self.cached_image = ctx
                .make_image(w as usize, h as usize, buf.as_raw(), druid::piet::ImageFormat::RgbaSeparate)
                .ok();
        }
        // 如果 PietImage 存在（无论是刚创建的还是之前缓存的），就绘制它
        if let Some(img) = &self.cached_image {
            // 将图像绘制到整个 widget 区域
            ctx.draw_image(img, full_rect, druid::piet::InterpolationMode::NearestNeighbor);
        }

        // --- 绘制选区和遮罩 ---
        // 确定当前应该高亮显示的矩形。
        // 如果有最终确定的选区 (`selection_rect`)，就用它。
        // 否则，如果正在选择中 (`is_selecting`)，就用当前的动态选区。
        // 否则为 `None`。
        let selection_to_draw = data.selection_rect.or_else(|| {
            if data.is_selecting {
                Some(data.get_current_selection())
            } else {
                None
            }
        });

        if let Some(r) = selection_to_draw {
            // 如果有选区，绘制一个半透明的黑色遮罩，覆盖除了选区以外的所有区域。
            let mask_color = Color::rgba8(0, 0, 0, 128);
            // 通过绘制四个矩形来实现“挖空”效果
            ctx.fill(Rect::new(0.0, 0.0, full_rect.width(), r.y0), &mask_color); // 上方
            ctx.fill(Rect::new(0.0, r.y1, full_rect.width(), full_rect.height()), &mask_color); // 下方
            ctx.fill(Rect::new(0.0, r.y0, r.x0, r.y1), &mask_color); // 左方
            ctx.fill(Rect::new(r.x1, r.y0, full_rect.width(), r.y1), &mask_color); // 右方

            // 在选区周围绘制一个白色的边框，使其更醒目。
            ctx.stroke(r, &Color::WHITE, 1.0);
        } else {
            // 如果没有任何选区（例如，程序刚启动时），给整个屏幕添加一个轻微的暗色滤镜，
            // 提示用户可以开始操作。
            ctx.fill(full_rect, &Color::rgba8(0, 0, 0, 72));
        }
    }
}


/// 创建右键/操作上下文菜单。
fn make_context_menu() -> Menu<AppState> {
    Menu::empty()
        // 添加“复制”菜单项
        .entry(MenuItem::new("复制").on_activate(|ctx, data: &mut AppState, _| {
            // 尝试裁剪图像，如果成功...
            if let Some(img) = data.crop_image() {
                // ...将其复制到剪贴板，然后退出程序。
                copy_image_to_clipboard(&img).ok();
                ctx.submit_command(druid::commands::QUIT_APP);
            }
        }))
        // 添加“另存为”菜单项
        .entry(MenuItem::new("另存为...").on_activate(|ctx, data: &mut AppState, _| {
            if let Some(img) = data.crop_image() {
                // 打开文件保存对话框，并设置文件类型过滤器为 PNG
                if let Some(path) = rfd::FileDialog::new().add_filter("PNG", &["png"]).save_file() {
                    // 保存图像到用户选择的路径，然后退出程序。
                    img.save(&path).ok();
                    ctx.submit_command(druid::commands::QUIT_APP);
                }
            }
        }))
        // 添加“扫描二维码”菜单项
        .entry(MenuItem::new("扫描二维码").on_activate(|ctx, data: &mut AppState, _| {
            if let Some(img) = data.crop_image() {
                match scan_qr_code(&img) {
                    // 扫描成功，且找到了二维码
                    Ok(Some(txt)) => {
                        // 将二维码内容复制到剪贴板，然后退出程序。
                        copy_text_to_clipboard(&txt).ok();
                        ctx.submit_command(druid::commands::QUIT_APP);
                    }
                    // 扫描成功，但未找到二维码
                    Ok(None) => {
                        // 显示一个提示对话框
                        MessageDialog::new().set_title("提示").set_description("未扫描到二维码").show();
                    }
                    // 扫描过程中发生错误
                    Err(e) => {
                        // 显示一个错误对话框
                        MessageDialog::new().set_title("错误").set_description(&format!("扫描失败: {}", e)).show();
                    }
                }
            }
        }))
        // 添加“退出”菜单项
        .entry(MenuItem::new("退出").on_activate(|ctx, _, _| {
            // 直接发送退出命令
            ctx.submit_command(druid::commands::QUIT_APP)
        }))
}

/// 程序主入口函数。
fn main() -> Result<()> {
    // 1. 捕获屏幕
    let monitors = Monitor::all()?; // 获取所有连接的显示器列表
    // 获取第一个显示器（主显示器）
    let primary_monitor = monitors.get(0).ok_or_else(|| anyhow::anyhow!("找不到任何显示器"))?;
    // 捕获该显示器的图像
    let image = primary_monitor.capture_image()?;

    // 2. 转换图像格式
    let (w, h) = (image.width(), image.height());
    let raw_pixels = image.into_raw(); // 获取原始的 BGRA 像素数据
    // 从原始像素数据创建一个 `image` 库的 `ImageBuffer`。
    // `xcap` 提供的是 BGRA 格式，但 `image` 库更常用 RGBA，幸运的是它们内存布局兼容，可以直接转换。
    let buffer = image::ImageBuffer::from_raw(w, h, raw_pixels)
        .ok_or_else(|| anyhow::anyhow!("从原始数据创建 ImageBuffer 失败"))?;
    // 将 `ImageBuffer` 包装成 `DynamicImage`，这是一个更通用的图像枚举类型。
    let dynamic_image = DynamicImage::ImageRgba8(buffer);

    // 3. 初始化应用状态
    let initial_state = AppState {
        screenshot: Arc::new(dynamic_image), // 将截图放入 Arc 中
        is_selecting: false,
        start_pos: Point::ZERO, // 初始位置设为 (0,0)
        current_pos: Point::ZERO,
        selection_rect: None, // 初始没有选区
    };

    // 4. 配置和启动窗口
    // 创建一个窗口描述符
    let window = WindowDesc::new(ScreenshotWidget {
        cached_image: None,
        previous_rect: None,
    })
    .window_size((w as f64, h as f64)) // 窗口大小与截图大小一致
    .show_titlebar(false)               // 隐藏标题栏，创建无边框窗口
    .resizable(false);                  // 禁止调整窗口大小

    // 启动 Druid 应用程序，传入窗口描述符和初始状态
    AppLauncher::with_window(window).launch(initial_state)?;

    Ok(())
}