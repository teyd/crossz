//! Where the crosshair shows, and per game: crosshair, stretch, offset and binds. A game
//! is edited in a sheet, so the page itself stays the same whichever game you pick.

use std::sync::Arc;

use crossz_core::{Action, Binding, Chord, Game, Key, Resolution, Show, scale::COMMON};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    form::{Field, Form},
    h_flex,
    input::{Input, NumberInput},
    menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenu, PopupMenuItem},
    setting::{SettingField, SettingGroup, SettingItem, SettingPage},
    switch::Switch,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    RenderImage, SharedString, Styled as _, WeakEntity, Window, div, img, px,
};

use super::{Recording, SettingsView, chord_button, options};
use crate::overlay;
use crate::store::Store;

pub(super) fn page(view: &SettingsView, this: WeakEntity<SettingsView>, cx: &App) -> SettingPage {
    let config = &Store::get(cx).config;
    let show = config.general.show;
    let mode = SettingGroup::new().item(
        SettingItem::new(
            "Show the crosshair",
            SettingField::dropdown(
                options([
                    ("in_games", "Only in my games"),
                    ("except_listed", "Everywhere except some programs"),
                    ("everywhere", "Everywhere"),
                ]),
                |cx| show_key(Store::get(cx).config.general.show).into(),
                |key, cx| {
                    let show = match key.as_ref() {
                        "except_listed" => Show::ExceptListed,
                        "everywhere" => Show::Everywhere,
                        _ => Show::InGames,
                    };
                    Store::edit(cx, |c| c.general.show = show);
                    Store::select_game(cx, None);
                },
            ),
        )
        .description(match show {
            Show::InGames => "Only while one of your games is in front, each with its own setup.",
            Show::ExceptListed => "In every program but the ones listed here.",
            Show::Everywhere => "In every program, with one setup.",
        }),
    );

    let games: Vec<SettingItem> = (config.games.iter().enumerate())
        .map(|(i, game)| game_row(Some(i), game.clone(), show, this.clone()))
        .collect();
    let add = add_game_row(view, show);
    let groups = match show {
        Show::InGames => vec![
            mode,
            SettingGroup::new()
                .title("Games")
                .description("Each game has its own crosshair, stretch, offset and binds. Right-click a game for more.")
                .item(add)
                .items(games),
        ],
        Show::ExceptListed => vec![
            mode,
            SettingGroup::new()
                .title("Hidden in")
                .item(add)
                .items(games),
            SettingGroup::new().title("Everywhere else").item(game_row(
                None,
                config.other_programs.clone(),
                show,
                this,
            )),
        ],
        Show::Everywhere => vec![
            mode,
            SettingGroup::new().title("All programs").item(game_row(
                None,
                config.other_programs.clone(),
                show,
                this,
            )),
        ],
    };
    SettingPage::new("Games")
        .icon(IconName::Gamepad2)
        .default_open(true)
        .groups(groups)
}

