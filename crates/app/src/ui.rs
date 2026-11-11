use std::sync::Arc;

use crossz_core::{
    Action, Arms, Binding, Chord, Circle, Color, Dot, Key, Outline, Profile, ProgramMode,
    ProgramRule, ScaleMode, Snap, scale::PRESETS,
};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _,
    alert::Alert,
    button::{Button, ButtonVariants as _},
    color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState},
    h_flex,
    input::{Input, InputState},
    menu::{DropdownMenu as _, PopupMenuItem},
    setting::{NumberFieldOptions, SettingField, SettingGroup, SettingItem, SettingPage, Settings},
    switch::Switch,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Context, Entity, FocusHandle, Global, Hsla, ImageSource,
    InteractiveElement as _, IntoElement, KeyDownEvent, MouseDownEvent, ParentElement as _, Render,
    RenderImage, Rgba, SharedString, Styled as _, Subscription, TitlebarOptions, WeakEntity,
    Window, WindowBounds, WindowOptions, div, img, px, size,
};

use crate::capture;
use crate::preview;
use crate::store::{self, Store};

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
        window_bounds: Some(WindowBounds::centered(size(px(920.), px(700.)), cx)),
        window_min_size: Some(size(px(720.), px(520.))),
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

#[derive(Clone, Copy, PartialEq)]
enum Recording {
    Binding(usize),
    MasterToggle,
}

struct Preview {
    profile: Profile,
    resolution: (u32, u32),
    image: Arc<RenderImage>,
    zoom: u32,
}

pub struct SettingsView {
    color: Entity<ColorPickerState>,
    outline_color: Entity<ColorPickerState>,
    new_program: Entity<InputState>,
    capture_focus: FocusHandle,
    recording: Option<Recording>,
    preview: Option<Preview>,
    _subscriptions: Vec<Subscription>,
}

impl SettingsView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let color = cx.new(|cx| ColorPickerState::new(window, cx));
        let outline_color = cx.new(|cx| ColorPickerState::new(window, cx));
        let new_program = cx.new(|cx| InputState::new(window, cx).placeholder("game.exe"));
        let subscriptions = vec![
            cx.subscribe(&color, |_, _, event: &ColorPickerEvent, cx| {
                if let ColorPickerEvent::Change(Some(hsla)) = event {
                    let color = from_hsla(*hsla);
                    Store::edit_profile(cx, |p| p.crosshair.color = color);
                }
            }),
            cx.subscribe(&outline_color, |_, _, event: &ColorPickerEvent, cx| {
                if let ColorPickerEvent::Change(Some(hsla)) = event {
                    let color = from_hsla(*hsla);
                    Store::edit_profile(cx, |p| {
                        p.crosshair.outline.get_or_insert(DEFAULT_OUTLINE).color = color;
                    });
                }
            }),
        ];
        SettingsView {
            color,
            outline_color,
            new_program,
            capture_focus: cx.focus_handle(),
            recording: None,
            preview: None,
            _subscriptions: subscriptions,
        }
    }

    /// Keeps the color pickers showing the edited profile's colors.
    fn sync_colors(&self, window: &mut Window, cx: &mut Context<Self>) {
        let crosshair = &Store::profile(cx).crosshair;
        let fill = to_hsla(crosshair.color);
        let outline = to_hsla(crosshair.outline.unwrap_or(DEFAULT_OUTLINE).color);
        for (picker, wanted) in [(&self.color, fill), (&self.outline_color, outline)] {
            if picker.read(cx).value() != Some(wanted) {
                picker.update(cx, |state, cx| state.set_value(wanted, window, cx));
            }
        }
    }

    fn preview(&mut self, window: &mut Window, cx: &mut Context<Self>) -> (Arc<RenderImage>, u32) {
        let profile = Store::profile(cx).clone();
        let resolution = Store::get(cx).preview_resolution;
        let fresh = (self.preview.as_ref())
            .is_some_and(|p| p.profile == profile && p.resolution == resolution);
        if !fresh {
            let (image, zoom) = preview::image(&profile, resolution);
            if let Some(old) = self.preview.take() {
                cx.drop_image(old.image, Some(window));
            }
            self.preview = Some(Preview {
                profile,
                resolution,
                image,
                zoom,
            });
        }
        let preview = self.preview.as_ref().expect("set above");
        (preview.image.clone(), preview.zoom)
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
            Store::edit(cx, |config| match target {
                Recording::Binding(i) => {
                    if let Some(binding) = config.bindings.get_mut(i) {
                        binding.trigger = chord;
                    }
                }
                Recording::MasterToggle => config.general.master_toggle = Some(chord),
            });
        }
        cx.notify();
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_colors(window, cx);
        let (preview, zoom) = self.preview(window, cx);
        let view = cx.entity().downgrade();
        let error = Store::get(cx).error.clone();

        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .when_some(error, |this, error| {
                this.child(Alert::error("config-error", error).banner())
            })
            .child(Settings::new("crossz-settings").pages([
                crosshair_page(self, preview, zoom, cx),
                programs_page(self, view.clone(), cx),
                bindings_page(self, view, cx),
                general_page(self, cx.entity().downgrade(), cx),
            ]))
    }
}

