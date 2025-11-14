//! 日期时间模块 (DateTime Module)
//!
//! 提供日期时间处理功能，支持格式化、解析和计算
//! Provides datetime handling with formatting, parsing, and calculations

use chrono::{DateTime, Datelike, Local, NaiveDate, NaiveDateTime, Timelike, Utc};
use std::ffi::{CStr, CString};
use std::os::raw::c_char;

// ============================================================================
// 当前时间 (Current Time)
// ============================================================================

/// 获取当前 Unix 时间戳（秒）
#[no_mangle]
pub extern "C" fn qi_datetime_now() -> i64 {
    Utc::now().timestamp()
}

/// 获取当前 Unix 时间戳（毫秒）
#[no_mangle]
pub extern "C" fn qi_datetime_now_millis() -> i64 {
    Utc::now().timestamp_millis()
}

/// 获取当前本地时间戳
#[no_mangle]
pub extern "C" fn qi_datetime_now_local() -> i64 {
    Local::now().timestamp()
}

// ============================================================================
// 格式化 (Formatting)
// ============================================================================

/// 格式化 Unix 时间戳为字符串（UTC）
/// format: "%Y-%m-%d %H:%M:%S", "%Y年%m月%d日", etc.
#[no_mangle]
pub extern "C" fn qi_datetime_format(timestamp: i64, format: *const c_char) -> *mut c_char {
    if format.is_null() {
        return std::ptr::null_mut();
    }

    let format_str = unsafe {
        match CStr::from_ptr(format).to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        }
    };

    let dt = match DateTime::from_timestamp(timestamp, 0) {
        Some(dt) => dt,
        None => return std::ptr::null_mut(),
    };

    let formatted = dt.format(format_str).to_string();
    CString::new(formatted).unwrap().into_raw()
}

/// 格式化 Unix 时间戳为字符串（本地时间）
#[no_mangle]
pub extern "C" fn qi_datetime_format_local(timestamp: i64, format: *const c_char) -> *mut c_char {
    if format.is_null() {
        return std::ptr::null_mut();
    }

    let format_str = unsafe {
        match CStr::from_ptr(format).to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        }
    };

    let dt = match DateTime::from_timestamp(timestamp, 0) {
        Some(dt) => dt.with_timezone(&Local),
        None => return std::ptr::null_mut(),
    };

    let formatted = dt.format(format_str).to_string();
    CString::new(formatted).unwrap().into_raw()
}

// ============================================================================
// 解析 (Parsing)
// ============================================================================

/// 解析日期时间字符串为 Unix 时间戳
/// format: "%Y-%m-%d %H:%M:%S"
/// datetime_str: "2024-01-15 14:30:00"
#[no_mangle]
pub extern "C" fn qi_datetime_parse(
    datetime_str: *const c_char,
    format: *const c_char,
) -> i64 {
    if datetime_str.is_null() || format.is_null() {
        return 0;
    }

    let dt_str = unsafe {
        match CStr::from_ptr(datetime_str).to_str() {
            Ok(s) => s,
            Err(_) => return 0,
        }
    };

    let fmt_str = unsafe {
        match CStr::from_ptr(format).to_str() {
            Ok(s) => s,
            Err(_) => return 0,
        }
    };

    match NaiveDateTime::parse_from_str(dt_str, fmt_str) {
        Ok(ndt) => ndt.and_utc().timestamp(),
        Err(_) => 0,
    }
}

// ============================================================================
// 日期组件 (Date Components)
// ============================================================================

/// 获取年份
#[no_mangle]
pub extern "C" fn qi_datetime_year(timestamp: i64) -> i64 {
    match DateTime::from_timestamp(timestamp, 0) {
        Some(dt) => dt.year() as i64,
        None => 0,
    }
}

/// 获取月份（1-12）
#[no_mangle]
pub extern "C" fn qi_datetime_month(timestamp: i64) -> i64 {
    match DateTime::from_timestamp(timestamp, 0) {
        Some(dt) => dt.month() as i64,
        None => 0,
    }
}

/// 获取日期（1-31）
#[no_mangle]
pub extern "C" fn qi_datetime_day(timestamp: i64) -> i64 {
    match DateTime::from_timestamp(timestamp, 0) {
        Some(dt) => dt.day() as i64,
        None => 0,
    }
}

/// 获取小时（0-23）
#[no_mangle]
pub extern "C" fn qi_datetime_hour(timestamp: i64) -> i64 {
    match DateTime::from_timestamp(timestamp, 0) {
        Some(dt) => dt.hour() as i64,
        None => 0,
    }
}

/// 获取分钟（0-59）
#[no_mangle]
pub extern "C" fn qi_datetime_minute(timestamp: i64) -> i64 {
    match DateTime::from_timestamp(timestamp, 0) {
        Some(dt) => dt.minute() as i64,
        None => 0,
    }
}

/// 获取秒（0-59）
#[no_mangle]
pub extern "C" fn qi_datetime_second(timestamp: i64) -> i64 {
    match DateTime::from_timestamp(timestamp, 0) {
        Some(dt) => dt.second() as i64,
        None => 0,
    }
}

