use tauri::{
    App, Manager,
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

const TRAY_ICON_SIZE: u32 = 32;

fn tray_image() -> Image<'static> {
    let mut rgba = vec![0_u8; (TRAY_ICON_SIZE * TRAY_ICON_SIZE * 4) as usize];

    for y in 0..TRAY_ICON_SIZE {
        for x in 0..TRAY_ICON_SIZE {
            let index = ((y * TRAY_ICON_SIZE + x) * 4) as usize;
            let in_bubble = (7..25).contains(&x) && (6..22).contains(&y);
            let in_tail = (7..13).contains(&x) && (20..26).contains(&y) && y - 20 >= x - 7;
            let (red, green, blue) = if in_bubble || in_tail {
                (255, 255, 255)
            } else {
                (35, 105, 190)
            };
            rgba[index] = red;
            rgba[index + 1] = green;
            rgba[index + 2] = blue;
            rgba[index + 3] = 255;
        }
    }

    Image::new_owned(rgba, TRAY_ICON_SIZE, TRAY_ICON_SIZE)
}

pub fn install(app: &mut App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open IPMsgSec", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;

    TrayIconBuilder::new()
        .icon(tray_image())
        .tooltip("IPMsgSec")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