const DEFAULT_ARMS: Arms = Arms {
    length: 6.0,
    thickness: 2.0,
    gap: 2.0,
    t_style: false,
};
const DEFAULT_DOT: Dot = Dot { size: 2.0 };
const DEFAULT_CIRCLE: Circle = Circle {
    radius: 8.0,
    thickness: 1.0,
};
const DEFAULT_OUTLINE: Outline = Outline {
    thickness: 1.0,
    color: Color::BLACK,
};

fn crosshair_page(
    view: &SettingsView,
    preview: Arc<RenderImage>,
    zoom: u32,
    cx: &App,
) -> SettingPage {
    let store = Store::get(cx);
    let profile = Store::profile(cx);
    let crosshair = &profile.crosshair;
    let profile_names: Vec<_> = store
        .config
        .profiles
        .iter()
        .map(|p| p.name.clone())
        .collect();
    let only_profile = profile_names.len() == 1;
    let (arms, dot, circle, outline) = (
        crosshair.arms.is_some(),
        crosshair.dot.is_some(),
        crosshair.circle.is_some(),
        crosshair.outline.is_some(),
    );
    let scaled = profile.scale != ScaleMode::Native;
    let stretched = matches!(profile.scale, ScaleMode::Stretched { .. });

    let profile_group = SettingGroup::new().title("Profile").items([
        SettingItem::new(
            "Editing",
            SettingField::dropdown(
                options(profile_names.iter().map(|n| (n.as_str(), n.as_str()))),
                |cx| Store::profile(cx).name.clone().into(),
                |name, cx| {
                    let store = cx.global_mut::<Store>();
                    if let Some(i) = store.config.profiles.iter().position(|p| p.name == *name) {
                        store.editing = i;
                    }
                    cx.refresh_windows();
                },
            ),
        ),
        SettingItem::new(
            "Name",
            SettingField::input(
                |cx| Store::profile(cx).name.clone().into(),
                |name, cx| {
                    let name = name.trim();
                    let old = Store::profile(cx).name.clone();
                    if !name.is_empty() && Store::get(cx).config.profile(name).is_none() {
                        Store::edit(cx, |config| store::rename_profile(config, &old, name));
                    }
                },
            ),
        ),
        SettingItem::render(move |_, _, _| {
            h_flex()
                .w_full()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("profile-new")
                        .small()
                        .icon(IconName::Plus)
                        .label("New")
                        .on_click(|_, _, cx| add_profile(cx, None)),
                )
                .child(
                    Button::new("profile-duplicate")
                        .small()
                        .label("Duplicate")
                        .on_click(|_, _, cx| add_profile(cx, Some(Store::profile(cx).clone()))),
                )
                .child(
                    Button::new("profile-delete")
                        .small()
                        .danger()
                        .label("Delete")
                        .disabled(only_profile)
                        .on_click(|_, _, cx| {
                            let editing = Store::get(cx).editing;
                            Store::edit(cx, |config| store::remove_profile(config, editing));
                            let store = cx.global_mut::<Store>();
                            store.editing = editing.saturating_sub(1);
                        }),
                )
        }),
    ]);

    let preview_group = SettingGroup::new().title("Preview").items([
        SettingItem::render(move |_, window, _| {
            let side = px(preview::SIZE as f32 / window.scale_factor());
            h_flex()
                .w_full()
                .justify_center()
                .child(img(ImageSource::Render(preview.clone())).size(side))
        }),
        SettingItem::new(
            "Screen resolution",
            SettingField::dropdown(
                options(
                    ["1920x1080", "2560x1440", "3440x1440", "3840x2160", "1280x960"]
                        .map(|r| (r, r)),
                ),
                |cx| resolution_label(Store::get(cx).preview_resolution).into(),
                |value, cx| {
                    if let Some(resolution) = parse_resolution(&value) {
                        cx.global_mut::<Store>().preview_resolution = resolution;
                        cx.refresh_windows();
                    }
                },
            ),
        )
        .description(format!(
            "Shown {zoom}× larger, one square per screen pixel. Top: dark background, bottom: light."
        )),
    ]);

    let scale_group = SettingGroup::new().title("Scaling").items([
        SettingItem::new(
            "Mode",
            SettingField::dropdown(
                options([
                    ("native", "Native"),
                    ("stretched", "Stretched"),
                    ("uniform", "Black bars"),
                ]),
                |cx| scale_mode_key(Store::profile(cx).scale).into(),
                |key, cx| {
                    Store::edit_profile(cx, |p| {
                        let (w, h) = virtual_resolution(p.scale);
                        p.scale = match key.as_ref() {
                            "stretched" => ScaleMode::Stretched {
                                width: w,
                                height: h,
                            },
                            "uniform" => ScaleMode::Uniform { height: h },
                            _ => ScaleMode::Native,
                        };
                    });
                },
            ),
        )
        .description("Stretched draws the crosshair as a stretched 4:3 game would. Black bars scales it evenly by height."),
        SettingItem::new(
            "Game resolution",
            SettingField::dropdown(
                options(
                    PRESETS
                        .iter()
                        .map(|&r| resolution_label(r))
                        .chain(["Custom".to_owned()])
                        .map(|r| (r.clone(), r)),
                ),
                |cx| {
                    let r = virtual_resolution(Store::profile(cx).scale);
                    if PRESETS.contains(&r) {
                        resolution_label(r).into()
                    } else {
                        "Custom".into()
                    }
                },
                |value, cx| {
                    if let Some((w, h)) = parse_resolution(&value) {
                        Store::edit_profile(cx, |p| set_virtual_resolution(p, w, h));
                    }
                },
            ),
        )
        .disabled(!scaled),
        number(
            "Width",
            (320.0, 7680.0, 1.0),
            |p| virtual_resolution(p.scale).0 as f32,
            |p, v| {
                let h = virtual_resolution(p.scale).1;
                set_virtual_resolution(p, v as u32, h);
            },
        )
        .disabled(!stretched),
        number(
            "Height",
            (240.0, 4320.0, 1.0),
            |p| virtual_resolution(p.scale).1 as f32,
            |p, v| {
                let w = virtual_resolution(p.scale).0;
                set_virtual_resolution(p, w, v as u32);
            },
        )
        .disabled(!scaled),
        SettingItem::new(
            "Pixel snapping",
            SettingField::dropdown(
                options([
                    ("round", "Round"),
                    ("floor", "Thinner"),
                    ("ceil", "Thicker"),
                    ("anti_alias", "Smooth (anti-aliased)"),
                ]),
                |cx| snap_key(Store::profile(cx).snap).into(),
                |key, cx| {
                    let snap = match key.as_ref() {
                        "floor" => Snap::Floor,
                        "ceil" => Snap::Ceil,
                        "anti_alias" => Snap::AntiAlias,
                        _ => Snap::Round,
                    };
                    Store::edit_profile(cx, |p| p.snap = snap);
                },
            ),
        )
        .description("How sizes that land between pixels, like 1.5px, are drawn."),
        number(
            "Offset X",
            (-50.0, 50.0, 1.0),
            |p| p.offset[0] as f32,
            |p, v| p.offset[0] = v as i32,
        ),
        number(
            "Offset Y",
            (-50.0, 50.0, 1.0),
            |p| p.offset[1] as f32,
            |p, v| p.offset[1] = v as i32,
        ),
    ]);

    let color = view.color.clone();
    let outline_color = view.outline_color.clone();
    let shape_group = SettingGroup::new()
        .title("Shape")
        .description("Sizes are in game pixels, before scaling.")
        .items([
            SettingItem::new(
                "Color",
                SettingField::render(move |_, _, _| ColorPicker::new(&color).small()),
            ),
            number(
                "Opacity %",
                (0.0, 100.0, 5.0),
                |p| p.crosshair.opacity * 100.0,
                |p, v| p.crosshair.opacity = v / 100.0,
            ),
            toggle(
                "Lines",
                |p| p.crosshair.arms.is_some(),
                |p, on| p.crosshair.arms = on.then_some(DEFAULT_ARMS),
            ),
            number(
                "Line length",
                (0.0, 100.0, 1.0),
                |p| arms_of(p).length,
                |p, v| arms_mut(p).length = v,
            )
            .disabled(!arms),
            number(
                "Line thickness",
                (0.5, 20.0, 0.5),
                |p| arms_of(p).thickness,
                |p, v| arms_mut(p).thickness = v,
            )
            .disabled(!arms),
            number(
                "Gap",
                (-10.0, 50.0, 1.0),
                |p| arms_of(p).gap,
                |p, v| arms_mut(p).gap = v,
            )
            .disabled(!arms),
            SettingItem::new(
                "T-style",
                SettingField::checkbox(
                    |cx| arms_of(Store::profile(cx)).t_style,
                    |on, cx| Store::edit_profile(cx, |p| arms_mut(p).t_style = on),
                ),
            )
            .disabled(!arms),
            toggle(
                "Center dot",
                |p| p.crosshair.dot.is_some(),
                |p, on| p.crosshair.dot = on.then_some(DEFAULT_DOT),
            ),
            number(
                "Dot size",
                (0.5, 20.0, 0.5),
                |p| p.crosshair.dot.unwrap_or(DEFAULT_DOT).size,
                |p, v| p.crosshair.dot.get_or_insert(DEFAULT_DOT).size = v,
            )
            .disabled(!dot),
            toggle(
                "Circle",
                |p| p.crosshair.circle.is_some(),
                |p, on| p.crosshair.circle = on.then_some(DEFAULT_CIRCLE),
            ),
            number(
                "Circle radius",
                (1.0, 100.0, 1.0),
                |p| p.crosshair.circle.unwrap_or(DEFAULT_CIRCLE).radius,
                |p, v| p.crosshair.circle.get_or_insert(DEFAULT_CIRCLE).radius = v,
            )
            .disabled(!circle),
            number(
                "Circle thickness",
                (0.5, 20.0, 0.5),
                |p| p.crosshair.circle.unwrap_or(DEFAULT_CIRCLE).thickness,
                |p, v| p.crosshair.circle.get_or_insert(DEFAULT_CIRCLE).thickness = v,
            )
            .disabled(!circle),
            toggle(
                "Outline",
                |p| p.crosshair.outline.is_some(),
                |p, on| p.crosshair.outline = on.then_some(DEFAULT_OUTLINE),
            ),
            number(
                "Outline thickness",
                (0.5, 10.0, 0.5),
                |p| p.crosshair.outline.unwrap_or(DEFAULT_OUTLINE).thickness,
                |p, v| p.crosshair.outline.get_or_insert(DEFAULT_OUTLINE).thickness = v,
            )
            .disabled(!outline),
            SettingItem::new(
                "Outline color",
                SettingField::render(move |_, _, _| ColorPicker::new(&outline_color).small()),
            )
            .disabled(!outline),
        ]);

    SettingPage::new("Crosshair")
        .icon(IconName::Crosshair)
        .default_open(true)
        .groups([profile_group, preview_group, scale_group, shape_group])
}

