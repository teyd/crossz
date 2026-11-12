//! The Crosshairs page: built-in and your own crosshairs, where clicking one uses it, and
//! below them the paint editor for the selected one of yours.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use crossz_core::{Color, Crosshair, Pixels, Scale, Show, presets, render};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _, WindowExt as _,
    button::{Button, ButtonGroup, ButtonVariant, ButtonVariants as _},
    color_picker::ColorPicker,
    h_flex,
    menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenuItem},
    setting::{NumberFieldOptions, SettingField, SettingGroup, SettingItem, SettingPage},
    switch::Switch,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Bounds, Context, FocusHandle, Hsla, ImageSource, InteractiveElement as _,
    IntoElement, KeyBinding, MouseButton, MouseDownEvent, MouseMoveEvent, ParentElement as _,
    Pixels as Px, RenderImage, SharedString, StatefulInteractiveElement as _, Styled as _,
    WeakEntity, Window, actions, canvas, div, fill, img, point, px, size,
};

use super::{SettingsView, Tool, from_hsla, to_hsla};
use crate::store::{self, Store};

actions!(crossz_paint, [Undo, Redo]);

const PAINT_CONTEXT: &str = "PaintEditor";
const THUMBNAIL: f32 = 64.0;
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
    ]);
}

pub(super) fn page(
    view: &mut SettingsView,
    this: WeakEntity<SettingsView>,
    window: &mut Window,
    cx: &mut App,
) -> SettingPage {
    let store = Store::get(cx);
    let selected = store.config.general.crosshair.clone();
    let is_custom = store.active_is_custom();
    let custom: Vec<Crosshair> = store.config.crosshairs.clone();
    let mut tiles = |crosshairs: &[Crosshair]| -> Vec<Tile> {
        crosshairs
            .iter()
            .map(|c| {
                let key = format!("thumb:{}", c.name);
                let (image, _) = view.image(key, c, Scale::ONE, THUMBNAIL, window, cx);
                Tile {
                    name: c.name.clone(),
                    image,
                    selected: c.name == selected,
                    custom: !presets::is_preset(&c.name),
                }
            })
            .collect()
    };
    let (builtin, yours) = (tiles(presets::all()), tiles(&custom));

    let mut groups: Vec<SettingGroup> = own_crosshair_note(cx).into_iter().collect();
    groups.extend([
        SettingGroup::new()
            .title("Built-in")
            .item(SettingItem::render(move |_, _, cx| gallery(&builtin, cx))),
        SettingGroup::new()
            .title("Yours")
            .item(SettingItem::render(move |_, _, cx| {
                if yours.is_empty() {
                    return div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("None yet. Create a new crosshair, or right-click a built-in one and duplicate it to paint your own version.")
                        .into_any_element();
                }
                gallery(&yours, cx)
            })),
    ]);
    if is_custom {
        groups.push(paint_group(view, this));
        groups.push(details_group());
    } else {
        groups.push(
            SettingGroup::new().title("Edit").item(SettingItem::render(move |_, _, cx| {
                h_flex()
                    .w_full()
                    .gap_3()
                    .child(
                        div()
                            .flex_1()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Built-in crosshairs can't be edited. Duplicate this one to paint your own version."),
                    )
                    .child(
                        Button::new("crosshair-duplicate")
                            .outline()
                            .label("Duplicate")
                            .on_click(|_, _, cx| {
                                let name = Store::get(cx).active().name.clone();
                                duplicate(&name, cx);
                            }),
                    )
            })),
        );
    }

    SettingPage::new("Crosshairs")
        .icon(IconName::Crosshair)
        .title_suffix(|_, _| {
            Button::new("crosshair-new")
                .icon(IconName::Plus)
                .label("New crosshair")
                .on_click(|_, _, cx| new_crosshair(cx))
        })
        .groups(groups)
}

/// Names the games that keep their own crosshair, since selecting one doesn't change them.
fn own_crosshair_note(cx: &App) -> Option<SettingGroup> {
    let store = Store::get(cx);
    if store.config.general.show != Show::InGames {
        return None;
    }
    let games: Vec<String> = (store.games_with_own_crosshair().iter())
        .map(|g| {
            format!(
                "{} ({})",
                g.display_name(),
                g.crosshair.as_deref().unwrap_or_default()
            )
        })
        .collect();
    let text = match games.as_slice() {
        [] => return None,
        [one] => format!("{one} uses its own crosshair. Change it on the Games page."),
        many => format!(
            "{} use their own crosshairs. Change them on the Games page.",
            many.join(", ")
        ),
    };
    Some(
        SettingGroup::new().item(SettingItem::render(move |_, _, cx| {
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(text.clone())
        })),
    )
}