/// 获取星期几（1=周一, 7=周日）
#[no_mangle]
pub extern "C" fn qi_datetime_weekday(timestamp: i64) -> i64 {
    match DateTime::from_timestamp(timestamp, 0) {
        Some(dt) => dt.weekday().num_days_from_monday() as i64 + 1,
        None => 0,
    }
}

// ============================================================================
// 日期计算 (Date Calculations)
// ============================================================================

/// 添加秒数
#[no_mangle]
pub extern "C" fn qi_datetime_add_seconds(timestamp: i64, seconds: i64) -> i64 {
    timestamp + seconds
}

/// 添加分钟数
#[no_mangle]
pub extern "C" fn qi_datetime_add_minutes(timestamp: i64, minutes: i64) -> i64 {
    timestamp + (minutes * 60)
}

/// 添加小时数
#[no_mangle]
pub extern "C" fn qi_datetime_add_hours(timestamp: i64, hours: i64) -> i64 {
    timestamp + (hours * 3600)
}

/// 添加天数
#[no_mangle]
pub extern "C" fn qi_datetime_add_days(timestamp: i64, days: i64) -> i64 {
    timestamp + (days * 86400)
}

/// 计算两个时间戳之间的天数差
#[no_mangle]
pub extern "C" fn qi_datetime_diff_days(timestamp1: i64, timestamp2: i64) -> i64 {
    (timestamp1 - timestamp2) / 86400
}

/// 计算两个时间戳之间的小时数差
#[no_mangle]
pub extern "C" fn qi_datetime_diff_hours(timestamp1: i64, timestamp2: i64) -> i64 {
    (timestamp1 - timestamp2) / 3600
}

/// 计算两个时间戳之间的分钟数差
#[no_mangle]
pub extern "C" fn qi_datetime_diff_minutes(timestamp1: i64, timestamp2: i64) -> i64 {
    (timestamp1 - timestamp2) / 60
}

/// 计算两个时间戳之间的秒数差
#[no_mangle]
pub extern "C" fn qi_datetime_diff_seconds(timestamp1: i64, timestamp2: i64) -> i64 {
    timestamp1 - timestamp2
}

// ============================================================================
// 日期创建 (Date Creation)
// ============================================================================

/// 从年月日创建日期时间戳（UTC，时分秒为 0）
#[no_mangle]
pub extern "C" fn qi_datetime_from_ymd(year: i64, month: i64, day: i64) -> i64 {
    match NaiveDate::from_ymd_opt(year as i32, month as u32, day as u32) {
        Some(date) => date.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp(),
        None => 0,
    }
}

/// 从年月日时分秒创建时间戳（UTC）
#[no_mangle]
pub extern "C" fn qi_datetime_from_ymdhms(
    year: i64,
    month: i64,
    day: i64,
    hour: i64,
    minute: i64,
    second: i64,
) -> i64 {
    match NaiveDate::from_ymd_opt(year as i32, month as u32, day as u32) {
        Some(date) => match date.and_hms_opt(hour as u32, minute as u32, second as u32) {
            Some(dt) => dt.and_utc().timestamp(),
            None => 0,
        },
        None => 0,
    }
}

// ============================================================================
// 工具函数 (Utility Functions)
// ============================================================================

/// 检查是否为闰年
#[no_mangle]
pub extern "C" fn qi_datetime_is_leap_year(year: i64) -> i64 {
    let year = year as i32;
    if (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0) {
        1
    } else {
        0
    }
}

/// 获取某月的天数
#[no_mangle]
pub extern "C" fn qi_datetime_days_in_month(year: i64, month: i64) -> i64 {
    if month < 1 || month > 12 {
        return 0;
    }

    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if qi_datetime_is_leap_year(year) == 1 {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

/// 释放字符串
#[no_mangle]
pub extern "C" fn qi_datetime_free_string(s: *mut c_char) {
    if s.is_null() {
        return;
    }
    unsafe {
        let _ = CString::from_raw(s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_datetime_now() {
        let now = qi_datetime_now();
        assert!(now > 0);
    }

    #[test]
    fn test_datetime_format() {
        let timestamp = 1704556800; // 2024-01-06 16:00:00 UTC
        let format = CString::new("%Y-%m-%d").unwrap();
        let result = qi_datetime_format(timestamp, format.as_ptr());
        assert!(!result.is_null());

        let result_str = unsafe { CStr::from_ptr(result).to_str().unwrap() };
        assert_eq!(result_str, "2024-01-06");

        qi_datetime_free_string(result);
    }

    #[test]
    fn test_datetime_components() {
        let timestamp = 1704556800; // 2024-01-06 16:00:00 UTC
        assert_eq!(qi_datetime_year(timestamp), 2024);
        assert_eq!(qi_datetime_month(timestamp), 1);
        assert_eq!(qi_datetime_day(timestamp), 6);
        assert_eq!(qi_datetime_hour(timestamp), 16);
    }

    #[test]
    fn test_leap_year() {
        assert_eq!(qi_datetime_is_leap_year(2024), 1);
        assert_eq!(qi_datetime_is_leap_year(2023), 0);
        assert_eq!(qi_datetime_is_leap_year(2000), 1);
        assert_eq!(qi_datetime_is_leap_year(1900), 0);
    }
}