fn programs_page(view: &SettingsView, this: WeakEntity<SettingsView>, cx: &App) -> SettingPage {
    let config = &Store::get(cx).config;
    let profile_names: Vec<String> = config.profiles.iter().map(|p| p.name.clone()).collect();

    let general = SettingGroup::new().items([
        SettingItem::new(
            "Show the crosshair",
            SettingField::dropdown(
                options([
                    ("whitelist", "Only in listed programs"),
                    ("blacklist", "Everywhere except listed programs"),
                    ("always", "Everywhere"),
                ]),
                |cx| mode_key(Store::get(cx).config.programs.mode).into(),
                |key, cx| {
                    let mode = match key.as_ref() {
                        "blacklist" => ProgramMode::Blacklist,
                        "always" => ProgramMode::Always,
                        _ => ProgramMode::Whitelist,
                    };
                    Store::edit(cx, |c| c.programs.mode = mode);
                },
            ),
        )
        .description("Crossz follows the window of the program in the foreground."),
        SettingItem::new(
            "Default profile",
            SettingField::dropdown(
                options(profile_names.iter().map(|n| (n.as_str(), n.as_str()))),
                |cx| {
                    let config = &Store::get(cx).config;
                    let name = config.default_profile().map(|p| p.name.clone());
                    name.unwrap_or_default().into()
                },
                |name, cx| Store::edit(cx, |c| c.programs.default_profile = name.to_string()),
            ),
        )
        .description("Used where a program has no profile of its own."),
    ]);

    let rules: Vec<SettingItem> = config
        .programs
        .rules
        .iter()
        .map(|rule| rule_row(rule.clone(), profile_names.clone()))
        .collect();
    let rules = SettingGroup::new()
        .title("Programs")
        .items(rules)
        .item(add_program_row(view.new_program.clone(), this));

    SettingPage::new("Programs")
        .icon(IconName::Gamepad2)
        .groups([general, rules])
}

