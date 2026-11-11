use std::collections::HashMap;
use std::sync::Arc;

use crossz_core::{
    Action, Arms, Binding, Chord, Circle, Color, Crosshair, Dot, Game, Key, Outline, Resolution,
    Scale, Show, Snap, scale::COMMON,
};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _,
    alert::Alert,
    button::{Button, ButtonVariants as _},
    color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState},
    h_flex,
    input::{Input, InputState},
    menu::{DropdownMenu as _, PopupMenu, PopupMenuItem},
    setting::{NumberFieldOptions, SettingField, SettingGroup, SettingItem, SettingPage, Settings},
    switch::Switch,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AnyWindowHandle, App, AppContext as _, Context, Entity, FocusHandle, Global, Hsla,
    ImageSource, InteractiveElement as _, IntoElement, KeyDownEvent, MouseDownEvent,
    ParentElement as _, Render, RenderImage, Rgba, SharedString, StatefulInteractiveElement as _,
    Styled as _, Subscription, TitlebarOptions, WeakEntity, Window, WindowBounds, WindowOptions,
    div, img, px, size,
};

use crate::store::{self, Store};
use crate::updater::{self, State as UpdateState};
use crate::{Update, capture, overlay, preview};

struct SettingsWindow(AnyWindowHandle);

impl Global for SettingsWindow {}

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
        window_bounds: Some(WindowBounds::centered(size(px(1180.), px(760.)), cx)),
        window_min_size: Some(size(px(960.), px(560.))),
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
const THUMBNAIL: f32 = 64.0;
const SCREENS: [(u32, u32); 5] = [
    (1920, 1080),
    (2560, 1440),
    (3440, 1440),
    (3840, 2160),
    (1920, 1200),
];

#[derive(Clone, Copy, PartialEq)]
enum Recording {
    /// A binding of the selected game.
    Binding(usize),
    MasterToggle,
}

struct CachedImage {
    crosshair: Crosshair,
    scale: Scale,
    size: u32,
    image: Arc<RenderImage>,
    zoom: u32,
}

pub struct SettingsView {
    color: Entity<ColorPickerState>,
    outline_color: Entity<ColorPickerState>,
    new_game: Entity<InputState>,
    import_code: Entity<InputState>,
    capture_focus: FocusHandle,
    recording: Option<Recording>,
    images: HashMap<String, CachedImage>,
    _subscriptions: Vec<Subscription>,
}

