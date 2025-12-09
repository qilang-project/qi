//! 网络模块 FFI 接口
//!
//! 为 Qi 语言提供 C 接口的网络操作函数（TCP、UDP 等）

#![allow(non_snake_case)]

use super::http::{TcpConnectionConfig, TcpConnection, NetworkInterface};
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::time::Duration;
use std::sync::Mutex;
use std::collections::HashMap;

// 全局网络接口实例
use std::sync::OnceLock;
static 全局网络接口: OnceLock<NetworkInterface> = OnceLock::new();

// TCP 连接池和句柄计数器（使用全局静态变量）
static TCP连接池: OnceLock<Mutex<HashMap<i64, TcpConnection>>> = OnceLock::new();
static 连接句柄计数器: OnceLock<Mutex<i64>> = OnceLock::new();

fn 获取网络接口() -> Option<&'static NetworkInterface> {
    全局网络接口.get()
}

fn 初始化网络接口() {
    全局网络接口.get_or_init(|| {
        NetworkInterface::new().unwrap_or_else(|_| {
            panic!("Failed to initialize network interface")
        })
    });
}

fn 获取连接池() -> &'static Mutex<HashMap<i64, TcpConnection>> {
    TCP连接池.get_or_init(|| Mutex::new(HashMap::new()))
}

fn 获取句柄计数器() -> &'static Mutex<i64> {
    连接句柄计数器.get_or_init(|| Mutex::new(0))
}

/// 从TCP连接池中取出连接并返回TcpStream（用于WebSocket升级）
/// 这将从池中移除连接，调用者获得TcpStream的所有权
pub(crate) fn 取出TCP流(handle: i64) -> Option<std::net::TcpStream> {
    let mut 连接池 = 获取连接池().lock().unwrap();
    连接池.remove(&handle).map(|conn| conn.into_stream())
}

/// 克隆TCP连接的流（保留原连接在池中）
pub(crate) fn 克隆TCP流(handle: i64) -> Option<std::net::TcpStream> {
    let 连接池 = 获取连接池().lock().unwrap();
    if let Some(conn) = 连接池.get(&handle) {
        conn.try_clone_stream().ok()
    } else {
        None
    }
}

/// 初始化网络模块
#[no_mangle]
pub extern "C" fn qi_network_init() {
    初始化网络接口();
}

/// TCP 连接到指定地址和端口
/// 返回连接句柄（>0 成功，<0 失败）
#[no_mangle]
pub extern "C" fn qi_network_tcp_connect(host: *const c_char, port: u16, timeout_ms: i64) -> i64 {
    if host.is_null() {
        return -1;
    }

    // 确保网络接口已初始化
    if 获取网络接口().is_none() {
        初始化网络接口();
    }

    unsafe {
        let 主机 = CStr::from_ptr(host).to_string_lossy().to_string();
        let mut 配置 = TcpConnectionConfig::new(主机.clone(), port);

        if timeout_ms > 0 {
            配置 = 配置.with_timeout(Duration::from_millis(timeout_ms as u64));
        }

        match TcpConnection::connect(配置) {
            Ok(连接) => {
                let mut 句柄计数 = 获取句柄计数器().lock().unwrap();
                *句柄计数 += 1;
                let 句柄 = *句柄计数;

                let mut 连接池 = 获取连接池().lock().unwrap();
                连接池.insert(句柄, 连接);

                句柄
            }
            Err(_) => -1,
        }
    }
}

/// 从 TCP 连接读取数据
/// 返回实际读取的字节数（<0 表示错误）
#[no_mangle]
pub extern "C" fn qi_network_tcp_read(handle: i64, buffer: *mut u8, buffer_size: i64) -> i64 {
    if buffer.is_null() || buffer_size <= 0 {
        return -1;
    }

    let mut 连接池 = 获取连接池().lock().unwrap();
    if let Some(连接) = 连接池.get_mut(&handle) {
        let 缓冲区 = unsafe { std::slice::from_raw_parts_mut(buffer, buffer_size as usize) };
        match 连接.read(缓冲区) {
            Ok(字节数) => 字节数 as i64,
            Err(_) => -1,
        }
    } else {
        -1
    }
}

