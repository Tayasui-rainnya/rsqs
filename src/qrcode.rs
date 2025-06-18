// src/qrcode.rs

use anyhow::Result;
use image::{ImageBuffer, Rgba};

/// 接收一个 RGBA 图像，并尝试扫描其中的二维码。
///
/// # 返回
/// - `Ok(Some(String))`: 成功扫描到二维码，并返回其内容。
/// - `Ok(None)`: 图像中未找到可识别的二维码。
/// - `Err(e)`: 在扫描过程中发生错误。
pub fn scan_qr_code(image_buffer: &ImageBuffer<Rgba<u8>, Vec<u8>>) -> Result<Option<String>> {
    // 创建一个解码器实例
    let decoder = bardecoder::default_decoder();

    // bardecoder 需要 image crate 的 `DynamicImage` 类型
    let image = image::DynamicImage::ImageRgba8(image_buffer.clone());

    // 解码图像。decode 方法返回一个结果的向量，因为一张图里可能有多个码
    let results = decoder.decode(&image);

    // 我们只需要找到第一个成功解码的内容即可
    let first_decoded_text = results
        .into_iter()
        // `filter_map` 会过滤掉 Err 并解包 Ok
        .filter_map(Result::ok)
        .next();

    Ok(first_decoded_text)
}