/// A listed game, or with `None` the setup used outside listed games.
fn game_row(
    index: Option<usize>,
    game: Game,
    show: Show,
    this: WeakEntity<SettingsView>,
) -> SettingItem {
    SettingItem::render(move |_, _, cx| {
        let key = index.map_or("other".to_owned(), |i| format!("{i}-{}", game.exe));
        let id = |part: &str| SharedString::from(format!("game-{part}-{key}"));
        let icon = (game.path.as_deref()).and_then(|path| overlay::icon(path, cx));
        let (name, detail) = match (index, show) {
            (Some(_), _) => (game.display_name().to_owned(), game.exe.clone()),
            (None, Show::Everywhere) => ("All programs".to_owned(), summary(&game)),
            (None, _) => ("Every other program".to_owned(), summary(&game)),
        };
        let (exe, remove_exe) = (game.exe.clone(), game.exe.clone());
        let editable = index.is_none() || show == Show::InGames;
        let toggles = index.is_some() && show == Show::InGames;
        let this = this.clone();
        let menu_this = this.clone();
        let (menu_exe, enabled) = (game.exe.clone(), game.enabled);
        h_flex()
            .id(id("row"))
            .w_full()
            .gap_3()
            .context_menu(move |menu, _, _| {
                let (edit_this, toggle_exe, remove_exe) =
                    (menu_this.clone(), menu_exe.clone(), menu_exe.clone());
                menu.when(editable, |menu| {
                    menu.item(PopupMenuItem::new("Edit…").on_click(move |_, window, cx| {
                        open_editor(index, edit_this.clone(), window, cx)
                    }))
                })
                .when(toggles, |menu| {
                    menu.item(
                        PopupMenuItem::new(if enabled { "Disable" } else { "Enable" }).on_click(
                            move |_, _, cx| {
                                Store::edit(cx, |c| {
                                    if let Some(g) =
                                        c.games.iter_mut().find(|g| g.exe == toggle_exe)
                                    {
                                        g.enabled = !g.enabled;
                                    }
                                });
                            },
                        ),
                    )
                })
                .when(index.is_some(), |menu| {
                    menu.separator()
                        .item(PopupMenuItem::new("Remove").on_click(move |_, _, cx| {
                            Store::edit(cx, |c| c.games.retain(|g| g.exe != remove_exe));
                        }))
                })
            })
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
            .when(editable, |row| {
                row.child(
                    Button::new(id("edit"))
                        .outline()
                        .icon(IconName::Pencil)
                        .label("Edit…")
                        .on_click(move |_, window, cx| {
                            open_editor(index, this.clone(), window, cx)
                        }),
                )
            })
            .when(toggles, |row| {
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
            })
            .when(index.is_some(), |row| {
                row.child(
                    Button::new(id("remove"))
                        .ghost()
                        .icon(IconName::Trash)
                        .tooltip("Remove")
                        .on_click(move |_, _, cx| {
                            Store::edit(cx, |c| c.games.retain(|g| g.exe != remove_exe));
                        }),
                )
            })
    })
}

/// "Own crosshair: Dot · stretched like 1280x960 · 1 bind"
fn summary(game: &Game) -> String {
    let stretch = match game.resolution {
        Resolution::Native => "not stretched".to_owned(),
        r => format!("stretched like {r}"),
    };
    let binds = match game.bindings.len() {
        0 => "no binds".to_owned(),
        1 => "1 bind".to_owned(),
        n => format!("{n} binds"),
    };
    match &game.crosshair {
        Some(own) => format!("Own crosshair: {own} · {stretch} · {binds}"),
        None => format!("{stretch} · {binds}"),
    }
}

fn program_icon(icon: Option<Arc<RenderImage>>, cx: &App) -> AnyElement {
    match icon {
        Some(icon) => img(gpui_kit::ImageSource::Render(icon))
            .size(px(28.))
            .into_any_element(),
        None => div()
            .size(px(28.))
            .flex()
            .items_center()
            .justify_center()
            .text_color(cx.theme().muted_foreground)
            .child(IconName::Gamepad2)
            .into_any_element(),
    }
}

fn add_game_row(view: &SettingsView, show: Show) -> SettingItem {
    let input = view.new_game.clone();
    let what = if show == Show::InGames {
        "game"
    } else {
        "program"
    };
    SettingItem::render(move |_, _, _| {
        let add_input = input.clone();
        h_flex()
            .w_full()
            .gap_2()
            .child(
                Button::new("game-running")
                    .icon(IconName::Plus)
                    .label(format!("Add a running {what}"))
                    .dropdown_caret(true)
                    .dropdown_menu(running_programs_menu),
            )
            .child(div().flex_1().child(Input::new(&input)))
            .child(
                Button::new("game-add")
                    .outline()
                    .label("Add")
                    .on_click(move |_, window, cx| {
                        let exe = add_input.read(cx).value().trim().to_owned();
                        add_game(Game::new(exe), cx);
                        add_input.update(cx, |state, cx| state.set_value("", window, cx));
                    }),
            )
    })
}

