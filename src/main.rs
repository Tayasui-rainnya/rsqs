#![windows_subsystem = "windows"]

// ----------- 依赖导入 -----------
use anyhow::Result;
use arboard::{Clipboard, ImageData}; // 剪贴板操作
use druid::menu::MenuEventCtx;
use druid::piet::PietImage;
use druid::{
    AppLauncher, BoxConstraints, Color, Data, Env, Event, EventCtx, LayoutCtx, LifeCycle,
    LifeCycleCtx, Menu, MenuItem, PaintCtx, Point, Rect, RenderContext, Size, UpdateCtx, Widget,
    WindowDesc,
};
//  image v0.24.9
use image::{DynamicImage, GenericImageView, ImageBuffer, Rgba}; // 图像处理
use rfd::MessageDialog; // 文件/消息对话框
use std::sync::Arc;
use xcap::Monitor; // 屏幕捕获

mod qrcode; // 二维码识别模块
use qrcode::scan_qr_code;

// ----------- 应用状态结构体 -----------
#[derive(Clone, Data)]
/// 保存截图、选区状态等信息
struct AppState {
    #[data(same_fn = "PartialEq::eq")]
    screenshot: Arc<DynamicImage>, // 当前屏幕截图
    is_selecting: bool,            // 是否正在选择区域
    start_pos: Point,              // 选择起点
    current_pos: Point,            // 当前鼠标位置
    selection_rect: Option<Rect>,  // 已确定的选区
}

impl AppState {
    /// 获取当前拖动的选区（未最终确定时）
    fn get_current_selection(&self) -> Rect {
        Rect::from_points(self.start_pos, self.current_pos).abs()
    }

    /// 裁剪选区对应的图像
    fn crop_image(&self) -> Option<ImageBuffer<Rgba<u8>, Vec<u8>>> {
        self.selection_rect.map(|rect| {
            self.screenshot
                .crop_imm(
                    rect.x0.max(0.0) as u32,
                    rect.y0.max(0.0) as u32,
                    rect.width() as u32,
                    rect.height() as u32,
                )
                .to_rgba8()
        })
    }
    
}

// ----------- 剪贴板辅助函数 -----------
/// 将图片复制到剪贴板
fn copy_image_to_clipboard(image: &ImageBuffer<Rgba<u8>, Vec<u8>>) -> Result<()> {
    let mut clipboard = Clipboard::new()?;
    let image_data = ImageData {
        width: image.width() as usize,
        height: image.height() as usize,
        bytes: image.as_raw().into(),
    };
    clipboard.set_image(image_data)?;
    Ok(())
}

/// 将文本复制到剪贴板
fn copy_text_to_clipboard(text: &str) -> Result<()> {
    let mut clipboard = Clipboard::new()?;
    clipboard.set_text(text.to_string())?;
    Ok(())
}

// ----------- 截图控件实现 -----------
struct ScreenshotWidget {
    cached_image: Option<PietImage>, // 缓存的屏幕图像
    previous_rect: Option<Rect>,     // 上一次选区
}

impl Widget<AppState> for ScreenshotWidget {
    /// 处理鼠标和键盘事件，实现选区和菜单弹出
    fn event(&mut self, ctx: &mut EventCtx, event: &Event, data: &mut AppState, _env: &Env) {
        match event {
            // 鼠标左键按下：开始选择
            Event::MouseDown(e) if e.button.is_left() => {
                data.selection_rect = None; 
                data.is_selecting = true;
                data.start_pos = e.pos;
                data.current_pos = e.pos;
                self.previous_rect = Some(data.get_current_selection());
                ctx.request_paint();
            }

            // 鼠标拖动：更新选择区域
            Event::MouseMove(e) if data.is_selecting => {
                let old_rect = self.previous_rect.unwrap_or_else(|| data.get_current_selection());
                data.current_pos = e.pos;
                let new_rect = data.get_current_selection();
                self.previous_rect = Some(new_rect);

                // 只重绘变化的区域以提高性能
                let dirty = old_rect.union(new_rect).inset(2.0);
                ctx.request_paint_rect(dirty);
            }
            
            // 鼠标左键抬起：完成选择并显示菜单
            Event::MouseUp(e) if e.button.is_left() => {
                if data.is_selecting {
                    data.is_selecting = false; // 结束选择状态

                    let sel = data.get_current_selection();
                    // 只有当选区足够大时才认为是有效选择
                    if sel.width() > 1.0 && sel.height() > 1.0 {
                        data.selection_rect = Some(sel);
                        ctx.show_context_menu(make_context_menu(), e.pos);
                    } else {
                        data.selection_rect = None;
                    }
                    ctx.request_paint(); 
                }
            }

            // 鼠标右键按下：直接显示菜单（全屏选区）
            Event::MouseDown(e) if e.button.is_right() => {
                if data.selection_rect.is_none() {
                    let screen_rect = ctx.size().to_rect();
                    data.selection_rect = Some(screen_rect);
                    ctx.request_paint();
                }
                ctx.show_context_menu(make_context_menu(), e.pos);
            }
            
            // ESC 键退出
            Event::KeyDown(key_event) if key_event.key == druid::keyboard_types::Key::Escape => {
                ctx.submit_command(druid::commands::QUIT_APP);
            }

            _ => {}
        }
    }
    
