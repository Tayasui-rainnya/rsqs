use anyhow::Result;
use druid::widget::Controller;
use druid::menu::MenuEventCtx;
use druid::piet::Image;
use druid::{
    AppLauncher, BoxConstraints, Color, Data, Env, Event, EventCtx, LayoutCtx, LifeCycle,
    LifeCycleCtx, Menu, MenuItem, PaintCtx, Point, Rect, RenderContext, Size, UpdateCtx, Widget,
    WindowDesc, WidgetExt,
};
use druid::piet::PietImage;
use image::{DynamicImage, GenericImageView, ImageBuffer, ImageFormat, Rgba};
use std::sync::Arc;
use xcap::Monitor;

// --- 1. AppState ---
#[derive(Clone, Data)]
struct AppState {
    #[data(same_fn = "PartialEq::eq")]
    screenshot: Arc<DynamicImage>,
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
            self.screenshot.crop_imm(
                rect.x0.max(0.0) as u32,
                rect.y0.max(0.0) as u32,
                rect.width() as u32,
                rect.height() as u32,
            ).to_rgba8()
        })
    }
}

// --- 2. ScreenshotWidget ---
struct ScreenshotWidget {
    cached_image: Option<PietImage>,
}

impl Widget<AppState> for ScreenshotWidget {
    fn event(&mut self, ctx: &mut EventCtx, event: &Event, data: &mut AppState, _env: &Env) {
        match event {
            Event::MouseDown(e) => {
                if e.button.is_left() {
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
                    } else {
                        data.selection_rect = None;
                    }
                    ctx.request_paint();
                }
            }
            _ => (),
        }
    }

    fn lifecycle(&mut self, _ctx: &mut LifeCycleCtx, _event: &LifeCycle, _data: &AppState, _env: &Env) {}

    fn update(&mut self, ctx: &mut UpdateCtx, _old_data: &AppState, _data: &AppState, _env: &Env) {
        ctx.request_paint();
    }

    fn layout(&mut self, _ctx: &mut LayoutCtx, _bc: &BoxConstraints, data: &AppState, _env: &Env) -> Size {
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

// --- 3. Controller 和菜单 (已修改) ---
struct EditController;
impl<W: Widget<AppState>> Controller<AppState, W> for EditController {
    fn event(&mut self, child: &mut W, ctx: &mut EventCtx, event: &Event, data: &mut AppState, env: &Env) {
        if let Event::MouseDown(e) = event {
            if e.button.is_right() && data.selection_rect.is_some() {
                ctx.show_context_menu(make_context_menu(), e.pos);
            }
        }
        child.event(ctx, event, data, env)
    }
}

fn make_context_menu() -> Menu<AppState> {
    Menu::empty()
        .entry(
            MenuItem::new("复制").on_activate(|ctx: &mut MenuEventCtx, data: &mut AppState, _env: &Env| {
                if let Some(cropped_image) = data.crop_image() {
                    use clipboard::{ClipboardContext, ClipboardProvider};
                    
                    let mut bytes: Vec<u8> = Vec::new();
                    cropped_image.write_to(&mut std::io::Cursor::new(&mut bytes), ImageFormat::Png)
                        .expect("Failed to write image to buffer");

                    let mut clip_ctx: ClipboardContext = ClipboardProvider::new().unwrap();
                    let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);
                    clip_ctx.set_contents(encoded).unwrap();
                    
                    println!("已复制到剪贴板");
                    
                    ctx.submit_command(druid::commands::QUIT_APP);
                }
            })
        )
        .entry(
            MenuItem::new("另存为...").on_activate(|ctx: &mut MenuEventCtx, data: &mut AppState, _env: &Env| {
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
            })
        )
        .entry(
            MenuItem::new("退出").on_activate(|ctx: &mut MenuEventCtx, _data: &mut AppState, _env: &Env| {
                ctx.submit_command(druid::commands::QUIT_APP);
            })
        )
}

// --- 4. 主函数 ---
fn main() -> Result<()> {
    let monitors = Monitor::all()?;
    if monitors.is_empty() {
        anyhow::bail!("找不到显示器");
    }
    let monitor = &monitors[0];
    let screenshot = monitor.capture_image()?;
    let (width, height) = screenshot.dimensions();

    let initial_state = AppState {
        screenshot: Arc::new(DynamicImage::ImageRgba8(screenshot)),
        is_selecting: false,
        start_pos: Point::ZERO,
        current_pos: Point::ZERO,
        selection_rect: None,
    };

    let window = WindowDesc::<AppState>::new(
        ScreenshotWidget { cached_image: None }
            .controller(EditController)
    ) 
        .title("RsQs Screenshot")
        .window_size((width as f64, height as f64))
        .show_titlebar(false)
        .resizable(false);

    AppLauncher::with_window(window)
        .log_to_console()
        .launch(initial_state)?;

    Ok(())
}