/// Programs with a window right now, rescanned each time the menu opens.
fn running_programs_menu(
    menu: PopupMenu,
    _: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let programs = overlay::running_programs();
    if programs.is_empty() {
        let note = if cfg!(any(windows, target_os = "macos")) {
            "No programs with a window found"
        } else {
            "Not available on this system yet"
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
    Store::edit(cx, |c| {
        if !c
            .games
            .iter()
            .any(|g| g.exe.eq_ignore_ascii_case(&game.exe))
        {
            c.games.push(game);
        }
    });
}

fn open_editor(
    index: Option<usize>,
    this: WeakEntity<SettingsView>,
    window: &mut Window,
    cx: &mut App,
) {
    Store::select_game(cx, index);
    let _ = this.update(cx, |view, cx| view.load_game_inputs(window, cx));
    window.open_sheet(cx, move |sheet, window, cx| {
        let title = match Store::get(cx).selected_game {
            Some(_) => Store::get(cx).game().display_name().to_owned(),
            None if Store::get(cx).config.general.show == Show::Everywhere => {
                "All programs".to_owned()
            }
            None => "Every other program".to_owned(),
        };
        sheet
            .title(title)
            .size(px(480.))
            .child(editor(this.clone(), window, cx))
    });
}

impl SettingsView {
    /// Fills the game editor's number fields from the game being edited.
    fn load_game_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let game = Store::get(cx).game().clone();
        let (width, height) = match game.resolution {
            Resolution::Custom { width, height } => (width, height),
            Resolution::Native => (1280, 960),
        };
        let inputs = &self.game_inputs;
        for (input, value) in [
            (&inputs.offset_x, game.offset[0] as i64),
            (&inputs.offset_y, game.offset[1] as i64),
            (&inputs.width, width as i64),
            (&inputs.height, height as i64),
        ] {
            input.update(cx, |state, cx| {
                state.set_value(value.to_string(), window, cx)
            });
        }
    }
}

fn editor(this: WeakEntity<SettingsView>, _window: &mut Window, cx: &mut App) -> AnyElement {
    let Some(view) = this.upgrade() else {
        return div().into_any_element();
    };
    let view = view.read(cx);
    let inputs = &view.game_inputs;
    let (offset_x, offset_y, width, height) = (
        inputs.offset_x.clone(),
        inputs.offset_y.clone(),
        inputs.width.clone(),
        inputs.height.clone(),
    );
    let recording = view.recording;
    let focus = view.capture_focus.clone();
    let game = Store::get(cx).game().clone();
    let custom = !COMMON.contains(&game.resolution);
    // Other programs follow the selected crosshair; only listed games may pick their own.
    let can_pick = Store::get(cx).selected_game.is_some() || game.crosshair.is_some();
    let selected = Store::get(cx).active().name.clone();

    let binds = (game.bindings.iter().enumerate()).map(|(i, binding)| {
        bind_row(
            i,
            binding.clone(),
            recording == Some(Recording::Binding(i)),
            focus.clone(),
            this.clone(),
            cx,
        )
    });
    let has_aim_bind = (game.bindings.iter()).any(|b| b.trigger.key == Key::MOUSE2);
    let add_this = this.clone();

    v_flex()
        .gap_6()
        .child(
            Form::new()
                .when(can_pick, |form| {
                    form.child(crosshair_field(game.crosshair.clone(), selected))
                })
                .child(
                    Field::new()
                        .label("Stretch like")
                        .description("Draws the crosshair the way it looks at this resolution, stretched over your screen. Your game can run at any resolution.")
                        .child(stretch_picker(game.resolution, this.clone())),
                )
                .when(custom, |form| {
                    form.child(
                        Field::new().label("Custom resolution").child(
                            h_flex()
                                .gap_2()
                                .child(NumberInput::new(&width))
                                .child("×")
                                .child(NumberInput::new(&height)),
                        ),
                    )
                })
                .child(
                    Field::new()
                        .label("Offset (screen pixels)")
                        .description("Moves the crosshair if a game's center is off.")
                        .child(
                            h_flex()
                                .gap_2()
                                .child("X")
                                .child(NumberInput::new(&offset_x))
                                .child("Y")
                                .child(NumberInput::new(&offset_y)),
                        ),
                ),
        )
        .child(
            v_flex()
                .gap_2()
                .child(div().font_weight(gpui_kit::FontWeight::SEMIBOLD).child("Binds"))
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("Keys and mouse buttons that hide, show or switch the crosshair while this game is in front."),
                )
                .children(binds)
                .child(
                    h_flex()
                        .gap_2()
                        .when(!has_aim_bind, |row| {
                            row.child(
                                Button::new("bind-aim")
                                    .outline()
                                    .label("Hide while aiming (hold Mouse2)")
                                    .on_click(|_, _, cx| {
                                        Store::edit_game(cx, |g| {
                                            g.bindings.push(Binding {
                                                trigger: Chord::key(Key::MOUSE2),
                                                action: Action::HoldToHide,
                                            });
                                        });
                                    }),
                            )
                        })
                        .child(
                            Button::new("bind-add")
                                .outline()
                                .icon(IconName::Plus)
                                .label("Add bind")
                                .on_click(move |_, window, cx| {
                                    let index = Store::get(cx).game().bindings.len();
                                    Store::edit_game(cx, |g| {
                                        g.bindings.push(Binding {
                                            trigger: Chord::key(Key::MOUSE2),
                                            action: Action::Toggle,
                                        });
                                    });
                                    let _ = add_this.update(cx, |view, cx| {
                                        view.record(Recording::Binding(index), window, cx);
                                    });
                                }),
                        ),
                ),
        )
        .into_any_element()
}

