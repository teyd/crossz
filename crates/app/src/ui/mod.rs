//! The settings window: the live preview on the left, settings pages on the right.

mod crosshairs;
mod games;
mod general;

use std::collections::HashMap;
use std::sync::Arc;

use crossz_core::{Chord, Color, Crosshair, Pixels, Resolution, Scale};
use gpui_kit::component::{
    ActiveTheme as _,
    alert::Alert,
    button::Button,
    color_picker::{ColorPickerEvent, ColorPickerState},
    h_flex,
    input::{InputEvent, InputState},
    setting::Settings,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AnyWindowHandle, App, AppContext as _, Context, Entity, FocusHandle, Global, Hsla,
    ImageSource, InteractiveElement as _, IntoElement, KeyDownEvent, MouseDownEvent,
    ParentElement as _, Render, RenderImage, Rgba, SharedString, Styled as _, Subscription,
    TitlebarOptions, WeakEntity, Window, WindowBounds, WindowOptions, div, img, px, size,
};

use crate::store::Store;
use crate::updater::State as UpdateState;
use crate::{Update, capture, preview};

struct SettingsWindow(AnyWindowHandle);

impl Global for SettingsWindow {}

/// Registers the settings window's key bindings. Call once at startup.
pub fn init(cx: &mut App) {
    crosshairs::bind_keys(cx);
}

