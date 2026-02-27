//! GUI FFI bindings for qi-gui library
//!
//! 图形化窗口接口
//!
//! This module provides FFI bindings to the qi-gui library when available.
//! When GUI library is not linked, stub implementations are provided that return errors.

use std::os::raw::c_char;

/// Event callback function type
/// Parameters: window_id, event_type, param1, param2
type EventCallback = extern "C" fn(u64, i32, i64, i64);

// When GUI library is available, link to it
#[cfg(has_gui)]
extern "C" {
    fn qi_gui_create_window_impl(title: *const c_char, width: u32, height: u32) -> u64;
    fn qi_gui_destroy_window_impl(window_id: u64);
    fn qi_gui_set_title_impl(window_id: u64, title: *const c_char);
    fn qi_gui_get_title_impl(window_id: u64) -> *mut c_char;
    fn qi_gui_show_window_impl(window_id: u64);
    fn qi_gui_hide_window_impl(window_id: u64);
    fn qi_gui_is_visible_impl(window_id: u64) -> i32;
    fn qi_gui_set_event_callback_impl(window_id: u64, callback: EventCallback);
    fn qi_gui_enable_event_printing_impl(window_id: u64);
    fn qi_gui_get_position_x_impl(window_id: u64) -> i64;
    fn qi_gui_get_position_y_impl(window_id: u64) -> i64;
    fn qi_gui_set_position_impl(window_id: u64, x: i32, y: i32);
    fn qi_gui_get_width_impl(window_id: u64) -> i64;
    fn qi_gui_get_height_impl(window_id: u64) -> i64;
    fn qi_gui_set_size_impl(window_id: u64, width: u32, height: u32);
    fn qi_gui_run_impl();
    fn qi_gui_version_impl() -> *mut c_char;
    fn qi_gui_free_string_impl(s: *mut c_char);

    // Audio functions
    fn qi_gui_audio_load_impl(file_path: *const c_char) -> u64;
    fn qi_gui_audio_play_impl(audio_id: u64);
    fn qi_gui_audio_pause_impl(audio_id: u64);
    fn qi_gui_audio_stop_impl(audio_id: u64);
    fn qi_gui_audio_set_volume_impl(audio_id: u64, volume: f32);
    fn qi_gui_audio_is_playing_impl(audio_id: u64) -> i32;
    fn qi_gui_audio_is_finished_impl(audio_id: u64) -> i32;
    fn qi_gui_audio_free_impl(audio_id: u64);

    // Renderer functions
    fn qi_gui_renderer_create_impl(window_id: u64) -> u64;
    fn qi_gui_renderer_clear_impl(renderer_id: u64, r: u8, g: u8, b: u8);
    fn qi_gui_renderer_draw_pixel_impl(renderer_id: u64, x: u32, y: u32, r: u8, g: u8, b: u8);
    fn qi_gui_renderer_draw_rect_impl(renderer_id: u64, x: u32, y: u32, width: u32, height: u32, r: u8, g: u8, b: u8);
    fn qi_gui_renderer_draw_line_impl(renderer_id: u64, x0: i32, y0: i32, x1: i32, y1: i32, r: u8, g: u8, b: u8);
    fn qi_gui_renderer_draw_circle_impl(renderer_id: u64, cx: i32, cy: i32, radius: u32, r: u8, g: u8, b: u8);
    fn qi_gui_renderer_draw_image_impl(renderer_id: u64, file_path: *const c_char, x: u32, y: u32) -> i32;
    fn qi_gui_renderer_draw_text_impl(renderer_id: u64, text: *const c_char, x: i32, y: i32, r: u8, g: u8, b: u8);
    fn qi_gui_renderer_draw_text_scaled_impl(renderer_id: u64, text: *const c_char, x: i32, y: i32, scale: u32, r: u8, g: u8, b: u8);
    fn qi_gui_renderer_free_impl(renderer_id: u64);
}

