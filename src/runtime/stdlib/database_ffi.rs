// 数据库模块 FFI
//
// 提供 SQLite 数据库操作、参数绑定和有所有权的事务句柄。

use rusqlite::types::{Value as SqlValue, ValueRef};
use rusqlite::{params_from_iter, Connection, Error as SqlError};
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use std::ffi::CStr;
#[cfg(not(feature = "runtime-rc-string"))]
use std::ffi::CString;
use std::os::raw::c_char;
use std::sync::{Arc, Mutex};

struct ConnectionState {
    conn: Connection,
    // 0 表示旧 API 开启的连接级事务，正数表示新事务句柄。
    active_transaction: Option<i64>,
}

lazy_static::lazy_static! {
    static ref CONNECTIONS: Mutex<HashMap<i64, Arc<Mutex<ConnectionState>>>> =
        Mutex::new(HashMap::new());
    static ref TRANSACTIONS: Mutex<HashMap<i64, i64>> = Mutex::new(HashMap::new());
    static ref NEXT_CONNECTION_ID: Mutex<i64> = Mutex::new(1);
    static ref NEXT_TRANSACTION_ID: Mutex<i64> = Mutex::new(1);
}

fn next_id(counter: &Mutex<i64>) -> i64 {
    let mut value = counter.lock().unwrap();
    let id = *value;
    *value += 1;
    id
}

fn connection(conn_id: i64) -> Option<Arc<Mutex<ConnectionState>>> {
    CONNECTIONS.lock().unwrap().get(&conn_id).cloned()
}

fn transaction(tx_id: i64) -> Option<(i64, Arc<Mutex<ConnectionState>>)> {
    let conn_id = *TRANSACTIONS.lock().unwrap().get(&tx_id)?;
    Some((conn_id, connection(conn_id)?))
}

fn c_string(value: String) -> *mut c_char {
    #[cfg(feature = "runtime-rc-string")]
    {
        return super::qi_str::rc_cstr_from_string(value);
    }
    #[cfg(not(feature = "runtime-rc-string"))]
    {
        CString::new(value)
            .map(CString::into_raw)
            .unwrap_or(std::ptr::null_mut())
    }
}

fn write_success(rows: usize, last_insert_id: i64) -> JsonValue {
    json!({
        "成功": 1,
        "影响行数": rows as i64,
        "最后插入id": last_insert_id,
        "错误码": 0,
        "错误": ""
    })
}

fn sqlite_error_code(error: &SqlError) -> i32 {
    error
        .sqlite_error()
        .map(|info| info.extended_code)
        .unwrap_or(-1)
}

fn write_failure(message: impl Into<String>, code: i32) -> JsonValue {
    json!({
        "成功": 0,
        "影响行数": 0,
        "最后插入id": 0,
        "错误码": code,
        "错误": message.into()
    })
}

fn write_result_json(result: Result<(usize, i64), SqlError>) -> *mut c_char {
    let value = match result {
        Ok((rows, last_insert_id)) => write_success(rows, last_insert_id),
        Err(error) => write_failure(error.to_string(), sqlite_error_code(&error)),
    };
    c_string(value.to_string())
}

fn parse_params(params_json: &str) -> Result<Vec<SqlValue>, String> {
    let value: JsonValue =
        serde_json::from_str(params_json).map_err(|error| format!("参数 JSON 无效: {error}"))?;
    let values = value
        .as_array()
        .ok_or_else(|| "参数 JSON 必须是数组".to_string())?;

    values
        .iter()
        .map(|value| match value {
            JsonValue::Null => Ok(SqlValue::Null),
            JsonValue::Bool(value) => Ok(SqlValue::Integer(i64::from(*value))),
            JsonValue::Number(value) => {
                if let Some(integer) = value.as_i64() {
                    Ok(SqlValue::Integer(integer))
                } else if let Some(float) = value.as_f64() {
                    Ok(SqlValue::Real(float))
                } else {
                    Err("参数数字超出 SQLite 支持范围".to_string())
                }
            }
            JsonValue::String(value) => Ok(SqlValue::Text(value.clone())),
            JsonValue::Array(_) | JsonValue::Object(_) => {
                Err("参数只支持空值、布尔、整数、浮点和文本".to_string())
            }
        })
        .collect()
}

