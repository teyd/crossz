//! The paint editor: draws one of your crosshairs pixel by pixel. It replaces the library on
//! the Crosshairs page while open. Changes show at once; Save keeps them and Cancel puts the
//! crosshair back as it was when the editor opened (a new one is removed).

use std::cell::Cell;
use std::rc::Rc;

use crossz_core::{Color, Config, Crosshair, Pixels};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _,
    button::{Button, ButtonGroup, ButtonVariants as _},
    color_picker::ColorPicker,
    h_flex,
    input::Input,
    menu::{DropdownMenu as _, PopupMenuItem},
    switch::Switch,
    v_flex,
};
use gpui_kit::{
    AnyElement, App, Bounds, Context, FocusHandle, Hsla, InteractiveElement as _, IntoElement,
    KeyBinding, MouseButton, MouseDownEvent, MouseMoveEvent, ParentElement as _, Pixels as Px,
    Styled as _, WeakEntity, Window, actions, canvas, div, fill, point, px, size,
};

use super::layout::Section;
use super::{SettingsView, Tool, from_hsla, to_hsla};
use crate::store::{self, Store};

actions!(crossz_paint, [Undo, Redo, CloseEditor]);

const PAINT_CONTEXT: &str = "PaintEditor";
const CANVAS: f32 = 440.0;
const SIZES: [u32; 9] = [9, 11, 13, 15, 17, 21, 25, 33, 41];
const SWATCHES: [Color; 7] = [
    Color::WHITE,
    Color::BLACK,
    Color::rgb(0, 255, 0),
    Color::rgb(0, 255, 255),
    Color::rgb(255, 255, 0),
    Color::rgb(255, 0, 255),
    Color::rgb(255, 0, 0),
];

pub(super) fn bind_keys(cx: &mut App) {
    let (undo, redo) = if cfg!(target_os = "macos") {
        ("cmd-z", "cmd-shift-z")
    } else {
        ("ctrl-z", "ctrl-shift-z")
    };
    cx.bind_keys([
        KeyBinding::new(undo, Undo, Some(PAINT_CONTEXT)),
        KeyBinding::new(redo, Redo, Some(PAINT_CONTEXT)),
        KeyBinding::new("escape", CloseEditor, Some(PAINT_CONTEXT)),
    ]);
}

/// What Cancel restores.
pub(super) struct Session {
    /// The crosshair as it was when the editor opened.
    original: Crosshair,
    /// The crosshair that was selected before a new one was created.
    previous: String,
    /// The crosshair was made for this edit, so Cancel removes it.
    created: bool,
}

impl Session {
    fn restore(&self, config: &mut Config) {
        let current = config.general.crosshair.clone();
        if self.created {
            store::remove_crosshair(config, &current);
            if config.crosshair(&self.previous).is_some() {
                config.general.crosshair = self.previous.clone();
            }
            return;
        }
        if current != self.original.name {
            store::rename_crosshair(config, &current, &self.original.name);
        }
        if let Some(c) = (config.crosshairs.iter_mut()).find(|c| c.name == self.original.name) {
            *c = self.original.clone();
        }
    }
}

/// Opens the editor on the selected crosshair. `created_from` is the selection before the
/// crosshair being edited was just made, if it was.
pub(super) fn open(this: &WeakEntity<SettingsView>, created_from: Option<String>, cx: &mut App) {
    let store = Store::get(cx);
    let session = Session {
        original: store.active().clone(),
        previous: created_from
            .clone()
            .unwrap_or_else(|| store.config.general.crosshair.clone()),
        created: created_from.is_some(),
    };
    let _ = this.update(cx, |v, cx| {
        v.paint.session = Some(session);
        v.paint.focus_pending = true;
        v.paint.open = true;
        cx.notify();
    });
}

/// Keeps the changes; a stroke still being drawn is saved, not lost with the canvas.
fn save(this: &WeakEntity<SettingsView>, cx: &mut App) {
    let _ = this.update(cx, |v, cx| {
        v.end_stroke(cx);
        v.paint.session = None;
        v.paint.open = false;
        cx.notify();
    });
}

