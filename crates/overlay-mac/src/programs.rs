use std::path::Path;

use objc2::rc::Retained;
use objc2_app_kit::{NSApplicationActivationPolicy, NSImage, NSWorkspace};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_core_graphics::{CGBitmapContextCreate, CGColorSpace, CGContext};
use objc2_foundation::NSString;

/// An app with a Dock presence, for picking games from a list.
#[derive(Debug, Clone)]
pub struct Program {
    /// Executable file name, which is what game rules match on.
    pub exe: String,
    pub path: String,
    pub title: String,
}

/// Straight-alpha BGRA pixels, top-down.
#[derive(Debug, Clone)]
pub struct Icon {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

/// Regular apps that are running, sorted by name.
pub fn running_programs() -> Vec<Program> {
    let apps = NSWorkspace::sharedWorkspace().runningApplications();
    let mut programs: Vec<Program> = apps
        .iter()
        .filter(|app| app.activationPolicy() == NSApplicationActivationPolicy::Regular)
        .filter_map(|app| {
            let path = app.executableURL()?.path()?.to_string();
            let exe = Path::new(&path).file_name()?.to_string_lossy().into_owned();
            let title = app.localizedName().map_or(exe.clone(), |n| n.to_string());
            Some(Program { exe, path, title })
        })
        .collect();
    programs.sort_by_key(|p| p.title.to_lowercase());
    programs.dedup_by(|a, b| a.exe == b.exe);
    programs
}

/// The app's icon at 32×32 pixels. For an executable inside a bundle, that's the
/// bundle's icon rather than the generic one for executables.
pub fn icon(path: &str) -> Option<Icon> {
    const SIZE: usize = 32;
    let bundle = Path::new(path)
        .ancestors()
        .find(|p| p.extension().is_some_and(|e| e == "app"))
        .unwrap_or(Path::new(path));
    let image: Retained<NSImage> =
        NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(&bundle.to_string_lossy()));
    let mut rect = CGRect::new(
        CGPoint::new(0.0, 0.0),
        CGSize::new(SIZE as f64, SIZE as f64),
    );
    // SAFETY: `rect` is a valid in/out rect; no context or hints.
    let cg_image = unsafe { image.CGImageForProposedRect_context_hints(&mut rect, None, None) }?;

    let mut pixels = vec![0u8; SIZE * SIZE * 4];
    let space = CGColorSpace::new_device_rgb()?;
    let info = crate::window::premultiplied_bgra();
    // SAFETY: `pixels` holds SIZE rows of SIZE premultiplied BGRA pixels, as described,
    // and outlives the context, which is dropped at the end of this block.
    unsafe {
        let context = CGBitmapContextCreate(
            pixels.as_mut_ptr().cast(),
            SIZE,
            SIZE,
            8,
            SIZE * 4,
            Some(&space),
            info.0,
        )?;
        let bounds = CGRect::new(
            CGPoint::new(0.0, 0.0),
            CGSize::new(SIZE as f64, SIZE as f64),
        );
        CGContext::draw_image(Some(&context), bounds, Some(&cg_image));
    }
    for px in pixels.as_chunks_mut::<4>().0 {
        let a = px[3] as u32;
        if a > 0 && a < 255 {
            for c in &mut px[..3] {
                *c = (*c as u32 * 255 / a).min(255) as u8;
            }
        }
    }
    Some(Icon {
        width: SIZE as u32,
        height: SIZE as u32,
        bgra: pixels,
    })
}
