mod selection;
mod menu;

use anyhow::Result;
use clipboard::{ClipboardContext, ClipboardProvider};
use image::{imageops, ImageBuffer, Rgba};
use image::ImageFormat;
use std::io::{Cursor, stdin, stdout, Write};
use xcap::Monitor;
use minifb::{Window, WindowOptions, ScaleMode, Key};
use selection::Selection;


use druid::Event;
use druid::LocalizedString;
use druid::WidgetExt;
use druid::widget::Controller;
use druid::{Env, WindowDesc, Widget, AppLauncher, Data, Menu, MenuItem};


#[derive(Data, Clone)]
struct AppData;

struct EditController;
impl<W: Widget<AppData>> Controller<AppData, W> for EditController {
    fn event(&mut self, child: &mut W, ctx: &mut druid::EventCtx, event: &Event, _data: &mut AppData, env: &Env) {
        if let Event::MouseDown(e) = event {
            if e.button.is_right() {
                ctx.show_context_menu(make_context_menu(), e.pos);
            }
        }
        child.event(ctx, event, _data, env)
    }
}

fn make_context_menu() -> Menu<AppData> {
    Menu::empty()
        .entry(MenuItem::new(LocalizedString::new("复制")).on_activate(|_ctx, _data, _env| {
            // 实现“复制”功能
            println!("复制");
        }))
        .entry(MenuItem::new(LocalizedString::new("另存为")).on_activate(|_ctx, _data, _env| {
            // 实现“另存为”功能
            println!("另存为");
        }))
}

fn make_window() -> impl Widget<AppData> {
    druid::widget::Label::new("  ")
        .center()
        .controller(EditController {})
}

fn main() -> Result<()> {
    // 1) 捕获所有屏幕并缓存
    let monitors = Monitor::all()?;
    let mut screenshots = Vec::with_capacity(monitors.len());
    for m in &monitors {
        screenshots.push(m.capture_image()?);
    }

    // 2) 使用第一张截图
    let img_buf = &screenshots[0];
    let (width, height) = img_buf.dimensions();
    let base_buffer: Vec<u32> = img_buf.pixels().map(|p| {
        let [r, g, b, _] = p.0;
        ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
    }).collect();

    // 3) 创建窗口
    let mut window = Window::new(
        "RsQs Select",
        width as usize,
        height as usize,
        WindowOptions { borderless: true, resize: false, scale_mode: ScaleMode::Stretch, ..Default::default() },
    )?;
    window.set_target_fps(60);

    // 4) 选区逻辑与裁剪缓存
    let mut selection = Selection::new();
    let mut cropped: Option<ImageBuffer<Rgba<u8>, Vec<u8>>> = None;
    let mut right_was_down = false;

    while window.is_open() && !window.is_key_down(Key::Escape) {
        // 更新选区状态
        selection.update(&mut window);

        // 在完成选区且尚未裁剪时，执行裁剪并缓存
        if let Some(rect) = selection.made {
            if cropped.is_none() {
                let sub = imageops::crop_imm(
                    img_buf,
                    rect.x as u32,
                    rect.y as u32,
                    rect.w as u32,
                    rect.h as u32,
                ).to_image();
                cropped = Some(sub);
            }
        } else {
            // 尚在拖拽或新选区，清理上次裁剪缓存
            cropped = None;
        }

        // 检测右键点击弹出菜单
        let right_down = window.get_mouse_down(minifb::MouseButton::Right);
        if right_down && !right_was_down {
            if let Some(ref img) = cropped {
                // 简易菜单：在控制台输出
                println!("右键菜单: 1) 复制 2) 另存为");
                print!("请选择: "); stdout().flush().unwrap();
                let mut choice = String::new(); stdin().read_line(&mut choice).unwrap();
                match choice.trim() {
                    "1" => {
                        // 复制到剪贴板（PNG base64 格式）
                        let mut ctx: ClipboardContext = ClipboardProvider::new().unwrap();
                        let mut buf = Vec::new();
                        {
                            let mut cursor = Cursor::new(&mut buf);
                            img.write_to(&mut cursor, ImageFormat::Png).unwrap();
                        }
                        let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &buf);
                        ctx.set_contents(encoded).unwrap();
                        println!("已复制到剪贴板 (PNG base64)");
                    }
                    "2" => {
                        // 弹出文件保存对话框
                        if let Some(path) = rfd::FileDialog::new()
                            .set_file_name("selection.png")
                            .save_file()
                        {
                            img.save(path).unwrap();
                            println!("已保存到指定路径");
                        }
                    }
                    _ => println!("无效选择"),
                }
            }
        }
        right_was_down = right_down;

        // 准备并绘制帧
        let mut frame = base_buffer.clone();
        selection.draw_overlay(&mut frame, width as usize, height as usize);
        window.update_with_buffer(&frame, width as usize, height as usize)?;
    }


    // 5) 创建 Druid 窗口    
    let window = WindowDesc::new(make_window());
        AppLauncher::with_window(window)
        .log_to_console()
        .launch(AppData)
        .unwrap();




    // 程序退出时，screenshots 和 cropped 会自动释放
    Ok(())
}