/// 向 TCP 连接写入数据
/// 返回实际写入的字节数（<0 表示错误）
#[no_mangle]
pub extern "C" fn qi_network_tcp_write(handle: i64, data: *const u8, data_size: i64) -> i64 {
    if data.is_null() || data_size <= 0 {
        return -1;
    }

    let mut 连接池 = 获取连接池().lock().unwrap();
    if let Some(连接) = 连接池.get_mut(&handle) {
        let 数据 = unsafe { std::slice::from_raw_parts(data, data_size as usize) };
        match 连接.write(数据) {
            Ok(字节数) => 字节数 as i64,
            Err(_) => -1,
        }
    } else {
        -1
    }
}

/// 从 TCP 连接读取数据并返回为字符串（高级版本）
/// 返回接收到的数据字符串，失败返回空字符串
#[no_mangle]
pub extern "C" fn qi_network_tcp_read_string(handle: i64, buffer_size: i64) -> *mut c_char {
    if buffer_size <= 0 {
        return CString::new("").unwrap().into_raw();
    }

    let mut 缓冲区 = vec![0u8; buffer_size as usize];
    let mut 连接池 = 获取连接池().lock().unwrap();

    if let Some(连接) = 连接池.get_mut(&handle) {
        match 连接.read(&mut 缓冲区) {
            Ok(size) => {
                if size > 0 {
                    if let Ok(字符串) = String::from_utf8(缓冲区[..size].to_vec()) {
                        if let Ok(c_str) = CString::new(字符串) {
                            return c_str.into_raw();
                        }
                    }
                    // 如果不是有效的 UTF-8，尝试 lossy 转换
                    let 字符串 = String::from_utf8_lossy(&缓冲区[..size]).to_string();
                    if let Ok(c_str) = CString::new(字符串) {
                        return c_str.into_raw();
                    }
                }
            }
            Err(_) => {}
        }
    }

    CString::new("").unwrap().into_raw()
}

/// 向 TCP 连接写入字符串数据（高级版本）
/// 返回写入的字节数（<0 表示错误）
#[no_mangle]
pub extern "C" fn qi_network_tcp_write_string(handle: i64, data: *const c_char) -> i64 {
    if data.is_null() {
        return -1;
    }

    unsafe {
        let 数据字符串 = CStr::from_ptr(data).to_string_lossy();
        let 数据字节 = 数据字符串.as_bytes();

        let mut 连接池 = 获取连接池().lock().unwrap();
        if let Some(连接) = 连接池.get_mut(&handle) {
            match 连接.write(数据字节) {
                Ok(字节数) => {
                    // 写入后刷新确保数据发送
                    let _ = 连接.flush();
                    字节数 as i64
                }
                Err(_) => -1,
            }
        } else {
            -1
        }
    }
}

/// 关闭 TCP 连接
/// 返回 1 成功，0 失败
#[no_mangle]
pub extern "C" fn qi_network_tcp_close(handle: i64) -> i64 {
    let mut 连接池 = 获取连接池().lock().unwrap();
    if 连接池.remove(&handle).is_some() {
        1
    } else {
        0
    }
}

/// TCP 刷新缓冲区
/// 返回 1 成功，0 失败
#[no_mangle]
pub extern "C" fn qi_network_tcp_flush(handle: i64) -> i64 {
    let mut 连接池 = 获取连接池().lock().unwrap();
    if let Some(连接) = 连接池.get_mut(&handle) {
        match 连接.flush() {
            Ok(_) => 1,
            Err(_) => 0,
        }
    } else {
        0
    }
}

/// 获取 TCP 连接已读取的字节数
#[no_mangle]
pub extern "C" fn qi_network_tcp_bytes_read(handle: i64) -> i64 {
    let 连接池 = 获取连接池().lock().unwrap();
    if let Some(连接) = 连接池.get(&handle) {
        连接.bytes_read() as i64
    } else {
        -1
    }
}

/// 获取 TCP 连接已写入的字节数
#[no_mangle]
pub extern "C" fn qi_network_tcp_bytes_written(handle: i64) -> i64 {
    let 连接池 = 获取连接池().lock().unwrap();
    if let Some(连接) = 连接池.get(&handle) {
        连接.bytes_written() as i64
    } else {
        -1
    }
}

