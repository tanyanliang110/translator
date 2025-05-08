use eframe::{App, CreationContext};

use log::warn;
use std::{
   sync::{mpsc, Arc, Mutex}, thread::{self, sleep}, time::Duration
};
use deepl::DeepLApi;
use crate::{
    cfg::{ get_window_size, init_config},
    hotkey::{ctrl_c, HotkeySetting},
    mouse::MouseState,
    ui::{self, get_icon_data, State, LINK_COLOR_COMMON, LINK_COLOR_DOING},
};

pub fn setup_ui_task(cc: &CreationContext) -> Box<dyn App> {
    let ctx = cc.egui_ctx.clone();
    let (task_tx, task_rx) = mpsc::sync_channel(1);

    let state = Arc::new(Mutex::new(State {
        text: "请选中需要翻译的文字触发划词翻译".to_string(),
        source_lang: deepl::Lang::EN,
        target_lang: deepl::Lang::ZH,
        link_color: LINK_COLOR_COMMON,
    }));

    // 监听鼠标动作
    {
        let state = state.clone();
        let mouse_state = Arc::new(Mutex::new(MouseState::new()));

        {
            let mouse_state = mouse_state.clone();
            thread::spawn(move || {
                if let Err(err) = rdev::listen(move |event| {
                    match event.event_type {
                        rdev::EventType::ButtonPress(button) => {
                            if button == rdev::Button::Left {
                                mouse_state.lock().unwrap().down();
                            }
                        }
                        rdev::EventType::ButtonRelease(button) => {
                            if button == rdev::Button::Left {
                                mouse_state.lock().unwrap().release()
                            }
                        }
                        rdev::EventType::MouseMove { x: _, y: _ } => {
                            mouse_state.lock().unwrap().moving()
                        }
                        _ => {}
                    };
                }) {
                    warn!("rdev listen error: {:?}", err)
                }
            });
        }

        {
            thread::spawn(move || {
                let mut clipboard_last = "".to_string();
                loop {
                    if mouse_state.lock().unwrap().is_select() && !ctx.input().pointer.has_pointer()
                    {
                        if let Some(text_new) = ctrl_c() {
                            if text_new != clipboard_last {
                                clipboard_last = text_new.clone();
                                // 新翻译任务 UI
                                {
                                    let mut state = state.lock().unwrap();
                                    state.text = text_new.clone();
                                    state.link_color = LINK_COLOR_DOING;
                                }
                                // 开始翻译
                                let result = {
                                    let target_lang = {
                                        let state = state.lock().unwrap();
                                        state.target_lang.clone()
                                    };
                                    {
                                        let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
                                        rt.block_on(async {
                                            let api = DeepLApi::with("请替换为你的deepl API key").new();
                                            match api.translate_text(&text_new, target_lang).await {
                                                Ok(resp) => resp.translations[0].text.clone(),
                                                Err(err) => {
                                                    log::error!("DeepL API error: {:?}", err);
                                                    "翻译接口失效，请更换".to_string()
                                                }
                                            }
                                        })
                                    }
                                };

                                // 翻译结束 UI
                                {
                                    let mut state = state.lock().unwrap();
                                    state.text = result;
                                    state.link_color = LINK_COLOR_COMMON;
                                }
                            }
                        }
                    }
                    sleep(Duration::from_millis(100));
                }
            });
        }
    }

    // 监听翻译按钮触发
    {
        let state = state.clone();
        thread::spawn(move || {
            loop {
                task_rx.recv().ok();
                {
                    // 新翻译任务 UI
                    {
                        let mut state = state.lock().unwrap();
                        state.link_color = LINK_COLOR_DOING;
                    }

                    // 开始翻译
                    let result = {
                        let (text,target_lang) = {
                            let state = state.lock().unwrap();
                            (state.text.clone(),  deepl::Lang::ZH)
                        };
                        {
                            let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
                            rt.block_on(async {
                                let api = DeepLApi::with("请替换为你的deepl API key").new();
                                let translated = api.translate_text(&text, target_lang).await;
                                translated.map(|resp| resp.translations[0].text.clone()).unwrap_or("翻译接口失效，请更换".to_string())
                            })
                        }
                    };

                    // 翻译结束 UI
                    {
                        let mut state = state.lock().unwrap();
                        state.text = result;
                        state.link_color = LINK_COLOR_COMMON;
                    }
                }
            }
        });
    }

    Box::new(ui::MyApp::new(state, task_tx, cc))
}

pub fn run() {
    init_config();

    let (hotkey_tx, hotkey_rx) = mpsc::sync_channel(1);

    let mut hotkey_settings = HotkeySetting::default();
    hotkey_settings.register_hotkey(hotkey_tx.clone());

    loop {
        match hotkey_rx.recv() {
            Ok(_) => {
                hotkey_settings.unregister_all();
                launch_window();
                hotkey_settings.register_hotkey(hotkey_tx.clone());
            }
            Err(err) => {
                panic!("{}", err)
            }
        }
    }
}

fn launch_window() {
    let (width, height) = get_window_size();

    let native_options = eframe::NativeOptions {
        always_on_top: true,
        decorated: false,
        initial_window_size: Some(egui::vec2(width, height)),
        icon_data: Some(get_icon_data()),
        run_and_return: true,
        ..Default::default()
    };
    eframe::run_native("Translator", native_options, Box::new(setup_ui_task));
}
