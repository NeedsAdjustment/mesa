use std::sync::{LazyLock, Mutex};

/// Mesa blue, used when the wallpaper cannot be sampled.
pub const FALLBACK_ACCENT: (u8, u8, u8) = (8, 101, 254);

static CACHED_ACCENT: LazyLock<Mutex<Option<(u8, u8, u8)>>> = LazyLock::new(|| Mutex::new(None));

/// Returns a usable accent colour derived from the desktop wallpaper.
///
/// The sampled colour is normalised to a medium-lightness, moderately
/// saturated tone so it remains legible on both light and dark acrylic.
pub fn accent_rgb() -> (u8, u8, u8) {
    if let Ok(cache) = CACHED_ACCENT.lock()
        && let Some(accent) = *cache
    {
        return accent;
    }

    let accent = detect_wallpaper_accent().unwrap_or(FALLBACK_ACCENT);
    eprintln!(
        "[mesa] global theme accent: #{:02x}{:02x}{:02x}",
        accent.0, accent.1, accent.2
    );

    if let Ok(mut cache) = CACHED_ACCENT.lock() {
        *cache = Some(accent);
    }

    accent
}

#[derive(Clone, Copy, Default)]
struct Bucket {
    r: f32,
    g: f32,
    b: f32,
    weight: f32,
}

#[cfg(target_os = "windows")]
fn detect_wallpaper_accent() -> Option<(u8, u8, u8)> {
    let path = wallpaper_path()?;
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase());
    if !matches!(extension.as_deref(), Some("jpg" | "jpeg")) {
        return None;
    }

    let file = std::fs::File::open(path).ok()?;
    let mut decoder = jpeg_decoder::Decoder::new(file);
    decoder.scale(96, 96).ok()?;
    let pixels = decoder.decode().ok()?;
    let info = decoder.info()?;
    if info.pixel_format != jpeg_decoder::PixelFormat::RGB24 {
        return None;
    }

    let width = info.width as usize;
    let height = info.height as usize;
    if width == 0 || height == 0 {
        return None;
    }

    let step_x = (width / 80).max(1);
    let step_y = (height / 80).max(1);
    let mut buckets = [Bucket::default(); 24];

    for y in (0..height).step_by(step_y) {
        for x in (0..width).step_by(step_x) {
            let index = (y * width + x) * 3;
            let Some(pixel) = pixels.get(index..index + 3) else {
                continue;
            };
            let r = pixel[0] as f32 / 255.0;
            let g = pixel[1] as f32 / 255.0;
            let b = pixel[2] as f32 / 255.0;
            let (hue, saturation, lightness) = rgb_to_hsl(r, g, b);

            if saturation < 0.18 || !(0.12..=0.88).contains(&lightness) {
                continue;
            }

            let bucket_index = ((hue / 15.0).floor() as usize).min(23);
            let weight = saturation * (1.0 - (lightness - 0.5).abs() * 1.2);
            let bucket = &mut buckets[bucket_index];
            bucket.r += r * weight;
            bucket.g += g * weight;
            bucket.b += b * weight;
            bucket.weight += weight;
        }
    }

    let best = buckets
        .iter()
        .max_by(|left, right| left.weight.total_cmp(&right.weight))?;
    if best.weight <= 0.0 {
        return None;
    }

    let average = (
        best.r / best.weight,
        best.g / best.weight,
        best.b / best.weight,
    );
    let (hue, saturation, _) = rgb_to_hsl(average.0, average.1, average.2);
    let saturation = (saturation * 1.35).clamp(0.36, 0.68);
    let (r, g, b) = hsl_to_rgb(hue, saturation, 0.56);

    Some((
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
    ))
}

#[cfg(not(target_os = "windows"))]
fn detect_wallpaper_accent() -> Option<(u8, u8, u8)> {
    None
}

#[cfg(target_os = "windows")]
fn wallpaper_path() -> Option<std::path::PathBuf> {
    use std::ffi::{OsString, c_void};
    use std::os::windows::ffi::OsStringExt;

    const SPI_GETDESKWALLPAPER: u32 = 0x0073;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn SystemParametersInfoW(
            action: u32,
            parameter: u32,
            data: *mut c_void,
            win_ini: u32,
        ) -> i32;
    }

    let mut buffer = [0u16; 260];
    let succeeded = unsafe {
        SystemParametersInfoW(
            SPI_GETDESKWALLPAPER,
            buffer.len() as u32,
            buffer.as_mut_ptr().cast(),
            0,
        )
    };
    if succeeded == 0 {
        return None;
    }

    let length = buffer
        .iter()
        .position(|character| *character == 0)
        .unwrap_or(buffer.len());
    if length == 0 {
        return None;
    }

    let path = std::path::PathBuf::from(OsString::from_wide(&buffer[..length]));
    path.is_file().then_some(path)
}

fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let lightness = (max + min) / 2.0;

    if (max - min).abs() < f32::EPSILON {
        return (0.0, 0.0, lightness);
    }

    let delta = max - min;
    let saturation = if lightness > 0.5 {
        delta / (2.0 - max - min)
    } else {
        delta / (max + min)
    };
    let hue = if max == r {
        ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    } * 60.0;

    (hue, saturation, lightness)
}

fn hsl_to_rgb(hue: f32, saturation: f32, lightness: f32) -> (f32, f32, f32) {
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let hue = hue.rem_euclid(360.0) / 60.0;
    let x = chroma * (1.0 - (hue.rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = if hue < 1.0 {
        (chroma, x, 0.0)
    } else if hue < 2.0 {
        (x, chroma, 0.0)
    } else if hue < 3.0 {
        (0.0, chroma, x)
    } else if hue < 4.0 {
        (0.0, x, chroma)
    } else if hue < 5.0 {
        (x, 0.0, chroma)
    } else {
        (chroma, 0.0, x)
    };
    let m = lightness - chroma / 2.0;

    (r + m, g + m, b + m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsl_round_trip_preserves_hue_and_saturation() {
        const SATURATION: f32 = 0.5;
        const LIGHTNESS: f32 = 0.56;

        for expected_hue in [0.0, 45.0, 90.0, 135.0, 180.0, 225.0, 270.0, 315.0] {
            let (r, g, b) = hsl_to_rgb(expected_hue, SATURATION, LIGHTNESS);
            let (hue, saturation, lightness) = rgb_to_hsl(r, g, b);

            assert!(
                (hue - expected_hue).abs() < 1.0,
                "hue {expected_hue} became {hue}"
            );
            assert!(
                (saturation - SATURATION).abs() < 0.01,
                "saturation for hue {expected_hue} became {saturation}"
            );
            assert!(
                (lightness - LIGHTNESS).abs() < 0.01,
                "lightness for hue {expected_hue} became {lightness}"
            );
        }
    }

    #[test]
    fn fallback_accent_is_mesa_blue_and_usable() {
        assert_eq!(FALLBACK_ACCENT, (8, 101, 254));

        let (r, g, b) = FALLBACK_ACCENT;
        let (_, saturation, lightness) =
            rgb_to_hsl(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);

        assert!(saturation > 0.5, "fallback should stay saturated");
        assert!(
            (0.3..=0.7).contains(&lightness),
            "fallback should stay legible"
        );
    }
}