impl SettingsView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let color = cx.new(|cx| ColorPickerState::new(window, cx));
        let outline_color = cx.new(|cx| ColorPickerState::new(window, cx));
        let new_game = cx.new(|cx| InputState::new(window, cx).placeholder("game.exe"));
        let import_code =
            cx.new(|cx| InputState::new(window, cx).placeholder("Paste a Valorant crosshair code"));
        let subscriptions = vec![
            cx.subscribe(&color, |_, _, event: &ColorPickerEvent, cx| {
                if let ColorPickerEvent::Change(Some(hsla)) = event {
                    let color = from_hsla(*hsla);
                    Store::edit_crosshair(cx, |c| c.color = color);
                }
            }),
            cx.subscribe(&outline_color, |_, _, event: &ColorPickerEvent, cx| {
                if let ColorPickerEvent::Change(Some(hsla)) = event {
                    let color = from_hsla(*hsla);
                    Store::edit_crosshair(cx, |c| {
                        c.outline.get_or_insert(Outline::DEFAULT).color = color;
                    });
                }
            }),
        ];
        SettingsView {
            color,
            outline_color,
            new_game,
            import_code,
            capture_focus: cx.focus_handle(),
            recording: None,
            images: HashMap::new(),
            _subscriptions: subscriptions,
        }
    }

    /// Keeps the color pickers showing the selected crosshair's colors.
    fn sync_colors(&self, window: &mut Window, cx: &mut Context<Self>) {
        let crosshair = Store::get(cx).crosshair();
        let fill = to_hsla(crosshair.color);
        let outline = to_hsla(crosshair.outline.unwrap_or(Outline::DEFAULT).color);
        for (picker, wanted) in [(&self.color, fill), (&self.outline_color, outline)] {
            if picker.read(cx).value() != Some(wanted) {
                picker.update(cx, |state, cx| state.set_value(wanted, window, cx));
            }
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
    }

    fn preview_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let store = Store::get(cx);
        let game = store.game().clone();
        let crosshair = store.crosshair().clone();
        let (width, height) = store.screen;
        let listed = store.selected_game.is_some();
        let scale = game.resolution.scale(width, height);
        let (image, zoom) = self.image("preview".into(), &crosshair, scale, PREVIEW, window, cx);
        let game_name = if listed {
            game.display_name().to_owned()
        } else {
            "other programs".to_owned()
        };
        let how = match game.resolution {
            Resolution::Native => "at native resolution".to_owned(),
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
                        "In {game_name} {how}, on a {width}x{height} screen."
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
            .child(
                Button::new("preview-screen")
                    .small()
                    .outline()
                    .label(format!("Screen: {width}x{height}"))
                    .dropdown_caret(true)
                    .dropdown_menu(move |menu, _, _| {
                        SCREENS.into_iter().fold(menu, |menu, screen| {
                            menu.item(
                                PopupMenuItem::new(format!("{}x{}", screen.0, screen.1))
                                    .checked(screen == (width, height))
                                    .on_click(move |_, _, cx| {
                                        cx.global_mut::<Store>().screen = screen;
                                        cx.refresh_windows();
                                    }),
                            )
                        })
                    }),
            )
            .into_any_element()
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_colors(window, cx);
        let preview = self.preview_panel(window, cx);
        let crosshairs = crosshairs_page(self, window, cx);
        let this = cx.entity().downgrade();
        let error = Store::get(cx).error.clone();
        let input_unavailable = Store::get(cx).input_unavailable;
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
                            Alert::info("update-ready", format!("crossz {version} is ready."))
                                .banner()
                                .title("Update"),
                        )
                    })
                    .child(
                        Settings::new("crossz-settings")
                            .sidebar_width(px(170.))
                            .pages([
                                games_page(self, this.clone(), cx),
                                crosshairs,
                                general_page(self, this, cx),
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
        .child(div().flex_1().child(
            Alert::warning(
                "input-permission",
                "Binds need Input Monitoring permission. Allow crossz, then restart it.",
            )
            .banner(),
        ))
        .child(
            Button::new("open-input-monitoring")
                .small()
                .outline()
                .label("Open System Settings")
                .on_click(|_, _, cx| {
                    cx.open_url(
                        "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent",
                    );
                }),
        )
}

fn games_page(view: &SettingsView, this: WeakEntity<SettingsView>, cx: &App) -> SettingPage {
    let store = Store::get(cx);
    let show = store.config.general.show;

    let show_group = SettingGroup::new().item(
        SettingItem::new(
            "Show the crosshair",
            SettingField::dropdown(
                options([
                    ("in_games", "Only in these games"),
                    ("everywhere", "Everywhere"),
                    ("except_listed", "Everywhere except these games"),
                ]),
                |cx| show_key(Store::get(cx).config.general.show).into(),
                |key, cx| {
                    let show = match key.as_ref() {
                        "everywhere" => Show::Everywhere,
                        "except_listed" => Show::ExceptListed,
                        _ => Show::InGames,
                    };
                    Store::edit(cx, |c| c.general.show = show);
                },
            ),
        )
        .description("crossz follows the window of the game in the foreground."),
    );

    let mut rows: Vec<SettingItem> = (store.config.games.iter().enumerate())
        .map(|(i, game)| game_row(Some(i), game.clone(), store.selected_game == Some(i)))
        .collect();
    if show != Show::InGames {
        rows.push(game_row(
            None,
            store.config.other_programs.clone(),
            store.selected_game.is_none(),
        ));
    }
    rows.push(add_game_row(view.new_game.clone()));
    let games_group = SettingGroup::new().title("Games").items(rows);

    let game = store.game();
    let title = match store.selected_game {
        Some(_) => game.display_name().to_owned(),
        None => "Other programs".to_owned(),
    };
    let custom = !COMMON.contains(&game.resolution);
    let mut items = vec![
        SettingItem::new(
            "Crosshair",
            SettingField::render(|_, _, cx| crosshair_picker(cx)),
        ),
        SettingItem::new(
            "Stretch like",
            SettingField::dropdown(
                resolution_options(),
                |cx| resolution_key(Store::get(cx).game().resolution).into(),
                |key, cx| {
                    let resolution = match key.as_ref() {
                        "custom" if !COMMON.contains(&Store::get(cx).game().resolution) => return,
                        "custom" => Resolution::custom(1600, 1200),
                        key => key.parse().unwrap_or_default(),
                    };
                    Store::edit_game(cx, |g| g.resolution = resolution);
                },
            ),
        )
        .description(
            "Draws the crosshair the way it looks at this resolution, stretched to your screen. Your game can run at any resolution.",
        ),
    ];
    if custom {
        items.push(game_number(
            "Width",
            (320.0, 7680.0, 1.0),
            |g| custom_size(g).0 as f32,
            |g, v| {
                let (_, h) = custom_size(g);
                g.resolution = Resolution::custom((v as u32).max(1), h);
            },
        ));
        items.push(game_number(
            "Height",
            (240.0, 4320.0, 1.0),
            |g| custom_size(g).1 as f32,
            |g, v| {
                let (w, _) = custom_size(g);
                g.resolution = Resolution::custom(w, (v as u32).max(1));
            },
        ));
    }
    items.push(game_number(
        "Offset X",
        (-50.0, 50.0, 1.0),
        |g| g.offset[0] as f32,
        |g, v| {
            g.offset[0] = v as i32;
        },
    ));
    items.push(game_number(
        "Offset Y",
        (-50.0, 50.0, 1.0),
        |g| g.offset[1] as f32,
        |g, v| {
            g.offset[1] = v as i32;
        },
    ));
    let editor = SettingGroup::new()
        .title(title.clone())
        .description(
            if show == Show::ExceptListed && store.selected_game.is_some() {
                "The crosshair is hidden in listed games while showing everywhere except them."
            } else {
                "Applies while this game is in the foreground."
            },
        )
        .items(items);

    let bindings: Vec<SettingItem> = (game.bindings.iter().enumerate())
        .map(|(i, binding)| {
            binding_row(
                i,
                binding.clone(),
                view.recording == Some(Recording::Binding(i)),
                view.capture_focus.clone(),
                this.clone(),
            )
        })
        .collect();
    let add_binding = SettingItem::render(move |_, _, _| {
        let this = this.clone();
        h_flex().w_full().justify_end().child(
            Button::new("binding-add")
                .small()
                .icon(IconName::Plus)
                .label("Add binding")
                .on_click(move |_, window, cx| {
                    let index = Store::get(cx).game().bindings.len();
                    Store::edit_game(cx, |g| {
                        g.bindings.push(Binding {
                            trigger: Chord::key(Key::MOUSE2),
                            action: Action::HoldToHide,
                        });
                    });
                    let _ = this.update(cx, |view, cx| {
                        view.record(Recording::Binding(index), window, cx);
                    });
                }),
        )
    });
    let bindings_group = SettingGroup::new()
        .title(format!("Bindings in {title}"))
        .description(
            "For example, Mouse2 with “Hide while held” hides the crosshair while you aim down sights.",
        )
        .items(bindings)
        .item(add_binding);

    SettingPage::new("Games")
        .icon(IconName::Gamepad2)
        .default_open(true)
        .groups([show_group, games_group, editor, bindings_group])
}

fn game_row(index: Option<usize>, game: Game, selected: bool) -> SettingItem {
    SettingItem::render(move |_, _, cx| {
        let key = index.map_or("other".to_owned(), |i| format!("{i}-{}", game.exe));
        let id = |part: &str| SharedString::from(format!("game-{part}-{key}"));
        let icon = (game.path.as_deref()).and_then(|path| overlay::icon(path, cx));
        let (name, detail) = match index {
            Some(_) => (game.display_name().to_owned(), game.exe.clone()),
            None => (
                "Other programs".to_owned(),
                "Every program that isn't listed".to_owned(),
            ),
        };
        let exe = game.exe.clone();
        let remove_exe = game.exe.clone();
        h_flex()
            .w_full()
            .gap_3()
            .child(program_icon(icon, cx))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(div().truncate().child(name))
                    .child(
                        div()
                            .text_xs()
                            .truncate()
                            .text_color(cx.theme().muted_foreground)
                            .child(detail),
                    ),
            )
            .child(
                Button::new(id("edit"))
                    .small()
                    .label(if selected { "Editing" } else { "Edit" })
                    .when(selected, |b| b.primary())
                    .when(!selected, |b| b.outline())
                    .on_click(move |_, _, cx| Store::select_game(cx, index)),
            )
            .when(index.is_some(), |row| {
                row.child(Switch::new(id("enabled")).checked(game.enabled).on_change(
                    move |on: &bool, _, cx| {
                        let on = *on;
                        Store::edit(cx, |c| {
                            if let Some(g) = c.games.iter_mut().find(|g| g.exe == exe) {
                                g.enabled = on;
                            }
                        });
                    },
                ))
                .child(
                    Button::new(id("remove"))
                        .small()
                        .ghost()
                        .icon(IconName::Trash)
                        .on_click(move |_, _, cx| {
                            Store::edit(cx, |c| c.games.retain(|g| g.exe != remove_exe));
                        }),
                )
            })
    })
}

fn program_icon(icon: Option<Arc<RenderImage>>, cx: &App) -> AnyElement {
    match icon {
        Some(icon) => img(ImageSource::Render(icon))
            .size(px(24.))
            .into_any_element(),
        None => div()
            .size(px(24.))
            .flex()
            .items_center()
            .justify_center()
            .text_color(cx.theme().muted_foreground)
            .child(IconName::Gamepad2)
            .into_any_element(),
    }
}

fn add_game_row(input: Entity<InputState>) -> SettingItem {
    SettingItem::render(move |_, _, _| {
        let add_input = input.clone();
        h_flex()
            .w_full()
            .gap_2()
            .child(
                Button::new("game-running")
                    .small()
                    .primary()
                    .icon(IconName::Plus)
                    .label("Add a running game")
                    .dropdown_caret(true)
                    .dropdown_menu(running_games_menu),
            )
            .child(div().flex_1().child(Input::new(&input).small()))
            .child(
                Button::new("game-add")
                    .small()
                    .outline()
                    .label("Add by name")
                    .on_click(move |_, window, cx| {
                        let exe = add_input.read(cx).value().trim().to_owned();
                        add_game(Game::new(exe), cx);
                        add_input.update(cx, |state, cx| state.set_value("", window, cx));
                    }),
            )
    })
}

/// Programs with a window right now, newest scan each time the menu opens.
fn running_games_menu(menu: PopupMenu, _: &mut Window, cx: &mut Context<PopupMenu>) -> PopupMenu {
    let programs = overlay::running_programs();
    if programs.is_empty() {
        let note = if cfg!(windows) {
            "No programs with a window found"
        } else {
            "Only available on Windows"
        };
        return menu.item(PopupMenuItem::new(note).disabled(true));
    }
    let listed: Vec<String> = (Store::get(cx).config.games.iter())
        .map(|g| g.exe.to_ascii_lowercase())
        .collect();
    let menu = menu.scrollable(true).max_h(px(420.));
    programs.into_iter().fold(menu, |menu, program| {
        let icon = overlay::icon(&program.path, cx);
        let already = listed.contains(&program.exe.to_ascii_lowercase());
        let (title, exe) = (program.title.clone(), program.exe.clone());
        menu.item(
            PopupMenuItem::element(move |_, cx| {
                h_flex()
                    .gap_2()
                    .child(program_icon(icon.clone(), cx))
                    .child(
                        v_flex().child(div().child(title.clone())).child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(exe.clone()),
                        ),
                    )
            })
            .disabled(already)
            .on_click(move |_, _, cx| {
                let mut game = Game::new(program.exe.clone());
                game.name = program.title.clone();
                game.path = Some(program.path.clone());
                add_game(game, cx);
            }),
        )
    })
}