fn cancel(this: &WeakEntity<SettingsView>, cx: &mut App) {
    let _ = this.update(cx, |v, cx| {
        v.paint.stroke = None;
        v.paint.last = None;
        if let Some(session) = v.paint.session.take() {
            Store::edit(cx, |config| session.restore(config));
        }
        v.paint.open = false;
        cx.notify();
    });
}

/// The editor for the selected crosshair: its name, the canvas, then Cancel and Save.
pub(super) fn editor_page(
    view: &SettingsView,
    this: WeakEntity<SettingsView>,
    cx: &App,
) -> Vec<AnyElement> {
    let name = v_flex()
        .gap_1()
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("Name"),
        )
        .child(
            div()
                .w(px(320.))
                .child(Input::new(&view.crosshair_inputs.name)),
        );
    let buttons = h_flex()
        .w_full()
        .justify_end()
        .gap_2()
        .child(
            Button::new("crosshair-cancel")
                .outline()
                .label("Cancel")
                .tooltip("Undo all changes (Esc)")
                .on_click({
                    let this = this.clone();
                    move |_, _, cx| cancel(&this, cx)
                }),
        )
        .child(
            Button::new("crosshair-save")
                .primary()
                .label("Save")
                .on_click({
                    let this = this.clone();
                    move |_, _, cx| save(&this, cx)
                }),
        );
    // Shortcuts work anywhere in the editor; it takes focus when it opens.
    let (undo, redo, close) = (this.clone(), this.clone(), this.clone());
    let editor = v_flex()
        .key_context(PAINT_CONTEXT)
        .track_focus(&view.paint.focus)
        .on_action(move |_: &Undo, _, cx| {
            let _ = undo.update(cx, |v, cx| v.undo(false, cx));
        })
        .on_action(move |_: &Redo, _, cx| {
            let _ = redo.update(cx, |v, cx| v.undo(true, cx));
        })
        .on_action(move |_: &CloseEditor, _, cx| cancel(&close, cx))
        .gap_8()
        .child(name)
        .child(paint_section(view, this, cx))
        .child(buttons);
    vec![editor.into_any_element()]
}

fn paint_section(view: &SettingsView, this: WeakEntity<SettingsView>, cx: &App) -> Section {
    let crosshair_pixels = view.paint.stroke.clone();
    let tool = view.paint.tool;
    let mirror = view.paint.mirror;
    let (can_undo, can_redo) = (!view.paint.undo.is_empty(), !view.paint.redo.is_empty());
    let picker = view.paint_color.clone();
    let focus = view.paint.focus.clone();

    Section::new()
        .title("Paint")
        .description("Left click paints, right click erases. Each square is one game pixel; the lines mark the center.")
        .child({
            let pixels = crosshair_pixels
                .clone()
                .or_else(|| Store::get(cx).active().pixels.clone())
                .unwrap_or_else(|| Pixels::new(15, 15));
            let set_tool = |tool: Tool| {
                let this = this.clone();
                move |_: &gpui_kit::ClickEvent, _: &mut Window, cx: &mut App| {
                    let _ = this.update(cx, |v, cx| {
                        v.paint.tool = tool;
                        cx.notify();
                    });
                }
            };
            v_flex()
                .w_full()
                .gap_3()
                .child(
                    h_flex()
                        .gap_2()
                        .flex_wrap()
                        .child(
                            ButtonGroup::new("paint-tool")
                                .outline()
                                .child(
                                    Button::new("tool-pencil")
                                        .icon(IconName::Pencil)
                                        .label("Paint")
                                        .selected(tool == Tool::Pencil)
                                        .on_click(set_tool(Tool::Pencil)),
                                )
                                .child(
                                    Button::new("tool-eraser")
                                        .icon(IconName::Eraser)
                                        .label("Erase")
                                        .selected(tool == Tool::Eraser)
                                        .on_click(set_tool(Tool::Eraser)),
                                ),
                        )
                        .child(ColorPicker::new(&picker).featured_colors(SWATCHES.map(to_hsla).to_vec()))
                        .child(
                            Switch::new("paint-mirror")
                                .label("Mirror")
                                .checked(mirror)
                                .on_change({
                                    let this = this.clone();
                                    move |on: &bool, _, cx| {
                                        let on = *on;
                                        let _ = this.update(cx, |v, cx| {
                                            v.paint.mirror = on;
                                            cx.notify();
                                        });
                                    }
                                }),
                        )
                        .child(div().flex_1())
                        .child(
                            Button::new("paint-undo")
                                .ghost()
                                .icon(IconName::Undo2)
                                .tooltip("Undo")
                                .disabled(!can_undo)
                                .on_click({
                                    let this = this.clone();
                                    move |_, _, cx| {
                                        let _ = this.update(cx, |v, cx| v.undo(false, cx));
                                    }
                                }),
                        )
                        .child(
                            Button::new("paint-redo")
                                .ghost()
                                .icon(IconName::Redo2)
                                .tooltip("Redo")
                                .disabled(!can_redo)
                                .on_click({
                                    let this = this.clone();
                                    move |_, _, cx| {
                                        let _ = this.update(cx, |v, cx| v.undo(true, cx));
                                    }
                                }),
                        )
                        .child(size_menu(&pixels, this.clone()))
                        .child(
                            Button::new("paint-clear")
                                .outline()
                                .label("Clear")
                                .on_click({
                                    let this = this.clone();
                                    move |_, _, cx| {
                                        let _ = this.update(cx, |v, cx| {
                                            v.replace_pixels(|p| Pixels::new(p.width(), p.height()), cx)
                                        });
                                    }
                                }),
                        ),
                )
                .child(
                    h_flex()
                        .w_full()
                        .justify_center()
                        .child(paint_canvas(pixels, focus.clone(), this.clone(), cx)),
                )
        })
}