/// 解析域名到 IP 地址
/// 返回 IP 地址字符串（需要调用 qi_network_free_string 释放）
#[no_mangle]
pub extern "C" fn qi_network_resolve_host(host: *const c_char) -> *mut c_char {
    if host.is_null() {
        return std::ptr::null_mut();
    }

    unsafe {
        let 主机名 = CStr::from_ptr(host).to_string_lossy().to_string();

        // 尝试解析为 socket 地址
        use std::net::ToSocketAddrs;
        let 地址字符串 = format!("{}:0", 主机名);

        match 地址字符串.to_socket_addrs() {
            Ok(mut 地址列表) => {
                if let Some(地址) = 地址列表.next() {
                    let ip字符串 = 地址.ip().to_string();
                    CString::new(ip字符串).unwrap().into_raw()
                } else {
                    std::ptr::null_mut()
                }
            }
            Err(_) => std::ptr::null_mut(),
        }
    }
}

/// 检查端口是否可用
/// 返回 1 可用，0 不可用
#[no_mangle]
pub extern "C" fn qi_network_port_available(port: u16) -> i64 {
    use std::net::TcpListener;

    match TcpListener::bind(("127.0.0.1", port)) {
        Ok(_) => 1,
        Err(_) => 0,
    }
}

/// 获取本机 IP 地址
/// 返回 IP 地址字符串（需要调用 qi_network_free_string 释放）
#[no_mangle]
pub extern "C" fn qi_network_get_local_ip() -> *mut c_char {
    use std::net::UdpSocket;

    // 使用 UDP 连接到外部地址获取本机 IP
    match UdpSocket::bind("0.0.0.0:0") {
        Ok(socket) => {
            match socket.connect("8.8.8.8:80") {
                Ok(_) => {
                    match socket.local_addr() {
                        Ok(addr) => {
                            let ip = addr.ip().to_string();
                            CString::new(ip).unwrap().into_raw()
                        }
                        Err(_) => {
                            CString::new("127.0.0.1").unwrap().into_raw()
                        }
                    }
                }
                Err(_) => {
                    CString::new("127.0.0.1").unwrap().into_raw()
                }
            }
        }
        Err(_) => {
            CString::new("127.0.0.1").unwrap().into_raw()
        }
    }
}

/// 释放网络模块分配的字符串内存
#[no_mangle]
pub extern "C" fn qi_network_free_string(s: *mut c_char) {
    if !s.is_null() {
        unsafe {
            let _ = CString::from_raw(s);
        }
    }
}

// ============================================================================
// TCP 服务器功能
// ============================================================================

use std::net::TcpListener;
use std::sync::Arc;

// TCP 服务器监听器池
static TCP服务器池: OnceLock<Mutex<HashMap<i64, Arc<TcpListener>>>> = OnceLock::new();

fn 获取服务器池() -> &'static Mutex<HashMap<i64, Arc<TcpListener>>> {
    TCP服务器池.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 创建 TCP 服务器监听指定端口
/// 返回服务器句柄（>0 成功，<0 失败）
#[no_mangle]
pub extern "C" fn qi_network_tcp_listen(host: *const c_char, port: u16, backlog: i32) -> i64 {
    if host.is_null() {
        return -1;
    }

    unsafe {
        let 主机 = CStr::from_ptr(host).to_string_lossy().to_string();
        let 地址 = format!("{}:{}", 主机, port);

        match TcpListener::bind(&地址) {
            Ok(listener) => {
                let mut 句柄计数 = 获取句柄计数器().lock().unwrap();
                *句柄计数 += 1;
                let 句柄 = *句柄计数;

                let mut 服务器池 = 获取服务器池().lock().unwrap();
                服务器池.insert(句柄, Arc::new(listener));

                句柄
            }
            Err(_) => -1,
        }
    }
}

/// 接受 TCP 客户端连接（阻塞）
/// 返回客户端连接句柄（>0 成功，<0 失败）
#[no_mangle]
pub extern "C" fn qi_network_tcp_accept(server_handle: i64) -> i64 {
    let 服务器池 = 获取服务器池().lock().unwrap();

    if let Some(listener) = 服务器池.get(&server_handle) {
        match listener.accept() {
            Ok((stream, _addr)) => {
                // 将接受的连接转换为 TcpConnection
                match TcpConnection::from_stream(stream) {
                    Ok(连接) => {
                        let mut 句柄计数 = 获取句柄计数器().lock().unwrap();
                        *句柄计数 += 1;
                        let 句柄 = *句柄计数;

                        let mut 连接池 = 获取连接池().lock().unwrap();
                        连接池.insert(句柄, 连接);

                        句柄
                    }
                    Err(_) => -1,
                }
            }
            Err(_) => -1,
        }
    } else {
        -1
    }
}

