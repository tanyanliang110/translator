use eframe::{App, CreationContext};

use log::warn;
use std::{
   sync::{mpsc, Arc, Mutex}, thread::{self, sleep}, time::Duration
};
use reqwest::Client;
use serde_json::json;
use crate::{
    cfg::{ get_window_size, init_config},
    hotkey::{ctrl_c, HotkeySetting},
    mouse::MouseState,
    ui::{self, get_icon_data, State, LINK_COLOR_COMMON, LINK_COLOR_DOING},
};
const MODEL: &str = "MODEL_NAME"; // 替换为你的模型名称
const API_URL: &str = "https://ark.cn-beijing.volces.com/api/v3/chat/completions";// 替换为你的API URL

async fn call_api(text: &str) -> String {
    let client = Client::new();
    
    // 构建请求头（错误直接 panic）
    let auth_header = format!("Bearer {}", "请替换为你的API key");
    
    // 构建消息体（强制类型校验）
    let body = json!({
        "model": MODEL,
        "messages": [
            {
                "role": "system",
                "content": "你是一个专业翻译的专家，严格将内容翻译成中文，保持原意且不添加解释，并且回复的内容不需要说明，只需要翻译的内容"
            },
            {
                "role": "user",
                "content": text
            }
        ]
    });

    // 发送请求并解析响应（错误直接 panic）
    client.post(API_URL)
        .header("Content-Type", "application/json")
        .header("Authorization", auth_header)
        .json(&body)
        .send()
        .await
        .expect("HTTP请求失败")
        .json::<serde_json::Value>()
        .await
        .expect("响应解析失败")["choices"][0]["message"]["content"]
        .as_str()
        .expect("无效的响应格式")
        .to_string()
}
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
                                let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
                                let result = rt.block_on(call_api(&text_new));

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
                    let text = {
                                 let state = state.lock().unwrap();
                                 state.text.clone()
                             };
                    let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
                    let result = rt.block_on(call_api(&text));

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
