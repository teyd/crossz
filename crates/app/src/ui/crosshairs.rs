//! The crosshair library (built-in and your own) and the paint editor for your own.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use crossz_core::{Color, Crosshair, Pixels, Scale, presets, render};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _,
    button::{Button, ButtonGroup, ButtonVariants as _},
    color_picker::ColorPicker,
    h_flex,
    menu::{DropdownMenu as _, PopupMenuItem},
    setting::{NumberFieldOptions, SettingField, SettingGroup, SettingItem, SettingPage},
    switch::Switch,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Bounds, Context, Hsla, ImageSource, InteractiveElement as _, IntoElement,
    MouseButton, MouseDownEvent, MouseMoveEvent, ParentElement as _, Pixels as Px, RenderImage,
    SharedString, StatefulInteractiveElement as _, Styled as _, WeakEntity, Window, canvas, div,
    fill, img, point, px, size,
};

use super::{SettingsView, Tool, from_hsla, to_hsla};
use crate::store::{self, Store};

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

pub(super) fn library_page(
    view: &mut SettingsView,
    window: &mut Window,
    cx: &mut App,
) -> SettingPage {
    let store = Store::get(cx);
    let selected = store.selected_crosshair.clone();
    let is_custom = store.crosshair_is_custom();
    let game_label = store.game_label();
    let custom: Vec<Crosshair> = store.config.crosshairs.clone();
    let mut tiles = |crosshairs: &[Crosshair]| -> Vec<(String, Arc<RenderImage>, bool)> {
        crosshairs
            .iter()
            .map(|c| {
                let key = format!("thumb:{}", c.name);
                let (image, _) = view.image(key, c, Scale::ONE, THUMBNAIL, window, cx);
                (c.name.clone(), image, c.name == selected)
            })
            .collect()
    };
    let (builtin, yours) = (tiles(presets::all()), tiles(&custom));

    let actions = SettingItem::render(move |_, _, cx| {
        let used = Store::get(cx).game().crosshair == Store::get(cx).selected_crosshair;
        h_flex()
            .w_full()
            .gap_2()
            .child(
                Button::new("crosshair-use")
                    .primary()
                    .label(if used {
                        format!("Used in {game_label}")
                    } else {
                        format!("Use in {game_label}")
                    })
                    .disabled(used)
                    .on_click(|_, _, cx| {
                        let name = Store::get(cx).selected_crosshair.clone();
                        Store::edit_game(cx, |g| g.crosshair = name);
                    }),
            )
            .child(
                Button::new("crosshair-copy")
                    .outline()
                    .icon(IconName::Pencil)
                    .label(if is_custom { "Duplicate" } else { "Customize" })
                    .on_click(|_, _, cx| copy_selected(cx)),
            )
            .when(is_custom, |row| {
                row.child(
                    Button::new("crosshair-delete")
                        .danger()
                        .icon(IconName::Trash)
                        .label("Delete")
                        .on_click(|_, _, cx| {
                            let name = Store::get(cx).selected_crosshair.clone();
                            Store::edit(cx, |c| store::remove_crosshair(c, &name));
                        }),
                )
            })
    });

    SettingPage::new("Library")
        .icon(IconName::Crosshair)
        .groups([
            SettingGroup::new()
                .title("Built-in")
                .description("Always available. Customize one to paint your own version.")
                .item(SettingItem::render(move |_, _, cx| gallery(&builtin, cx))),
            SettingGroup::new()
                .title("Yours")
                .description("Drawn in the Editor.")
                .item(SettingItem::render(move |_, _, cx| {
                    v_flex()
                        .w_full()
                        .gap_3()
                        .when(!yours.is_empty(), |this| this.child(gallery(&yours, cx)))
                        .child(
                            h_flex().child(
                                Button::new("crosshair-new")
                                    .outline()
                                    .icon(IconName::Plus)
                                    .label("New crosshair")
                                    .on_click(|_, _, cx| new_crosshair(cx)),
                            ),
                        )
                })),
            SettingGroup::new().title("Selected").item(actions),
        ])
}

