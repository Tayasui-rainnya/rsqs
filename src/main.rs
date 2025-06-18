#![windows_subsystem = "windows"]

use anyhow::Result;
use arboard::{Clipboard, ImageData};
use druid::menu::MenuEventCtx;
use druid::piet::{Image, PietImage};
use druid::{
    AppLauncher, BoxConstraints, Color, Data, Env, Event, EventCtx, LayoutCtx, LifeCycle,
    LifeCycleCtx, Menu, MenuItem, PaintCtx, Point, Rect, RenderContext, Size, UpdateCtx, Widget,
    WindowDesc,
};
// 这里的 image 是我们在 Cargo.toml 中指定的 v0.24.9
use image::{DynamicImage, GenericImageView, ImageBuffer, Rgba};
use rfd::MessageDialog;
use std::sync::Arc;
use xcap::Monitor;

mod qrcode;
use qrcode::scan_qr_code;

#[derive(Clone, Data)]
struct AppState {
    #[data(same_fn = "PartialEq::eq")]
    screenshot: Arc<DynamicImage>, // 这个是 image v0.24.9 的 DynamicImage
    is_selecting: bool,
    start_pos: Point,
    current_pos: Point,
    selection_rect: Option<Rect>,
}

impl AppState {
    fn get_current_selection(&self) -> Rect {
        Rect::from_points(self.start_pos, self.current_pos).abs()
    }

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

    fn get_full_image(&self) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
        self.screenshot.to_rgba8()
    }
}


fn copy_image_to_clipboard(image: &ImageBuffer<Rgba<u8>, Vec<u8>>) -> Result<()> {
    let mut clipboard = Clipboard::new()?;
    let image_data = ImageData {
        width: image.width() as usize,
        height: image.height() as usize,
        bytes: image.as_raw().into(),
    };
    clipboard.set_image(image_data)?;
    println!("图像已作为图片格式复制到剪贴板");
    Ok(())
}

fn copy_text_to_clipboard(text: &str) -> Result<()> {
    let mut clipboard = Clipboard::new()?;
    clipboard.set_text(text.to_string())?;
    println!("文本 \"{}\" 已复制到剪贴板", text);
    Ok(())
}

struct ScreenshotWidget {
    cached_image: Option<PietImage>,
}

impl Widget<AppState> for ScreenshotWidget {
    fn event(&mut self, ctx: &mut EventCtx, event: &Event, data: &mut AppState, _env: &Env) {
        match event {
            Event::KeyDown(e) => {
                use druid::keyboard_types::Key;
                match e.key {
                    Key::Enter => {
                        let image_to_copy = data
                            .selection_rect
                            .and_then(|_| data.crop_image())
                            .unwrap_or_else(|| data.get_full_image());

                        if let Err(e) = copy_image_to_clipboard(&image_to_copy) {
                            eprintln!("复制失败: {}", e);
                        }
                        ctx.submit_command(druid::commands::QUIT_APP);
                    }
                    Key::Escape => {
                        ctx.submit_command(druid::commands::QUIT_APP);
                    }
                    _ => {}
                }
            }
            Event::MouseDown(e) => {
                if e.button.is_left() {
                    if e.count >= 2 {
                        let full_image = data.get_full_image();
                        if let Err(e) = copy_image_to_clipboard(&full_image) {
                            eprintln!("复制失败: {}", e);
                        }
                        ctx.submit_command(druid::commands::QUIT_APP);
                        return;
                    }
                    data.is_selecting = true;
                    data.start_pos = e.pos;
                    data.current_pos = e.pos;
                    data.selection_rect = None;
                    ctx.request_paint();
                }
            }
            Event::MouseMove(e) => {
                if data.is_selecting {
                    data.current_pos = e.pos;
                    ctx.request_paint();
                }
            }
            Event::MouseUp(e) => {
                if e.button.is_left() && data.is_selecting {
                    data.is_selecting = false;
                    let selection = data.get_current_selection();
                    if selection.width() > 1.0 && selection.height() > 1.0 {
                        data.selection_rect = Some(selection);
                        ctx.show_context_menu(make_context_menu(), e.pos);
                    } else {
                        data.selection_rect = None;
                    }
                    ctx.request_paint();
                }
            }
            _ => (),
        }
    }

    fn lifecycle(
        &mut self,
        _ctx: &mut LifeCycleCtx,
        _event: &LifeCycle,
        _data: &AppState,
        _env: &Env,
    ) {
    }
    fn update(&mut self, ctx: &mut UpdateCtx, _old_data: &AppState, _data: &AppState, _env: &Env) {
        ctx.request_paint();
    }
    fn layout(
        &mut self,
        _ctx: &mut LayoutCtx,
        _bc: &BoxConstraints,
        data: &AppState,
        _env: &Env,
    ) -> Size {
        let (width, height) = data.screenshot.dimensions();
        Size::new(width as f64, height as f64)
    }
    fn paint(&mut self, ctx: &mut PaintCtx, data: &AppState, _env: &Env) {
        if self.cached_image.is_none() {
            let (width, height) = data.screenshot.dimensions();
            let image_data = data.screenshot.to_rgba8();
            let druid_image = ctx
                .make_image(
                    width as usize,
                    height as usize,
                    image_data.as_raw(),
                    druid::piet::ImageFormat::RgbaSeparate,
                )
                .expect("Failed to create image");
            self.cached_image = Some(druid_image);
        }
        if let Some(druid_image) = &self.cached_image {
            let size = druid_image.size();
            let full_rect = Rect::from_origin_size(Point::ZERO, size);
            ctx.draw_image(
                druid_image,
                full_rect,
                druid::piet::InterpolationMode::NearestNeighbor,
            );
            let mask_color = Color::rgba8(0, 0, 0, 128);
            if let Some(selection) = data.selection_rect.or_else(|| {
                if data.is_selecting {
                    Some(data.get_current_selection())
                } else {
                    None
                }
            }) {
                let (width, height) = (size.width, size.height);
                let top = Rect::new(0.0, 0.0, width, selection.y0);
                let bottom = Rect::new(0.0, selection.y1, width, height);
                let left = Rect::new(0.0, selection.y0, selection.x0, selection.y1);
                let right = Rect::new(selection.x1, selection.y0, width, selection.y1);
                ctx.fill(top, &mask_color);
                ctx.fill(bottom, &mask_color);
                ctx.fill(left, &mask_color);
                ctx.fill(right, &mask_color);
                ctx.stroke(selection, &Color::WHITE, 1.0);
            } else {
                ctx.fill(full_rect, &Color::rgba8(0, 0, 0, 64));
            }
        }
    }
}