fn size_menu(pixels: &Pixels, this: WeakEntity<SettingsView>) -> AnyElement {
    let (w, h) = (pixels.width(), pixels.height());
    Button::new("paint-size")
        .outline()
        .label(format!("{w}×{h}"))
        .tooltip("Canvas size")
        .dropdown_caret(true)
        .dropdown_menu(move |menu, _, _| {
            SIZES.iter().fold(menu, |menu, &n| {
                let this = this.clone();
                menu.item(
                    PopupMenuItem::new(format!("{n}×{n}"))
                        .checked(n == w && n == h)
                        .on_click(move |_, _, cx| {
                            let _ =
                                this.update(cx, |v, cx| v.replace_pixels(|p| p.resized(n, n), cx));
                        }),
                )
            })
        })
        .into_any_element()
}

/// The drawing, magnified to a fixed size, over a checkerboard with the center marked.
fn paint_canvas(
    pixels: Pixels,
    focus: FocusHandle,
    this: WeakEntity<SettingsView>,
    cx: &App,
) -> AnyElement {
    let bounds: Rc<Cell<Option<Bounds<Px>>>> = Rc::default();
    let n = pixels.width().max(pixels.height()) as f32;
    let cell = (CANVAS / n).floor();
    let (width, height) = (cell * pixels.width() as f32, cell * pixels.height() as f32);
    // The square under a window position, if it's on the canvas.
    let hit = {
        let bounds = bounds.clone();
        let (w, h) = (pixels.width(), pixels.height());
        move |position: gpui_kit::Point<Px>| -> Option<(u32, u32)> {
            let b = bounds.get()?;
            let x = ((position.x - b.origin.x) / px(cell)).floor();
            let y = ((position.y - b.origin.y) / px(cell)).floor();
            (x >= 0.0 && y >= 0.0 && (x as u32) < w && (y as u32) < h)
                .then_some((x as u32, y as u32))
        }
    };
    let stroke = move |this: &WeakEntity<SettingsView>,
                       at: Option<(u32, u32)>,
                       erase: bool,
                       cx: &mut App| {
        if let Some((x, y)) = at {
            let _ = this.update(cx, |v, cx| v.stroke_at(x, y, erase, cx));
        }
    };
    let end = |this: &WeakEntity<SettingsView>, cx: &mut App| {
        let _ = this.update(cx, |v, cx| v.end_stroke(cx));
    };
    let theme = cx.theme();
    let (light, dark, guide): (Hsla, Hsla, Hsla) =
        (theme.background, theme.muted, theme.ring.opacity(0.6));

    let paint_bounds = bounds.clone();
    div()
        .id("paint-canvas")
        .w(px(width))
        .h(px(height))
        .border_1()
        .border_color(theme.border)
        .cursor_crosshair()
        .child(
            canvas(
                move |b, _, _| paint_bounds.set(Some(b)),
                move |b, _, window, _| {
                    for y in 0..pixels.height() {
                        for x in 0..pixels.width() {
                            let origin = point(
                                b.origin.x + px(x as f32 * cell),
                                b.origin.y + px(y as f32 * cell),
                            );
                            let square = Bounds::new(origin, size(px(cell), px(cell)));
                            window.paint_quad(fill(
                                square,
                                if (x + y) % 2 == 0 { light } else { dark },
                            ));
                            if let Some(color) = pixels.get(x, y) {
                                window.paint_quad(fill(square, to_hsla(color)));
                            }
                        }
                    }
                    // Center lines, between or through the middle squares.
                    let (mid_x, mid_y) =
                        (b.origin.x + px(width / 2.0), b.origin.y + px(height / 2.0));
                    window.paint_quad(fill(
                        Bounds::new(point(mid_x, b.origin.y), size(px(1.), px(height))),
                        guide,
                    ));
                    window.paint_quad(fill(
                        Bounds::new(point(b.origin.x, mid_y), size(px(width), px(1.))),
                        guide,
                    ));
                },
            )
            .size_full(),
        )
        .on_mouse_down(MouseButton::Left, {
            let (this, hit, focus) = (this.clone(), hit.clone(), focus.clone());
            move |e: &MouseDownEvent, window, cx| {
                window.focus(&focus, cx);
                stroke(&this, hit(e.position), false, cx);
            }
        })
        .on_mouse_down(MouseButton::Right, {
            let (this, hit) = (this.clone(), hit.clone());
            move |e: &MouseDownEvent, window, cx| {
                window.focus(&focus, cx);
                stroke(&this, hit(e.position), true, cx);
            }
        })
        .on_mouse_move({
            let (this, hit) = (this.clone(), hit.clone());
            move |e: &MouseMoveEvent, _, cx| match e.pressed_button {
                Some(MouseButton::Left) => stroke(&this, hit(e.position), false, cx),
                Some(MouseButton::Right) => stroke(&this, hit(e.position), true, cx),
                _ => {}
            }
        })
        .on_mouse_up(MouseButton::Left, {
            let this = this.clone();
            move |_, _, cx| end(&this, cx)
        })
        .on_mouse_up(MouseButton::Right, {
            let this = this.clone();
            move |_, _, cx| end(&this, cx)
        })
        .on_mouse_up_out(MouseButton::Left, {
            let this = this.clone();
            move |_, _, cx| end(&this, cx)
        })
        .on_mouse_up_out(MouseButton::Right, move |_, _, cx| end(&this, cx))
        .into_any_element()
}

