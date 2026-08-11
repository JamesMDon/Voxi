use std::ffi::c_void;
use std::mem::size_of;
use windows::core::Result;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Graphics::Gdi::{
    CreateDIBSection, DeleteObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP,
    HDC,
};
use windows::Win32::UI::WindowsAndMessaging::{
    SetMenuItemInfoW, HMENU, MENUITEMINFOW, MIIM_BITMAP,
};

const ICON_SIZE: i32 = 16;
const PIXEL_COUNT: usize = (ICON_SIZE * ICON_SIZE) as usize;
const SAMPLE_SCALE: usize = 4;
const BLUE: u32 = 0x00AAFF;
const RED: u32 = 0xFF4444;

#[derive(Clone, Copy)]
enum IconKind {
    Read,
    Speed,
    Voice,
    Exit,
}

struct OwnedBitmap(HBITMAP);

impl Drop for OwnedBitmap {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.0);
        }
    }
}

pub(crate) struct MenuIcons {
    bitmaps: [OwnedBitmap; 4],
}

impl MenuIcons {
    pub(crate) unsafe fn install(menu: HMENU, item_ids: [usize; 4]) -> Result<Self> {
        let icons = Self {
            bitmaps: [
                create_bitmap(IconKind::Read)?,
                create_bitmap(IconKind::Speed)?,
                create_bitmap(IconKind::Voice)?,
                create_bitmap(IconKind::Exit)?,
            ],
        };

        for (item_id, bitmap) in item_ids.into_iter().zip(&icons.bitmaps) {
            SetMenuItemInfoW(
                menu,
                item_id as u32,
                false,
                &MENUITEMINFOW {
                    cbSize: size_of::<MENUITEMINFOW>() as u32,
                    fMask: MIIM_BITMAP,
                    hbmpItem: bitmap.0,
                    ..Default::default()
                },
            )?;
        }

        Ok(icons)
    }
}

unsafe fn create_bitmap(kind: IconKind) -> Result<OwnedBitmap> {
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: ICON_SIZE,
            biHeight: -ICON_SIZE,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            biSizeImage: (PIXEL_COUNT * size_of::<u32>()) as u32,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits: *mut c_void = std::ptr::null_mut();
    let bitmap = CreateDIBSection(
        HDC::default(),
        &info,
        DIB_RGB_COLORS,
        &mut bits,
        HANDLE::default(),
        0,
    )?;

    let pixels = rasterize(kind);
    std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits.cast::<u32>(), PIXEL_COUNT);
    Ok(OwnedBitmap(bitmap))
}

fn rasterize(kind: IconKind) -> [u32; PIXEL_COUNT] {
    let color = if matches!(kind, IconKind::Exit) {
        RED
    } else {
        BLUE
    };
    let mut pixels = [0; PIXEL_COUNT];

    for pixel_y in 0..ICON_SIZE as usize {
        for pixel_x in 0..ICON_SIZE as usize {
            let mut covered = 0;
            for sample_y in 0..SAMPLE_SCALE {
                for sample_x in 0..SAMPLE_SCALE {
                    let x = pixel_x as f32 + (sample_x as f32 + 0.5) / SAMPLE_SCALE as f32;
                    let y = pixel_y as f32 + (sample_y as f32 + 0.5) / SAMPLE_SCALE as f32;
                    if contains(kind, x, y) {
                        covered += 1;
                    }
                }
            }

            let alpha = (covered * 255 / (SAMPLE_SCALE * SAMPLE_SCALE)) as u8;
            pixels[pixel_y * ICON_SIZE as usize + pixel_x] = premultiplied(color, alpha);
        }
    }
    pixels
}

fn contains(kind: IconKind, x: f32, y: f32) -> bool {
    match kind {
        IconKind::Read => {
            rounded_rect(x, y, 2.0, 2.5, 13.5, 11.5, 2.5)
                || triangle(x, y, (4.0, 13.75), (5.0, 10.25), (8.1, 10.25))
        }
        IconKind::Speed => {
            triangle(x, y, (2.0, 3.0), (2.0, 13.0), (8.0, 8.0))
                || triangle(x, y, (7.5, 3.0), (7.5, 13.0), (13.5, 8.0))
        }
        IconKind::Voice => {
            rounded_rect(x, y, 1.8, 6.5, 4.0, 9.5, 1.0)
                || rounded_rect(x, y, 4.8, 4.5, 7.0, 11.5, 1.0)
                || rounded_rect(x, y, 7.8, 2.5, 10.0, 13.5, 1.0)
                || rounded_rect(x, y, 10.8, 5.5, 13.0, 10.5, 1.0)
        }
        IconKind::Exit => {
            distance_to_segment(x, y, 3.5, 3.5, 12.5, 12.5) <= 1.15
                || distance_to_segment(x, y, 12.5, 3.5, 3.5, 12.5) <= 1.15
        }
    }
}

fn rounded_rect(x: f32, y: f32, left: f32, top: f32, right: f32, bottom: f32, r: f32) -> bool {
    let nearest_x = x.clamp(left + r, right - r);
    let nearest_y = y.clamp(top + r, bottom - r);
    let dx = x - nearest_x;
    let dy = y - nearest_y;
    dx * dx + dy * dy <= r * r
}

fn triangle(x: f32, y: f32, a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> bool {
    let sign = |p1: (f32, f32), p2: (f32, f32), p3: (f32, f32)| {
        (p1.0 - p3.0) * (p2.1 - p3.1) - (p2.0 - p3.0) * (p1.1 - p3.1)
    };
    let point = (x, y);
    let d1 = sign(point, a, b);
    let d2 = sign(point, b, c);
    let d3 = sign(point, c, a);
    let has_negative = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let has_positive = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(has_negative && has_positive)
}

fn distance_to_segment(x: f32, y: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let dx = x2 - x1;
    let dy = y2 - y1;
    let length_squared = dx * dx + dy * dy;
    let t = (((x - x1) * dx + (y - y1) * dy) / length_squared).clamp(0.0, 1.0);
    let nearest_x = x1 + t * dx;
    let nearest_y = y1 + t * dy;
    ((x - nearest_x).powi(2) + (y - nearest_y).powi(2)).sqrt()
}

fn premultiplied(rgb: u32, alpha: u8) -> u32 {
    if alpha == 0 {
        return 0;
    }
    let alpha = alpha as u32;
    let red = ((rgb >> 16) & 0xFF) * alpha / 255;
    let green = ((rgb >> 8) & 0xFF) * alpha / 255;
    let blue = (rgb & 0xFF) * alpha / 255;
    (alpha << 24) | (red << 16) | (green << 8) | blue
}

#[cfg(test)]
mod tests {
    use super::{rasterize, IconKind, PIXEL_COUNT};

    #[test]
    fn menu_icons_have_antialiased_transparent_edges() {
        for kind in [
            IconKind::Read,
            IconKind::Speed,
            IconKind::Voice,
            IconKind::Exit,
        ] {
            let pixels = rasterize(kind);
            assert_eq!(pixels.len(), PIXEL_COUNT);
            assert_eq!(pixels[0], 0);
            assert!(pixels.iter().any(|pixel| pixel >> 24 == 255));
            assert!(pixels.iter().any(|pixel| (1..255).contains(&(pixel >> 24))));
        }
    }

    #[test]
    fn read_icon_has_a_solid_center() {
        let pixels = rasterize(IconKind::Read);
        for x in [5, 8, 10] {
            assert_eq!(pixels[7 * 16 + x] >> 24, 255);
        }
    }
}
