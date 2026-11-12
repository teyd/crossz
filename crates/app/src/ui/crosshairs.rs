//! The Crosshairs page: the library of built-in and your own crosshairs on the left, where
//! clicking one uses it, and the selected one's options on the right. Editing one of yours
//! replaces both with the paint editor (`paint`).

use std::sync::Arc;

use crossz_core::{Crosshair, Pixels, Scale, Show, presets, render};
use gpui_kit::TestSupportExt as _;
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, WindowExt as _,
    button::{Button, ButtonVariant, ButtonVariants as _},
    h_flex,
    input::NumberInput,
    label::Label,
    menu::{ContextMenuExt as _, PopupMenuItem},
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ImageSource, InteractiveElement as _, IntoElement, ParentElement as _,
    RenderImage, SharedString, StatefulInteractiveElement as _, Styled as _, WeakEntity, Window,
    div, img, px,
};

use super::layout::{Section, note};
use super::{SettingsView, paint};
use crate::store::{self, Store};

const THUMBNAIL: f32 = 64.0;
const OPTIONS: f32 = 280.0;

pub(super) fn page(
    view: &mut SettingsView,
    this: WeakEntity<SettingsView>,
    window: &mut Window,
    cx: &mut App,
) -> Vec<AnyElement> {
    let store = Store::get(cx);
    let selected = store.config.general.crosshair.clone();
    // Deleting the edited crosshair selects a built-in one, which ends editing.
    if view.paint.open && !store.active_is_custom() {
        view.paint.open = false;
        view.paint.session = None;
        view.paint.stroke = None;
        view.paint.last = None;
    }
    if view.paint.open {
        return paint::editor_page(view, this, cx);
    }

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

    let new = this.clone();
    let toolbar = h_flex()
        .w_full()
        .gap_3()
        .child(
            Button::new("crosshair-new")
                .icon(IconName::Plus)
                .label("New crosshair")
                .on_click(move |_, _, cx| new_crosshair(new.clone(), cx)),
        )
        .child(div().flex_1().min_w_0().children(own_crosshair_note(cx)));
    let yours = if yours.is_empty() {
        note(
            "None yet. Create a new crosshair, or right-click a built-in one and duplicate it to paint your own version.",
            cx,
        )
    } else {
        gallery(&yours, this.clone(), cx)
    };
    let library = v_flex()
        .flex_1()
        .min_w_0()
        .gap_8()
        .child(toolbar)
        .child(
            Section::new()
                .title("Built-in")
                .child(gallery(&builtin, this.clone(), cx)),
        )
        .child(
            Section::new()
                .title("Yours")
                .description("Double-click one of yours to edit it.")
                .child(yours),
        );
    vec![
        h_flex()
            .w_full()
            .items_start()
            .gap_6()
            .child(library)
            .child(
                div()
                    .w(px(OPTIONS))
                    .flex_none()
                    .child(options(view, this, cx)),
            )
            .into_any_element(),
    ]
}

/// The selected crosshair's own options: zoom and opacity, and what can be done with it.
fn options(view: &SettingsView, this: WeakEntity<SettingsView>, cx: &App) -> AnyElement {
    let store = Store::get(cx);
    let name = store.config.general.crosshair.clone();
    let inputs = &view.crosshair_inputs;
    let field = |label: &'static str, hint: Option<&'static str>, input: AnyElement| {
        v_flex()
            .gap_1()
            .child(Label::new(label).text_sm())
            .child(input)
            .when_some(hint, |field, hint| field.child(note(hint, cx)))
    };
    let duplicate_button = {
        let (name, this) = (name.clone(), this.clone());
        Button::new("crosshair-duplicate")
            .outline()
            .icon(IconName::Copy)
            .label("Duplicate")
            .on_click(move |_, _, cx| duplicate(&name, this.clone(), cx))
    };
    let section = Section::new().title(name.clone());
    if !store.active_is_custom() {
        return section
            .description("Built-in")
            .child(note(
                "Built-in crosshairs stay as they are. Duplicate it to paint your own version, with its own zoom and opacity.",
                cx,
            ))
            .child(h_flex().child(duplicate_button))
            .into_any_element();
    }
    section
        .description("Yours")
        .child(field(
            "Zoom",
            Some("Bigger or smaller, on top of each game's stretch."),
            div()
                .w(px(140.))
                .child(NumberInput::new(&inputs.zoom))
                .into_any_element(),
        ))
        .child(field(
            "Opacity %",
            None,
            div()
                .w(px(140.))
                .child(NumberInput::new(&inputs.opacity))
                .into_any_element(),
        ))
        .child(
            h_flex()
                .gap_2()
                .flex_wrap()
                .child(
                    Button::new("crosshair-edit")
                        .outline()
                        .icon(IconName::Pencil)
                        .label("Edit")
                        .on_click(move |_, _, cx| paint::open(&this, None, cx)),
                )
                .child(duplicate_button)
                .child(
                    Button::new("crosshair-delete")
                        .outline()
                        .danger()
                        .icon(IconName::Trash)
                        .label("Delete…")
                        .on_click(move |_, window, cx| confirm_delete(name.clone(), window, cx)),
                ),
        )
        .into_any_element()
}

