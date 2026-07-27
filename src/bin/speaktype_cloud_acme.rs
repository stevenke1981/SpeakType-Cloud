#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use acme_ui::{ActiveTheme, Badge, Button, Card, Progress, Separator, StyledExt};
use gpui::{
    AppContext as _, Context, IntoElement, ParentElement as _, Render, Styled as _, Window,
    WindowOptions, div, px,
};

struct SpeakTypeAcmeApp {
    recording: bool,
    progress: f32,
}

impl SpeakTypeAcmeApp {
    fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            recording: false,
            progress: 0.0,
        }
    }
}

impl Render for SpeakTypeAcmeApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors;

        let status = if self.recording {
            Badge::new("錄音中").danger()
        } else {
            Badge::new("待命").primary()
        };

        let header = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(20.0))
                            .text_color(colors.foreground)
                            .child("SpeakType Cloud"),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(colors.muted_foreground)
                            .child("GPUI + AcmeUIKit frontend foundation"),
                    ),
            )
            .child(status);

        let recorder = Card::new()
            .title("語音輸入")
            .description("按住 Ctrl+Shift+Space 開始錄音，放開後辨識並貼入原視窗。")
            .child(
                div()
                    .v_flex()
                    .gap_3()
                    .child(Progress::new(self.progress))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(Button::new("record").primary().label("開始錄音"))
                            .child(Button::new("settings").secondary().label("設定"))
                            .child(Button::new("history").ghost().label("歷史紀錄")),
                    ),
            );

        let provider = Card::new()
            .title("雲端辨識")
            .description("保留既有 OpenAI、xAI 與 OpenRouter provider 核心，逐步接上新的 GPUI 狀態層。")
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(Badge::new("OpenAI").primary())
                    .child(Badge::new("xAI").warning())
                    .child(Badge::new("OpenRouter")),
            );

        div()
            .size_full()
            .bg(colors.background)
            .text_color(colors.foreground)
            .p_6()
            .v_flex()
            .gap_4()
            .child(header)
            .child(Separator::new())
            .child(recorder)
            .child(provider)
    }
}

fn main() {
    gpui_platform::application().run(move |cx| {
        acme_ui::init(cx);

        cx.spawn(async move |cx| {
            if let Err(error) = cx.open_window(WindowOptions::default(), |_, cx| {
                cx.new(SpeakTypeAcmeApp::new)
            }) {
                eprintln!("failed to open SpeakType Cloud Acme window: {error:?}");
            }
        })
        .detach();
    });
}