fn gallery(tiles: &[(String, Arc<RenderImage>, bool)], cx: &App) -> AnyElement {
    let theme = cx.theme();
    h_flex()
        .w_full()
        .flex_wrap()
        .gap_3()
        .children(tiles.iter().map(|(name, image, selected)| {
            let pick = name.clone();
            v_flex()
                .id(SharedString::from(format!("tile-{name}")))
                .w(px(THUMBNAIL + 32.))
                .p_2()
                .gap_1()
                .items_center()
                .rounded(theme.radius)
                .border_1()
                .border_color(if *selected { theme.ring } else { theme.border })
                .when(*selected, |tile| tile.bg(theme.accent))
                .cursor_pointer()
                .child(img(ImageSource::Render(image.clone())).size(px(THUMBNAIL)))
                .child(div().text_xs().text_center().child(name.clone()))
                .on_click(move |_, _, cx| Store::select_crosshair(cx, pick.clone()))
        }))
        .into_any_element()
}

/// Adds a crosshair to yours and selects it.
fn add_crosshair(cx: &mut App, base: &str, pixels: Pixels) {
    let name = store::unique_name(&Store::get(cx).config, base);
    let crosshair = Crosshair::pixels(name.clone(), pixels);
    Store::edit(cx, |c| c.crosshairs.push(crosshair));
    Store::select_crosshair(cx, name);
}

fn new_crosshair(cx: &mut App) {
    add_crosshair(cx, "My crosshair", Pixels::new(15, 15));
}

/// Copies the selected crosshair into yours. A built-in one becomes the pixels it
/// draws at 1×, ready to paint over.
fn copy_selected(cx: &mut App) {
    let source = Store::get(cx).crosshair().clone();
    let pixels = (source.pixels.clone()).unwrap_or_else(|| {
        Pixels::from_image(&render(
            &Crosshair {
                zoom: 1.0,
                ..source.clone()
            },
            Scale::ONE,
        ))
    });
    let base = if Store::get(cx).crosshair_is_custom() {
        format!("{} copy", source.name)
    } else {
        format!("My {}", source.name.to_lowercase())
    };
    add_crosshair(cx, &base, pixels);
}

pub(super) fn editor_page(
    view: &SettingsView,
    this: WeakEntity<SettingsView>,
    cx: &App,
) -> SettingPage {
    let page = SettingPage::new("Editor").icon(IconName::Pencil);
    let store = Store::get(cx);
    if !store.crosshair_is_custom() {
        let selected = store.selected_crosshair.clone();
        return page.group(
            SettingGroup::new()
                .title("Nothing to edit")
                .description("Pick one of your crosshairs in the Library, or start one here.")
                .item(SettingItem::render(move |_, _, _| {
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("editor-new")
                                .primary()
                                .icon(IconName::Plus)
                                .label("New crosshair")
                                .on_click(|_, _, cx| new_crosshair(cx)),
                        )
                        .child(
                            Button::new("editor-customize")
                                .outline()
                                .label(format!("Customize {selected}"))
                                .on_click(|_, _, cx| copy_selected(cx)),
                        )
                })),
        );
    }

    let crosshair = store.crosshair().clone();
    let pixels = view
        .paint
        .stroke
        .clone()
        .or(crosshair.pixels.clone())
        .unwrap_or_else(|| Pixels::new(15, 15));
    let tool = view.paint.tool;
    let mirror = view.paint.mirror;
    let (can_undo, can_redo) = (!view.paint.undo.is_empty(), !view.paint.redo.is_empty());
    let paint_color = view.paint_color.clone();

    let details = SettingGroup::new().title("Details").items([
        SettingItem::new(
            "Name",
            SettingField::input(
                |cx| Store::get(cx).crosshair().name.clone().into(),
                |name, cx| {
                    let name = name.trim();
                    let old = Store::get(cx).selected_crosshair.clone();
                    if !name.is_empty() && Store::get(cx).config.crosshair(name).is_none() {
                        Store::edit(cx, |c| store::rename_crosshair(c, &old, name));
                        Store::select_crosshair(cx, name.to_owned());
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
                |cx| Store::get(cx).crosshair().zoom as f64,
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
                |cx| (Store::get(cx).crosshair().opacity as f64 * 100.0).round(),
                |v, cx| Store::edit_crosshair(cx, |c| c.opacity = v as f32 / 100.0),
            ),
        ),
    ]);

    let size_this = this.clone();
    let toolbar_this = this.clone();
    let paint = SettingGroup::new()
        .title("Paint")
        .description("Left click paints, right click erases. Each square is one game pixel; the lines mark the center.")
        .item(SettingItem::render(move |_, _, _| {
            let this = toolbar_this.clone();
            let picker_color = paint_color.clone();
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
                                        .selected(tool == Tool::Pencil),
                                )
                                .child(
                                    Button::new("tool-eraser")
                                        .icon(IconName::Eraser)
                                        .label("Erase")
                                        .selected(tool == Tool::Eraser),
                                )
                                .on_click({
                                    let this = this.clone();
                                    move |selected: &Vec<usize>, _, cx| {
                                        let tool = match selected.first() {
                                            Some(1) => Tool::Eraser,
                                            _ => Tool::Pencil,
                                        };
                                        let _ = this.update(cx, |v, cx| {
                                            v.paint.tool = tool;
                                            cx.notify();
                                        });
                                    }
                                }),
                        )
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
                                .disabled(!can_redo)
                                .on_click({
                                    let this = this.clone();
                                    move |_, _, cx| {
                                        let _ = this.update(cx, |v, cx| v.undo(true, cx));
                                    }
                                }),
                        )
                        .child(
                            Button::new("paint-clear")
                                .ghost()
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
                        .gap_2()
                        .child(ColorPicker::new(&picker_color))
                        .children(SWATCHES.iter().enumerate().map(|(i, &color)| {
                            let picker = picker_color.clone();
                            div()
                                .id(SharedString::from(format!("swatch-{i}")))
                                .size(px(22.))
                                .rounded_sm()
                                .border_1()
                                .border_color(gpui_kit::black().opacity(0.3))
                                .bg(to_hsla(color))
                                .cursor_pointer()
                                .on_click(move |_, window, cx| {
                                    picker.update(cx, |state, cx| state.set_value(to_hsla(color), window, cx));
                                })
                        }))
                        .child(div().flex_1())
                        .child(size_menu(&pixels, size_this.clone())),
                )
                .child(
                    h_flex()
                        .w_full()
                        .justify_center()
                        .child(paint_canvas(pixels.clone(), this.clone())),
                )
        }));

    page.groups([paint, details])
}