/// Names the games that keep their own crosshair, since selecting one doesn't change them.
fn own_crosshair_note(cx: &App) -> Option<AnyElement> {
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
    Some(note(text, cx))
}

struct Tile {
    name: String,
    image: Arc<RenderImage>,
    selected: bool,
    custom: bool,
}

fn gallery(tiles: &[Tile], this: WeakEntity<SettingsView>, cx: &App) -> AnyElement {
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
            let (click_this, menu_this) = (this.clone(), this.clone());
            v_flex()
                .id(SharedString::from(format!("tile-{}", tile.name)))
                .test_support()
                .aria_label(tile.name.clone())
                .w(px(THUMBNAIL + 44.))
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
                .child(
                    div()
                        .w_full()
                        .text_xs()
                        .text_center()
                        .truncate()
                        .child(tile.name.clone()),
                )
                .on_click(move |e, _, cx| {
                    Store::use_crosshair(cx, pick.clone());
                    if custom && e.click_count() == 2 {
                        paint::open(&click_this, None, cx);
                    }
                })
                .context_menu(move |menu, _, _| {
                    let (use_name, edit_name, duplicate_name, delete_name) =
                        (name.clone(), name.clone(), name.clone(), name.clone());
                    let (edit_this, duplicate_this) = (menu_this.clone(), menu_this.clone());
                    menu.item(
                        PopupMenuItem::new("Use")
                            .disabled(selected)
                            .on_click(move |_, _, cx| Store::use_crosshair(cx, use_name.clone())),
                    )
                    .item(PopupMenuItem::new("Edit").on_click(move |_, window, cx| {
                        if custom {
                            Store::use_crosshair(cx, edit_name.clone());
                            paint::open(&edit_this, None, cx);
                        } else {
                            explain_builtin(edit_name.clone(), edit_this.clone(), window, cx);
                        }
                    }))
                    .item(PopupMenuItem::new("Duplicate").on_click(move |_, _, cx| {
                        duplicate(&duplicate_name, duplicate_this.clone(), cx)
                    }))
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

/// Adds a crosshair to yours under a free name based on `base`, uses it and edits it.
fn add_crosshair(cx: &mut App, base: &str, crosshair: Crosshair, this: WeakEntity<SettingsView>) {
    let previous = Store::get(cx).config.general.crosshair.clone();
    let name = store::unique_name(&Store::get(cx).config, base);
    let crosshair = Crosshair {
        name: name.clone(),
        ..crosshair
    };
    Store::edit(cx, |c| c.crosshairs.push(crosshair));
    Store::use_crosshair(cx, name);
    paint::open(&this, Some(previous), cx);
}

fn new_crosshair(this: WeakEntity<SettingsView>, cx: &mut App) {
    add_crosshair(
        cx,
        "My crosshair",
        Crosshair::pixels("", Pixels::new(15, 15)),
        this,
    );
}

/// Built-in crosshairs can't be edited; offers to duplicate one instead.
fn explain_builtin(
    name: String,
    this: WeakEntity<SettingsView>,
    window: &mut Window,
    cx: &mut App,
) {
    window.open_alert_dialog(cx, move |alert, _, _| {
        let (name, this) = (name.clone(), this.clone());
        alert
            .title(format!("“{name}” can't be edited"))
            .description(
                "Built-in crosshairs stay as they are. Duplicate it to paint your own version.",
            )
            .confirm()
            .ok_text("Duplicate")
            .on_ok(move |_, _, cx| {
                duplicate(&name, this.clone(), cx);
                true
            })
    });
}

/// Copies a crosshair into yours and edits the copy. A built-in one becomes the pixels
/// it draws at 1×, ready to paint over.
fn duplicate(name: &str, this: WeakEntity<SettingsView>, cx: &mut App) {
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
    add_crosshair(cx, &base, copy, this);
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
