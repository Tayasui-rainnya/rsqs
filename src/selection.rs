use minifb::{MouseButton, MouseMode};

#[derive(Debug, Clone, Copy)]
/// 矩形选区结构体
pub struct Rect { pub x: usize, pub y: usize, pub w: usize, pub h: usize }

pub struct Selection {
    pub start: Option<(usize, usize)>,
    pub end: Option<(usize, usize)>,
    pub made: Option<Rect>,
}

impl Selection {
    pub fn new() -> Self {
        Selection { start: None, end: None, made: None }
    }

    /// 每帧更新鼠标状态
    pub fn update(&mut self, window: &mut minifb::Window) {
        let down = window.get_mouse_down(MouseButton::Left);
        if down {
            // 如果已有完成选区，且重新按下，则清空旧选区，开始新选区
            if self.made.is_some() {
                self.start = None;
                self.end = None;
                self.made = None;
            }
            if let Some((mx, my)) = window.get_mouse_pos(MouseMode::Clamp) {
                let x = mx as usize;
                let y = my as usize;
                if self.start.is_none() {
                    self.start = Some((x, y));
                }
                self.end = Some((x, y));
            }
        } else if self.start.is_some() && self.end.is_some() && self.made.is_none() {
            // 松开按钮时，将当前拖拽区域设为完成选区
            let (sx, sy) = self.start.unwrap();
            let (ex, ey) = self.end.unwrap();
            let x0 = sx.min(ex);
            let y0 = sy.min(ey);
            let w = sx.max(ex) - x0;
            let h = sy.max(ey) - y0;
            self.made = Some(Rect { x: x0, y: y0, w: w.max(1), h: h.max(1) });
        }
    }

    /// 在缓冲区绘制半透明遮罩和选区边框
    pub fn draw_overlay(&self, buffer: &mut [u32], width: usize, height: usize) {
        // 整体遮罩：暗化图像 50%
        for pix in buffer.iter_mut() {
            let r = ((*pix >> 16) & 0xFF) as u8;
            let g = ((*pix >> 8)  & 0xFF) as u8;
            let b = (*pix        & 0xFF) as u8;
            let r2 = (r as f32 * 0.5) as u8;
            let g2 = (g as f32 * 0.5) as u8;
            let b2 = (b as f32 * 0.5) as u8;
            *pix = ((r2 as u32) << 16) | ((g2 as u32) << 8) | (b2 as u32);
        }
        // 计算当前应高亮的矩形
        let maybe_rect = if let Some(rect) = self.made {
            Some(rect)
        } else if let (Some((sx, sy)), Some((ex, ey))) = (self.start, self.end) {
            let x0 = sx.min(ex);
            let y0 = sy.min(ey);
            let w = sx.max(ex) - x0;
            let h = sy.max(ey) - y0;
            Some(Rect { x: x0, y: y0, w: w.max(1), h: h.max(1) })
        } else {
            None
        };
        // 反遮罩，并绘制白色边框
        if let Some(rect) = maybe_rect {
            // 反遮罩
            for y in rect.y..(rect.y + rect.h).min(height) {
                let base = y * width;
                for x in rect.x..(rect.x + rect.w).min(width) {
                    let idx = base + x;
                    let pix = buffer[idx];
                    let r = ((pix >> 16) & 0xFF) as u8;
                    let g = ((pix >> 8)  & 0xFF) as u8;
                    let b = (pix         & 0xFF) as u8;
                    let r2 = (r as f32 * 2.0).min(255.0) as u8;
                    let g2 = (g as f32 * 2.0).min(255.0) as u8;
                    let b2 = (b as f32 * 2.0).min(255.0) as u8;
                    buffer[idx] = ((r2 as u32) << 16) | ((g2 as u32) << 8) | (b2 as u32);
                }
            }
            // 边框
            let x0 = rect.x;
            let y0 = rect.y;
            let x1 = (rect.x + rect.w).min(width - 1);
            let y1 = (rect.y + rect.h).min(height - 1);
            let white = 0xFFFFFF;
            for x in x0..=x1 {
                buffer[y0 * width + x] = white;
                buffer[y1 * width + x] = white;
            }
            for y in y0..=y1 {
                buffer[y * width + x0] = white;
                buffer[y * width + x1] = white;
            }
        }
    }
}