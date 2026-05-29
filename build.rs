fn main() {
    // 生成便签形状图标
    generate_icons();

    // 编译 Slint UI 文件
    slint_build::compile("ui/app-window.slint").unwrap();

    // 嵌入 Windows 应用图标
    #[cfg(target_os = "windows")]
    {
        if std::path::Path::new("assets/icon.ico").exists() {
            let mut res = winresource::WindowsResource::new();
            res.set_icon("assets/icon.ico");
            res.set("ProductName", "便签");
            res.compile().unwrap();
        }
    }
}

// ---- 图标生成 ----

use image::{RgbaImage, Rgba};

/// 为所有尺寸生成便签形状图标，输出 PNG + 多尺寸 ICO
fn generate_icons() {

    std::fs::create_dir_all("assets").ok();

    // 多尺寸：小尺寸保任务栏清晰，大尺寸保 EXE 高质量
    let sizes = &[16u32, 24, 32, 48, 64, 128, 256];

    // 生成并收集各尺寸的 PNG 字节
    let mut png_entries: Vec<(u32, Vec<u8>)> = Vec::new();
    let mut biggest_img: Option<RgbaImage> = None;

    for &size in sizes {
        let img = make_sticky_note_sprite(size);
        let png_bytes = encode_png(&img);
        if biggest_img.is_none() || size > biggest_img.as_ref().unwrap().width() {
            biggest_img = Some(img);
        }
        png_entries.push((size, png_bytes));
    }

    // 保存最大尺寸 PNG 给 Slint 窗口图标
    if let Some(ref img) = biggest_img {
        img.save("assets/icon.png").ok();
    }

    // 构建多尺寸 ICO 文件
    let ico_bytes = build_ico(&png_entries);
    std::fs::write("assets/icon.ico", &ico_bytes).ok();

    println!("cargo:warning=便签图标已生成: {} 尺寸 (16-256px)", png_entries.len());
}

// ---- 便签形状绘制 ----

/// 像素在圆角矩形内部吗？
fn inside_rounded_rect(x: u32, y: u32, w: u32, h: u32, r: u32) -> bool {
    let fx = x as f32;
    let fy = y as f32;
    let fw = w as f32 - 1.0;
    let fh = h as f32 - 1.0;
    let fr = r as f32;

    // 四个角的圆心
    let corners = [(fr, fr), (fw - fr, fr), (fr, fh - fr), (fw - fr, fh - fr)];

    // 先快速判断是否在中心矩形内
    if fx >= fr && fx <= fw - fr && fy >= fr && fy <= fh - fr {
        return true;
    }
    // 快速判断四个边矩形
    if fy >= fr && fy <= fh - fr {
        return true; // 在左右矩形条内
    }
    if fx >= fr && fx <= fw - fr {
        return true; // 在上下矩形条内
    }
    // 检查四个角
    for &(cx, cy) in &corners {
        let dx = fx - cx;
        let dy = fy - cy;
        if (dx * dx + dy * dy) <= (fr * fr) {
            return true;
        }
    }
    false
}

/// 像素在折角三角形内吗？（右上角）
fn inside_fold(x: u32, y: u32, w: u32, fold_sz: u32) -> bool {
    let fx = x as i32;
    let fy = y as i32;
    let fw = w as i32;
    let fs = fold_sz as i32;
    // 折角三角形：右上角，沿对角线向左下
    // 条件：x >= w - fold_sz 且 y <= fold_sz 且 (x - (w - fold_sz)) + y <= fold_sz
    fx >= fw - fs && fy <= fs && (fx - (fw - fs)) + fy <= fs
}

