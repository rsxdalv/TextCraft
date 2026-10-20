//! The desktop's colour, for tinting the window's bars.
//!
//! A focused Windows 11 app draws its title bar on Mica, a blurred, darkened copy of
//! the wallpaper, which is why Notepad wears blue on a blue desktop. This shell cannot
//! sit on Mica (see `docs/ui-reference.md` §4), so it approximates the colour: the
//! wallpaper's average, darkened. It is read on a background thread — decoding a 4K
//! image is not something to do before the first frame — and the window repaints
//! once the answer is in.

use std::sync::OnceLock;

use egui::Color32;

static TINT: OnceLock<Option<Color32>> = OnceLock::new();

/// The tint, once it has been worked out; `None` until then and wherever there is no
/// wallpaper to read.
pub fn get() -> Option<Color32> {
    TINT.get().copied().flatten()
}

/// Starts working the tint out. Called once, before the first frame.
pub fn start(ctx: &egui::Context) {
    if cfg!(test) || !cfg!(windows) {
        return;
    }
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let _ = TINT.set(read_wallpaper_average());
        ctx.request_repaint();
    });
}

/// Windows keeps the current wallpaper, already converted to JPEG, under the user's
/// roaming profile.
fn wallpaper_path() -> Option<std::path::PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    let path = std::path::Path::new(&appdata)
        .join("Microsoft")
        .join("Windows")
        .join("Themes")
        .join("TranscodedWallpaper");
    path.exists().then_some(path)
}

fn read_wallpaper_average() -> Option<Color32> {
    let image = image::ImageReader::open(wallpaper_path()?)
        .ok()?
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?;
    let [r, g, b] = average_colour(&image.thumbnail(32, 32).to_rgb8());
    Some(darkened(r, g, b))
}

/// The mean of an image's pixels, per channel.
fn average_colour(image: &image::RgbImage) -> [u8; 3] {
    let count = u64::from(image.width()) * u64::from(image.height());
    if count == 0 {
        return [0; 3];
    }
    let mut sum = [0u64; 3];
    for pixel in image.pixels() {
        for (total, channel) in sum.iter_mut().zip(pixel.0) {
            *total += u64::from(channel);
        }
    }
    sum.map(|total| (total / count) as u8)
}

/// Pulls a colour down to a title-bar depth without losing its hue: scaled so the
/// brightest channel lands near `0x5a`, which is where Mica sits on a dark theme.
fn darkened(r: u8, g: u8, b: u8) -> Color32 {
    let brightest = r.max(g).max(b).max(1) as f32;
    let scale = (0x5a as f32 / brightest).min(1.0);
    let channel = |value: u8| (value as f32 * scale) as u8;
    Color32::from_rgb(channel(r), channel(g), channel(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bright_colour_is_pulled_down_and_keeps_its_hue() {
        let tint = darkened(0x20, 0x40, 0xff);
        let [r, g, b, _] = tint.to_array();
        assert!(
            (0x58..=0x5a).contains(&b),
            "the brightest channel lands near the target"
        );
        assert!(r < g && g < b, "and the order of the channels is kept");
    }

    #[test]
    fn a_colour_that_is_already_dark_is_left_alone() {
        assert_eq!(
            darkened(0x02, 0x00, 0x57),
            Color32::from_rgb(0x02, 0x00, 0x57)
        );
    }

    #[test]
    fn the_average_of_two_pixels_is_between_them() {
        let image = image::RgbImage::from_fn(2, 1, |x, _| {
            if x == 0 {
                image::Rgb([0, 100, 200])
            } else {
                image::Rgb([100, 100, 0])
            }
        });
        assert_eq!(average_colour(&image), [50, 100, 100]);
    }
}