/// One bind as a sentence: "[Hold to hide ▾] [Mouse2]".
fn bind_row(
    index: usize,
    binding: Binding,
    recording: bool,
    focus: gpui_kit::FocusHandle,
    this: WeakEntity<SettingsView>,
    cx: &App,
) -> AnyElement {
    let id = |part: &str| SharedString::from(format!("bind-{part}-{index}-{}", binding.trigger));
    let action = binding.action.clone();
    let switch_to = match &binding.action {
        Action::SetCrosshair(name) => Some(name.clone()),
        _ => None,
    };
    let set = move |action: Action| {
        move |_: &gpui_kit::ClickEvent, _: &mut Window, cx: &mut App| {
            let action = action.clone();
            Store::edit_game(cx, |g| {
                if let Some(b) = g.bindings.get_mut(index) {
                    b.action = action;
                }
            });
        }
    };
    let first_crosshair = crossz_core::presets::all()[0].name.clone();
    h_flex()
        .w_full()
        .gap_2()
        .child(
            Button::new(id("action"))
                .outline()
                .label(verb(&binding.action))
                .min_w(px(170.))
                .dropdown_caret(true)
                .dropdown_menu(move |menu, _, _| {
                    let item = |a: Action| {
                        let selected =
                            std::mem::discriminant(&a) == std::mem::discriminant(&action);
                        PopupMenuItem::new(verb(&a))
                            .checked(selected)
                            .on_click(set(a))
                    };
                    menu.item(item(Action::HoldToHide))
                        .item(item(Action::HoldToShow))
                        .item(item(Action::Toggle))
                        .item(item(Action::SetCrosshair(
                            switch_to.clone().unwrap_or_else(|| first_crosshair.clone()),
                        )))
                }),
        )
        .child(chord_button(
            id("key"),
            Some(binding.trigger),
            recording,
            focus,
            Recording::Binding(index),
            this,
            cx,
        ))
        .when_some(
            match &binding.action {
                Action::SetCrosshair(name) => Some(name.clone()),
                _ => None,
            },
            |row, name| {
                row.child("to").child(
                    Button::new(id("crosshair"))
                        .outline()
                        .label(name.clone())
                        .dropdown_caret(true)
                        .dropdown_menu(move |menu, _, cx| {
                            crosshair_items(menu, &name, cx, move |picked, cx| {
                                Store::edit_game(cx, |g| {
                                    if let Some(b) = g.bindings.get_mut(index) {
                                        b.action = Action::SetCrosshair(picked);
                                    }
                                });
                            })
                        }),
                )
            },
        )
        .child(div().flex_1())
        .child(
            Button::new(id("remove"))
                .ghost()
                .icon(IconName::Trash)
                .tooltip("Remove bind")
                .on_click(move |_, _, cx| {
                    Store::edit_game(cx, |g| {
                        if index < g.bindings.len() {
                            g.bindings.remove(index);
                        }
                    });
                }),
        )
        .into_any_element()
}