/// 关闭 TCP 服务器
/// 返回 1 成功，0 失败
#[no_mangle]
pub extern "C" fn qi_network_tcp_server_close(server_handle: i64) -> i64 {
    let mut 服务器池 = 获取服务器池().lock().unwrap();
    if 服务器池.remove(&server_handle).is_some() {
        1
    } else {
        0
    }
}

// ============================================================================
// UDP 功能
// ============================================================================

use std::net::UdpSocket;

// UDP Socket 池
static UDP套接字池: OnceLock<Mutex<HashMap<i64, UdpSocket>>> = OnceLock::new();

#[allow(non_snake_case)]
fn 获取UDP池() -> &'static Mutex<HashMap<i64, UdpSocket>> {
    UDP套接字池.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 创建 UDP Socket 并绑定到指定地址和端口
/// 返回 Socket 句柄（>0 成功，<0 失败）
#[no_mangle]
pub extern "C" fn qi_network_udp_bind(host: *const c_char, port: u16) -> i64 {
    if host.is_null() {
        return -1;
    }

    unsafe {
        let 主机 = CStr::from_ptr(host).to_string_lossy().to_string();
        let 地址 = format!("{}:{}", 主机, port);

        match UdpSocket::bind(&地址) {
            Ok(socket) => {
                let mut 句柄计数 = 获取句柄计数器().lock().unwrap();
                *句柄计数 += 1;
                let 句柄 = *句柄计数;

                let mut UDP池 = 获取UDP池().lock().unwrap();
                UDP池.insert(句柄, socket);

                句柄
            }
            Err(_) => -1,
        }
    }
}

/// UDP 发送字符串到指定地址（简化版本）
/// 返回发送的字节数（<0 表示错误）
#[no_mangle]
pub extern "C" fn qi_network_udp_send_string(
    handle: i64,
    message: *const c_char,
    host: *const c_char,
    port: u16,
) -> i64 {
    if message.is_null() || host.is_null() {
        return -1;
    }

    unsafe {
        let 消息 = CStr::from_ptr(message).to_string_lossy();
        let 目标主机 = CStr::from_ptr(host).to_string_lossy().to_string();
        let 目标地址 = format!("{}:{}", 目标主机, port);

        let mut UDP池 = 获取UDP池().lock().unwrap();
        if let Some(socket) = UDP池.get_mut(&handle) {
            match socket.send_to(消息.as_bytes(), &目标地址) {
                Ok(字节数) => 字节数 as i64,
                Err(_) => -1,
            }
        } else {
            -1
        }
    }
}

/// UDP 发送数据到指定地址
/// 返回发送的字节数（<0 表示错误）
#[no_mangle]
pub extern "C" fn qi_network_udp_send_to(
    handle: i64,
    data: *const u8,
    data_size: i64,
    host: *const c_char,
    port: u16,
) -> i64 {
    if data.is_null() || data_size <= 0 || host.is_null() {
        return -1;
    }

    unsafe {
        let 目标主机 = CStr::from_ptr(host).to_string_lossy().to_string();
        let 目标地址 = format!("{}:{}", 目标主机, port);

        let mut UDP池 = 获取UDP池().lock().unwrap();
        if let Some(socket) = UDP池.get_mut(&handle) {
            let 数据 = std::slice::from_raw_parts(data, data_size as usize);
            match socket.send_to(数据, &目标地址) {
                Ok(字节数) => 字节数 as i64,
                Err(_) => -1,
            }
        } else {
            -1
        }
    }
}