#[no_mangle]
pub extern "C" fn qi_gui_create_window(title: *const c_char, width: i64, height: i64) -> i64 {
    #[cfg(has_gui)]
    {
        if title.is_null() {
            return 0;
        }
        unsafe {
            qi_gui_create_window_impl(title, width as u32, height as u32) as i64
        }
    }

    #[cfg(not(has_gui))]
    {
        eprintln!("错误: GUI 库未安装。请安装完整版本以使用图形化功能。");
        0
    }
}

#[no_mangle]
pub extern "C" fn qi_gui_destroy_window(window_id: i64) {
    #[cfg(has_gui)]
    {
        if window_id <= 0 {
            return;
        }
        unsafe {
            qi_gui_destroy_window_impl(window_id as u64);
        }
    }

    #[cfg(not(has_gui))]
    {
        let _ = window_id;
    }
}

#[no_mangle]
pub extern "C" fn qi_gui_set_title(window_id: i64, title: *const c_char) {
    #[cfg(has_gui)]
    {
        if window_id <= 0 || title.is_null() {
            return;
        }
        unsafe {
            qi_gui_set_title_impl(window_id as u64, title);
        }
    }

    #[cfg(not(has_gui))]
    {
        let _ = (window_id, title);
    }
}

#[no_mangle]
pub extern "C" fn qi_gui_get_title(window_id: i64) -> *mut c_char {
    #[cfg(has_gui)]
    {
        if window_id <= 0 {
            return std::ptr::null_mut();
        }
        unsafe {
            qi_gui_get_title_impl(window_id as u64)
        }
    }

    #[cfg(not(has_gui))]
    {
        let _ = window_id;
        std::ptr::null_mut()
    }
}

#[no_mangle]
pub extern "C" fn qi_gui_show_window(window_id: i64) {
    #[cfg(has_gui)]
    {
        if window_id <= 0 {
            return;
        }
        unsafe {
            qi_gui_show_window_impl(window_id as u64);
        }
    }

    #[cfg(not(has_gui))]
    {
        let _ = window_id;
    }
}

#[no_mangle]
pub extern "C" fn qi_gui_hide_window(window_id: i64) {
    #[cfg(has_gui)]
    {
        if window_id <= 0 {
            return;
        }
        unsafe {
            qi_gui_hide_window_impl(window_id as u64);
        }
    }

    #[cfg(not(has_gui))]
    {
        let _ = window_id;
    }
}

#[no_mangle]
pub extern "C" fn qi_gui_is_visible(window_id: i64) -> i64 {
    #[cfg(has_gui)]
    {
        if window_id <= 0 {
            return 0;
        }
        unsafe {
            qi_gui_is_visible_impl(window_id as u64) as i64
        }
    }

    #[cfg(not(has_gui))]
    {
        let _ = window_id;
        0
    }
}

#[no_mangle]
pub extern "C" fn qi_gui_set_event_callback(window_id: i64, callback: EventCallback) {
    #[cfg(has_gui)]
    {
        if window_id <= 0 {
            return;
        }
        unsafe {
            qi_gui_set_event_callback_impl(window_id as u64, callback);
        }
    }

    #[cfg(not(has_gui))]
    {
        let _ = (window_id, callback);
    }
}

#[no_mangle]
pub extern "C" fn qi_gui_enable_event_printing(window_id: i64) {
    #[cfg(has_gui)]
    {
        if window_id <= 0 {
            return;
        }
        unsafe {
            qi_gui_enable_event_printing_impl(window_id as u64);
        }
    }

    #[cfg(not(has_gui))]
    {
        let _ = window_id;
    }
}

#[no_mangle]
pub extern "C" fn qi_gui_get_position_x(window_id: i64) -> i64 {
    #[cfg(has_gui)]
    {
        if window_id <= 0 {
            return 0;
        }
        unsafe {
            qi_gui_get_position_x_impl(window_id as u64)
        }
    }

    #[cfg(not(has_gui))]
    {
        let _ = window_id;
        0
    }
}