/// 绘制一张便签形状图
fn make_sticky_note_sprite(size: u32) -> RgbaImage {
    let mut img = RgbaImage::new(size, size);

    // 尺寸相关参数
    let margin = (size as f32 * 0.12).ceil() as u32;   // 外边距
    let r = (size as f32 * 0.16).ceil() as u32;         // 圆角半径
    let fold_sz = (size as f32 * 0.24).ceil() as u32;   // 折角大小

    let w = size;
    let h = size;

    // 便签主体颜色的范围（留 margin）
    let body_x0 = margin;
    let body_y0 = margin;
    let body_w = w - margin * 2;
    let body_h = h - margin * 2;

    // 颜色定义 — 灰白纸质便签（柔和、不刺眼）
    let paper_body   = Rgba([228, 226, 218, 255]);   // #E4E2DA 中性灰白纸
    let paper_fold   = Rgba([208, 205, 195, 255]);   // #D0CDC3 折角区域
    let paper_crease = Rgba([188, 184, 174, 255]);   // #BCB8AE 折痕线
    let transparent  = Rgba([0, 0, 0, 0]);

    // 抗锯齿折角的渐变折痕（从左下到右上的一窄条）
    let crease_thickness = (size as f32 * 0.04).max(1.0) as i32;

    for y in 0..h {
        for x in 0..w {
            // 将坐标映射到便签本体的局部坐标
            let lx = x as i32 - body_x0 as i32;
            let ly = y as i32 - body_y0 as i32;

            let body_pixel = lx >= 0 && lx < body_w as i32 && ly >= 0 && ly < body_h as i32
                && inside_rounded_rect(lx as u32, ly as u32, body_w, body_h, r);

            if !body_pixel {
                img.put_pixel(x, y, transparent);
                continue;
            }

            let in_fold = inside_fold(lx as u32, ly as u32, body_w, fold_sz);
            let in_crease = {
                let fx = lx;
                let fy = ly;
                let fw = body_w as i32;
                let fs = fold_sz as i32;
                // 折痕线：从 (w-fold_sz, 0) 到 (w, fold_sz) 的对角线
                let diag = fx - (fw - fs) + fy;
                diag >= fs - crease_thickness && diag <= fs + crease_thickness && fx >= fw - fs && fy <= fs
            };

            let pixel = if in_crease {
                paper_crease
            } else if in_fold {
                let fx = x as f32 - body_x0 as f32;
                let fy = y as f32 - body_y0 as f32;
                let fw = body_w as f32;
                let fs = fold_sz as f32;
                let t = (fx - (fw - fs) + fy) / (fs * 2.0);
                let t = t.clamp(0.0, 1.0);
                blend(&paper_fold, &paper_body, 1.0 - t)
            } else {
                paper_body
            };

            img.put_pixel(x, y, pixel);
        }
    }

    img
}

fn blend(a: &Rgba<u8>, b: &Rgba<u8>, t: f32) -> Rgba<u8> {
    let t = t.clamp(0.0, 1.0);
    Rgba([
        (a[0] as f32 * t + b[0] as f32 * (1.0 - t)) as u8,
        (a[1] as f32 * t + b[1] as f32 * (1.0 - t)) as u8,
        (a[2] as f32 * t + b[2] as f32 * (1.0 - t)) as u8,
        255,
    ])
}

// ---- PNG / ICO 编码工具 ----

fn encode_png(img: &RgbaImage) -> Vec<u8> {
    use image::DynamicImage;
    let mut buf = std::io::Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(img.clone())
        .write_to(&mut buf, image::ImageFormat::Png)
        .ok();
    buf.into_inner()
}

/// 构建多尺寸 ICO（每个尺寸用 PNG 编码）
fn build_ico(entries: &[(u32, Vec<u8>)]) -> Vec<u8> {
    // ICO header
    let mut ico = Vec::new();
    ico.extend_from_slice(&[0u8, 0]);         // reserved
    ico.extend_from_slice(&[1u8, 0]);         // type = ICO
    ico.extend_from_slice(&(entries.len() as u16).to_le_bytes()); // count

    // 计算各条目偏移
    let header_size = 6 + 16 * entries.len();
    let mut offsets = Vec::new();
    let mut cur = header_size as u32;
    for &(_, ref data) in entries {
        offsets.push(cur);
        cur += data.len() as u32;
    }

    // 写条目头
    for (i, &(size, ref data)) in entries.iter().enumerate() {
        let sz = size.min(256) % 256;
        ico.push(sz as u8);                     // width  (0 = 256)
        ico.push(sz as u8);                     // height (0 = 256)
        ico.push(0);                            // palette
        ico.push(0);                            // reserved
        ico.extend_from_slice(&[1u8, 0]);       // planes
        ico.extend_from_slice(&[32u8, 0]);      // bpp
        ico.extend_from_slice(&(data.len() as u32).to_le_bytes()); // size
        ico.extend_from_slice(&offsets[i].to_le_bytes());          // offset
    }

    // 写图像数据
    for &(_, ref data) in entries {
        ico.extend_from_slice(data);
    }

    ico
}