fn row_value(value: ValueRef<'_>) -> JsonValue {
    match value {
        ValueRef::Null => JsonValue::Null,
        ValueRef::Integer(value) => json!(value),
        ValueRef::Real(value) => json!(value),
        ValueRef::Text(value) => json!(String::from_utf8_lossy(value)),
        ValueRef::Blob(value) => json!(value),
    }
}

fn query_json(conn: &Connection, sql: &str, params: &[SqlValue]) -> Result<String, SqlError> {
    let mut stmt = conn.prepare(sql)?;
    let column_count = stmt.column_count();
    let column_names: Vec<String> = (0..column_count)
        .map(|index| stmt.column_name(index).unwrap_or("").to_string())
        .collect();
    let mut rows = stmt.query(params_from_iter(params.iter()))?;
    let mut results = Vec::new();

    while let Some(row) = rows.next()? {
        let mut row_map = serde_json::Map::new();
        for (index, column_name) in column_names.iter().enumerate() {
            row_map.insert(column_name.clone(), row_value(row.get_ref(index)?));
        }
        results.push(JsonValue::Object(row_map));
    }

    Ok(JsonValue::Array(results).to_string())
}

fn execute_parameterized(
    state: &ConnectionState,
    sql: &str,
    params: &[SqlValue],
) -> Result<(usize, i64), SqlError> {
    let rows = state.conn.execute(sql, params_from_iter(params.iter()))?;
    Ok((rows, state.conn.last_insert_rowid()))
}

unsafe fn ffi_text<'a>(value: *const c_char) -> Option<std::borrow::Cow<'a, str>> {
    if value.is_null() {
        return None;
    }
    Some(CStr::from_ptr(value).to_string_lossy())
}

/// 连接数据库
#[no_mangle]
pub extern "C" fn qi_db_connect(path: *const c_char) -> i64 {
    let Some(path) = (unsafe { ffi_text(path) }) else {
        return -1;
    };

    match Connection::open(path.as_ref()) {
        Ok(conn) => {
            let id = next_id(&NEXT_CONNECTION_ID);
            CONNECTIONS.lock().unwrap().insert(
                id,
                Arc::new(Mutex::new(ConnectionState {
                    conn,
                    active_transaction: None,
                })),
            );
            id
        }
        Err(_) => -1,
    }
}

/// 执行 SQL（旧 API，保留兼容）
#[no_mangle]
pub extern "C" fn qi_db_execute(conn_id: i64, sql: *const c_char) -> i64 {
    let Some(sql) = (unsafe { ffi_text(sql) }) else {
        return -1;
    };
    let Some(connection) = connection(conn_id) else {
        return -1;
    };
    let state = connection.lock().unwrap();
    if matches!(state.active_transaction, Some(tx_id) if tx_id > 0) {
        return -1;
    }
    state
        .conn
        .execute(sql.as_ref(), [])
        .map(|rows| rows as i64)
        .unwrap_or(-1)
}

/// 参数化执行，参数是 JSON 数组；返回结构化写结果 JSON。
#[no_mangle]
pub extern "C" fn qi_db_execute_params(
    conn_id: i64,
    sql: *const c_char,
    params_json: *const c_char,
) -> *mut c_char {
    let Some(sql) = (unsafe { ffi_text(sql) }) else {
        return c_string(write_failure("SQL 为空", -1).to_string());
    };
    let Some(params_json) = (unsafe { ffi_text(params_json) }) else {
        return c_string(write_failure("参数 JSON 为空", -1).to_string());
    };
    let params = match parse_params(params_json.as_ref()) {
        Ok(params) => params,
        Err(error) => return c_string(write_failure(error, -1).to_string()),
    };
    let Some(connection) = connection(conn_id) else {
        return c_string(write_failure("数据库连接无效", -1).to_string());
    };
    let state = connection.lock().unwrap();
    if state.active_transaction.is_some() {
        return c_string(write_failure("数据库连接正被事务占用", -1).to_string());
    }
    write_result_json(execute_parameterized(&state, sql.as_ref(), &params))
}

