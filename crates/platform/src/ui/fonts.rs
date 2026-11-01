//! The faces a document can be drawn in, and the code that makes the settings dropdown
//! mean something.
//!
//! ## Why the list is what it is
//!
//! The `Settings ▸ Font` dropdown used to offer Consolas, Cascadia Mono, Segoe UI and
//! Courier New. Not one of those four is in this build: they are Windows font names, and
//! an `egui` application that ships its own fonts never looks at the ones an operating
//! system happens to have installed. So every option in that list painted exactly the
//! same face — a control that reports a choice and does nothing with it, and one that
//! named fonts a person could not get by choosing them.
//!
//! What is offered now is what is shipped: four monospaced families under their real
//! names, each embedded from the OFL file it comes in, so choosing one changes the glyphs
//! on screen. `crates/platform/assets/fonts/README.md` records where each file came from
//! and the licence it ships under; the licence texts sit beside the fonts.
//!
//! All four are monospaced, because both surfaces that draw a document assume it: the
//! rendered Markdown view indents with spaces, which is exact only when a level is
//! exactly its source's worth of them ([`crate::ui::markdown_view`]). A proportional
//! family in this list would misrender one view or the other.
//!
//! Atkinson Hyperlegible Mono is the lever `NC-P007` needs: distinguishable letterforms
//! are its whole reason for existing, and it is the family here a low-vision reader can
//! ask for without leaving the app.

use egui::{Context, FontData, FontDefinitions, FontFamily};

/// The families the settings page offers, in the order it shows them, under the names
/// their authors give them. That is the point of the list: what it says is what it is.
pub const FAMILIES: [&str; 4] = [
    "JetBrains Mono",
    "IBM Plex Mono",
    "Atkinson Hyperlegible Mono",
    "Fira Mono",
];

/// What a shell starts with, and what a stored name this build does not carry falls back
/// to — the same rule the settings store applies to a value it cannot honour.
pub const DEFAULT: &str = FAMILIES[0];

/// The shipped faces, each under the name `egui` knows it by. The names here and
/// [`FAMILIES`] are the same strings, asserted below: a family offered without a file
/// would be the bug this module exists to fix, wearing a different hat.
const FILES: [(&str, &[u8]); FAMILIES.len()] = [
    (
        "JetBrains Mono",
        include_bytes!("../../assets/fonts/JetBrainsMono[wght].ttf"),
    ),
    (
        "IBM Plex Mono",
        include_bytes!("../../assets/fonts/IBMPlexMono-Regular.ttf"),
    ),
    (
        "Atkinson Hyperlegible Mono",
        include_bytes!("../../assets/fonts/AtkinsonHyperlegibleMono[wght].ttf"),
    ),
    (
        "Fira Mono",
        include_bytes!("../../assets/fonts/FiraMono-Regular.ttf"),
    ),
];

/// The faces behind every choice: `egui`'s own built-ins. The tail is not decoration — a
/// document can hold a box-drawing character, a CJK letter or an emoji that none of these
/// four covers, and a family with nothing behind it renders such a glyph as nothing at
/// all. `egui` walks a family's list in order, so the chosen face is the one that draws
/// everything it has, and these are what draw the rest.
const FALLBACK: [&str; 4] = [
    "Hack",
    "Ubuntu-Light",
    "NotoEmoji-Regular",
    "emoji-icon-font",
];

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// A font discovered on the host operating system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemFont {
    /// The family name extracted from the font's metadata.
    pub name: &'static str,
    /// Absolute path to the font file.
    pub path: PathBuf,
}

/// Discovered system fonts on the user's computer.
pub fn system_fonts() -> &'static [SystemFont] {
    static DISCOVERED: OnceLock<Vec<SystemFont>> = OnceLock::new();
    DISCOVERED.get_or_init(|| {
        let mut dirs = Vec::new();
        #[cfg(target_os = "windows")]
        {
            if let Some(windir) = std::env::var_os("WINDIR") {
                dirs.push(Path::new(&windir).join("Fonts"));
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            dirs.push(PathBuf::from("/usr/share/fonts"));
            dirs.push(PathBuf::from("/usr/local/share/fonts"));
            if let Some(home) = std::env::var_os("HOME") {
                let home_p = Path::new(&home);
                dirs.push(home_p.join(".local/share/fonts"));
                dirs.push(home_p.join(".fonts"));
            }
        }

        let mut list = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for d in dirs {
            if !d.is_dir() {
                continue;
            }
            scan_dir_for_fonts(&d, &mut list, &mut seen);
        }

        list.sort_by(|a, b| a.name.cmp(b.name));
        list
    })
}