/// UDP 接收数据（阻塞）
/// 返回接收的字节数（<0 表示错误）
/// sender_host 和 sender_port 用于返回发送方地址（可选）
#[no_mangle]
pub extern "C" fn qi_network_udp_recv_from(
    handle: i64,
    buffer: *mut u8,
    buffer_size: i64,
    sender_host: *mut *mut c_char,
    sender_port: *mut u16,
) -> i64 {
    if buffer.is_null() || buffer_size <= 0 {
        return -1;
    }

    let mut UDP池 = 获取UDP池().lock().unwrap();
    if let Some(socket) = UDP池.get_mut(&handle) {
        let 缓冲区 = unsafe { std::slice::from_raw_parts_mut(buffer, buffer_size as usize) };

        match socket.recv_from(缓冲区) {
            Ok((字节数, 地址)) => {
                // 如果提供了发送方信息指针，填充它们
                if !sender_host.is_null() {
                    let ip字符串 = 地址.ip().to_string();
                    unsafe {
                        *sender_host = CString::new(ip字符串).unwrap().into_raw();
                    }
                }
                if !sender_port.is_null() {
                    unsafe {
                        *sender_port = 地址.port();
                    }
                }
                字节数 as i64
            }
            Err(_) => -1,
        }
    } else {
        -1
    }
}

/// UDP 接收数据并返回为字符串（简化版本）
/// 返回接收到的数据字符串，失败返回空字符串
#[no_mangle]
pub extern "C" fn qi_network_udp_recv_string(handle: i64, buffer_size: i64) -> *mut c_char {
    if buffer_size <= 0 {
        return CString::new("").unwrap().into_raw();
    }

    let mut 缓冲区 = vec![0u8; buffer_size as usize];
    let mut UDP池 = 获取UDP池().lock().unwrap();

    if let Some(socket) = UDP池.get_mut(&handle) {
        match socket.recv_from(&mut 缓冲区) {
            Ok((size, _sender_addr)) => {
                if size > 0 {
                    if let Ok(字符串) = String::from_utf8(缓冲区[..size].to_vec()) {
                        if let Ok(c_str) = CString::new(字符串) {
                            return c_str.into_raw();
                        }
                    }
                }
            }
            Err(_) => {}
        }
    }

    CString::new("").unwrap().into_raw()
}

/// 关闭 UDP Socket
/// 返回 1 成功，0 失败
#[no_mangle]
pub extern "C" fn qi_network_udp_close(handle: i64) -> i64 {
    let mut UDP池 = 获取UDP池().lock().unwrap();
    if UDP池.remove(&handle).is_some() {
        1
    } else {
        0
    }
}

/// 设置 UDP Socket 超时时间（毫秒）
/// 返回 1 成功，0 失败
#[no_mangle]
pub extern "C" fn qi_network_udp_set_timeout(handle: i64, timeout_ms: i64) -> i64 {
    let mut UDP池 = 获取UDP池().lock().unwrap();
    if let Some(socket) = UDP池.get_mut(&handle) {
        let 超时 = if timeout_ms > 0 {
            Some(Duration::from_millis(timeout_ms as u64))
        } else {
            None
        };

        match socket.set_read_timeout(超时) {
            Ok(_) => match socket.set_write_timeout(超时) {
                Ok(_) => 1,
                Err(_) => 0,
            },
            Err(_) => 0,
        }
    } else {
        0
    }
}

/// 设置 UDP 广播模式
/// 返回 1 成功，0 失败
#[no_mangle]
pub extern "C" fn qi_network_udp_set_broadcast(handle: i64, enable: i32) -> i64 {
    let mut UDP池 = 获取UDP池().lock().unwrap();
    if let Some(socket) = UDP池.get_mut(&handle) {
        match socket.set_broadcast(enable != 0) {
            Ok(_) => 1,
            Err(_) => 0,
        }
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn test_network_init() {
        qi_network_init();
        unsafe {
            assert!(全局网络接口.get().is_some());
        }
    }

    #[test]
    fn test_port_available() {
        // 测试一个不太可能被占用的端口
        let result = qi_network_port_available(54321);
        assert!(result == 1 || result == 0); // 可能可用或不可用
    }

    #[test]
    fn test_get_local_ip() {
        let ip_ptr = qi_network_get_local_ip();
        assert!(!ip_ptr.is_null());

        let ip_str = unsafe { CStr::from_ptr(ip_ptr).to_string_lossy() };
        assert!(!ip_str.is_empty());

        qi_network_free_string(ip_ptr);
    }

    #[test]
    fn test_resolve_host() {
        let host = CString::new("localhost").unwrap();
        let ip_ptr = qi_network_resolve_host(host.as_ptr());

        if !ip_ptr.is_null() {
            let ip_str = unsafe { CStr::from_ptr(ip_ptr).to_string_lossy() };
            println!("Resolved localhost to: {}", ip_str);
            qi_network_free_string(ip_ptr);
        }
    }
}
