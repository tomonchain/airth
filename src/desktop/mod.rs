//! The macOS app: Airth in its own window, with a Dock icon, a menu bar icon
//! and a global shortcut (⌥⇧A), like other desktop assistants.
//!
//! The window shows the same app as the browser, served by the background
//! service when it runs, otherwise by this process. Closing the window keeps
//! Airth running in the menu bar; Quit stops it (objectives keep running in
//! the background service, if it is installed).

use crate::hub::Hub;
use crate::icon;
use crate::ui::{self, UiOptions};
use anyhow::{Context, Result};
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use tao::dpi::{LogicalPosition, LogicalSize};
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::platform::macos::WindowBuilderExtMacOS;
use tao::window::WindowBuilder;
use tray_icon::menu::{
    AboutMetadata, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu, accelerator::Accelerator,
};
use tray_icon::{Icon, TrayIconBuilder};
use wry::{NewWindowResponse, WebViewBuilder};

enum UserEvent {
    Menu(MenuEvent),
    HotKey(GlobalHotKeyEvent),
    /// The page asks to move the window (a press on its title bar area).
    Drag,
    /// A double click on the title bar area.
    Zoom,
}

/// The page draws its own title bar: it knows it runs in the app window, and
/// a press on an empty spot of the top strip moves the window.
/// (Runs before the document exists; theme.js applies it.)
const WINDOW_SCRIPT: &str = r#"window.airthShell = "mac";"#;

/// Open a link outside the app, in the default browser.
fn open_outside(url: &str) {
    if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("mailto:") {
        let _ = std::process::Command::new("/usr/bin/open").arg(url).spawn();
    }
}