fn make_context_menu() -> Menu<AppState> {
    Menu::empty()
        .entry(
            MenuItem::new("复制").on_activate(
                |ctx: &mut MenuEventCtx, data: &mut AppState, _env: &Env| {
                    if let Some(cropped_image) = data.crop_image() {
                        if let Err(e) = copy_image_to_clipboard(&cropped_image) {
                            eprintln!("无法复制图像到剪贴板: {}", e);
                        } else {
                            ctx.submit_command(druid::commands::QUIT_APP);
                        }
                    }
                },
            ),
        )
        .entry(
            MenuItem::new("另存为...").on_activate(
                |ctx: &mut MenuEventCtx, data: &mut AppState, _env: &Env| {
                    if let Some(cropped_image) = data.crop_image() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("PNG Image", &["png"])
                            .set_file_name("screenshot.png")
                            .save_file()
                        {
                            if let Err(e) = cropped_image.save(&path) {
                                eprintln!("保存文件失败: {}", e);
                            } else {
                                println!("文件已保存到: {:?}", path);
                                ctx.submit_command(druid::commands::QUIT_APP);
                            }
                        }
                    }
                },
            ),
        )
        .entry(
            MenuItem::new("扫描二维码").on_activate(
                |ctx: &mut MenuEventCtx, data: &mut AppState, _env: &Env| {
                    if let Some(cropped_image) = data.crop_image() {
                        match scan_qr_code(&cropped_image) {
                            Ok(Some(text)) => {
                                if let Err(e) = copy_text_to_clipboard(&text) {
                                    eprintln!("无法复制二维码内容到剪贴板: {}", e);
                                    MessageDialog::new()
                                        .set_title("错误")
                                        .set_description(&format!("无法复制到剪贴板: {}", e))
                                        .show();
                                } else {
                                    ctx.submit_command(druid::commands::QUIT_APP);
                                }
                            }
                            Ok(None) => {
                                println!("在选区内未找到二维码");
                                MessageDialog::new()
                                    .set_title("提示")
                                    .set_description("未扫描到二维码")
                                    .show();
                            }
                            Err(e) => {
                                eprintln!("扫描二维码时出错: {}", e);
                                MessageDialog::new()
                                    .set_title("扫描错误")
                                    .set_description(&format!("扫描时发生错误: {}", e))
                                    .show();
                            }
                        }
                    }
                },
            ),
        )
        .entry(
            MenuItem::new("退出").on_activate(
                |ctx: &mut MenuEventCtx, _data: &mut AppState, _env: &Env| {
                    ctx.submit_command(druid::commands::QUIT_APP);
                },
            ),
        )
}

fn main() -> Result<()> {
    let monitors = Monitor::all()?;
    if monitors.is_empty() {
        anyhow::bail!("找不到显示器");
    }
    let monitor = &monitors[0];
    
    // --- 草泥马的依赖版本不同啊啊啊啊 ---
    // 1. 从 xcap 捕获图像。此时 `screenshot_from_xcap` 是一个基于 image v0.25.x 的类型
    let screenshot_from_xcap = monitor.capture_image()?;

    // 2. 从 v0.25.x 的图像中提取原始数据
    let width = screenshot_from_xcap.width();
    let height = screenshot_from_xcap.height();
    let raw_pixels = screenshot_from_xcap.into_raw(); 

    // 3. 使用这些原始数据创建一个我们应用内部使用的 v0.24.x 版本的 ImageBuffer
    let screenshot_v024_buffer = image::ImageBuffer::from_raw(width, height, raw_pixels)
        .ok_or_else(|| anyhow::anyhow!("Failed to convert image buffer between versions"))?;

    // 4. 将 v0.24.x 的 buffer 包装成我们 AppState 需要的 DynamicImage
    let screenshot_for_app = image::DynamicImage::ImageRgba8(screenshot_v024_buffer);

    // 5. 使用这个转换后的、版本正确的图像来初始化状态
    let initial_state = AppState {
        screenshot: Arc::new(screenshot_for_app),
        is_selecting: false,
        start_pos: Point::ZERO,
        current_pos: Point::ZERO,
        selection_rect: None,
    };
    // --- 转换结束 ---

    let window = WindowDesc::<AppState>::new(ScreenshotWidget { cached_image: None })
        .title("RsQs Screenshot")
        .window_size((width as f64, height as f64))
        .show_titlebar(false)
        .resizable(false);

    AppLauncher::with_window(window)
        .log_to_console()
        .launch(initial_state)?;

    Ok(())
}