impl SettingsView {
    fn custom_pixels(cx: &App) -> Option<Pixels> {
        let store = Store::get(cx);
        store
            .active_is_custom()
            .then(|| store.active().pixels.clone())
            .flatten()
    }

    /// Paints the cell under the pointer and every cell on the way from the last one:
    /// mouse moves arrive about once a frame, so fast strokes would otherwise skip.
    fn stroke_at(&mut self, x: u32, y: u32, erase: bool, cx: &mut Context<Self>) {
        if self.paint.stroke.is_none() {
            let Some(current) = Self::custom_pixels(cx) else {
                return;
            };
            self.paint.undo.push(current.clone());
            self.paint.redo.clear();
            self.paint.stroke = Some(current);
        }
        let color = (!erase && self.paint.tool == Tool::Pencil)
            .then(|| self.paint_color.read(cx).value().map(from_hsla))
            .flatten();
        let mirror = self.paint.mirror;
        let from = self.paint.last.unwrap_or((x, y));
        self.paint.last = Some((x, y));
        if let Some(stroke) = self.paint.stroke.as_mut() {
            for (x, y) in line(from, (x, y)) {
                if mirror {
                    stroke.set_mirrored(x, y, color);
                } else {
                    stroke.set(x, y, color);
                }
            }
        }
        cx.notify();
        cx.refresh_windows();
    }

    fn end_stroke(&mut self, cx: &mut Context<Self>) {
        self.paint.last = None;
        if let Some(pixels) = self.paint.stroke.take() {
            Store::edit_crosshair(cx, |c| c.pixels = Some(pixels));
        }
    }

