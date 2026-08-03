use super::commands_diagnostics::configured_flag;
use super::commands_settings_runtime::*;
use super::models::Settings;
#[test]fn diagnostics_only_exposes_path_configured_flag(){assert!(configured_flag("D:/R/project-library"));assert!(!configured_flag("  "));}
#[test]fn runtime_settings_preserve_saved_token_when_incoming_token_empty(){let e=Settings{github_token:"ghp_saved".into(),..Default::default()};let m=merge_runtime_settings(Settings{github_token:String::new(),..Default::default()},&e).unwrap();assert_eq!(m.github_token,"ghp_saved");}
#[test]fn runtime_settings_allow_explicit_token_replacement(){let e=Settings{github_token:"ghp_saved".into(),..Default::default()};let m=merge_runtime_settings(Settings{github_token:"ghp_new".into(),..Default::default()},&e).unwrap();assert_eq!(m.github_token,"ghp_new");}
#[test]fn runtime_settings_public_view_reflects_normalized_values(){let p=merge_runtime_settings(Settings{proxy:"127.0.0.1:7890".into(),github_token:"ghp_new".into(),cran_mirror:"https://cloud.r-project.org".into(),full_search:true,..Default::default()},&Settings::default()).unwrap().public_view();assert_eq!(p.proxy,"http://127.0.0.1:7890");assert!(p.github_token_configured);}
#[test]fn recoverable_settings_read_errors_do_not_preserve_token(){assert!(is_recoverable_settings_read_error("设置文件损坏，已备份；请重新确认设置后再保存"));assert!(!is_recoverable_settings_read_error("存储目录无效"));let m=merge_runtime_settings(Settings{github_token:String::new(),..Default::default()},&Settings::default()).unwrap();assert!(m.github_token.is_empty());}
#[test]fn runtime_settings_recover_from_corrupt_saved_settings(){let s=recover_existing_settings_for_runtime(Err("设置文件损坏，已备份；请重新确认设置后再保存".into())).unwrap();assert!(s.github_token.is_empty());}
#[test]fn runtime_settings_keep_unrecoverable_saved_settings_error(){assert_eq!(recover_existing_settings_for_runtime(Err("存储目录无效".into())).unwrap_err(),"存储目录无效");}
#[test]fn clearing_token_from_recovered_settings_uses_default_public_state(){assert!(!clear_github_token_settings(Settings::default()).unwrap().public_view().github_token_configured);}
#[test]fn clearing_token_preserves_other_settings(){let s=Settings{proxy:"127.0.0.1:7890".into(),github_token:"ghp_saved".into(),full_search:true,..Default::default()};let p=clear_github_token_settings(s).unwrap().public_view();assert!(!p.github_token_configured);assert!(p.full_search);}
