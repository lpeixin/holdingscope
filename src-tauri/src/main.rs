#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod db;
mod model;
mod source;
mod sync;

use model::{Catalog, Issue, Portfolio, SyncReport};
use sync::Service;
use tauri::{Manager, State};
use tauri_plugin_opener::OpenerExt;

#[tauri::command]
async fn list_managers(state: State<'_, Service>) -> Result<Catalog, Issue> {
    Ok(state.catalog()?)
}

#[tauri::command]
async fn sync_catalog(state: State<'_, Service>) -> Result<SyncReport, Issue> {
    state.sync_catalog().await
}

#[tauri::command]
async fn get_portfolio(state: State<'_, Service>, id: String) -> Result<Option<Portfolio>, Issue> {
    Ok(state.portfolio(&id)?)
}

#[tauri::command]
async fn refresh_portfolio(
    state: State<'_, Service>,
    id: String,
    force: bool,
) -> Result<Option<Portfolio>, Issue> {
    state.refresh_portfolio(&id, force).await
}

#[tauri::command]
async fn load_more_activity(
    state: State<'_, Service>,
    id: String,
) -> Result<Option<Portfolio>, Issue> {
    state.load_more_activity(&id).await
}

#[tauri::command]
fn open_source(app: tauri::AppHandle, id: String) -> Result<(), Issue> {
    let url = source::holdings_url(&id).map_err(|error| Issue::SourceLink.log(error))?;
    app.opener()
        .open_url(url.as_str(), None::<&str>)
        .map_err(|error| Issue::SourceLink.log(error))
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let directory = app.path().app_data_dir()?;
            std::fs::create_dir_all(&directory)?;
            let service = Service::new(&directory.join("holdingscope.sqlite3"))
                .map_err(std::io::Error::other)?;
            app.manage(service);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_managers,
            sync_catalog,
            get_portfolio,
            refresh_portfolio,
            load_more_activity,
            open_source
        ])
        .run(tauri::generate_context!())
        .expect("HoldingScope could not start");
}