    /// Replaces the whole drawing, e.g. to clear or resize it, as one undoable step.
    fn replace_pixels(&mut self, change: impl FnOnce(&Pixels) -> Pixels, cx: &mut Context<Self>) {
        let Some(current) = Self::custom_pixels(cx) else {
            return;
        };
        let next = change(&current);
        self.paint.undo.push(current);
        self.paint.redo.clear();
        Store::edit_crosshair(cx, |c| c.pixels = Some(next));
        cx.notify();
    }

    fn undo(&mut self, redo: bool, cx: &mut Context<Self>) {
        // Mid-stroke the drawing isn't saved yet; the stroke becomes one step on mouse-up.
        if self.paint.stroke.is_some() {
            return;
        }
        let Some(current) = Self::custom_pixels(cx) else {
            return;
        };
        let (from, to) = if redo {
            (&mut self.paint.redo, &mut self.paint.undo)
        } else {
            (&mut self.paint.undo, &mut self.paint.redo)
        };
        if let Some(previous) = from.pop() {
            to.push(current);
            Store::edit_crosshair(cx, |c| c.pixels = Some(previous));
        }
        cx.notify();
    }
}

/// The cells on a straight line between two cells, both included (Bresenham).
fn line((x0, y0): (u32, u32), (x1, y1): (u32, u32)) -> Vec<(u32, u32)> {
    let (mut x, mut y) = (x0 as i64, y0 as i64);
    let (dx, dy) = ((x1 as i64 - x).abs(), -(y1 as i64 - y).abs());
    let (sx, sy) = (
        if x < x1 as i64 { 1 } else { -1 },
        if y < y1 as i64 { 1 } else { -1 },
    );
    let mut error = dx + dy;
    let mut cells = vec![(x as u32, y as u32)];
    while (x, y) != (x1 as i64, y1 as i64) {
        let doubled = 2 * error;
        if doubled >= dy {
            error += dy;
            x += sx;
        }
        if doubled <= dx {
            error += dx;
            y += sy;
        }
        cells.push((x as u32, y as u32));
    }
    cells
}

#[cfg(test)]
mod tests {
    use crossz_core::{Action, Binding};

    use super::*;

    fn config_with_mine() -> Config {
        let mut c = Config::default();
        c.crosshairs
            .push(Crosshair::pixels("mine", Pixels::new(3, 3)));
        c.general.crosshair = "mine".into();
        c.games[0].crosshair = Some("mine".into());
        c.other_programs.bindings.push(Binding {
            trigger: "F6".parse().unwrap(),
            action: Action::SetCrosshair("mine".into()),
        });
        c.validate().unwrap();
        c
    }

    #[test]
    fn cancel_puts_an_edited_crosshair_back_even_when_renamed() {
        let mut c = config_with_mine();
        let session = Session {
            original: c.crosshairs[0].clone(),
            previous: "mine".into(),
            created: false,
        };
        store::rename_crosshair(&mut c, "mine", "ours");
        c.crosshairs[0].pixels = Some(Pixels::new(9, 9));
        c.crosshairs[0].zoom = 3.0;
        session.restore(&mut c);
        c.validate().unwrap();
        assert_eq!(c.crosshairs, config_with_mine().crosshairs);
        assert_eq!(c.general.crosshair, "mine");
        assert_eq!(c.games[0].crosshair.as_deref(), Some("mine"));
    }

    #[test]
    fn cancel_removes_a_new_crosshair_and_reselects_the_previous_one() {
        let mut c = Config::default();
        c.crosshairs
            .push(Crosshair::pixels("My crosshair", Pixels::new(15, 15)));
        c.general.crosshair = "My crosshair".into();
        let session = Session {
            original: c.crosshairs[0].clone(),
            previous: "Dot".into(),
            created: true,
        };
        session.restore(&mut c);
        c.validate().unwrap();
        assert!(c.crosshairs.is_empty());
        assert_eq!(c.general.crosshair, "Dot");
    }

    use super::line;

    #[test]
    fn line_has_no_gaps() {
        assert_eq!(line((0, 0), (3, 1)), vec![(0, 0), (1, 0), (2, 1), (3, 1)]);
        assert_eq!(line((2, 2), (2, 2)), vec![(2, 2)]);
        assert_eq!(line((3, 0), (0, 3)).len(), 4);
    }
}