pub fn run(hub: Hub, options: UiOptions) -> Result<()> {
    // The background service serves the app when it runs; otherwise we do.
    let (url, own) = match ui::running_url(&hub) {
        Some(url) => (url, None),
        None => {
            let running = ui::start(hub, options)?;
            (running.url.clone(), Some(running))
        }
    };
    let origin = url.split("/#").next().unwrap_or(&url).to_string();

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();

    // The app menu. The Edit menu is what makes ⌘C, ⌘V, ⌘A work in the page.
    let quit = MenuItem::new(
        "Quit Airth",
        true,
        "CmdOrCtrl+Q".parse::<Accelerator>().ok(),
    );
    let reload = MenuItem::new("Reload", true, "CmdOrCtrl+R".parse::<Accelerator>().ok());
    let inbox = MenuItem::new("Inbox", true, "CmdOrCtrl+I".parse::<Accelerator>().ok());
    let new_chat = MenuItem::new("New Chat", true, "CmdOrCtrl+N".parse::<Accelerator>().ok());
    let menu = Menu::new();
    let app_menu = Submenu::new("Airth", true);
    app_menu.append_items(&[
        &PredefinedMenuItem::about(
            None,
            Some(AboutMetadata {
                name: Some("Airth".into()),
                version: Some(crate::update::version()),
                comments: Some("Your personal agent.".into()),
                ..Default::default()
            }),
        ),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::hide(None),
        &PredefinedMenuItem::hide_others(None),
        &PredefinedMenuItem::show_all(None),
        &PredefinedMenuItem::separator(),
        &quit,
    ])?;
    let file_menu = Submenu::new("File", true);
    file_menu.append_items(&[&new_chat, &inbox])?;
    let edit_menu = Submenu::new("Edit", true);
    edit_menu.append_items(&[
        &PredefinedMenuItem::undo(None),
        &PredefinedMenuItem::redo(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::cut(None),
        &PredefinedMenuItem::copy(None),
        &PredefinedMenuItem::paste(None),
        &PredefinedMenuItem::select_all(None),
    ])?;
    let view_menu = Submenu::new("View", true);
    view_menu.append_items(&[&reload, &PredefinedMenuItem::fullscreen(None)])?;
    let window_menu = Submenu::new("Window", true);
    window_menu.append_items(&[
        &PredefinedMenuItem::minimize(None),
        &PredefinedMenuItem::close_window(None),
    ])?;
    menu.append_items(&[&app_menu, &file_menu, &edit_menu, &view_menu, &window_menu])?;
    menu.init_for_nsapp();

    let window = WindowBuilder::new()
        .with_title("Airth")
        .with_inner_size(LogicalSize::new(1240.0, 840.0))
        .with_min_inner_size(LogicalSize::new(720.0, 480.0))
        // The page fills the whole window; the traffic lights sit on it.
        .with_titlebar_transparent(true)
        .with_title_hidden(true)
        .with_fullsize_content_view(true)
        .with_traffic_light_inset(LogicalPosition::new(16.0, 19.0))
        .build(&event_loop)
        .context("cannot open the window")?;
    let nav_origin = origin.clone();
    let ipc = event_loop.create_proxy();
    let webview = WebViewBuilder::new()
        .with_url(&url)
        .with_initialization_script(WINDOW_SCRIPT)
        .with_ipc_handler(move |request| {
            let event = match request.body().as_str() {
                "drag" => UserEvent::Drag,
                "zoom" => UserEvent::Zoom,
                _ => return,
            };
            let _ = ipc.send_event(event);
        })
        .with_navigation_handler(move |to| {
            if to.starts_with(&nav_origin) || to.starts_with("about:") {
                return true;
            }
            open_outside(&to);
            false
        })
        .with_new_window_req_handler(|to, _| {
            open_outside(&to);
            NewWindowResponse::Deny
        })
        .build(&window)
        .context("cannot create the web view")?;

    // Menu bar icon.
    let tray_open = MenuItem::new("Open Airth", true, None);
    let tray_inbox = MenuItem::new("Inbox", true, None);
    let tray_quit = MenuItem::new("Quit Airth", true, None);
    let tray_menu = Menu::new();
    tray_menu.append_items(&[
        &tray_open,
        &tray_inbox,
        &PredefinedMenuItem::separator(),
        &tray_quit,
    ])?;
    let mut tray = None;

    // ⌥⇧A shows or hides Airth from anywhere.
    let hotkeys = GlobalHotKeyManager::new().ok();
    let hotkey = HotKey::new(Some(Modifiers::ALT | Modifiers::SHIFT), Code::KeyA);
    if let Some(m) = &hotkeys
        && let Err(e) = m.register(hotkey)
    {
        eprintln!("the ⌥⇧A shortcut is taken by another app: {e}");
    }

    let proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |e| {
        let _ = proxy.send_event(UserEvent::Menu(e));
    }));
    let proxy = event_loop.create_proxy();
    GlobalHotKeyEvent::set_event_handler(Some(move |e| {
        let _ = proxy.send_event(UserEvent::HotKey(e));
    }));

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        let _keep = &hotkeys;
        let show = || {
            window.set_visible(true);
            window.set_minimized(false);
            window.set_focus();
        };
        let go = |hash: &str| {
            show();
            let _ = webview.evaluate_script(&format!("location.hash = {hash:?}"));
        };
        match event {
            Event::NewEvents(StartCause::Init) => {
                let pixels = icon::draw(44, false);
                tray = Icon::from_rgba(pixels, 44, 44).ok().and_then(|icon| {
                    TrayIconBuilder::new()
                        .with_icon_templated(icon)
                        .with_tooltip("Airth")
                        .with_menu(Box::new(tray_menu.clone()))
                        .build()
                        .ok()
                });
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => window.set_visible(false),
            Event::Reopen { .. } => show(),
            Event::UserEvent(UserEvent::Menu(e)) => {
                let id = e.id();
                if id == quit.id() || id == tray_quit.id() {
                    if let Some(server) = &own {
                        server.stop();
                    }
                    tray.take();
                    *control_flow = ControlFlow::Exit;
                } else if id == tray_open.id() {
                    show();
                } else if id == inbox.id() || id == tray_inbox.id() {
                    go("#/inbox");
                } else if id == new_chat.id() {
                    go("#/");
                } else if id == reload.id() {
                    let _ = webview.reload();
                }
            }
            Event::UserEvent(UserEvent::Drag) => {
                let _ = window.drag_window();
            }
            Event::UserEvent(UserEvent::Zoom) => window.set_maximized(!window.is_maximized()),
            Event::UserEvent(UserEvent::HotKey(e))
                if e.id == hotkey.id() && e.state == HotKeyState::Pressed =>
            {
                if window.is_visible() && window.is_focused() {
                    window.set_visible(false);
                } else {
                    show();
                }
            }
            _ => {}
        }
    });
}