fn add_game(game: Game, cx: &mut App) {
    if game.exe.is_empty() {
        return;
    }
    let mut added = None;
    Store::edit(cx, |c| {
        if !c
            .games
            .iter()
            .any(|g| g.exe.eq_ignore_ascii_case(&game.exe))
        {
            c.games.push(game);
            added = Some(c.games.len() - 1);
        }
    });
    if let Some(index) = added {
        Store::select_game(cx, Some(index));
    }
}

fn crosshair_picker(cx: &App) -> AnyElement {
    let current = Store::get(cx).game().crosshair.clone();
    Button::new("game-crosshair")
        .small()
        .outline()
        .label(current.clone())
        .dropdown_caret(true)
        .dropdown_menu(move |menu, _, cx| {
            let config = &Store::get(cx).config;
            let names = |custom: bool| -> Vec<String> {
                (config.all_crosshairs())
                    .filter(|c| config.crosshairs.iter().any(|x| x.name == c.name) == custom)
                    .map(|c| c.name.clone())
                    .collect()
            };
            let (builtin, custom) = (names(false), names(true));
            let item = |name: String| {
                PopupMenuItem::new(name.clone())
                    .checked(name == current)
                    .on_click(move |_, _, cx| {
                        let name = name.clone();
                        Store::edit_game(cx, |g| g.crosshair = name.clone());
                        Store::select_crosshair(cx, name);
                    })
            };
            let menu = builtin
                .into_iter()
                .fold(menu.label("Built-in"), |m, n| m.item(item(n)));
            if custom.is_empty() {
                return menu;
            }
            custom
                .into_iter()
                .fold(menu.separator().label("Custom"), |m, n| m.item(item(n)))
        })
        .into_any_element()
}

