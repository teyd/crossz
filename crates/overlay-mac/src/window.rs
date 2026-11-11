//! The overlay panel. AppKit windows belong to the main thread, so everything here runs
//! there; other threads hand work over with `on_main`.

use std::cell::RefCell;

use crossz_core::Image;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSPanel, NSScreenSaverWindowLevel, NSWindowCollectionBehavior,
    NSWindowSharingType, NSWindowStyleMask,
};
use objc2_core_foundation::{CFData, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGDataProvider, CGImage, CGImageAlphaInfo,
    CGImageByteOrderInfo,
};
use objc2_quartz_core::kCAFilterNearest;

thread_local! {
    static PANEL: RefCell<Option<Retained<NSPanel>>> = const { RefCell::new(None) };
}

/// Where and what to show, in AppKit coordinates (points, origin bottom left).
pub struct Frame {
    pub x: f64,
    pub y: f64,
    /// Pixels per point on the display it's on.
    pub scale: f64,
}

pub fn create(mtm: MainThreadMarker) {
    let style = NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel;
    let rect = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(1.0, 1.0));
    let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
        NSPanel::alloc(mtm),
        rect,
        style,
        NSBackingStoreType::Buffered,
        false,
    );
    // SAFETY: we keep the panel alive ourselves in PANEL; AppKit must not free it on close.
    unsafe { panel.setReleasedWhenClosed(false) };
    panel.setOpaque(false);
    panel.setBackgroundColor(Some(&NSColor::clearColor()));
    panel.setHasShadow(false);
    panel.setIgnoresMouseEvents(true);
    // Panels hide whenever their app stops being active, e.g. on switching from the
    // settings window to the game; the crosshair must stay.
    panel.setHidesOnDeactivate(false);
    // Above other windows, including fullscreen games in their own Space.
    panel.setLevel(NSScreenSaverWindowLevel);
    panel.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );
    if let Some(view) = panel.contentView() {
        view.setWantsLayer(true);
        if let Some(layer) = view.layer() {
            // SAFETY: setting a filter constant on a layer we own.
            unsafe { layer.setMagnificationFilter(kCAFilterNearest) };
        }
    }
    PANEL.with(|p| *p.borrow_mut() = Some(panel));
}

pub fn show(image: &Image, frame: &Frame) {
    PANEL.with(|p| {
        let Some(panel) = p.borrow().clone() else {
            return;
        };
        let Some(cg_image) = cg_image(image) else {
            return;
        };
        let size = CGSize::new(
            image.width as f64 / frame.scale,
            image.height as f64 / frame.scale,
        );
        panel.setFrame_display(CGRect::new(CGPoint::new(frame.x, frame.y), size), false);
        if let Some(layer) = panel.contentView().and_then(|v| v.layer()) {
            layer.setContentsScale(frame.scale);
            // SAFETY: a CGImage is a valid layer `contents` object.
            unsafe {
                let contents: &AnyObject = &*(&*cg_image as *const CGImage).cast::<AnyObject>();
                layer.setContents(Some(contents));
            }
        }
        panel.orderFrontRegardless();
    });
}

pub fn hide() {
    PANEL.with(|p| {
        if let Some(panel) = p.borrow().as_ref() {
            panel.orderOut(None);
        }
    });
}

pub fn exclude_from_capture(exclude: bool) {
    PANEL.with(|p| {
        if let Some(panel) = p.borrow().as_ref() {
            let sharing = if exclude {
                NSWindowSharingType::None
            } else {
                NSWindowSharingType::ReadOnly
            };
            panel.setSharingType(sharing);
        }
    });
}

/// Premultiplied BGRA: 32-bit little-endian pixels with alpha first.
pub fn premultiplied_bgra() -> CGBitmapInfo {
    CGBitmapInfo(CGImageAlphaInfo::PremultipliedFirst.0 | CGImageByteOrderInfo::Order32Little.0)
}

/// Our premultiplied BGRA pixels as a CGImage: 32-bit little-endian with alpha first is
/// exactly that byte order.
fn cg_image(image: &Image) -> Option<objc2_core_foundation::CFRetained<CGImage>> {
    // SAFETY: the pointer and length describe `image.bgra`, which CFData copies.
    let data = unsafe { CFData::new(None, image.bgra.as_ptr(), image.bgra.len() as isize) }?;
    let provider = CGDataProvider::with_cf_data(Some(&data))?;
    let space = CGColorSpace::new_device_rgb()?;
    let info = premultiplied_bgra();
    // SAFETY: dimensions and row length match the buffer; no decode array.
    unsafe {
        CGImage::new(
            image.width as usize,
            image.height as usize,
            8,
            32,
            image.width as usize * 4,
            Some(&space),
            info,
            Some(&provider),
            std::ptr::null(),
            false,
            CGColorRenderingIntent::RenderingIntentDefault,
        )
    }
}