fn scan_dir_for_fonts(
    dir: &Path,
    list: &mut Vec<SystemFont>,
    seen: &mut std::collections::HashSet<String>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_dir_for_fonts(&path, list, seen);
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let ext_l = ext.to_ascii_lowercase();
            if ext_l == "ttf" || ext_l == "otf" {
                if let Ok(bytes) = std::fs::read(&path) {
                    if let Some(name) = extract_ttf_name(&bytes) {
                        if !FAMILIES.contains(&name.as_str()) && !seen.contains(&name) {
                            seen.insert(name.clone());
                            let leaked: &'static str = Box::leak(name.into_boxed_str());
                            list.push(SystemFont { name: leaked, path });
                        }
                    }
                }
            }
        }
    }
}

/// Extracts the font family name (Name ID 1) from a TrueType or OpenType font byte buffer.
fn extract_ttf_name(data: &[u8]) -> Option<String> {
    if data.len() < 12 {
        return None;
    }
    let num_tables = u16::from_be_bytes([data[4], data[5]]) as usize;
    for i in 0..num_tables {
        let offset = 12 + i * 16;
        if offset + 16 > data.len() {
            break;
        }
        let tag = &data[offset..offset + 4];
        if tag == b"name" {
            let table_offset = u32::from_be_bytes([
                data[offset + 8],
                data[offset + 9],
                data[offset + 10],
                data[offset + 11],
            ]) as usize;
            if table_offset + 6 > data.len() {
                break;
            }
            let count =
                u16::from_be_bytes([data[table_offset + 2], data[table_offset + 3]]) as usize;
            let string_offset =
                u16::from_be_bytes([data[table_offset + 4], data[table_offset + 5]]) as usize;
            let storage = table_offset + string_offset;

            for target_id in [1, 4] {
                for r in 0..count {
                    let rec = table_offset + 6 + r * 12;
                    if rec + 12 > data.len() {
                        break;
                    }
                    let plat = u16::from_be_bytes([data[rec], data[rec + 1]]);
                    let name_id = u16::from_be_bytes([data[rec + 6], data[rec + 7]]);
                    let length = u16::from_be_bytes([data[rec + 8], data[rec + 9]]) as usize;
                    let s_offset = u16::from_be_bytes([data[rec + 10], data[rec + 11]]) as usize;

                    if name_id == target_id {
                        let start = storage + s_offset;
                        if start + length <= data.len() {
                            let raw = &data[start..start + length];
                            if plat == 0 || plat == 3 {
                                let u16_chars: Vec<u16> = raw
                                    .chunks_exact(2)
                                    .map(|chunk| u16::from_be_bytes([chunk[0], chunk[1]]))
                                    .collect();
                                if let Ok(s) = String::from_utf16(&u16_chars) {
                                    let trimmed = s.trim().to_owned();
                                    if !trimmed.is_empty() {
                                        return Some(trimmed);
                                    }
                                }
                            } else if let Ok(s) = std::str::from_utf8(raw) {
                                let trimmed = s.trim().to_owned();
                                if !trimmed.is_empty() {
                                    return Some(trimmed);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// A family's chain: the chosen face first, then the built-ins.
fn chain(chosen: &str) -> Vec<String> {
    std::iter::once(chosen)
        .chain(FALLBACK)
        .map(str::to_owned)
        .collect()
}

/// Whether `choice` is one of the families this build carries.
fn carried(choice: &str) -> bool {
    FAMILIES.contains(&choice) || system_fonts().iter().any(|f| f.name == choice)
}

/// The key a context remembers its installed face under.
fn installed_key() -> egui::Id {
    egui::Id::new("textcraft-installed-font")
}

use std::collections::HashSet;
use std::sync::Mutex;

static LOADED_SYSTEM_FONTS: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// Whether `name` is already bound to a font family in this build or loaded from the system.
pub fn is_font_bound(name: &str) -> bool {
    if FAMILIES.contains(&name) {
        return true;
    }
    if let Ok(guard) = LOADED_SYSTEM_FONTS.lock() {
        if let Some(set) = guard.as_ref() {
            return set.contains(name);
        }
    }
    false
}

/// Ensures that `name` is registered and bound in this context's font definitions,
/// loading system font file bytes on demand if needed. Returns `true` if available.
pub fn ensure_font(ctx: &Context, name: &str) -> bool {
    if is_font_bound(name) {
        return true;
    }
    if let Some(sys) = system_fonts().iter().find(|f| f.name == name) {
        if let Ok(_bytes) = std::fs::read(&sys.path) {
            if let Ok(mut guard) = LOADED_SYSTEM_FONTS.lock() {
                guard
                    .get_or_insert_with(HashSet::new)
                    .insert(name.to_owned());
            }
            let current = ctx
                .data(|data| data.get_temp::<String>(installed_key()))
                .unwrap_or_else(|| DEFAULT.to_owned());
            ctx.data_mut(|data| data.remove_temp::<String>(installed_key()));
            apply(ctx, &current);
            return true;
        }
    }
    false
}

/// Points the document face at `choice`, registering the shipped files the first time
/// this context asks for one.
pub fn apply(ctx: &Context, choice: &str) {
    let wanted = if carried(choice) { choice } else { DEFAULT };
    let key = installed_key();
    if ctx.data(|data| data.get_temp::<String>(key)).as_deref() == Some(wanted) {
        return;
    }

    let mut fonts = FontDefinitions::default();
    if let Some(bytes) = system_ui_font() {
        fonts.font_data.insert(
            "Segoe UI".to_owned(),
            FontData::from_owned(bytes.clone()).into(),
        );
        if let Some(chain) = fonts.families.get_mut(&FontFamily::Proportional) {
            chain.insert(0, "Segoe UI".to_owned());
        }
    }
    for (name, bytes) in FILES {
        fonts
            .font_data
            .insert(name.to_owned(), FontData::from_static(bytes).into());
    }
    for family in FAMILIES {
        fonts
            .families
            .insert(FontFamily::Name(family.into()), chain(family));
    }

    // Also include any previously loaded system fonts!
    if let Ok(guard) = LOADED_SYSTEM_FONTS.lock() {
        if let Some(set) = guard.as_ref() {
            for extra in set {
                if let Some(sys) = system_fonts().iter().find(|f| f.name == extra.as_str()) {
                    if let Ok(bytes) = std::fs::read(&sys.path) {
                        fonts
                            .font_data
                            .insert(extra.clone(), FontData::from_owned(bytes).into());
                        fonts
                            .families
                            .insert(FontFamily::Name(extra.clone().into()), chain(extra));
                    }
                }
            }
        }
    }

    // If `wanted` is an operating system font, load its file bytes!
    if let Some(sys) = system_fonts().iter().find(|f| f.name == wanted) {
        if let Ok(bytes) = std::fs::read(&sys.path) {
            fonts
                .font_data
                .insert(wanted.to_owned(), FontData::from_owned(bytes).into());
            fonts
                .families
                .insert(FontFamily::Name(wanted.into()), chain(wanted));
            if let Ok(mut guard) = LOADED_SYSTEM_FONTS.lock() {
                guard
                    .get_or_insert_with(HashSet::new)
                    .insert(wanted.to_owned());
            }
        }
    }

    fonts.families.insert(FontFamily::Monospace, chain(wanted));

    ctx.set_fonts(fonts);
    ctx.data_mut(|data| data.insert_temp(key, wanted.to_owned()));
}

/// The bytes of the operating system's UI font, if this machine has one.
fn system_ui_font() -> Option<&'static Vec<u8>> {
    static FONT: std::sync::OnceLock<Option<Vec<u8>>> = std::sync::OnceLock::new();
    FONT.get_or_init(|| {
        let windir = std::env::var_os("WINDIR")?;
        std::fs::read(
            std::path::Path::new(&windir)
                .join("Fonts")
                .join("segoeui.ttf"),
        )
        .ok()
    })
    .as_ref()
}

/// Brings a context up with the default face, for the moment before its first frame —
/// the same part [`crate::ui::theme::install`] serves for colours.
pub fn install(ctx: &Context) {
    apply(ctx, DEFAULT);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Brings a context up with `choice` and runs the frame that makes it take effect: a
    /// context picks up new font definitions at the start of a pass, so the face it has
    /// been given cannot be read before one has run.
    fn brought_up(ctx: &egui::Context, choice: &str) {
        apply(ctx, choice);
        let mut output = ctx.run_ui(egui::RawInput::default(), |_| {});
        output.textures_delta.clear();
    }

    /// The marks a context paints for `text` after being brought up with `choice`: the
    /// width the text lays out to, and a hash of the font atlas it is rasterized into.
    fn marks_of(ctx: &egui::Context, choice: &str, text: &str) -> (f32, u64) {
        brought_up(ctx, choice);
        ctx.fonts_mut(|fonts| {
            let galley = fonts.layout(
                text.to_owned(),
                egui::FontId::monospace(15.0),
                egui::Color32::WHITE,
                f32::INFINITY,
            );
            (galley.rect.width(), atlas_hash(&fonts.image()))
        })
    }

    /// FNV-1a over the rasterized glyph atlas.
    fn atlas_hash(image: &egui::ColorImage) -> u64 {
        image
            .pixels
            .iter()
            .fold(0xcbf2_9ce4_8422_2325u64, |hash, pixel| {
                pixel.to_array().into_iter().fold(hash, |hash, byte| {
                    (hash ^ byte as u64).wrapping_mul(0x0000_0100_0000_01b3)
                })
            })
    }

    /// The chain a context resolves `FontFamily::Monospace` to.
    fn monospace_chain(ctx: &egui::Context) -> Vec<String> {
        ctx.fonts(|fonts| {
            fonts
                .definitions()
                .families
                .get(&FontFamily::Monospace)
                .cloned()
                .unwrap_or_default()
        })
    }

    #[test]
    fn every_family_the_settings_page_offers_ships_a_file() {
        // The bug this pins: the dropdown's list and the shipped faces drifting apart, so
        // an option paints some other face. `FILES` is declared with
        // `FAMILIES.len()` entries, which keeps the count honest at compile time; this
        // keeps the *names* honest.
        for family in FAMILIES {
            assert!(
                FILES.iter().any(|(name, _)| *name == family),
                "{family} is offered but nothing is shipped for it"
            );
        }
        for (name, _) in FILES {
            assert!(
                carried(name),
                "{name} is shipped but the settings page never offers it"
            );
        }
    }

    #[test]
    fn a_shipped_file_is_a_font_file() {
        // Cheap and deliberately dumb: every TrueType/glyf font starts with an sfnt
        // version (`\0\x01\0\x00`, or `true`/`ttcf` for the Apple flavours) and every
        // OpenType-with-CFF one with `OTTO`. A truncated download, an HTML error page
        // saved as `.ttf`, or a git-lfs pointer starts none of these ways.
        for (name, bytes) in FILES {
            let head = &bytes[..4.min(bytes.len())];
            let sfnt = head == [0, 1, 0, 0] || head == b"true" || head == b"ttcf";
            assert!(
                sfnt || head == b"OTTO",
                "{name}: starts with {head:02x?}, which is not a font"
            );
        }
    }

    #[test]
    fn choosing_a_family_changes_the_glyphs_that_are_painted() {
        // The claim `NC-P002` makes — "applies to the editor" — in the form that can be
        // checked without a screen: the same string at the same size comes out of the
        // rasterizer as different marks for each of the four families. Comparing widths
        // was tried first and is the wrong instrument: three of these four families share
        // a 0.6 em advance, so JetBrains Mono and Fira Mono measure identically while
        // being obviously different faces. The atlas is where a font actually differs.
        let ctx = egui::Context::default();
        let text = "The quick brown fox jumps 0123456789";
        let mut marks = Vec::new();
        for family in FAMILIES {
            let (width, hash) = marks_of(&ctx, family, text);
            assert!(width > 0.0, "{family} laid out to nothing");
            marks.push((family, width, hash));
        }

        for (family, _, hash) in &marks {
            for (other, _, other_hash) in &marks {
                if family != other {
                    assert_ne!(
                        hash, other_hash,
                        "{family} and {other} rasterize to the same glyphs, so the choice \
                         selects one face for both"
                    );
                }
            }
        }
    }

    #[test]
    fn the_chosen_family_is_first_in_the_chain_and_the_built_ins_stay_behind_it() {
        // Both halves matter. First, the choice has to lead the chain, or it does nothing.
        // Second, the chain must not *end* with the choice: these four are Latin-text
        // fonts, and a document holding a glyph none of them carries has to fall back to
        // something rather than paint a hole.
        let ctx = egui::Context::default();
        for family in FAMILIES {
            brought_up(&ctx, family);
            let chain = monospace_chain(&ctx);
            assert_eq!(
                chain.first().map(String::as_str),
                Some(family),
                "{family} is not what the document face resolves to"
            );
            for fallback in FALLBACK {
                assert!(
                    chain[1..].contains(&fallback.to_owned()),
                    "{fallback} is missing from the chain behind {family}, so a glyph it \
                     lacks would render as nothing"
                );
            }
        }
    }

    #[test]
    fn a_family_this_build_does_not_carry_falls_back_to_the_default() {
        // A settings file can name anything: a newer build's list, a hand-edited line.
        // The rule is the store's — honour what is known, otherwise start over.
        let ctx = egui::Context::default();
        for value in ["Consolas", "Segoe UI", "", "   "] {
            brought_up(&ctx, value);
            assert_eq!(
                monospace_chain(&ctx).first().map(String::as_str),
                Some(DEFAULT),
                "{value:?} resolved to something other than the default"
            );
        }
    }

    #[test]
    fn the_default_family_is_one_the_settings_page_offers() {
        assert!(
            carried(DEFAULT),
            "the starting face is not in the list the user is shown"
        );
    }
}
