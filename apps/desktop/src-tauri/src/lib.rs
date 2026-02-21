pub(crate) mod auth_service;
pub(crate) mod commands;
pub(crate) mod db;

pub struct DbState {
    pub conn: std::sync::Mutex<rusqlite::Connection>,
}

impl DbState {
    pub fn new(conn: rusqlite::Connection) -> Self {
        Self {
            conn: std::sync::Mutex::new(conn),
        }
    }
}

fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::Manager;

    let show_hide =
        tauri::menu::MenuItem::with_id(app, "show_hide", "Show/Hide", true, None::<&str>)?;
    let quit = tauri::menu::MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = tauri::menu::Menu::with_items(app, &[&show_hide, &quit])?;

    tauri::tray::TrayIconBuilder::new()
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show_hide" => {
                if let Some(window) = app.get_webview_window("main") {
                    if window.is_visible().unwrap_or(false) {
                        if let Err(e) = window.hide() {
                            tracing::warn!("Failed to hide window: {e}");
                        }
                    } else {
                        if let Err(e) = window.show() {
                            tracing::warn!("Failed to show window: {e}");
                        }
                        if let Err(e) = window.set_focus() {
                            tracing::warn!("Failed to focus window: {e}");
                        }
                    }
                }
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .build(app)?;
    Ok(())
}

fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new().commands(tauri_specta::collect_commands![
        commands::health::health_check,
        commands::auth::auth_register_start,
        commands::auth::auth_verify_email,
        commands::auth::auth_register_complete,
        commands::auth::auth_login,
        commands::auth::auth_refresh,
        commands::auth::auth_logout,
        commands::auth::auth_recover_start,
        commands::auth::auth_recover_verify,
        commands::auth::auth_recover_complete,
        commands::auth::auth_check_identity,
        commands::auth::auth_get_public_key,
    ])
}

pub fn run() {
    use tauri::Manager;

    tracing_subscriber::fmt::init();

    let builder = specta_builder();

    #[cfg(debug_assertions)]
    builder
        .export(
            specta_typescript::Typescript::default()
                .bigint(specta_typescript::BigIntExportBehavior::Number),
            "../src/bindings.ts",
        )
        .expect("failed to export typescript bindings");

    tauri::Builder::default()
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_decorum::init())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);

            let app_data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_data_dir)?;

            let db_path = app_data_dir.join("openconv.db");
            let conn =
                db::init_db(&db_path).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            app.manage(DbState::new(conn));

            let crypto_db_path = app_data_dir.join("crypto.db");
            let api_base_url =
                std::env::var("OPENCONV_API_URL").unwrap_or_else(|_| "http://localhost:3000".into());
            let auth_svc = auth_service::AuthService::new(crypto_db_path, api_base_url)
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            app.manage(auth_service::AuthState {
                auth_service: auth_svc,
            });

            setup_tray(app)?;

            #[cfg(target_os = "macos")]
            {
                if let Some(window) = app.get_webview_window("main") {
                    tauri_plugin_decorum::WebviewWindowExt::create_overlay_titlebar(&window)
                        .expect("failed to create overlay titlebar");
                }
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_bindings() {
        specta_builder()
            .export(
                specta_typescript::Typescript::default()
                    .bigint(specta_typescript::BigIntExportBehavior::Number),
                "../src/bindings.ts",
            )
            .expect("failed to export typescript bindings");
    }
}