/// Shows the settings window, reusing it if it's already open.
pub fn open(cx: &mut App) {
    if let Some(handle) = cx.try_global::<SettingsWindow>().map(|w| w.0)
        && handle
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        return;
    }
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::centered(size(px(1180.), px(780.)), cx)),
        window_min_size: Some(size(px(980.), px(600.))),
        titlebar: Some(TitlebarOptions {
            title: Some("crossz".into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    match gpui_kit::open_window(options, cx, |window, cx| {
        cx.new(|cx| SettingsView::new(window, cx))
    }) {
        Ok((handle, _)) => cx.set_global(SettingsWindow(handle)),
        Err(e) => Store::report(cx, format!("couldn't open the settings window: {e}")),
    }
}

pub fn is_open(cx: &mut App) -> bool {
    cx.try_global::<SettingsWindow>()
        .map(|w| w.0)
        .is_some_and(|handle| handle.update(cx, |_, _, _| ()).is_ok())
}

/// Side of the live preview, in logical pixels.
const PREVIEW: f32 = 256.0;

#[derive(Clone, Copy, PartialEq)]
enum Recording {
    /// A binding of the game being edited.
    Binding(usize),
    MasterToggle,
}

#[derive(Clone, Copy, PartialEq)]
enum Tool {
    Pencil,
    Eraser,
}

/// The paint editor's state. A stroke edits a copy, saved when the mouse is released.
struct Paint {
    /// The Crosshairs page shows the editor for the selected crosshair instead of the
    /// gallery.
    open: bool,
    /// The crosshair the undo history belongs to.
    target: String,
    /// Focused by clicking the canvas, so undo and redo shortcuts reach the editor.
    focus: FocusHandle,
    tool: Tool,
    mirror: bool,
    stroke: Option<Pixels>,
    /// The last cell painted in the current stroke.
    last: Option<(u32, u32)>,
    undo: Vec<Pixels>,
    redo: Vec<Pixels>,
}

struct CachedImage {
    crosshair: Crosshair,
    scale: Scale,
    size: u32,
    image: Arc<RenderImage>,
    zoom: u32,
}

/// Number fields of the game editor, which lives in a sheet outside `Settings`.
struct GameInputs {
    offset_x: Entity<InputState>,
    offset_y: Entity<InputState>,
    width: Entity<InputState>,
    height: Entity<InputState>,
}

pub struct SettingsView {
    paint_color: Entity<ColorPickerState>,
    new_game: Entity<InputState>,
    game_inputs: GameInputs,
    capture_focus: FocusHandle,
    recording: Option<Recording>,
    paint: Paint,
    images: HashMap<String, CachedImage>,
    _subscriptions: Vec<Subscription>,
}

impl SettingsView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let paint_color =
            cx.new(|cx| ColorPickerState::new(window, cx).default_value(to_hsla(Color::WHITE)));
        let new_game =
            cx.new(|cx| InputState::new(window, cx).placeholder("Program name, e.g. game.exe"));
        let number = |min: f64, max: f64, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| InputState::new(window, cx).step(1.0).min(min).max(max))
        };
        let game_inputs = GameInputs {
            offset_x: number(-50.0, 50.0, window, cx),
            offset_y: number(-50.0, 50.0, window, cx),
            width: number(320.0, 7680.0, window, cx),
            height: number(240.0, 4320.0, window, cx),
        };
        let mut subscriptions =
            vec![
                cx.subscribe(&paint_color, |_, _, _: &ColorPickerEvent, cx| {
                    cx.notify();
                }),
            ];
        let on_number = |input: &Entity<InputState>,
                         apply: fn(&mut crossz_core::Game, i64),
                         cx: &mut Context<Self>| {
            cx.subscribe(input, move |_, input, event: &InputEvent, cx| {
                if let InputEvent::Change = event
                    && let Ok(value) = input.read(cx).value().trim().parse::<f64>()
                {
                    Store::edit_game(cx, |g| apply(g, value.round() as i64));
                }
            })
        };
        subscriptions.extend([
            on_number(&game_inputs.offset_x, |g, v| g.offset[0] = v as i32, cx),
            on_number(&game_inputs.offset_y, |g, v| g.offset[1] = v as i32, cx),
            on_number(
                &game_inputs.width,
                |g, v| {
                    if let Resolution::Custom { height, .. } = g.resolution {
                        g.resolution = Resolution::custom((v as u32).max(1), height);
                    }
                },
                cx,
            ),
            on_number(
                &game_inputs.height,
                |g, v| {
                    if let Resolution::Custom { width, .. } = g.resolution {
                        g.resolution = Resolution::custom(width, (v as u32).max(1));
                    }
                },
                cx,
            ),
        ]);
        SettingsView {
            paint_color,
            new_game,
            game_inputs,
            capture_focus: cx.focus_handle(),
            recording: None,
            paint: Paint {
                open: false,
                target: String::new(),
                focus: cx.focus_handle(),
                tool: Tool::Pencil,
                mirror: true,
                stroke: None,
                last: None,
                undo: Vec::new(),
                redo: Vec::new(),
            },
            images: HashMap::new(),
            _subscriptions: subscriptions,
        }
    }

    /// A crosshair image, re-rendered only when what it shows changes.
    fn image(
        &mut self,
        key: String,
        crosshair: &Crosshair,
        scale: Scale,
        logical_size: f32,
        window: &mut Window,
        cx: &mut App,
    ) -> (Arc<RenderImage>, u32) {
        let size = (logical_size * window.scale_factor()).round() as u32;
        let fresh = (self.images.get(&key))
            .is_some_and(|c| c.crosshair == *crosshair && c.scale == scale && c.size == size);
        if !fresh {
            let (image, zoom) = preview::crosshair(crosshair, scale, size);
            let cached = CachedImage {
                crosshair: crosshair.clone(),
                scale,
                size,
                image,
                zoom,
            };
            if let Some(old) = self.images.insert(key.clone(), cached) {
                cx.drop_image(old.image, Some(window));
            }
        }
        let cached = &self.images[&key];
        (cached.image.clone(), cached.zoom)
    }

    fn record(&mut self, target: Recording, window: &mut Window, cx: &mut Context<Self>) {
        self.recording = Some(target);
        window.focus(&self.capture_focus, cx);
        cx.notify();
        cx.refresh_windows();
    }

    fn finish_recording(&mut self, chord: Option<Chord>, cx: &mut Context<Self>) {
        let Some(target) = self.recording.take() else {
            return;
        };
        if let Some(chord) = chord {
            match target {
                Recording::Binding(i) => Store::edit_game(cx, |game| {
                    if let Some(binding) = game.bindings.get_mut(i) {
                        binding.trigger = chord;
                    }
                }),
                Recording::MasterToggle => {
                    Store::edit(cx, |c| c.general.master_toggle = Some(chord));
                }
            }
        }
        cx.notify();
        cx.refresh_windows();
    }

    fn preview_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let store = Store::get(cx);
        let game = store.game().clone();
        let crosshair = store.crosshair().clone();
        let (width, height) = store.screen;
        let game_label = store.game_label();
        let scale = game.resolution.scale(width, height);
        let (image, zoom) = self.image("preview".into(), &crosshair, scale, PREVIEW, window, cx);
        let how = match game.resolution {
            Resolution::Native => "not stretched".to_owned(),
            r => format!("stretched like {r}"),
        };
        let theme = cx.theme();

        v_flex()
            .w(px(PREVIEW + 48.))
            .flex_none()
            .h_full()
            .p_6()
            .gap_3()
            .bg(theme.sidebar)
            .border_r_1()
            .border_color(theme.sidebar_border)
            .child(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child("Preview"),
            )
            .child(
                img(ImageSource::Render(image))
                    .size(px(PREVIEW))
                    .rounded(theme.radius),
            )
            .child(div().text_lg().child(crosshair.name.clone()))
            .child(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(format!(
                        "In {game_label}, {how}, on your {width}x{height} screen."
                    )),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(format!(
                        "Shown {zoom}× larger: each square is one screen pixel."
                    )),
            )
            .into_any_element()
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = Store::get(cx).config.general.crosshair.clone();
        if self.paint.target != selected {
            self.paint.target = selected;
            self.paint.stroke = None;
            self.paint.undo.clear();
            self.paint.redo.clear();
        }
        let this = cx.entity().downgrade();
        let preview = self.preview_panel(window, cx);
        let crosshairs = crosshairs::page(self, this.clone(), window, cx);
        let store = Store::get(cx);
        let error = store.error.clone();
        let input_unavailable = store.input_unavailable;
        let ready = match &cx.global::<Update>().state {
            UpdateState::Ready { version, .. } => Some(version.clone()),
            _ => None,
        };

        h_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(preview)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .when_some(error, |this, error| {
                        this.child(Alert::error("config-error", error).banner())
                    })
                    .when(input_unavailable, |this| this.child(input_banner()))
                    .when_some(ready, |this, version| {
                        this.child(
                            Alert::info("update-ready", format!("crossz {version} is ready. Restart it from the tray or General."))
                                .banner(),
                        )
                    })
                    .child(
                        Settings::new("crossz-settings")
                            .sidebar_width(px(180.))
                            .pages([
                                games::page(self, this.clone(), cx),
                                crosshairs,
                                general::page(self, this, cx),
                            ]),
                    ),
            )
    }
}