/// 查询 SQL（旧 API，保留兼容）
#[no_mangle]
pub extern "C" fn qi_db_query(conn_id: i64, sql: *const c_char) -> *mut c_char {
    let Some(sql) = (unsafe { ffi_text(sql) }) else {
        return std::ptr::null_mut();
    };
    let Some(connection) = connection(conn_id) else {
        return std::ptr::null_mut();
    };
    let state = connection.lock().unwrap();
    if matches!(state.active_transaction, Some(tx_id) if tx_id > 0) {
        return std::ptr::null_mut();
    }
    query_json(&state.conn, sql.as_ref(), &[])
        .map(c_string)
        .unwrap_or(std::ptr::null_mut())
}

/// 参数化查询，参数是 JSON 数组；成功返回 JSON 行数组，失败返回空指针。
#[no_mangle]
pub extern "C" fn qi_db_query_params(
    conn_id: i64,
    sql: *const c_char,
    params_json: *const c_char,
) -> *mut c_char {
    let Some(sql) = (unsafe { ffi_text(sql) }) else {
        return std::ptr::null_mut();
    };
    let Some(params_json) = (unsafe { ffi_text(params_json) }) else {
        return std::ptr::null_mut();
    };
    let Ok(params) = parse_params(params_json.as_ref()) else {
        return std::ptr::null_mut();
    };
    let Some(connection) = connection(conn_id) else {
        return std::ptr::null_mut();
    };
    let state = connection.lock().unwrap();
    if state.active_transaction.is_some() {
        return std::ptr::null_mut();
    }
    query_json(&state.conn, sql.as_ref(), &params)
        .map(c_string)
        .unwrap_or(std::ptr::null_mut())
}

/// 关闭数据库连接；有活动事务时先回滚并令事务句柄失效。
#[no_mangle]
pub extern "C" fn qi_db_close(conn_id: i64) -> i32 {
    let connection = CONNECTIONS.lock().unwrap().remove(&conn_id);
    let Some(connection) = connection else {
        return -1;
    };
    let active_transaction = {
        let mut state = connection.lock().unwrap();
        if state.active_transaction.is_some() {
            let _ = state.conn.execute("ROLLBACK", []);
        }
        state.active_transaction.take()
    };
    if let Some(tx_id) = active_transaction.filter(|tx_id| *tx_id > 0) {
        TRANSACTIONS.lock().unwrap().remove(&tx_id);
    }
    0
}

/// 开始连接级事务（旧 API，保留兼容）。
#[no_mangle]
pub extern "C" fn qi_db_begin_transaction(conn_id: i64) -> i32 {
    let Some(connection) = connection(conn_id) else {
        return -1;
    };
    let mut state = connection.lock().unwrap();
    if state.active_transaction.is_some() {
        return -1;
    }
    match state.conn.execute("BEGIN TRANSACTION", []) {
        Ok(_) => {
            state.active_transaction = Some(0);
            0
        }
        Err(_) => -1,
    }
}

/// 提交连接级事务（旧 API，保留兼容）。
#[no_mangle]
pub extern "C" fn qi_db_commit(conn_id: i64) -> i32 {
    let Some(connection) = connection(conn_id) else {
        return -1;
    };
    let mut state = connection.lock().unwrap();
    if state.active_transaction != Some(0) {
        return -1;
    }
    match state.conn.execute("COMMIT", []) {
        Ok(_) => {
            state.active_transaction = None;
            0
        }
        Err(_) => -1,
    }
}

/// 回滚连接级事务（旧 API，保留兼容）。
#[no_mangle]
pub extern "C" fn qi_db_rollback(conn_id: i64) -> i32 {
    let Some(connection) = connection(conn_id) else {
        return -1;
    };
    let mut state = connection.lock().unwrap();
    if state.active_transaction != Some(0) {
        return -1;
    }
    match state.conn.execute("ROLLBACK", []) {
        Ok(_) => {
            state.active_transaction = None;
            0
        }
        Err(_) => -1,
    }
}

