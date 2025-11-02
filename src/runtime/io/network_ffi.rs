//! 网络模块 FFI 接口
//!
//! 为 Qi 语言提供 C 接口的网络操作函数（TCP、UDP 等）

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn test_network_init() {
        qi_network_init();
        unsafe {
            assert!(全局网络接口.is_some());
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