/// macOS only reports keys and buttons to apps the user allowed under Input Monitoring.
fn input_banner() -> impl IntoElement {
    h_flex()
        .w_full()
        .items_center()
        .gap_2()
        .pr_3()
        .child(
            div().flex_1().child(
                Alert::warning(
                    "input-permission",
                    "Binds need Input Monitoring permission. Allow crossz, then restart it.",
                )
                .banner(),
            ),
        )
        .child(
            Button::new("open-input-monitoring")
                .outline()
                .label("Open System Settings")
                .on_click(|_, _, cx| {
                    cx.open_url(
                        "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent",
                    );
                }),
        )
}

/// Shows a chord; click it to record a new one. While recording, the next key press or
/// click on it becomes the chord, and Escape cancels.
fn chord_button(
    id: SharedString,
    chord: Option<Chord>,
    recording: bool,
    focus: FocusHandle,
    target: Recording,
    this: WeakEntity<SettingsView>,
    cx: &App,
) -> AnyElement {
    if !recording {
        let label = chord.map_or("Set key".to_owned(), |c| c.to_string());
        return Button::new(id)
            .outline()
            .label(label)
            .min_w(px(110.))
            .on_click(move |_, window, cx| {
                let _ = this.update(cx, |view, cx| view.record(target, window, cx));
            })
            .into_any_element();
    }
    let on_key = this.clone();
    let on_mouse = this;
    div()
        .id(id)
        .track_focus(&focus)
        .h(px(32.))
        .px_3()
        .flex()
        .items_center()
        .rounded(cx.theme().radius)
        .border_1()
        .border_color(cx.theme().ring)
        .bg(cx.theme().accent)
        .text_sm()
        .child("Press a key or click here with a mouse button (Esc cancels)")
        .on_key_down(move |event: &KeyDownEvent, _, cx| {
            cx.stop_propagation();
            let chord = match event.keystroke.key.as_str() {
                "escape" => None,
                _ => match capture::from_keystroke(&event.keystroke) {
                    Some(chord) => Some(chord),
                    None => return,
                },
            };
            let _ = on_key.update(cx, |view, cx| view.finish_recording(chord, cx));
        })
        .on_any_mouse_down(move |event: &MouseDownEvent, _, cx| {
            cx.stop_propagation();
            let chord = capture::from_mouse(event.button, &event.modifiers);
            let _ = on_mouse.update(cx, |view, cx| view.finish_recording(Some(chord), cx));
        })
        .into_any_element()
}

fn options<K: Into<SharedString>, L: Into<SharedString>>(
    items: impl IntoIterator<Item = (K, L)>,
) -> Vec<(SharedString, SharedString)> {
    items
        .into_iter()
        .map(|(k, l)| (k.into(), l.into()))
        .collect()
}

fn to_hsla(color: Color) -> Hsla {
    Rgba {
        r: color.r as f32 / 255.0,
        g: color.g as f32 / 255.0,
        b: color.b as f32 / 255.0,
        a: color.a as f32 / 255.0,
    }
    .into()
}

fn from_hsla(hsla: Hsla) -> Color {
    let rgba = Rgba::from(hsla);
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color {
        r: byte(rgba.r),
        g: byte(rgba.g),
        b: byte(rgba.b),
        a: byte(rgba.a),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_survive_the_color_picker() {
        for c in [
            Color::WHITE,
            Color::BLACK,
            Color::rgb(255, 0, 255),
            Color {
                a: 128,
                ..Color::GREEN
            },
        ] {
            assert_eq!(from_hsla(to_hsla(c)), c);
        }
    }
}