/// 开启独占事务，返回正数事务句柄。
#[no_mangle]
pub extern "C" fn qi_db_transaction_open(conn_id: i64) -> i64 {
    let Some(connection) = connection(conn_id) else {
        return -1;
    };
    let mut state = connection.lock().unwrap();
    if state.active_transaction.is_some() {
        return -1;
    }
    if state.conn.execute("BEGIN IMMEDIATE", []).is_err() {
        return -1;
    }
    let tx_id = next_id(&NEXT_TRANSACTION_ID);
    state.active_transaction = Some(tx_id);
    drop(state);
    TRANSACTIONS.lock().unwrap().insert(tx_id, conn_id);
    tx_id
}

/// 在独占事务中参数化执行。
#[no_mangle]
pub extern "C" fn qi_db_transaction_execute_params(
    tx_id: i64,
    sql: *const c_char,
    params_json: *const c_char,
) -> *mut c_char {
    let Some(sql) = (unsafe { ffi_text(sql) }) else {
        return c_string(write_failure("SQL 为空", -1).to_string());
    };
    let Some(params_json) = (unsafe { ffi_text(params_json) }) else {
        return c_string(write_failure("参数 JSON 为空", -1).to_string());
    };
    let params = match parse_params(params_json.as_ref()) {
        Ok(params) => params,
        Err(error) => return c_string(write_failure(error, -1).to_string()),
    };
    let Some((_, connection)) = transaction(tx_id) else {
        return c_string(write_failure("事务句柄无效", -1).to_string());
    };
    let state = connection.lock().unwrap();
    if state.active_transaction != Some(tx_id) {
        return c_string(write_failure("事务已结束", -1).to_string());
    }
    write_result_json(execute_parameterized(&state, sql.as_ref(), &params))
}

/// 在独占事务中参数化查询。
#[no_mangle]
pub extern "C" fn qi_db_transaction_query_params(
    tx_id: i64,
    sql: *const c_char,
    params_json: *const c_char,
) -> *mut c_char {
    let Some(sql) = (unsafe { ffi_text(sql) }) else {
        return std::ptr::null_mut();
    };
    let Some(params_json) = (unsafe { ffi_text(params_json) }) else {
        return std::ptr::null_mut();
    };
    let Ok(params) = parse_params(params_json.as_ref()) else {
        return std::ptr::null_mut();
    };
    let Some((_, connection)) = transaction(tx_id) else {
        return std::ptr::null_mut();
    };
    let state = connection.lock().unwrap();
    if state.active_transaction != Some(tx_id) {
        return std::ptr::null_mut();
    }
    query_json(&state.conn, sql.as_ref(), &params)
        .map(c_string)
        .unwrap_or(std::ptr::null_mut())
}

fn finish_transaction(tx_id: i64, sql: &str) -> i32 {
    let Some((_, connection)) = transaction(tx_id) else {
        return -1;
    };
    let result = {
        let mut state = connection.lock().unwrap();
        if state.active_transaction != Some(tx_id) {
            return -1;
        }
        match state.conn.execute(sql, []) {
            Ok(_) => {
                state.active_transaction = None;
                0
            }
            Err(_) => -1,
        }
    };
    if result == 0 {
        TRANSACTIONS.lock().unwrap().remove(&tx_id);
    }
    result
}

#[no_mangle]
pub extern "C" fn qi_db_transaction_commit(tx_id: i64) -> i32 {
    finish_transaction(tx_id, "COMMIT")
}

#[no_mangle]
pub extern "C" fn qi_db_transaction_rollback(tx_id: i64) -> i32 {
    finish_transaction(tx_id, "ROLLBACK")
}

