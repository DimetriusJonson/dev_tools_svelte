mod db;
mod http_server;
mod processor;
mod rest;
mod share;
mod update;
mod wsdl;

use std::sync::{Arc, Mutex};

use tauri::{
    Manager, RunEvent, WindowEvent, menu::{Menu, MenuItem}, tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
};

use db::AppDb;

pub const TRAY_MENU_ITEMS: [(&str, &str); 2] = [("open", "Открыть"), ("quit", "Выход")];

#[derive(Debug, PartialEq, Eq)]
pub enum TrayAction {
    Open,
    Quit,
    Ignore,
}

pub fn tray_action_for(id: &str) -> TrayAction {
    match id {
        "open" => TrayAction::Open,
        "quit" => TrayAction::Quit,
        _ => TrayAction::Ignore,
    }
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn init_database(app: &tauri::AppHandle) -> Result<AppDb, Box<dyn std::error::Error>> {
    let directory = app.path().app_data_dir()?;
    std::fs::create_dir_all(&directory)?;
    let connection = rusqlite::Connection::open(directory.join("webdev-useful-tools.db"))?;
    db::init_schema(&connection)?;
    Ok(AppDb {
        conn: Arc::new(Mutex::new(connection)),
    })
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app);
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            let database = init_database(app.handle())?;
            let share_store = share::ShareStore::new(database.clone());
            app.manage(database);
            app.manage(rest::RestCancelState::default());
            app.manage(wsdl::WsdlFetchState::default());
            app.manage(share::PendingDeepLink::default());
            share::install_deep_link_handler(app.handle());
            app.manage(share_store.clone());
            app.manage(http_server::start(share_store));

            let open_item = MenuItem::with_id(
                app,
                TRAY_MENU_ITEMS[0].0,
                TRAY_MENU_ITEMS[0].1,
                true,
                None::<&str>,
            )?;
            let quit_item = MenuItem::with_id(
                app,
                TRAY_MENU_ITEMS[1].0,
                TRAY_MENU_ITEMS[1].1,
                true,
                None::<&str>,
            )?;
            let menu = Menu::with_items(app, &[&open_item, &quit_item])?;

            TrayIconBuilder::with_id("main-tray")
                .icon(tauri::include_image!("icons/128x128.png"))
                .menu(&menu)
                .show_menu_on_left_click(false)
                .tooltip("WebDev Useful Tools")
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::DoubleClick { button: MouseButton::Left, .. } = event {
                        let app_handle = tray.app_handle();
                        if let Some(_window) = app_handle.get_webview_window("main") {
                            show_main_window(app_handle)
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            db::get_editor_state,
            db::save_editor_state,
            db::get_setting,
            db::set_setting,
            processor::process_large_file,
            processor::save_text_file,
            share::list_share_interfaces,
            share::get_share_server_status,
            share::restart_share_server,
            share::upload_share_content,
            share::upload_share_file,
            share::load_shared_upload,
            share::download_shared_upload,
            share::delete_shared_upload,
            share::consume_pending_deep_link,
            share::clear_pending_deep_link,
            rest::cmd_list_rest_projects,
            rest::cmd_create_rest_project,
            rest::cmd_rename_rest_project,
            rest::cmd_delete_rest_project,
            rest::cmd_list_rest_requests,
            rest::cmd_create_rest_request,
            rest::cmd_update_rest_request,
            rest::cmd_rename_rest_request,
            rest::cmd_delete_rest_request,
            rest::cmd_duplicate_rest_request,
            rest::cmd_save_rest_response,
            rest::cmd_clear_rest_response,
            rest::cmd_execute_rest_request,
            rest::cmd_cancel_rest_request,
            rest::cmd_download_rest_response,
            update::restart_app,
            wsdl::cmd_list_wsdl_projects,
            wsdl::cmd_create_wsdl_project,
            wsdl::cmd_rename_wsdl_project,
            wsdl::cmd_delete_wsdl_project,
            wsdl::cmd_list_wsdl_files,
            wsdl::cmd_import_wsdl,
            wsdl::cmd_delete_wsdl_file,
            wsdl::cmd_get_wsdl_operation,
            wsdl::cmd_save_wsdl_operation_state,
            wsdl::cmd_save_wsdl_response,
            wsdl::cmd_clear_wsdl_response,
            wsdl::cmd_cancel_wsdl_fetch
        ])
        .build(tauri::generate_context!())
        .expect("error while building the Tauri application")
        .run(|app_handle, event| match event {
            RunEvent::MenuEvent(menu_event) => match tray_action_for(menu_event.id.as_ref()) {
                TrayAction::Open => show_main_window(app_handle),
                TrayAction::Quit => app_handle.exit(0),
                TrayAction::Ignore => {}
            },
            RunEvent::ExitRequested { .. } | RunEvent::Exit => {
                if let Some(state) = app_handle.try_state::<share::ShareServerState>() {
                    state.stop();
                }
            }
            _ => {}
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_tray_actions() {
        assert_eq!(tray_action_for("open"), TrayAction::Open);
        assert_eq!(tray_action_for("quit"), TrayAction::Quit);
        assert_eq!(tray_action_for("other"), TrayAction::Ignore);
    }

    #[test]
    fn exposes_localized_menu_labels() {
        assert_eq!(TRAY_MENU_ITEMS[0].1, "Открыть");
        assert_eq!(TRAY_MENU_ITEMS[1].1, "Выход");
    }
}