#[no_mangle]
pub extern "C" fn qi_gui_get_position_y(window_id: i64) -> i64 {
    #[cfg(has_gui)]
    {
        if window_id <= 0 {
            return 0;
        }
        unsafe {
            qi_gui_get_position_y_impl(window_id as u64)
        }
    }

    #[cfg(not(has_gui))]
    {
        let _ = window_id;
        0
    }
}

#[no_mangle]
pub extern "C" fn qi_gui_set_position(window_id: i64, x: i64, y: i64) {
    #[cfg(has_gui)]
    {
        if window_id <= 0 {
            return;
        }
        unsafe {
            qi_gui_set_position_impl(window_id as u64, x as i32, y as i32);
        }
    }

    #[cfg(not(has_gui))]
    {
        let _ = (window_id, x, y);
    }
}

#[no_mangle]
pub extern "C" fn qi_gui_get_width(window_id: i64) -> i64 {
    #[cfg(has_gui)]
    {
        if window_id <= 0 {
            return 0;
        }
        unsafe {
            qi_gui_get_width_impl(window_id as u64)
        }
    }

    #[cfg(not(has_gui))]
    {
        let _ = window_id;
        0
    }
}

#[no_mangle]
pub extern "C" fn qi_gui_get_height(window_id: i64) -> i64 {
    #[cfg(has_gui)]
    {
        if window_id <= 0 {
            return 0;
        }
        unsafe {
            qi_gui_get_height_impl(window_id as u64)
        }
    }

    #[cfg(not(has_gui))]
    {
        let _ = window_id;
        0
    }
}

#[no_mangle]
pub extern "C" fn qi_gui_set_size(window_id: i64, width: i64, height: i64) {
    #[cfg(has_gui)]
    {
        if window_id <= 0 {
            return;
        }
        unsafe {
            qi_gui_set_size_impl(window_id as u64, width as u32, height as u32);
        }
    }

    #[cfg(not(has_gui))]
    {
        let _ = (window_id, width, height);
    }
}

#[no_mangle]
pub extern "C" fn qi_gui_run() {
    #[cfg(has_gui)]
    {
        unsafe {
            qi_gui_run_impl();
        }
    }

    #[cfg(not(has_gui))]
    {
        eprintln!("错误: GUI 库未安装。请安装完整版本以使用图形化功能。");
    }
}

#[no_mangle]
pub extern "C" fn qi_gui_version() -> *mut c_char {
    #[cfg(has_gui)]
    {
        unsafe {
            qi_gui_version_impl()
        }
    }

    #[cfg(not(has_gui))]
    {
        std::ptr::null_mut()
    }
}

#[no_mangle]
pub extern "C" fn qi_gui_free_string(s: *mut c_char) {
    #[cfg(has_gui)]
    {
        if s.is_null() {
            return;
        }
        unsafe {
            qi_gui_free_string_impl(s);
        }
    }

    #[cfg(not(has_gui))]
    {
        let _ = s;
    }
}

// Audio wrapper functions

#[no_mangle]
pub extern "C" fn qi_gui_audio_load(file_path: *const c_char) -> i64 {
    #[cfg(has_gui)]
    {
        if file_path.is_null() {
            return 0;
        }
        unsafe {
            qi_gui_audio_load_impl(file_path) as i64
        }
    }

    #[cfg(not(has_gui))]
    {
        let _ = file_path;
        0
    }
}

#[no_mangle]
pub extern "C" fn qi_gui_audio_play(audio_id: i64) {
    #[cfg(has_gui)]
    {
        if audio_id <= 0 {
            return;
        }
        unsafe {
            qi_gui_audio_play_impl(audio_id as u64);
        }
    }

    #[cfg(not(has_gui))]
    {
        let _ = audio_id;
    }
}

#[no_mangle]
pub extern "C" fn qi_gui_audio_pause(audio_id: i64) {
    #[cfg(has_gui)]
    {
        if audio_id <= 0 {
            return;
        }
        unsafe {
            qi_gui_audio_pause_impl(audio_id as u64);
        }
    }

    #[cfg(not(has_gui))]
    {
        let _ = audio_id;
    }
}

