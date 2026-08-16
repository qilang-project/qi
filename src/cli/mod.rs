//! Command-line interface module

pub mod commands;
pub mod doctor;
pub mod get;

// rustc 不接受非 ASCII 模块名自动映射文件名（E0754），要显式 #[path]。
#[path = "绑定生成.rs"]
pub mod 绑定生成;
#[path = "绑定类型.rs"]
mod 绑定类型;