/// 释放字符串
#[no_mangle]
pub extern "C" fn qi_db_free_string(value: *mut c_char) {
    #[cfg(feature = "runtime-rc-string")]
    {
        super::qi_str::rc_cstr_release(value);
    }
    #[cfg(not(feature = "runtime-rc-string"))]
    if !value.is_null() {
        unsafe {
            let _ = CString::from_raw(value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    fn text(value: &str) -> CString {
        CString::new(value).unwrap()
    }

    unsafe fn take_string(value: *mut c_char) -> String {
        assert!(!value.is_null());
        let result = CStr::from_ptr(value).to_string_lossy().into_owned();
        qi_db_free_string(value);
        result
    }

    #[test]
    fn database_operations_remain_compatible() {
        let path = text(":memory:");
        let conn_id = qi_db_connect(path.as_ptr());
        assert!(conn_id > 0);
        assert_eq!(
            qi_db_execute(
                conn_id,
                text("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, age INTEGER)")
                    .as_ptr()
            ),
            0
        );
        assert_eq!(
            qi_db_execute(
                conn_id,
                text("INSERT INTO users (name, age) VALUES ('Alice', 30)").as_ptr()
            ),
            1
        );
        let result = qi_db_query(conn_id, text("SELECT * FROM users").as_ptr());
        assert!(unsafe { take_string(result) }.contains("Alice"));
        assert_eq!(qi_db_close(conn_id), 0);
    }

    #[test]
    fn parameterized_queries_and_write_results_work() {
        let conn_id = qi_db_connect(text(":memory:").as_ptr());
        qi_db_execute(
            conn_id,
            text("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, active INTEGER)").as_ptr(),
        );
        let write = qi_db_execute_params(
            conn_id,
            text("INSERT INTO users (name, active) VALUES (?, ?)").as_ptr(),
            text(r#"["O'Reilly",true]"#).as_ptr(),
        );
        let write_json: JsonValue = serde_json::from_str(&unsafe { take_string(write) }).unwrap();
        assert_eq!(write_json["成功"], 1);
        assert_eq!(write_json["影响行数"], 1);
        assert_eq!(write_json["最后插入id"], 1);

        let rows = qi_db_query_params(
            conn_id,
            text("SELECT name, active FROM users WHERE name = ?").as_ptr(),
            text(r#"["O'Reilly"]"#).as_ptr(),
        );
        let rows_json: JsonValue = serde_json::from_str(&unsafe { take_string(rows) }).unwrap();
        assert_eq!(rows_json[0]["name"], "O'Reilly");
        assert_eq!(rows_json[0]["active"], 1);
        qi_db_close(conn_id);
    }

    #[test]
    fn transaction_handles_are_isolated_and_rollback() {
        let conn_id = qi_db_connect(text(":memory:").as_ptr());
        qi_db_execute(
            conn_id,
            text("CREATE TABLE items (id INTEGER PRIMARY KEY, quantity INTEGER)").as_ptr(),
        );
        qi_db_execute(
            conn_id,
            text("INSERT INTO items (quantity) VALUES (3)").as_ptr(),
        );

        let tx_id = qi_db_transaction_open(conn_id);
        assert!(tx_id > 0);
        assert_eq!(
            qi_db_execute(conn_id, text("UPDATE items SET quantity = 99").as_ptr()),
            -1
        );
        let result = qi_db_transaction_execute_params(
            tx_id,
            text("UPDATE items SET quantity = quantity - ? WHERE id = ? AND quantity >= ?")
                .as_ptr(),
            text("[2,1,2]").as_ptr(),
        );
        let result_json: JsonValue = serde_json::from_str(&unsafe { take_string(result) }).unwrap();
        assert_eq!(result_json["影响行数"], 1);
        assert_eq!(qi_db_transaction_rollback(tx_id), 0);

        let rows = qi_db_query_params(
            conn_id,
            text("SELECT quantity FROM items WHERE id = ?").as_ptr(),
            text("[1]").as_ptr(),
        );
        let rows_json: JsonValue = serde_json::from_str(&unsafe { take_string(rows) }).unwrap();
        assert_eq!(rows_json[0]["quantity"], 3);
        assert_eq!(qi_db_transaction_commit(tx_id), -1);
        qi_db_close(conn_id);
    }
}