#[no_mangle]
pub extern "C" fn qi_gui_audio_stop(audio_id: i64) {
    #[cfg(has_gui)]
    {
        if audio_id <= 0 {
            return;
        }
        unsafe {
            qi_gui_audio_stop_impl(audio_id as u64);
        }
    }

    #[cfg(not(has_gui))]
    {
        let _ = audio_id;
    }
}

#[no_mangle]
pub extern "C" fn qi_gui_audio_set_volume(audio_id: i64, volume: f64) {
    #[cfg(has_gui)]
    {
        if audio_id <= 0 {
            return;
        }
        unsafe {
            qi_gui_audio_set_volume_impl(audio_id as u64, volume as f32);
        }
    }

    #[cfg(not(has_gui))]
    {
        let _ = (audio_id, volume);
    }
}

#[no_mangle]
pub extern "C" fn qi_gui_audio_is_playing(audio_id: i64) -> i64 {
    #[cfg(has_gui)]
    {
        if audio_id <= 0 {
            return 0;
        }
        unsafe {
            qi_gui_audio_is_playing_impl(audio_id as u64) as i64
        }
    }

    #[cfg(not(has_gui))]
    {
        let _ = audio_id;
        0
    }
}

#[no_mangle]
pub extern "C" fn qi_gui_audio_is_finished(audio_id: i64) -> i64 {
    #[cfg(has_gui)]
    {
        if audio_id <= 0 {
            return 0;
        }
        unsafe {
            qi_gui_audio_is_finished_impl(audio_id as u64) as i64
        }
    }

    #[cfg(not(has_gui))]
    {
        let _ = audio_id;
        0
    }
}

#[no_mangle]
pub extern "C" fn qi_gui_audio_free(audio_id: i64) {
    #[cfg(has_gui)]
    {
        if audio_id <= 0 {
            return;
        }
        unsafe {
            qi_gui_audio_free_impl(audio_id as u64);
        }
    }

    #[cfg(not(has_gui))]
    {
        let _ = audio_id;
    }
}

// Renderer wrapper functions

#[no_mangle]
pub extern "C" fn qi_gui_renderer_create(window_id: i64) -> i64 {
    #[cfg(has_gui)]
    {
        if window_id <= 0 {
            return 0;
        }
        unsafe {
            qi_gui_renderer_create_impl(window_id as u64) as i64
        }
    }

    #[cfg(not(has_gui))]
    {
        let _ = window_id;
        0
    }
}

#[no_mangle]
pub extern "C" fn qi_gui_renderer_clear(renderer_id: i64, r: i64, g: i64, b: i64) {
    #[cfg(has_gui)]
    {
        if renderer_id <= 0 {
            return;
        }
        unsafe {
            qi_gui_renderer_clear_impl(renderer_id as u64, r as u8, g as u8, b as u8);
        }
    }

    #[cfg(not(has_gui))]
    {
        let _ = (renderer_id, r, g, b);
    }
}

#[no_mangle]
pub extern "C" fn qi_gui_renderer_draw_pixel(renderer_id: i64, x: i64, y: i64, r: i64, g: i64, b: i64) {
    #[cfg(has_gui)]
    {
        if renderer_id <= 0 {
            return;
        }
        unsafe {
            qi_gui_renderer_draw_pixel_impl(renderer_id as u64, x as u32, y as u32, r as u8, g as u8, b as u8);
        }
    }

    #[cfg(not(has_gui))]
    {
        let _ = (renderer_id, x, y, r, g, b);
    }
}

#[no_mangle]
pub extern "C" fn qi_gui_renderer_draw_rect(renderer_id: i64, x: i64, y: i64, width: i64, height: i64, r: i64, g: i64, b: i64) {
    #[cfg(has_gui)]
    {
        if renderer_id <= 0 {
            return;
        }
        unsafe {
            qi_gui_renderer_draw_rect_impl(renderer_id as u64, x as u32, y as u32, width as u32, height as u32, r as u8, g as u8, b as u8);
        }
    }

    #[cfg(not(has_gui))]
    {
        let _ = (renderer_id, x, y, width, height, r, g, b);
    }
}