fn size_menu(pixels: &Pixels, this: WeakEntity<SettingsView>) -> AnyElement {
    let (w, h) = (pixels.width(), pixels.height());
    Button::new("paint-size")
        .outline()
        .label(format!("Canvas {w}×{h}"))
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
fn paint_canvas(pixels: Pixels, this: WeakEntity<SettingsView>) -> AnyElement {
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

    let paint_bounds = bounds.clone();
    div()
        .id("paint-canvas")
        .w(px(width))
        .h(px(height))
        .cursor_crosshair()
        .child(
            canvas(
                move |b, _, _| paint_bounds.set(Some(b)),
                move |b, _, window, cx| {
                    let theme = cx.theme();
                    let (light, dark): (Hsla, Hsla) = (theme.background, theme.muted);
                    for y in 0..pixels.height() {
                        for x in 0..pixels.width() {
                            let origin = point(
                                b.origin.x + px(x as f32 * cell),
                                b.origin.y + px(y as f32 * cell),
                            );
                            let square = Bounds::new(origin, size(px(cell), px(cell)));
                            let checker = if (x + y) % 2 == 0 { light } else { dark };
                            window.paint_quad(fill(square, checker));
                            if let Some(color) = pixels.get(x, y) {
                                window.paint_quad(fill(square, to_hsla(color)));
                            }
                        }
                    }
                    // Center lines, between or through the middle squares.
                    let guide = theme.ring.opacity(0.6);
                    let cx_line = b.origin.x + px(width / 2.0);
                    let cy_line = b.origin.y + px(height / 2.0);
                    window.paint_quad(fill(
                        Bounds::new(point(cx_line, b.origin.y), size(px(1.), px(height))),
                        guide,
                    ));
                    window.paint_quad(fill(
                        Bounds::new(point(b.origin.x, cy_line), size(px(width), px(1.))),
                        guide,
                    ));
                },
            )
            .size_full(),
        )
        .on_mouse_down(MouseButton::Left, {
            let (this, hit) = (this.clone(), hit.clone());
            move |e: &MouseDownEvent, _, cx| stroke(&this, hit(e.position), false, cx)
        })
        .on_mouse_down(MouseButton::Right, {
            let (this, hit) = (this.clone(), hit.clone());
            move |e: &MouseDownEvent, _, cx| stroke(&this, hit(e.position), true, cx)
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
        Store::get(cx)
            .crosshair_is_custom()
            .then(|| Store::get(cx).crosshair().pixels.clone())
            .flatten()
    }

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
        if let Some(stroke) = self.paint.stroke.as_mut() {
            if mirror {
                stroke.set_mirrored(x, y, color);
            } else {
                stroke.set(x, y, color);
            }
        }
        cx.notify();
        cx.refresh_windows();
    }

    fn end_stroke(&mut self, cx: &mut Context<Self>) {
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
