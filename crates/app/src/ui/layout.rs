//! Page building blocks: titled sections of labeled rows, in the look of a settings form.

use gpui_kit::component::{ActiveTheme as _, group_box::GroupBox, h_flex, label::Label, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, IntoElement, ParentElement, RenderOnce, SharedString, Styled as _, Window, div,
};

/// A group of rows under an optional title and description.
#[derive(IntoElement, Default)]
pub struct Section {
    title: Option<SharedString>,
    description: Option<SharedString>,
    children: Vec<AnyElement>,
}

impl Section {
    pub fn new() -> Self {
        Section::default()
    }

    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }
}

impl ParentElement for Section {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Section {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        GroupBox::new()
            .when_some(self.title, |group, title| {
                group.title(v_flex().gap_1().child(title).when_some(
                    self.description,
                    |this, description| {
                        this.child(Label::new(description).text_sm().text_color(muted))
                    },
                ))
            })
            .gap_4()
            .children(self.children)
    }
}

/// A setting: its name and what it does on the left, its control on the right.
pub fn row(
    title: impl Into<SharedString>,
    description: Option<SharedString>,
    control: impl IntoElement,
    cx: &App,
) -> AnyElement {
    h_flex()
        .w_full()
        .justify_between()
        .items_center()
        .gap_3()
        .child(
            v_flex()
                .flex_1()
                .max_w_3_5()
                .child(Label::new(title.into()).text_sm())
                .when_some(description, |this, description| {
                    this.child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(description),
                    )
                }),
        )
        .child(control)
        .into_any_element()
}

/// Muted explanatory text, e.g. for an empty list.
pub fn note(text: impl Into<SharedString>, cx: &App) -> AnyElement {
    div()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
        .into_any_element()
}