#[no_mangle]
pub extern "C" fn qi_gui_renderer_draw_line(renderer_id: i64, x0: i64, y0: i64, x1: i64, y1: i64, r: i64, g: i64, b: i64) {
    #[cfg(has_gui)]
    {
        if renderer_id <= 0 {
            return;
        }
        unsafe {
            qi_gui_renderer_draw_line_impl(renderer_id as u64, x0 as i32, y0 as i32, x1 as i32, y1 as i32, r as u8, g as u8, b as u8);
        }
    }

    #[cfg(not(has_gui))]
    {
        let _ = (renderer_id, x0, y0, x1, y1, r, g, b);
    }
}

#[no_mangle]
pub extern "C" fn qi_gui_renderer_draw_circle(renderer_id: i64, cx: i64, cy: i64, radius: i64, r: i64, g: i64, b: i64) {
    #[cfg(has_gui)]
    {
        if renderer_id <= 0 {
            return;
        }
        unsafe {
            qi_gui_renderer_draw_circle_impl(renderer_id as u64, cx as i32, cy as i32, radius as u32, r as u8, g as u8, b as u8);
        }
    }

    #[cfg(not(has_gui))]
    {
        let _ = (renderer_id, cx, cy, radius, r, g, b);
    }
}

#[no_mangle]
pub extern "C" fn qi_gui_renderer_draw_image(renderer_id: i64, file_path: *const c_char, x: i64, y: i64) -> i64 {
    #[cfg(has_gui)]
    {
        if renderer_id <= 0 || file_path.is_null() {
            return -1;
        }
        unsafe {
            qi_gui_renderer_draw_image_impl(renderer_id as u64, file_path, x as u32, y as u32) as i64
        }
    }

    #[cfg(not(has_gui))]
    {
        let _ = (renderer_id, file_path, x, y);
        -1
    }
}

#[no_mangle]
pub extern "C" fn qi_gui_renderer_draw_text(renderer_id: i64, text: *const c_char, x: i64, y: i64, r: i64, g: i64, b: i64) {
    #[cfg(has_gui)]
    {
        if renderer_id <= 0 || text.is_null() {
            return;
        }
        unsafe {
            qi_gui_renderer_draw_text_impl(renderer_id as u64, text, x as i32, y as i32, r as u8, g as u8, b as u8);
        }
    }

    #[cfg(not(has_gui))]
    {
        let _ = (renderer_id, text, x, y, r, g, b);
    }
}

#[no_mangle]
pub extern "C" fn qi_gui_renderer_draw_text_scaled(renderer_id: i64, text: *const c_char, x: i64, y: i64, scale: i64, r: i64, g: i64, b: i64) {
    #[cfg(has_gui)]
    {
        if renderer_id <= 0 || text.is_null() {
            return;
        }
        unsafe {
            qi_gui_renderer_draw_text_scaled_impl(renderer_id as u64, text, x as i32, y as i32, scale as u32, r as u8, g as u8, b as u8);
        }
    }

    #[cfg(not(has_gui))]
    {
        let _ = (renderer_id, text, x, y, scale, r, g, b);
    }
}

#[no_mangle]
pub extern "C" fn qi_gui_renderer_free(renderer_id: i64) {
    #[cfg(has_gui)]
    {
        if renderer_id <= 0 {
            return;
        }
        unsafe {
            qi_gui_renderer_free_impl(renderer_id as u64);
        }
    }

    #[cfg(not(has_gui))]
    {
        let _ = renderer_id;
    }
}

#[cfg(all(test, has_gui))]
mod tests {
    use super::*;
    use std::ffi::CStr;

    #[test]
    fn test_gui_available() {
        unsafe {
            let version = qi_gui_version();
            assert!(!version.is_null());
            let version_str = CStr::from_ptr(version).to_str().unwrap();
            assert!(version_str.contains("qi-gui"));
            qi_gui_free_string(version);
        }
    }
}
