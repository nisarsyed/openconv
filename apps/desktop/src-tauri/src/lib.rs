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

pub fn run() {
    use tauri::Manager;

    tracing_subscriber::fmt::init();

    let builder =
        tauri_specta::Builder::<tauri::Wry>::new().commands(tauri_specta::collect_commands![
            commands::health::health_check,
        ]);

    #[cfg(debug_assertions)]
    builder
        .export(
            specta_typescript::Typescript::default()
                .bigint(specta_typescript::BigIntExportBehavior::Number),
            "../src/bindings.ts",
        )
        .expect("failed to export typescript bindings");

    tauri::Builder::default()
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);

            let app_data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_data_dir)?;

            let db_path = app_data_dir.join("openconv.db");
            let conn =
                db::init_db(&db_path).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            app.manage(DbState::new(conn));

            setup_tray(app)?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