struct Tile {
    name: String,
    image: Arc<RenderImage>,
    selected: bool,
    custom: bool,
}

fn gallery(tiles: &[Tile], cx: &App) -> AnyElement {
    let theme = cx.theme();
    h_flex()
        .w_full()
        .flex_wrap()
        .gap_3()
        .children(tiles.iter().map(|tile| {
            let (pick, name, custom, selected) = (
                tile.name.clone(),
                tile.name.clone(),
                tile.custom,
                tile.selected,
            );
            v_flex()
                .id(SharedString::from(format!("tile-{}", tile.name)))
                .w(px(THUMBNAIL + 32.))
                .p_2()
                .gap_1()
                .items_center()
                .rounded(theme.radius)
                .border_1()
                .border_color(if tile.selected {
                    theme.ring
                } else {
                    theme.border
                })
                .when(tile.selected, |t| t.bg(theme.accent))
                .when(!tile.selected, |t| t.hover(|t| t.bg(theme.muted)))
                .child(img(ImageSource::Render(tile.image.clone())).size(px(THUMBNAIL)))
                .child(div().text_xs().text_center().child(tile.name.clone()))
                .on_click(move |_, _, cx| Store::use_crosshair(cx, pick.clone()))
                .context_menu(move |menu, _, _| {
                    let (use_name, edit_name, duplicate_name, delete_name) =
                        (name.clone(), name.clone(), name.clone(), name.clone());
                    menu.item(
                        PopupMenuItem::new("Use")
                            .disabled(selected)
                            .on_click(move |_, _, cx| Store::use_crosshair(cx, use_name.clone())),
                    )
                    .item(PopupMenuItem::new("Edit").on_click(move |_, window, cx| {
                        if custom {
                            Store::use_crosshair(cx, edit_name.clone());
                        } else {
                            explain_builtin(edit_name.clone(), window, cx);
                        }
                    }))
                    .item(
                        PopupMenuItem::new("Duplicate")
                            .on_click(move |_, _, cx| duplicate(&duplicate_name, cx)),
                    )
                    .when(custom, |menu| {
                        menu.separator()
                            .item(
                                PopupMenuItem::new("Delete…").on_click(move |_, window, cx| {
                                    confirm_delete(delete_name.clone(), window, cx)
                                }),
                            )
                    })
                })
        }))
        .into_any_element()
}

/// Adds a crosshair to yours under a free name based on `base`, and uses it.
fn add_crosshair(cx: &mut App, base: &str, crosshair: Crosshair) {
    let name = store::unique_name(&Store::get(cx).config, base);
    let crosshair = Crosshair {
        name: name.clone(),
        ..crosshair
    };
    Store::edit(cx, |c| c.crosshairs.push(crosshair));
    Store::use_crosshair(cx, name);
}

fn new_crosshair(cx: &mut App) {
    add_crosshair(
        cx,
        "My crosshair",
        Crosshair::pixels("", Pixels::new(15, 15)),
    );
}

/// Built-in crosshairs can't be edited; offers to duplicate one instead.
fn explain_builtin(name: String, window: &mut Window, cx: &mut App) {
    window.open_alert_dialog(cx, move |alert, _, _| {
        let name = name.clone();
        alert
            .title(format!("“{name}” can't be edited"))
            .description(
                "Built-in crosshairs stay as they are. Duplicate it to paint your own version.",
            )
            .confirm()
            .ok_text("Duplicate")
            .on_ok(move |_, _, cx| {
                duplicate(&name, cx);
                true
            })
    });
}

/// Copies a crosshair into yours and uses the copy. A built-in one becomes the pixels
/// it draws at 1×, ready to paint over.
fn duplicate(name: &str, cx: &mut App) {
    let Some(source) = Store::get(cx).config.crosshair(name).cloned() else {
        return;
    };
    let (base, copy) = if !presets::is_preset(name) {
        (format!("{} copy", source.name), source)
    } else {
        // A built-in shape becomes the pixels it draws at 1×, keeping its look.
        let at_1x = Crosshair {
            zoom: 1.0,
            ..source.clone()
        };
        let pixels = Pixels::from_image(&render(&at_1x, Scale::ONE));
        let copy = Crosshair {
            shape: None,
            pixels: Some(pixels),
            ..at_1x
        };
        (format!("My {}", source.name.to_lowercase()), copy)
    };
    add_crosshair(cx, &base, copy);
}