fn binding_row(
    index: usize,
    binding: Binding,
    recording: bool,
    focus: FocusHandle,
    this: WeakEntity<SettingsView>,
) -> SettingItem {
    SettingItem::render(move |_, _, cx| {
        let id = |part: &str| SharedString::from(format!("binding-{part}-{index}"));
        let action = binding.action.clone();
        let action_menu = move |menu: PopupMenu, _: &mut Window, cx: &mut Context<PopupMenu>| {
            let set = |action: Action| {
                move |_: &_, _: &mut Window, cx: &mut App| {
                    let action = action.clone();
                    Store::edit_game(cx, |g| {
                        if let Some(b) = g.bindings.get_mut(index) {
                            b.action = action;
                        }
                    });
                }
            };
            let simple = [Action::HoldToHide, Action::Toggle, Action::HoldToShow];
            let menu = simple.into_iter().fold(menu, |menu, a| {
                menu.item(
                    PopupMenuItem::new(action_label(&a))
                        .checked(action == a)
                        .on_click(set(a.clone())),
                )
            });
            let names: Vec<String> = (Store::get(cx).config.all_crosshairs())
                .map(|c| c.name.clone())
                .collect();
            names.into_iter().fold(menu.separator(), |menu, name| {
                let a = Action::SetCrosshair(name);
                menu.item(
                    PopupMenuItem::new(action_label(&a))
                        .checked(action == a)
                        .on_click(set(a.clone())),
                )
            })
        };
        h_flex()
            .w_full()
            .gap_3()
            .child(chord_button(
                id("trigger"),
                Some(binding.trigger),
                recording,
                focus.clone(),
                Recording::Binding(index),
                this.clone(),
                cx,
            ))
            .child(div().flex_1())
            .child(
                Button::new(id("action"))
                    .small()
                    .outline()
                    .label(action_label(&binding.action))
                    .dropdown_caret(true)
                    .dropdown_menu(action_menu),
            )
            .child(
                Button::new(id("remove"))
                    .small()
                    .ghost()
                    .icon(IconName::Trash)
                    .on_click(move |_, _, cx| {
                        Store::edit_game(cx, |g| {
                            if index < g.bindings.len() {
                                g.bindings.remove(index);
                            }
                        });
                    }),
            )
    })
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
        let label = chord.map_or("Not set".to_owned(), |c| c.to_string());
        return div()
            .child(
                Button::new(id)
                    .small()
                    .outline()
                    .icon(IconName::Pencil)
                    .label(label)
                    .on_click(move |_, window, cx| {
                        let _ = this.update(cx, |view, cx| view.record(target, window, cx));
                    }),
            )
            .into_any_element();
    }
    let on_key = this.clone();
    let on_mouse = this;
    div()
        .child(
            div()
                .id(id)
                .track_focus(&focus)
                .px_3()
                .py_1()
                .rounded(cx.theme().radius)
                .border_1()
                .border_color(cx.theme().ring)
                .bg(cx.theme().accent)
                .text_sm()
                .child("Press a key, or click here with a mouse button (Esc cancels)")
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
                }),
        )
        .into_any_element()
}

