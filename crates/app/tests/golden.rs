//! Screenshot tests: each page of the real settings window, rendered headless and compared
//! with the PNGs in `tests/golden/`.
//!
//! Linux is the one canonical renderer (wgpu on Mesa's lavapipe software Vulkan), because
//! text anti-aliasing differs between machines: a golden rendered on one Mac fails on
//! another. On macOS and Windows this target does nothing. CI checks the goldens on Linux;
//! refresh them after an intended UI change with `mise run goldens`, which has CI render
//! them, and look at the images before committing.
//!
//! The target has no libtest harness: the headless renderer wants the main thread (like
//! gpui-kit's own rendering tests).

fn main() {
    #[cfg(target_os = "linux")]
    golden::run();
    #[cfg(not(target_os = "linux"))]
    println!("golden: skipped; the screenshots are Linux renders, checked on CI");
}

#[cfg(target_os = "linux")]
mod golden {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use crossz_core::{Color, Config, Crosshair, Pixels};
    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{
        AnyWindowHandle, Bounds, HeadlessAppContext, WindowBounds, WindowOptions, px, size,
    };

    /// Per-channel difference that still counts as the same pixel (anti-aliasing noise).
    const CHANNEL_TOLERANCE: u8 = 8;
    /// Pixels that may differ before a scene fails. Rendering is deterministic, so this is
    /// small enough that one changed letter fails.
    const MAX_DIFFERING: usize = 20;

    fn golden_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
    }

    /// Opens the settings window on `page` with a fixed config and screen, saving to `path`.
    fn scene(page: &str, config: Config, path: PathBuf) -> (HeadlessAppContext, AnyWindowHandle) {
        let mut cx = HeadlessAppContext::with_platform(
            gpui_kit::platform::current_platform(true).text_system(),
            Arc::new(crossz::testing::assets()),
            gpui_kit::platform::current_headless_renderer,
        );
        cx.update(|cx| crossz::testing::init(cx, config, path, (1920, 1080), page));
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                Default::default(),
                size(px(1180.), px(780.)),
            ))),
            focus: false,
            show: false,
            ..Default::default()
        };
        let (handle, _) = cx
            .update(|cx| {
                gpui_kit::open_window(options, cx, |window, cx| {
                    crossz::testing::settings_view(window, cx)
                })
            })
            .expect("open the settings window");
        (cx, handle)
    }

    fn render(
        cx: &mut HeadlessAppContext,
        handle: AnyWindowHandle,
        act: impl FnOnce(&mut gpui_kit::Window, &mut gpui_kit::App),
    ) {
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            act(window, cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| window.render_frame(cx))
            .unwrap();
    }

    /// Compares the window with `<name>.png`, or rewrites that file when
    /// `CROSSZ_UPDATE_GOLDENS` is set.
    fn assert_golden(
        cx: &mut HeadlessAppContext,
        handle: AnyWindowHandle,
        name: &str,
        mask: &[[u32; 4]],
    ) {
        let actual = cx
            .capture_screenshot(handle)
            .expect("offscreen rendering must be available");
        let golden = golden_dir().join(format!("{name}.png"));
        if std::env::var_os("CROSSZ_UPDATE_GOLDENS").is_some_and(|v| !v.is_empty()) {
            std::fs::create_dir_all(golden.parent().unwrap()).unwrap();
            actual.save(&golden).unwrap();
            return;
        }
        let expected = image::open(&golden)
            .unwrap_or_else(|e| {
                panic!(
                    "{}: {e}. Create it with `mise run goldens`.",
                    golden.display()
                )
            })
            .into_rgba8();
        let differing = if expected.dimensions() == actual.dimensions() {
            expected
                .enumerate_pixels()
                .zip(actual.pixels())
                .filter(|((x, y, a), b)| {
                    let masked = mask
                        .iter()
                        .any(|[x0, y0, x1, y1]| (x0..x1).contains(&x) && (y0..y1).contains(&y));
                    !masked
                        && a.0
                            .iter()
                            .zip(b.0.iter())
                            .any(|(a, b)| a.abs_diff(*b) > CHANNEL_TOLERANCE)
                })
                .count()
        } else {
            usize::MAX
        };
        let total = (actual.width() * actual.height()) as f64;
        if differing > MAX_DIFFERING {
            let out = golden.with_extension("actual.png");
            actual.save(&out).unwrap();
            panic!(
                "{name} differs from its golden: {} of {total} pixels (size {:?} vs {:?}). \
             See {}; if the change is intended, run `mise run goldens`.",
                if differing == usize::MAX {
                    "all".into()
                } else {
                    differing.to_string()
                },
                actual.dimensions(),
                expected.dimensions(),
                out.display()
            );
        }
    }

    /// A config with one small drawn crosshair of the user's, selected.
    fn with_own_crosshair() -> Config {
        let mut pixels = Pixels::new(9, 9);
        for i in 1..8 {
            pixels.set(4, i, Some(Color::WHITE));
            pixels.set(i, 4, Some(Color::WHITE));
        }
        let mut config = Config::default();
        config.crosshairs.push(Crosshair::pixels("Mine", pixels));
        config.general.crosshair = "Mine".into();
        config
    }

    /// Pixel rectangles `[x0, y0, x1, y1]` to leave out of a comparison.
    type Mask = &'static [[u32; 4]];

    pub fn run() {
        let dir = std::env::temp_dir().join(format!("crossz-golden-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut failures = Vec::new();
        let mut check =
            |name: &str,
             page: &str,
             mask: Mask,
             act: fn(&mut gpui_kit::Window, &mut gpui_kit::App)| {
                // Only the editor writes the config; the other pages show this path as text.
                let path = if name == "editor" {
                    dir.join("config.toml")
                } else {
                    PathBuf::from("/crossz/config.toml")
                };
                let config = if name == "crosshairs-yours" {
                    with_own_crosshair()
                } else {
                    Config::default()
                };
                let (mut cx, handle) = scene(page, config, path);
                render(&mut cx, handle, act);
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    assert_golden(&mut cx, handle, name, mask)
                })) {
                    Ok(()) => println!("golden {name} ... ok"),
                    Err(_) => {
                        println!("golden {name} ... FAILED");
                        failures.push(name.to_owned());
                    }
                }
            };
        check("crosshairs", "Crosshairs", &[], |_, _| {});
        check("crosshairs-yours", "Crosshairs", &[], |_, _| {});
        check("games", "Games", &[], |_, _| {});
        // The "crossz <version>" line changes with every release.
        check("general", "General", &[[630, 530, 800, 572]], |_, _| {});
        check("editor", "Crosshairs", &[], |window, cx| {
            window.click("crosshair-new", cx)
        });
        let _ = std::fs::remove_dir_all(&dir);
        if !failures.is_empty() {
            eprintln!("golden: {} failed: {}", failures.len(), failures.join(", "));
            std::process::exit(1);
        }
    }
}