    // 生命周期、更新、布局、绘制等方法
    fn lifecycle(
        &mut self,
        _ctx: &mut LifeCycleCtx,
        _event: &LifeCycle,
        _data: &AppState,
        _env: &Env,
    ) {
    }

    fn update(&mut self, ctx: &mut UpdateCtx, old: &AppState, data: &AppState, _env: &Env) {
        if !Arc::ptr_eq(&old.screenshot, &data.screenshot) {
            self.cached_image = None;
            ctx.request_paint();
        }
    }

    fn layout(&mut self, _ctx: &mut LayoutCtx, _bc: &BoxConstraints, data: &AppState, _env: &Env) -> Size {
        let (w, h) = data.screenshot.dimensions();
        Size::new(w as f64, h as f64)
    }

    /// 绘制截图和选区遮罩
    fn paint(&mut self, ctx: &mut PaintCtx, data: &AppState, _env: &Env) {
        let size = ctx.size();
        let full_rect = size.to_rect();

        // 首次绘制时生成缓存图像
        if self.cached_image.is_none() {
            let (w, h) = data.screenshot.dimensions();
            let buf = data.screenshot.to_rgba8();
            self.cached_image = ctx
                .make_image(w as usize, h as usize, buf.as_raw(), druid::piet::ImageFormat::RgbaSeparate)
                .ok();
        }
        if let Some(img) = &self.cached_image {
            ctx.draw_image(img, full_rect, druid::piet::InterpolationMode::NearestNeighbor);
        }

        // 绘制选区遮罩和边框
        let sel_rect = data.selection_rect.or_else(|| {
            if data.is_selecting {
                Some(data.get_current_selection())
            } else {
                None
            }
        });

        if let Some(r) = sel_rect {
            let mask = Color::rgba8(0, 0, 0, 128);
            ctx.fill(Rect::new(0.0, 0.0, full_rect.width(), r.y0), &mask);
            ctx.fill(Rect::new(0.0, r.y1, full_rect.width(), full_rect.height()), &mask);
            ctx.fill(Rect::new(0.0, r.y0, r.x0, r.y1), &mask);
            ctx.fill(Rect::new(r.x1, r.y0, full_rect.width(), r.y1), &mask);
            ctx.stroke(r, &Color::WHITE, 1.0);
        } else {
            ctx.fill(full_rect, &Color::rgba8(0, 0, 0, 72));
        }

    }
}

// ----------- 右键菜单生成 -----------
fn make_context_menu() -> Menu<AppState> {
    Menu::empty()
        .entry(MenuItem::new("复制").on_activate(|ctx: &mut MenuEventCtx, data: &mut AppState, _| {
            if let Some(img) = data.crop_image() {
                copy_image_to_clipboard(&img).ok();
                ctx.submit_command(druid::commands::QUIT_APP);
            }
        }))
        .entry(MenuItem::new("另存为...").on_activate(|ctx, data: &mut AppState, _| {
            if let Some(img) = data.crop_image() {
                if let Some(path) = rfd::FileDialog::new().add_filter("PNG", &["png"]).save_file() {
                    img.save(&path).ok();
                    ctx.submit_command(druid::commands::QUIT_APP);
                }
            }
        }))
        .entry(MenuItem::new("扫描二维码").on_activate(|ctx, data: &mut AppState, _| {
            if let Some(img) = data.crop_image() {
                match scan_qr_code(&img) {
                    Ok(Some(txt)) => { copy_text_to_clipboard(&txt).ok(); ctx.submit_command(druid::commands::QUIT_APP); }
                    Ok(None) => { MessageDialog::new().set_title("提示").set_description("未扫描到二维码").show(); }
                    Err(e) => { MessageDialog::new().set_title("错误").set_description(&format!("扫描失败: {}", e)).show(); }
                }
            }
        }))
        .entry(MenuItem::new("退出").on_activate(|ctx, _, _| ctx.submit_command(druid::commands::QUIT_APP)))
}

// ----------- 程序入口 -----------
fn main() -> Result<()> {
    // 捕获主显示器屏幕
    let mons = Monitor::all()?;
    let mon = mons.get(0).ok_or_else(|| anyhow::anyhow!("找不到显示器"))?;
    let img = mon.capture_image()?;
    let (w, h) = (img.width(), img.height());
    let raw = img.into_raw();
    let buf = image::ImageBuffer::from_raw(w, h, raw).ok_or_else(|| anyhow::anyhow!("转换失败"))?;
    let dyn_img = DynamicImage::ImageRgba8(buf);

    // 初始化应用状态和窗口
    let init = AppState { screenshot: Arc::new(dyn_img), is_selecting: false, start_pos: Point::ZERO, current_pos: Point::ZERO, selection_rect: None };
    let window = WindowDesc::new(ScreenshotWidget { cached_image: None, previous_rect: None })
        .window_size((w as f64, h as f64))
        .show_titlebar(false)
        .resizable(false);
    AppLauncher::with_window(window).launch(init)?;
    Ok(())
}