fn verb(action: &Action) -> &'static str {
    match action {
        Action::HoldToHide => "Hold to hide",
        Action::HoldToShow => "Hold to show",
        Action::Toggle => "Press to show/hide",
        Action::SetCrosshair(_) => "Press to switch",
    }
}

/// Built-in crosshairs, then yours, as menu items; `pick` gets the chosen name.
fn crosshair_items(
    menu: PopupMenu,
    current: &str,
    cx: &App,
    pick: impl Fn(String, &mut App) + Clone + 'static,
) -> PopupMenu {
    let config = &Store::get(cx).config;
    let custom: Vec<String> = config.crosshairs.iter().map(|c| c.name.clone()).collect();
    let builtin: Vec<String> = (crossz_core::presets::all().iter())
        .map(|c| c.name.clone())
        .collect();
    let item = |name: String| {
        let pick = pick.clone();
        PopupMenuItem::new(name.clone())
            .checked(name == current)
            .on_click(move |_, _, cx| pick(name.clone(), cx))
    };
    let menu = builtin
        .into_iter()
        .fold(menu.label("Built-in"), |m, n| m.item(item(n)));
    if custom.is_empty() {
        return menu;
    }
    custom
        .into_iter()
        .fold(menu.separator().label("Yours"), |m, n| m.item(item(n)))
}

/// Off, the game uses the crosshair selected on the Crosshairs page; on, its own.
fn crosshair_field(own: Option<String>, selected: String) -> Field {
    let description = match &own {
        Some(_) => "This game ignores the crosshair selected on the Crosshairs page.".to_owned(),
        None => format!("Uses the selected crosshair, {selected}."),
    };
    let toggle = Switch::new("game-own-crosshair")
        .label("Own crosshair")
        .checked(own.is_some())
        .on_change(move |on: &bool, _, cx| {
            let own = on.then(|| Store::get(cx).active().name.clone());
            Store::edit_game(cx, |g| g.crosshair = own);
        });
    Field::new()
        .label("Crosshair")
        .description(description)
        .child(
            h_flex()
                .gap_3()
                .child(toggle)
                .when_some(own, |row, current| {
                    row.child(
                        Button::new("game-crosshair")
                            .outline()
                            .label(current.clone())
                            .dropdown_caret(true)
                            .dropdown_menu(move |menu, _, cx| {
                                crosshair_items(menu, &current, cx, |name, cx| {
                                    Store::edit_game(cx, |g| g.crosshair = Some(name.clone()));
                                })
                            }),
                    )
                }),
        )
}

fn stretch_picker(current: Resolution, this: WeakEntity<SettingsView>) -> AnyElement {
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
    let custom = !COMMON.contains(&current);
    let text = if custom {
        format!("Custom: {current}")
    } else {
        label(current)
    };
    Button::new("game-stretch")
        .outline()
        .label(text)
        .dropdown_caret(true)
        .dropdown_menu(move |menu, _, _| {
            let this = this.clone();
            let menu = COMMON.iter().fold(menu, |menu, &r| {
                menu.item(
                    PopupMenuItem::new(label(r))
                        .checked(r == current)
                        .on_click(move |_, _, cx| Store::edit_game(cx, |g| g.resolution = r)),
                )
            });
            menu.separator().item(
                PopupMenuItem::new("Custom…")
                    .checked(custom)
                    .disabled(custom)
                    .on_click(move |_, window, cx| {
                        Store::edit_game(cx, |g| g.resolution = Resolution::custom(1600, 1200));
                        let _ = this.update(cx, |view, cx| view.load_game_inputs(window, cx));
                    }),
            )
        })
        .into_any_element()
}

fn show_key(show: Show) -> &'static str {
    match show {
        Show::InGames => "in_games",
        Show::ExceptListed => "except_listed",
        Show::Everywhere => "everywhere",
    }
}