fn crosshairs_page(view: &mut SettingsView, window: &mut Window, cx: &mut App) -> SettingPage {
    let store = Store::get(cx);
    let selected = store.selected_crosshair.clone();
    let is_custom = store.crosshair_is_custom();
    let all: Vec<(Crosshair, bool)> = (store.config.all_crosshairs())
        .map(|c| {
            (
                c.clone(),
                store.config.crosshairs.iter().any(|x| x.name == c.name),
            )
        })
        .collect();
    let mut tiles = |custom: bool| -> Vec<(String, Arc<RenderImage>, bool)> {
        (all.iter().filter(|(_, c)| *c == custom))
            .map(|(crosshair, _)| {
                let key = format!("thumb:{}", crosshair.name);
                let (image, _) = view.image(key, crosshair, Scale::ONE, THUMBNAIL, window, cx);
                (crosshair.name.clone(), image, crosshair.name == selected)
            })
            .collect()
    };
    let (builtin, custom) = (tiles(false), tiles(true));

    let builtin_group = SettingGroup::new()
        .title("Built-in")
        .description("Always available. Customize one to change it.")
        .item(SettingItem::render(move |_, _, cx| gallery(&builtin, cx)));
    let import_code = view.import_code.clone();
    let custom_group = SettingGroup::new()
        .title("Custom")
        .item(SettingItem::render(move |_, _, cx| {
            let code_input = import_code.clone();
            v_flex()
                .w_full()
                .gap_3()
                .when(!custom.is_empty(), |this| this.child(gallery(&custom, cx)))
                .child(
                    h_flex().child(
                        Button::new("crosshair-new")
                            .small()
                            .icon(IconName::Plus)
                            .label("New crosshair")
                            .on_click(|_, _, cx| {
                                copy_crosshair(cx, Crosshair::blank("New crosshair"))
                            }),
                    ),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .child(div().flex_1().child(Input::new(&import_code).small()))
                        .child(
                            Button::new("crosshair-import")
                                .small()
                                .outline()
                                .label("Import")
                                .on_click(move |_, window, cx| {
                                    let code = code_input.read(cx).value().to_string();
                                    if import_valorant(&code, cx) {
                                        code_input.update(cx, |state, cx| {
                                            state.set_value("", window, cx)
                                        });
                                    }
                                }),
                        ),
                )
        }));

    let game_name = {
        let store = Store::get(cx);
        match store.selected_game {
            Some(_) => store.game().display_name().to_owned(),
            None => "other programs".to_owned(),
        }
    };
    let actions = SettingItem::render(move |_, _, cx| {
        let used = Store::get(cx).game().crosshair == Store::get(cx).selected_crosshair;
        h_flex()
            .w_full()
            .justify_end()
            .gap_2()
            .child(
                Button::new("crosshair-use")
                    .small()
                    .primary()
                    .label(if used {
                        format!("Used in {game_name}")
                    } else {
                        format!("Use in {game_name}")
                    })
                    .disabled(used)
                    .on_click(|_, _, cx| {
                        let name = Store::get(cx).selected_crosshair.clone();
                        Store::edit_game(cx, |g| g.crosshair = name);
                    }),
            )
            .child(
                Button::new("crosshair-copy")
                    .small()
                    .outline()
                    .label(if is_custom { "Duplicate" } else { "Customize" })
                    .on_click(|_, _, cx| {
                        let crosshair = Store::get(cx).crosshair().clone();
                        copy_crosshair(cx, crosshair);
                    }),
            )
            .when(is_custom, |row| {
                row.child(
                    Button::new("crosshair-delete")
                        .small()
                        .danger()
                        .label("Delete")
                        .on_click(|_, _, cx| {
                            let name = Store::get(cx).selected_crosshair.clone();
                            Store::edit(cx, |c| store::remove_crosshair(c, &name));
                        }),
                )
            })
    });

    let mut editor = SettingGroup::new().title(selected.clone()).item(actions);
    if is_custom {
        editor = editor.items(crosshair_editor(view));
    } else {
        editor = editor
            .description("Built-in crosshairs can't be changed. Customize makes an editable copy.");
    }

    SettingPage::new("Crosshairs")
        .icon(IconName::Crosshair)
        .groups([builtin_group, custom_group, editor])
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

/// Adds a crosshair from a Valorant code; false (with the reason shown) if it isn't one.
fn import_valorant(code: &str, cx: &mut App) -> bool {
    let name = store::unique_name(&Store::get(cx).config, "Imported");
    match crossz_core::valorant::parse(code, &name) {
        Ok(crosshair) => {
            Store::edit(cx, |c| c.crosshairs.push(crosshair));
            Store::select_crosshair(cx, name);
            true
        }
        Err(e) => {
            Store::report(cx, format!("Couldn't import that code: {e}"));
            false
        }
    }
}

fn copy_crosshair(cx: &mut App, mut crosshair: Crosshair) {
    let base = if crosshair.name == "New crosshair" {
        crosshair.name.clone()
    } else {
        format!("My {}", crosshair.name.to_lowercase())
    };
    crosshair.name = store::unique_name(&Store::get(cx).config, &base);
    let name = crosshair.name.clone();
    Store::edit(cx, |c| c.crosshairs.push(crosshair));
    Store::select_crosshair(cx, name);
}

fn crosshair_editor(view: &SettingsView) -> Vec<SettingItem> {
    let crosshair = |cx: &App| Store::get(cx).crosshair().clone();
    let color = view.color.clone();
    let outline_color = view.outline_color.clone();
    vec![
        SettingItem::new(
            "Name",
            SettingField::input(
                move |cx| crosshair(cx).name.into(),
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
            "Color",
            SettingField::render(move |_, _, _| ColorPicker::new(&color).small()),
        ),
        number(
            "Opacity %",
            (0.0, 100.0, 5.0),
            |c| c.opacity * 100.0,
            |c, v| {
                c.opacity = v / 100.0;
            },
        ),
        SettingItem::new(
            "Pixel snapping",
            SettingField::dropdown(
                options([
                    ("round", "Round"),
                    ("floor", "Thinner"),
                    ("ceil", "Thicker"),
                    ("anti_alias", "Smooth (anti-aliased)"),
                ]),
                |cx| snap_key(Store::get(cx).crosshair().snap).into(),
                |key, cx| {
                    let snap = match key.as_ref() {
                        "floor" => Snap::Floor,
                        "ceil" => Snap::Ceil,
                        "anti_alias" => Snap::AntiAlias,
                        _ => Snap::Round,
                    };
                    Store::edit_crosshair(cx, |c| c.snap = snap);
                },
            ),
        )
        .description("How sizes that land between pixels, like 1.5px after scaling, are drawn."),
        toggle(
            "Lines",
            |c| c.arms.is_some(),
            |c, on| c.arms = on.then_some(Arms::DEFAULT),
        ),
        number(
            "Line length",
            (0.0, 100.0, 1.0),
            |c| arms_of(c).length,
            |c, v| arms_mut(c).length = v,
        ),
        number(
            "Line thickness",
            (0.5, 20.0, 0.5),
            |c| arms_of(c).thickness,
            |c, v| {
                arms_mut(c).thickness = v;
            },
        ),
        number(
            "Gap",
            (-10.0, 50.0, 1.0),
            |c| arms_of(c).gap,
            |c, v| arms_mut(c).gap = v,
        ),
        SettingItem::new(
            "T-style",
            SettingField::checkbox(
                |cx| arms_of(Store::get(cx).crosshair()).t_style,
                |on, cx| Store::edit_crosshair(cx, |c| arms_mut(c).t_style = on),
            ),
        ),
        number(
            "Line opacity %",
            (0.0, 100.0, 5.0),
            |c| arms_of(c).opacity * 100.0,
            |c, v| arms_mut(c).opacity = v / 100.0,
        ),
        toggle(
            "Outer lines",
            |c| c.outer.is_some(),
            |c, on| c.outer = on.then_some(OUTER_DEFAULT),
        ),
        number(
            "Outer length",
            (0.0, 100.0, 1.0),
            |c| outer_of(c).length,
            |c, v| outer_mut(c).length = v,
        ),
        number(
            "Outer thickness",
            (0.5, 20.0, 0.5),
            |c| outer_of(c).thickness,
            |c, v| outer_mut(c).thickness = v,
        ),
        number(
            "Outer gap",
            (-10.0, 100.0, 1.0),
            |c| outer_of(c).gap,
            |c, v| outer_mut(c).gap = v,
        ),
        number(
            "Outer opacity %",
            (0.0, 100.0, 5.0),
            |c| outer_of(c).opacity * 100.0,
            |c, v| outer_mut(c).opacity = v / 100.0,
        ),
        toggle(
            "Center dot",
            |c| c.dot.is_some(),
            |c, on| c.dot = on.then_some(Dot::DEFAULT),
        ),
        number(
            "Dot size",
            (0.5, 20.0, 0.5),
            |c| c.dot.unwrap_or(Dot::DEFAULT).size,
            |c, v| {
                c.dot.get_or_insert(Dot::DEFAULT).size = v;
            },
        ),
        number(
            "Dot opacity %",
            (0.0, 100.0, 5.0),
            |c| c.dot.unwrap_or(Dot::DEFAULT).opacity * 100.0,
            |c, v| c.dot.get_or_insert(Dot::DEFAULT).opacity = v / 100.0,
        ),
        toggle(
            "Circle",
            |c| c.circle.is_some(),
            |c, on| c.circle = on.then_some(Circle::DEFAULT),
        ),
        number(
            "Circle radius",
            (1.0, 100.0, 1.0),
            |c| circle_of(c).radius,
            |c, v| {
                c.circle.get_or_insert(Circle::DEFAULT).radius = v;
            },
        ),
        number(
            "Circle thickness",
            (0.5, 20.0, 0.5),
            |c| circle_of(c).thickness,
            |c, v| {
                c.circle.get_or_insert(Circle::DEFAULT).thickness = v;
            },
        ),
        toggle(
            "Outline",
            |c| c.outline.is_some(),
            |c, on| {
                c.outline = on.then_some(Outline::DEFAULT);
            },
        ),
        number(
            "Outline thickness",
            (0.5, 10.0, 0.5),
            |c| outline_of(c).thickness,
            |c, v| {
                c.outline.get_or_insert(Outline::DEFAULT).thickness = v;
            },
        ),
        SettingItem::new(
            "Outline color",
            SettingField::render(move |_, _, _| ColorPicker::new(&outline_color).small()),
        ),
    ]
}

fn general_page(view: &SettingsView, this: WeakEntity<SettingsView>, cx: &App) -> SettingPage {
    let store = Store::get(cx);
    let master = store.config.general.master_toggle;
    let recording = view.recording == Some(Recording::MasterToggle);
    let focus = view.capture_focus.clone();
    let path = store.path.clone();
    let path_label = path.display().to_string();
    let update_state = cx.global::<Update>().state.clone();

    let startup = SettingGroup::new().title("Startup").items([
        SettingItem::new(
            if cfg!(windows) {
                "Start with Windows"
            } else {
                "Start at login"
            },
            SettingField::switch(
                |cx| Store::get(cx).config.general.start_with_windows,
                |on, cx| Store::edit(cx, |c| c.general.start_with_windows = on),
            ),
        )
        .description("Starts in the tray when you log in."),
        SettingItem::new(
            "Start minimized",
            SettingField::switch(
                |cx| Store::get(cx).config.general.start_minimized,
                |on, cx| Store::edit(cx, |c| c.general.start_minimized = on),
            ),
        )
        .description("When starting at login, open to the tray instead of this window."),
    ]);

    let updates = SettingGroup::new().title("Updates").items([
        SettingItem::new(
            format!("crossz {}", updater::VERSION),
            SettingField::render(move |_, _, _| update_button(&update_state)),
        )
        .description(update_description(&cx.global::<Update>().state)),
        SettingItem::new(
            "Check for updates automatically",
            SettingField::switch(
                |cx| Store::get(cx).config.general.check_for_updates,
                |on, cx| Store::edit(cx, |c| c.general.check_for_updates = on),
            ),
        )
        .description("Daily. Updates are downloaded in the background and installed when you restart crossz."),
    ]);

    let controls = SettingGroup::new().title("Controls").items([
        SettingItem::new(
            "Master toggle",
            SettingField::render(move |_, _, cx| {
                h_flex()
                    .gap_2()
                    .child(chord_button(
                        "master-toggle".into(),
                        master,
                        recording,
                        focus.clone(),
                        Recording::MasterToggle,
                        this.clone(),
                        cx,
                    ))
                    .child(
                        Button::new("master-toggle-clear")
                            .small()
                            .ghost()
                            .icon(IconName::X)
                            .disabled(master.is_none())
                            .on_click(|_, _, cx| {
                                Store::edit(cx, |c| c.general.master_toggle = None);
                            }),
                    )
            }),
        )
        .description("Turns crossz on or off everywhere."),
        SettingItem::new(
            "Hide from screen capture",
            SettingField::switch(
                |cx| Store::get(cx).config.general.exclude_from_capture,
                |on, cx| Store::edit(cx, |c| c.general.exclude_from_capture = on),
            ),
        )
        .description("Keeps the crosshair out of screenshots, OBS and Discord streams."),
        SettingItem::new(
            "Settings file",
            SettingField::render(move |_, _, _| {
                let path = path.clone();
                Button::new("open-config")
                    .small()
                    .outline()
                    .icon(IconName::FolderOpen)
                    .label("Show in folder")
                    .on_click(move |_, _, cx| cx.reveal_path(&path))
            }),
        )
        .description(path_label),
    ]);

    SettingPage::new("General")
        .icon(IconName::Settings)
        .groups([startup, updates, controls])
}

fn update_button(state: &UpdateState) -> Button {
    let button = Button::new("update").small();
    match state {
        UpdateState::Ready { version, .. } => button
            .primary()
            .label(format!("Restart to update to {version}"))
            .on_click(|_, _, cx| Update::restart(cx)),
        UpdateState::Checking | UpdateState::Downloading(_) => {
            button.outline().label("Checking…").disabled(true)
        }
        UpdateState::Unavailable(_) => button.outline().label("Check now").disabled(true),
        _ => button
            .outline()
            .label("Check now")
            .on_click(|_, _, cx| Update::check(cx)),
    }
}

fn update_description(state: &UpdateState) -> String {
    match state {
        UpdateState::Idle => "Not checked yet.".into(),
        UpdateState::Checking => "Checking for updates…".into(),
        UpdateState::UpToDate => "You have the latest version.".into(),
        UpdateState::Downloading(v) => format!("Downloading {v}…"),
        UpdateState::Ready { version, .. } => {
            format!("{version} is downloaded and verified. Restart crossz to use it.")
        }
        UpdateState::Unavailable(reason) => reason.clone(),
        UpdateState::Failed(error) => error.clone(),
    }
}

fn number(
    title: &'static str,
    (min, max, step): (f64, f64, f64),
    get: fn(&Crosshair) -> f32,
    set: fn(&mut Crosshair, f32),
) -> SettingItem {
    SettingItem::new(
        title,
        SettingField::number_input(
            NumberFieldOptions { min, max, step },
            move |cx| (get(Store::get(cx).crosshair()) as f64 * 100.0).round() / 100.0,
            move |value, cx| Store::edit_crosshair(cx, |c| set(c, value as f32)),
        ),
    )
}

fn game_number(
    title: &'static str,
    (min, max, step): (f64, f64, f64),
    get: fn(&Game) -> f32,
    set: fn(&mut Game, f32),
) -> SettingItem {
    SettingItem::new(
        title,
        SettingField::number_input(
            NumberFieldOptions { min, max, step },
            move |cx| get(Store::get(cx).game()) as f64,
            move |value, cx| Store::edit_game(cx, |g| set(g, value as f32)),
        ),
    )
}

fn toggle(
    title: &'static str,
    get: fn(&Crosshair) -> bool,
    set: fn(&mut Crosshair, bool),
) -> SettingItem {
    SettingItem::new(
        title,
        SettingField::switch(
            move |cx| get(Store::get(cx).crosshair()),
            move |on, cx| Store::edit_crosshair(cx, |c| set(c, on)),
        ),
    )
}

const OUTER_DEFAULT: Arms = Arms {
    length: 2.0,
    thickness: 2.0,
    gap: 9.0,
    t_style: false,
    opacity: 1.0,
};

fn outer_of(c: &Crosshair) -> Arms {
    c.outer.unwrap_or(OUTER_DEFAULT)
}

fn outer_mut(c: &mut Crosshair) -> &mut Arms {
    c.outer.get_or_insert(OUTER_DEFAULT)
}

fn arms_of(c: &Crosshair) -> Arms {
    c.arms.unwrap_or(Arms::DEFAULT)
}

fn arms_mut(c: &mut Crosshair) -> &mut Arms {
    c.arms.get_or_insert(Arms::DEFAULT)
}

fn circle_of(c: &Crosshair) -> Circle {
    c.circle.unwrap_or(Circle::DEFAULT)
}

fn outline_of(c: &Crosshair) -> Outline {
    c.outline.unwrap_or(Outline::DEFAULT)
}

fn custom_size(game: &Game) -> (u32, u32) {
    match game.resolution {
        Resolution::Custom { width, height } => (width, height),
        Resolution::Native => (1280, 960),
    }
}

fn options<K: Into<SharedString>, L: Into<SharedString>>(
    items: impl IntoIterator<Item = (K, L)>,
) -> Vec<(SharedString, SharedString)> {
    items
        .into_iter()
        .map(|(k, l)| (k.into(), l.into()))
        .collect()
}

fn resolution_options() -> Vec<(SharedString, SharedString)> {
    let label = |r: Resolution| match r {
        Resolution::Native => "Native (no stretch)".to_owned(),
        Resolution::Custom { width, height } => {
            let aspect = match (width * 3 == height * 4, width * 4 == height * 5) {
                (true, _) => " (4:3)",
                (_, true) => " (5:4)",
                _ => "",
            };
            format!("{width}x{height}{aspect}")
        }
    };
    let common = COMMON.iter().map(|&r| (r.to_string(), label(r)));
    options(common.chain([("custom".to_owned(), "Custom".to_owned())]))
}

fn resolution_key(resolution: Resolution) -> String {
    if COMMON.contains(&resolution) {
        resolution.to_string()
    } else {
        "custom".into()
    }
}

fn show_key(show: Show) -> &'static str {
    match show {
        Show::InGames => "in_games",
        Show::ExceptListed => "except_listed",
        Show::Everywhere => "everywhere",
    }
}

fn snap_key(snap: Snap) -> &'static str {
    match snap {
        Snap::Round => "round",
        Snap::Floor => "floor",
        Snap::Ceil => "ceil",
        Snap::AntiAlias => "anti_alias",
    }
}

fn action_label(action: &Action) -> String {
    match action {
        Action::Toggle => "Toggle on/off".into(),
        Action::HoldToHide => "Hide while held".into(),
        Action::HoldToShow => "Show only while held".into(),
        Action::SetCrosshair(name) => format!("Switch to {name}"),
    }
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
