use std::sync::{Mutex,MutexGuard};
use tauri::AppHandle;
use crate::{models::{PublicSettings,Settings},storage};
static SETTINGS_UPDATE_LOCK: Mutex<()> = Mutex::new(());
pub(crate) fn merge_runtime_settings(incoming:Settings,existing:&Settings)->Result<Settings,String>{incoming.merged_with_existing_token(existing)}
pub(crate) fn lock_settings_update()->Result<MutexGuard<'static,()>,String>{SETTINGS_UPDATE_LOCK.lock().map_err(|_|"设置更新锁已损坏".to_string())}
pub(crate) fn load_existing_settings_for_runtime(app:&AppHandle)->Result<Settings,String>{recover_existing_settings_for_runtime(storage::load_existing_settings(app))}
pub(crate) fn recover_existing_settings_for_runtime(result:Result<Option<Settings>,String>)->Result<Settings,String>{match result{Ok(Some(s))=>Ok(s),Ok(None)=>Ok(Settings::default()),Err(e) if is_recoverable_settings_read_error(&e)=>Ok(Settings::default()),Err(e)=>Err(e)}}
pub(crate) fn is_recoverable_settings_read_error(error:&str)->bool{error.starts_with("设置文件超过安全读取上限，已备份")||error.starts_with("设置文件损坏，已备份")}
pub(crate) fn clear_github_token_settings(mut settings:Settings)->Result<Settings,String>{settings.github_token.clear();settings.normalized()}
#[tauri::command] pub(crate) fn load_settings(app:AppHandle)->Result<PublicSettings,String>{storage::load_settings(&app).map(|s|s.public_view())}
#[tauri::command] pub(crate) fn save_settings(app:AppHandle,settings:Settings)->Result<PublicSettings,String>{let _guard=lock_settings_update()?;let existing=load_existing_settings_for_runtime(&app)?;let settings=merge_runtime_settings(settings,&existing)?;storage::save_settings(&app,&settings)?;Ok(settings.public_view())}
#[tauri::command] pub(crate) fn clear_github_token(app:AppHandle)->Result<PublicSettings,String>{let _guard=lock_settings_update()?;let settings=clear_github_token_settings(load_existing_settings_for_runtime(&app)?)?;storage::save_settings(&app,&settings)?;Ok(settings.public_view())}