fn rule_row(rule: ProgramRule, profiles: Vec<String>) -> SettingItem {
    SettingItem::render(move |_, _, _| {
        let exe = rule.exe.clone();
        let id = |part: &str| SharedString::from(format!("rule-{part}-{exe}"));
        let current = rule
            .profile
            .clone()
            .unwrap_or_else(|| "Default profile".into());
        let profile_menu = {
            let exe = exe.clone();
            let profiles = profiles.clone();
            let selected = rule.profile.clone();
            move |menu: gpui_kit::component::menu::PopupMenu, _: &mut Window, _: &mut Context<_>| {
                let pick = |name: Option<String>| {
                    let exe = exe.clone();
                    move |_: &_, _: &mut Window, cx: &mut App| {
                        let name = name.clone();
                        Store::edit(cx, |c| {
                            if let Some(rule) = c.programs.rules.iter_mut().find(|r| r.exe == exe) {
                                rule.profile = name;
                            }
                        });
                    }
                };
                let menu = menu.item(
                    PopupMenuItem::new("Default profile")
                        .checked(selected.is_none())
                        .on_click(pick(None)),
                );
                profiles.iter().fold(menu, |menu, name| {
                    menu.item(
                        PopupMenuItem::new(name.clone())
                            .checked(selected.as_ref() == Some(name))
                            .on_click(pick(Some(name.clone()))),
                    )
                })
            }
        };
        let toggle_exe = exe.clone();
        let remove_exe = exe.clone();
        h_flex()
            .w_full()
            .gap_3()
            .child(div().flex_1().child(exe.clone()))
            .child(
                Button::new(id("profile"))
                    .small()
                    .outline()
                    .label(current)
                    .dropdown_caret(true)
                    .dropdown_menu(profile_menu),
            )
            .child(Switch::new(id("enabled")).checked(rule.enabled).on_change(
                move |on: &bool, _, cx| {
                    let on = *on;
                    Store::edit(cx, |c| {
                        if let Some(rule) =
                            c.programs.rules.iter_mut().find(|r| r.exe == toggle_exe)
                        {
                            rule.enabled = on;
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
                        Store::edit(cx, |c| c.programs.rules.retain(|r| r.exe != remove_exe));
                    }),
            )
    })
}

fn add_program_row(input: Entity<InputState>, _view: WeakEntity<SettingsView>) -> SettingItem {
    SettingItem::render(move |_, _, _| {
        let add_input = input.clone();
        h_flex()
            .w_full()
            .gap_2()
            .child(div().flex_1().child(Input::new(&input).small()))
            .child(
                Button::new("program-add")
                    .small()
                    .icon(IconName::Plus)
                    .label("Add")
                    .on_click(move |_, window, cx| {
                        let exe = add_input.read(cx).value().trim().to_owned();
                        add_program(exe, cx);
                        add_input.update(cx, |state, cx| state.set_value("", window, cx));
                    }),
            )
            .child(
                Button::new("program-running")
                    .small()
                    .outline()
                    .label("Running programs")
                    .dropdown_caret(true)
                    .dropdown_menu(|menu, _, _| {
                        let programs = crate::overlay::running_programs();
                        if programs.is_empty() {
                            return menu.item(PopupMenuItem::new("None found").disabled(true));
                        }
                        programs.into_iter().fold(menu, |menu, exe| {
                            menu.item(
                                PopupMenuItem::new(exe.clone())
                                    .on_click(move |_, _, cx| add_program(exe.clone(), cx)),
                            )
                        })
                    }),
            )
    })
}

fn add_program(exe: String, cx: &mut App) {
    if exe.is_empty() {
        return;
    }
    Store::edit(cx, |c| {
        if !c
            .programs
            .rules
            .iter()
            .any(|r| r.exe.eq_ignore_ascii_case(&exe))
        {
            c.programs.rules.push(ProgramRule {
                exe,
                profile: None,
                enabled: true,
            });
        }
    });
}

fn bindings_page(view: &SettingsView, this: WeakEntity<SettingsView>, cx: &App) -> SettingPage {
    let config = &Store::get(cx).config;
    let profile_names: Vec<String> = config.profiles.iter().map(|p| p.name.clone()).collect();
    let rows: Vec<SettingItem> = (config.bindings.iter().enumerate())
        .map(|(i, binding)| {
            binding_row(
                i,
                binding.clone(),
                view.recording == Some(Recording::Binding(i)),
                view.capture_focus.clone(),
                profile_names.clone(),
                this.clone(),
            )
        })
        .collect();
    let add = SettingItem::render(move |_, _, _| {
        let this = this.clone();
        h_flex().w_full().justify_end().child(
            Button::new("binding-add")
                .small()
                .icon(IconName::Plus)
                .label("Add binding")
                .on_click(move |_, window, cx| {
                    let index = Store::get(cx).config.bindings.len();
                    Store::edit(cx, |c| {
                        c.bindings.push(Binding {
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

    SettingPage::new("Bindings").icon(IconName::Keyboard).group(
        SettingGroup::new()
            .description(
                "Bindings work while a matched program is in the foreground. Hold bindings act while the key or button is down.",
            )
            .items(rows)
            .item(add),
    )
}

fn binding_row(
    index: usize,
    binding: Binding,
    recording: bool,
    focus: FocusHandle,
    profiles: Vec<String>,
    this: WeakEntity<SettingsView>,
) -> SettingItem {
    SettingItem::render(move |_, _, cx| {
        let id = |part: &str| SharedString::from(format!("binding-{part}-{index}"));
        let action = binding.action.clone();
        let action_menu = {
            let profiles = profiles.clone();
            move |menu: gpui_kit::component::menu::PopupMenu, _: &mut Window, _: &mut Context<_>| {
                let set = |action: Action| {
                    move |_: &_, _: &mut Window, cx: &mut App| {
                        let action = action.clone();
                        Store::edit(cx, |c| {
                            if let Some(b) = c.bindings.get_mut(index) {
                                b.action = action;
                            }
                        });
                    }
                };
                let simple = [
                    Action::Toggle,
                    Action::HoldToHide,
                    Action::HoldToShow,
                    Action::CycleProfile,
                ];
                let menu = simple.into_iter().fold(menu, |menu, a| {
                    menu.item(
                        PopupMenuItem::new(action_label(&a))
                            .checked(action == a)
                            .on_click(set(a.clone())),
                    )
                });
                profiles.iter().fold(menu.separator(), |menu, name| {
                    let a = Action::SetProfile(name.clone());
                    menu.item(
                        PopupMenuItem::new(action_label(&a))
                            .checked(action == a)
                            .on_click(set(a.clone())),
                    )
                })
            }
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
                        Store::edit(cx, |c| {
                            if index < c.bindings.len() {
                                c.bindings.remove(index);
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
) -> impl IntoElement {
    if !recording {
        let label = chord.map_or("Not set".to_owned(), |c| c.to_string());
        return div().child(
            Button::new(id)
                .small()
                .outline()
                .icon(IconName::Pencil)
                .label(label)
                .on_click(move |_, window, cx| {
                    let _ = this.update(cx, |view, cx| view.record(target, window, cx));
                }),
        );
    }
    let on_key = this.clone();
    let on_mouse = this;
    div().child(
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
            .child("Press a key or click here with a mouse button… (Esc cancels)")
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
}

fn general_page(view: &SettingsView, this: WeakEntity<SettingsView>, cx: &App) -> SettingPage {
    let master = Store::get(cx).config.general.master_toggle;
    let recording = view.recording == Some(Recording::MasterToggle);
    let focus = view.capture_focus.clone();
    let path = Store::get(cx).path.clone();
    let path_label = path.display().to_string();

    SettingPage::new("General").icon(IconName::Settings).group(
        SettingGroup::new().items([
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
        ]),
    )
}

fn add_profile(cx: &mut App, from: Option<Profile>) {
    let mut profile = from.unwrap_or_else(|| Profile {
        name: String::new(),
        scale: ScaleMode::Native,
        snap: Snap::Round,
        offset: [0, 0],
        crosshair: Default::default(),
    });
    let base = if profile.name.is_empty() {
        "New profile".to_owned()
    } else {
        format!("{} copy", profile.name)
    };
    profile.name = store::unique_name(&Store::get(cx).config, &base);
    Store::edit(cx, |c| c.profiles.push(profile));
    let store = cx.global_mut::<Store>();
    store.editing = store.config.profiles.len() - 1;
}

fn number(
    title: &'static str,
    (min, max, step): (f64, f64, f64),
    get: fn(&Profile) -> f32,
    set: fn(&mut Profile, f32),
) -> SettingItem {
    SettingItem::new(
        title,
        SettingField::number_input(
            NumberFieldOptions { min, max, step },
            move |cx| (get(Store::profile(cx)) as f64 * 100.0).round() / 100.0,
            move |value, cx| Store::edit_profile(cx, |p| set(p, value as f32)),
        ),
    )
}

fn toggle(
    title: &'static str,
    get: fn(&Profile) -> bool,
    set: fn(&mut Profile, bool),
) -> SettingItem {
    SettingItem::new(
        title,
        SettingField::switch(
            move |cx| get(Store::profile(cx)),
            move |on, cx| Store::edit_profile(cx, |p| set(p, on)),
        ),
    )
}

fn arms_of(profile: &Profile) -> Arms {
    profile.crosshair.arms.unwrap_or(DEFAULT_ARMS)
}

fn arms_mut(profile: &mut Profile) -> &mut Arms {
    profile.crosshair.arms.get_or_insert(DEFAULT_ARMS)
}

fn options<K: Into<SharedString>, L: Into<SharedString>>(
    items: impl IntoIterator<Item = (K, L)>,
) -> Vec<(SharedString, SharedString)> {
    items
        .into_iter()
        .map(|(k, l)| (k.into(), l.into()))
        .collect()
}

fn virtual_resolution(mode: ScaleMode) -> (u32, u32) {
    match mode {
        ScaleMode::Stretched { width, height } => (width, height),
        ScaleMode::Uniform { height } => (height * 4 / 3, height),
        ScaleMode::Native => (1280, 960),
    }
}

fn set_virtual_resolution(profile: &mut Profile, width: u32, height: u32) {
    profile.scale = match profile.scale {
        ScaleMode::Stretched { .. } => ScaleMode::Stretched { width, height },
        ScaleMode::Uniform { .. } => ScaleMode::Uniform { height },
        ScaleMode::Native => ScaleMode::Native,
    };
}

fn resolution_label((w, h): (u32, u32)) -> String {
    format!("{w}x{h}")
}

fn parse_resolution(label: &str) -> Option<(u32, u32)> {
    let (w, h) = label.split_once('x')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

fn scale_mode_key(mode: ScaleMode) -> &'static str {
    match mode {
        ScaleMode::Native => "native",
        ScaleMode::Stretched { .. } => "stretched",
        ScaleMode::Uniform { .. } => "uniform",
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

fn mode_key(mode: ProgramMode) -> &'static str {
    match mode {
        ProgramMode::Whitelist => "whitelist",
        ProgramMode::Blacklist => "blacklist",
        ProgramMode::Always => "always",
    }
}

fn action_label(action: &Action) -> String {
    match action {
        Action::Toggle => "Toggle".into(),
        Action::HoldToHide => "Hide while held".into(),
        Action::HoldToShow => "Show only while held".into(),
        Action::CycleProfile => "Next profile".into(),
        Action::SetProfile(name) => format!("Switch to {name}"),
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
