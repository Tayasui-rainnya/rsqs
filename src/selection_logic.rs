// src/selection_logic.rs
// No minifb dependency here anymore if it's purely logic

#[derive(Debug, Clone, Copy, PartialEq)] // Added PartialEq for potential Data derivation later
/// 矩形选区结构体
pub struct Rect { pub x: usize, pub y: usize, pub w: usize, pub h: usize }

#[derive(Debug, Clone, PartialEq)] // Added PartialEq
pub struct Selection {
    pub start: Option<(usize, usize)>,
    pub end: Option<(usize, usize)>,
    pub made: Option<Rect>,
}

impl Selection {
    pub fn new() -> Self {
        Selection { start: None, end: None, made: None }
    }

    // The update logic is now handled within the Druid widget's event handler.
    // The draw_overlay logic is now handled within the Druid widget's paint method.
    // This struct now primarily serves as a state holder for the selection process.
}