fn confirm_delete(name: String, window: &mut Window, cx: &mut App) {
    let fallback = presets::all()[0].name.clone();
    window.open_alert_dialog(cx, move |alert, _, _| {
        let name = name.clone();
        alert
            .title(format!("Delete “{name}”?"))
            .description(format!(
                "Games that picked it switch to the selected crosshair. If it's the selected one, {fallback} is selected instead."
            ))
            .confirm()
            .ok_text("Delete")
            .ok_variant(ButtonVariant::Danger)
            .on_ok(move |_, _, cx| {
                Store::edit(cx, |c| store::remove_crosshair(c, &name));
                true
            })
    });
}

fn details_group() -> SettingGroup {
    SettingGroup::new().title("Details").items([
        SettingItem::new(
            "Name",
            SettingField::input(
                |cx| Store::get(cx).active().name.clone().into(),
                |name, cx| {
                    let name = name.trim();
                    let old = Store::get(cx).config.general.crosshair.clone();
                    if !name.is_empty() && Store::get(cx).config.crosshair(name).is_none() {
                        Store::edit(cx, |c| store::rename_crosshair(c, &old, name));
                    }
                },
            ),
        ),
        SettingItem::new(
            "Zoom",
            SettingField::number_input(
                NumberFieldOptions {
                    min: 0.25,
                    max: 8.0,
                    step: 0.25,
                },
                |cx| Store::get(cx).active().zoom as f64,
                |v, cx| Store::edit_crosshair(cx, |c| c.zoom = v as f32),
            ),
        )
        .description("Makes the crosshair bigger or smaller, on top of each game's stretch."),
        SettingItem::new(
            "Opacity %",
            SettingField::number_input(
                NumberFieldOptions {
                    min: 0.0,
                    max: 100.0,
                    step: 5.0,
                },
                |cx| (Store::get(cx).active().opacity as f64 * 100.0).round(),
                |v, cx| Store::edit_crosshair(cx, |c| c.opacity = v as f32 / 100.0),
            ),
        ),
        SettingItem::render(|_, _, _| {
            h_flex().w_full().justify_end().child(
                Button::new("crosshair-delete")
                    .outline()
                    .danger()
                    .icon(IconName::Trash)
                    .label("Delete…")
                    .on_click(|_, window, cx| {
                        let name = Store::get(cx).config.general.crosshair.clone();
                        confirm_delete(name, window, cx);
                    }),
            )
        }),
    ])
}

fn paint_group(view: &SettingsView, this: WeakEntity<SettingsView>) -> SettingGroup {
    let crosshair_pixels = view.paint.stroke.clone();
    let tool = view.paint.tool;
    let mirror = view.paint.mirror;
    let (can_undo, can_redo) = (!view.paint.undo.is_empty(), !view.paint.redo.is_empty());
    let picker = view.paint_color.clone();
    let focus = view.paint.focus.clone();

    SettingGroup::new()
        .title("Paint")
        .description("Left click paints, right click erases. Each square is one game pixel; the lines mark the center.")
        .item(SettingItem::render(move |_, _, cx| {
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
            let undo_this = this.clone();
            let redo_this = this.clone();
            v_flex()
                .key_context(PAINT_CONTEXT)
                .track_focus(&focus)
                .on_action(move |_: &Undo, _, cx| {
                    let _ = undo_this.update(cx, |v, cx| v.undo(false, cx));
                })
                .on_action(move |_: &Redo, _, cx| {
                    let _ = redo_this.update(cx, |v, cx| v.undo(true, cx));
                })
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
        }))
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
    use super::line;

    #[test]
    fn line_has_no_gaps() {
        assert_eq!(line((0, 0), (3, 1)), vec![(0, 0), (1, 0), (2, 1), (3, 1)]);
        assert_eq!(line((2, 2), (2, 2)), vec![(2, 2)]);
        assert_eq!(line((3, 0), (0, 3)).len(), 4);
    }
}
