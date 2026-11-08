use std::{error::Error, fs, path::Path};

const ICON_SIZE: u32 = 32;

fn append_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn append_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn generate_windows_icon() -> Result<(), Box<dyn Error>> {
    let icon_path = Path::new("icons/icon.ico");
    if icon_path.exists() {
        return Ok(());
    }

    fs::create_dir_all("icons")?;

    // A 32x32, 32-bit BI_RGB bitmap wrapped in a Windows ICO file.
    // The tray icon is rendered independently at runtime; this icon is the
    // Windows executable/resource icon required by tauri-build.
    let pixel_bytes = ICON_SIZE * ICON_SIZE * 4;
    let mask_row_bytes = ICON_SIZE.div_ceil(32) * 4;
    let mask_bytes = mask_row_bytes * ICON_SIZE;
    let image_bytes = 40 + pixel_bytes + mask_bytes;

    let mut icon = Vec::with_capacity(22 + image_bytes as usize);

    // ICONDIR
    append_u16(&mut icon, 0); // reserved
    append_u16(&mut icon, 1); // icon resource
    append_u16(&mut icon, 1); // one image

    // ICONDIRENTRY
    icon.push(ICON_SIZE as u8);
    icon.push(ICON_SIZE as u8);
    icon.push(0); // palette colors
    icon.push(0); // reserved
    append_u16(&mut icon, 1); // planes
    append_u16(&mut icon, 32); // bits per pixel
    append_u32(&mut icon, image_bytes);
    append_u32(&mut icon, 22); // image offset

    // BITMAPINFOHEADER. ICO stores the combined XOR+AND bitmap height.
    append_u32(&mut icon, 40);
    append_u32(&mut icon, ICON_SIZE);
    append_u32(&mut icon, ICON_SIZE * 2);
    append_u16(&mut icon, 1);
    append_u16(&mut icon, 32);
    append_u32(&mut icon, 0); // BI_RGB
    append_u32(&mut icon, pixel_bytes);
    append_u32(&mut icon, 0);
    append_u32(&mut icon, 0);
    append_u32(&mut icon, 0);
    append_u32(&mut icon, 0);

    // Bottom-up BGRA pixels: blue background with a white speech bubble.
    for y in (0..ICON_SIZE).rev() {
        for x in 0..ICON_SIZE {
            let in_bubble = (7..25).contains(&x) && (6..22).contains(&y);
            let in_tail = (7..13).contains(&x)
                && (20..26).contains(&y)
                && y - 20 >= x - 7;
            let (blue, green, red) = if in_bubble || in_tail {
                (255, 255, 255)
            } else {
                (190, 105, 35)
            };
            icon.extend_from_slice(&[blue, green, red, 255]);
        }
    }

    // Zeroed AND mask (one bit per pixel, rows padded to 32 bits).
    icon.resize(icon.len() + mask_bytes as usize, 0);
    fs::write(icon_path, icon)?;
    Ok(())
}

fn main() {
    generate_windows_icon().expect("failed to generate Windows application icon");
    tauri_build::build()
}
