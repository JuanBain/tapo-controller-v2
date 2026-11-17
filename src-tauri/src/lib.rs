mod storage;
mod device;
mod commands;
mod scheduler;
mod discovery;
mod credentials;
mod validation;
mod capabilities;
mod custom_effects;

use std::sync::Arc;
use tauri::{
    Manager,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    WindowEvent,
};
use log::{info, warn};

use device::{get_target, new_device_manager, TapoDevice};
use storage::Storage;
use credentials::{CredentialManager, KeyringSecretStore};
use commands::{CredentialsState, StorageState, SchedulerState};

/// Auto-reconnect to all saved devices on startup.
/// Returns `(saved, reconnected)`.
async fn auto_reconnect(
    device_manager: &device::DeviceManager,
    storage: &Storage,
    credentials: &CredentialManager,
) -> (usize, usize) {
    let saved = storage.get_saved_devices();
    let Some(session) = credentials.session() else {
        return (saved.len(), 0);
    };
    let mut reconnected = 0;
    for saved_dev in &saved {
        let mut device = TapoDevice::new(&session.email, &session.password);
        match device.connect(&saved_dev.ip).await {
            Ok(info) => {
                info!("Auto-connected to {} ({})", saved_dev.ip, info.nickname);
                device_manager.write().await.insert(saved_dev.ip.clone(), Arc::new(device));
                reconnected += 1;
            }
            Err(e) => {
                warn!("Auto-connect to {} failed: {}", saved_dev.ip, e);
            }
        }
    }
    (saved.len(), reconnected)
}

/// Tray actions only run when the target is unambiguous (one connected device).
fn spawn_tray_action(dm: device::DeviceManager, brightness: Option<u8>, on: bool) {
    tauri::async_runtime::spawn(async move {
        let device = match get_target(&dm, None).await {
            Ok(d) => d,
            Err(e) => {
                warn!("Tray action skipped: {}", e);
                return;
            }
        };
        let result = if on {
            match device.turn_on().await {
                Ok(()) => match brightness {
                    Some(b) => device.set_brightness(b).await,
                    None => Ok(()),
                },
                Err(e) => Err(e),
            }
        } else {
            device.turn_off().await
        };
        if let Err(e) = result {
            warn!("Tray action failed: {}", e);
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![]),
        ))
        .plugin(tauri_plugin_store::Builder::new().build())
        .setup(|app| {
            // Initialize storage with app data dir
            let app_data_dir = app.path().app_data_dir()
                .expect("Failed to get app data dir");
            let storage = Arc::new(Storage::new(app_data_dir));
            let device_manager = new_device_manager();

            // Migration and session restore run before any command can be
            // invoked, so `auth_status` never races with them.
            let credentials = Arc::new(CredentialManager::new(Box::new(KeyringSecretStore), storage.clone()));
            let restore_report = credentials.restore();

            // Store state for commands
            app.manage(StorageState(storage.clone()));
            app.manage(CredentialsState(credentials.clone()));
            app.manage(device_manager.clone());

            // Start scheduler, then auto-reconnect, in background
            let dm_clone = device_manager.clone();
            let storage_clone = storage.clone();
            let app_handle = app.handle().clone();

            tauri::async_runtime::spawn(async move {
                match scheduler::create_scheduler_manager(dm_clone.clone(), storage_clone.clone()).await {
                    Ok(scheduler_mgr) => {
                        app_handle.manage(SchedulerState(scheduler_mgr));
                        info!("Scheduler started");
                    }
                    Err(e) => {
                        warn!("Failed to start scheduler: {}", e);
                    }
                }

                let (saved, reconnected) = auto_reconnect(&dm_clone, &storage_clone, &credentials).await;
                match credentials::legacy_cleanup_decision(&restore_report, saved, reconnected) {
                    credentials::LegacyCleanup::Removed => {
                        match storage_clone.remove_legacy_credentials() {
                            Ok(()) => info!(
                                "Removed data/credentials.json after the password was recovered from Windows Credential Manager and every saved device reconnected"
                            ),
                            Err(e) => warn!("data/credentials.json was kept: {}", e),
                        }
                    }
                    credentials::LegacyCleanup::Kept(reason) => {
                        warn!("Keeping data/credentials.json: {}", reason);
                    }
                    credentials::LegacyCleanup::NotPresent => {}
                }
            });

            // Build tray menu
            let show_item = MenuItem::with_id(app, "show", "Show App", true, None::<&str>)?;
            let separator1 = MenuItem::with_id(app, "sep1", "─── Quick Actions ───", false, None::<&str>)?;
            let on_item = MenuItem::with_id(app, "turn_on", "Turn On", true, None::<&str>)?;
            let off_item = MenuItem::with_id(app, "turn_off", "Turn Off", true, None::<&str>)?;
            let b25_item = MenuItem::with_id(app, "brightness_25", "Brightness 25%", true, None::<&str>)?;
            let b50_item = MenuItem::with_id(app, "brightness_50", "Brightness 50%", true, None::<&str>)?;
            let b100_item = MenuItem::with_id(app, "brightness_100", "Brightness 100%", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

            let menu = Menu::with_items(app, &[
                &show_item,
                &separator1,
                &on_item,
                &off_item,
                &b25_item,
                &b50_item,
                &b100_item,
                &quit_item,
            ])?;

            let _tray = TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("Tapo Controller")
                .icon(app.default_window_icon().unwrap().clone())
                .on_menu_event(move |app, event| {
                    let dm = app.state::<device::DeviceManager>();
                    match event.id.as_ref() {
                        "show" => {
                            if let Some(window) = app.get_webview_window("main") {
                                window.show().ok();
                                window.set_focus().ok();
                            }
                        }
                        "turn_on" => spawn_tray_action(dm.inner().clone(), None, true),
                        "turn_off" => spawn_tray_action(dm.inner().clone(), None, false),
                        "brightness_25" | "brightness_50" | "brightness_100" => {
                            let brightness: u8 = match event.id.as_ref() {
                                "brightness_25" => 25,
                                "brightness_50" => 50,
                                _ => 100,
                            };
                            spawn_tray_action(dm.inner().clone(), Some(brightness), true);
                        }
                        "quit" => {
                            app.exit(0);
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            window.show().ok();
                            window.set_focus().ok();
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            // Close to tray instead of quitting
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                window.hide().ok();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::save_credentials_api,
            commands::has_credentials,
            commands::login,
            commands::logout,
            commands::auth_status,
            commands::last_ip,
            commands::list_devices,
            commands::add_device,
            commands::delete_device,
            commands::reconnect_device,
            commands::discover_devices_cmd,
            commands::device_info,
            commands::device_power,
            commands::device_brightness,
            commands::device_color,
            commands::device_temperature,
            commands::device_effect,
            commands::device_segment_effect,
            commands::device_lighting_capabilities,
            commands::device_experiment,
            commands::device_custom_effect,
            commands::available_effects,
            commands::list_presets,
            commands::create_preset,
            commands::delete_preset_cmd,
            commands::apply_preset,
            commands::list_schedules,
            commands::create_schedule,
            commands::update_schedule,
            commands::delete_schedule_cmd,
            commands::toggle_schedule,
            commands::get_saved_credentials,
            commands::save_credentials_local,
            commands::clear_credentials_cmd,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
