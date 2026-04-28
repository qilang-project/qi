//! 网络模块 FFI 接口
//!
//! 为 Qi 语言提供 C 接口的网络操作函数（TCP、UDP 等）

#![allow(non_snake_case)]

use super::http::{TcpConnectionConfig, TcpConnection, NetworkInterface};
use std::ffi::{CStr, CString, c_void};
use std::os::raw::c_char;
use std::time::Duration;
use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};
use std::collections::HashMap;
use dashmap::DashMap;

// 全局网络接口实例
use std::sync::OnceLock;
static 全局网络接口: OnceLock<NetworkInterface> = OnceLock::new();

// 用 DashMap 替换 Mutex<HashMap>：连接查找走分片锁，**不同句柄的并发操作不再
// 互相阻塞**。每个 TcpConnection 还是裹一层 Mutex —— 是为了 read/write 需要
// &mut self；因为一条连接同一时刻只有一个 goroutine 在用，这个内层 Mutex
// 几乎永远不会真的竞争。
static TCP连接池: OnceLock<DashMap<i64, Mutex<TcpConnection>>> = OnceLock::new();
static 连接句柄计数器: AtomicI64 = AtomicI64::new(0);

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

fn 获取连接池() -> &'static DashMap<i64, Mutex<TcpConnection>> {
    TCP连接池.get_or_init(DashMap::new)
}

fn 下一个句柄() -> i64 {
    连接句柄计数器.fetch_add(1, Ordering::Relaxed) + 1
}

/// 从TCP连接池中取出连接并返回TcpStream（用于WebSocket升级）
/// 这将从池中移除连接，调用者获得TcpStream的所有权
pub(crate) fn 取出TCP流(handle: i64) -> Option<std::net::TcpStream> {
    获取连接池()
        .remove(&handle)
        .map(|(_, mu)| mu.into_inner().unwrap().into_stream())
}

/// 克隆TCP连接的流（保留原连接在池中）
pub(crate) fn 克隆TCP流(handle: i64) -> Option<std::net::TcpStream> {
    获取连接池().get(&handle).and_then(|entry| {
        let conn = entry.lock().unwrap();
        conn.try_clone_stream().ok()
    })
}

/// 初始化网络模块
#[no_mangle]
pub extern "C" fn qi_network_init() {
    初始化网络接口();
}

// ===== Listener 模式控制（详细实现在文件后部，访问 TCP服务器池）=====

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
                let 句柄 = 下一个句柄();
                获取连接池().insert(句柄, Mutex::new(连接));
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

    if let Some(entry) = 获取连接池().get(&handle) {
        let mut 连接 = entry.lock().unwrap();
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

    if let Some(entry) = 获取连接池().get(&handle) {
        let mut 连接 = entry.lock().unwrap();
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

    if let Some(entry) = 获取连接池().get(&handle) {
        let mut 连接 = entry.lock().unwrap();
        if let Ok(size) = 连接.read(&mut 缓冲区) {
            if size > 0 {
                if let Ok(字符串) = String::from_utf8(缓冲区[..size].to_vec()) {
                    if let Ok(c_str) = CString::new(字符串) {
                        return c_str.into_raw();
                    }
                }
                let 字符串 = String::from_utf8_lossy(&缓冲区[..size]).to_string();
                if let Ok(c_str) = CString::new(字符串) {
                    return c_str.into_raw();
                }
            }
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

        if let Some(entry) = 获取连接池().get(&handle) {
            let mut 连接 = entry.lock().unwrap();
            match 连接.write(数据字节) {
                Ok(字节数) => {
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
    if 获取连接池().remove(&handle).is_some() { 1 } else { 0 }
}

/// TCP 刷新缓冲区
/// 返回 1 成功，0 失败
#[no_mangle]
pub extern "C" fn qi_network_tcp_flush(handle: i64) -> i64 {
    if let Some(entry) = 获取连接池().get(&handle) {
        let mut 连接 = entry.lock().unwrap();
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
    if let Some(entry) = 获取连接池().get(&handle) {
        let 连接 = entry.lock().unwrap();
        连接.bytes_read() as i64
    } else {
        -1
    }
}

/// 获取 TCP 连接已写入的字节数
#[no_mangle]
pub extern "C" fn qi_network_tcp_bytes_written(handle: i64) -> i64 {
    if let Some(entry) = 获取连接池().get(&handle) {
        let 连接 = entry.lock().unwrap();
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

// TCP 服务器监听器池：用 DashMap，listener.accept(&self) 不需要内层锁
static TCP服务器池: OnceLock<DashMap<i64, Arc<TcpListener>>> = OnceLock::new();

fn 获取服务器池() -> &'static DashMap<i64, Arc<TcpListener>> {
    TCP服务器池.get_or_init(DashMap::new)
}

/// 创建 TCP 服务器监听指定端口
/// 返回服务器句柄（>0 成功，<0 失败）
#[no_mangle]
pub extern "C" fn qi_network_tcp_listen(host: *const c_char, port: u16, _backlog: i32) -> i64 {
    if host.is_null() {
        return -1;
    }

    unsafe {
        let 主机 = CStr::from_ptr(host).to_string_lossy().to_string();
        let 地址 = format!("{}:{}", 主机, port);

        match TcpListener::bind(&地址) {
            Ok(listener) => {
                // 登记 listener 的 raw fd —— SIGINT 时 handler 会 shutdown 它，
                // 让任何阻塞在 accept() 上的线程立即返回 -1。
                use std::os::fd::AsRawFd;
                let raw_fd = listener.as_raw_fd();
                crate::runtime::stdlib::signal_ffi::qi_signal_register_listener_fd(raw_fd);

                let 句柄 = 下一个句柄();
                获取服务器池().insert(句柄, Arc::new(listener));
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
    // 关键：先把 Arc 克隆出来，立刻 drop dashmap 的 shard guard。
    // 否则 accept() 可能阻塞数小时，期间这个 shard 上其他 listener 操作全卡死。
    let listener = match 获取服务器池().get(&server_handle) {
        Some(entry) => entry.clone(),
        None => return -1,
    };

    match listener.accept() {
        Ok((stream, _addr)) => match TcpConnection::from_stream(stream) {
            Ok(连接) => {
                let 句柄 = 下一个句柄();
                获取连接池().insert(句柄, Mutex::new(连接));
                句柄
            }
            Err(_) => -1,
        },
        Err(_) => -1,
    }
}

/// 关闭 TCP 服务器
/// 返回 1 成功，0 失败
#[no_mangle]
pub extern "C" fn qi_network_tcp_server_close(server_handle: i64) -> i64 {
    use std::os::fd::AsRawFd;
    match 获取服务器池().remove(&server_handle) {
        Some((_, listener)) => {
            crate::runtime::stdlib::signal_ffi::qi_signal_unregister_listener_fd(
                listener.as_raw_fd(),
            );
            1
        }
        None => 0,
    }
}

/// 二进制安全读：把读到的字节直接放进 字节切片 池
/// 返回字节切片句柄；连接关闭返回 0；错误返回 -1
#[no_mangle]
pub extern "C" fn qi_network_tcp_read_bytes(handle: i64, buffer_size: i64) -> i64 {
    let size = if buffer_size <= 0 { 4096 } else { buffer_size as usize };
    let mut buf = vec![0u8; size];

    let n = match 获取连接池().get(&handle) {
        Some(entry) => {
            let mut conn = entry.lock().unwrap();
            match conn.read(&mut buf) {
                Ok(n) => n,
                Err(_) => return -1,
            }
        }
        None => return -1,
    };
    if n == 0 {
        return 0;
    }
    buf.truncate(n);
    crate::runtime::stdlib::bytes_ffi::register_bytes(buf)
}

/// 二进制安全写：从 字节切片 句柄读出字节写入连接
/// 返回写入字节数；错误返回 -1
#[no_mangle]
pub extern "C" fn qi_network_tcp_write_bytes(handle: i64, bytes_handle: i64) -> i64 {
    let data = match crate::runtime::stdlib::bytes_ffi::clone_bytes(bytes_handle) {
        Some(v) => v,
        None => return -1,
    };

    if let Some(entry) = 获取连接池().get(&handle) {
        let mut c = entry.lock().unwrap();
        let mut written = 0usize;
        while written < data.len() {
            match c.write(&data[written..]) {
                Ok(0) => return -1,
                Ok(n) => written += n,
                Err(_) => return -1,
            }
        }
        let _ = c.flush();
        written as i64
    } else {
        -1
    }
}

/// 把指定 listener 设置为非阻塞或阻塞模式
/// 现在的服务器主循环用阻塞 accept + 信号 shutdown listener 的方式优雅关闭，
/// 这个开关保留主要是为了兼容老代码或别的用例。
#[no_mangle]
pub extern "C" fn qi_network_tcp_listener_set_nonblocking(
    server_handle: i64,
    nonblocking: i64,
) -> i64 {
    if let Some(entry) = 获取服务器池().get(&server_handle) {
        match entry.set_nonblocking(nonblocking != 0) {
            Ok(_) => 0,
            Err(_) => -1,
        }
    } else {
        -1
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
                let 句柄 = 下一个句柄();
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

// ============================================================================
// 真 M:N 异步服务器
// ============================================================================
//
// 设计：用 tokio multi-threaded runtime + tokio::net 接管整个 listener +
// per-connection 的生命周期。每条连接是一个 tokio 任务，read/write 全 async
// —— IO 等待时让出 worker，跟 Go 的 net/http + netpoller 一个路子。
// qi handler 仍是同步函数，每个请求处理时短暂占用 tokio worker（μs 级），
// 不会显著影响调度。
//
// **不支持**的场景（这版先不动）：
//   - 流式响应（handler 返回 0 表示自己写完）
//   - TLS / HTTP/2 / WebSocket 升级
//   - keep-alive 之外的复杂连接管理
//
// 这版的目的：证明 M:N 在简单 HTTP 上能拿到大幅提升。

use std::sync::OnceLock as StdOnceLock;

static ASYNC_RUNTIME: StdOnceLock<tokio::runtime::Runtime> = StdOnceLock::new();

fn 异步运行时() -> &'static tokio::runtime::Runtime {
    ASYNC_RUNTIME.get_or_init(|| {
        let workers = std::env::var("QI_ASYNC_WORKERS")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|n| *n > 0)
            .unwrap_or_else(|| num_cpus::get());
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(workers)
            .thread_name("qi-async")
            .build()
            .expect("failed to start tokio runtime for async server")
    })
}

// Qi 把函数值统一包成 closure 对象传过来：
//   offset 0..8  : trampoline 函数指针
//   offset 8..   : 捕获槽（这里都是 0 个捕获，所以无所谓）
// trampoline 的签名是 extern "C" fn(env, ...args) — 第一个参数是 closure 对象本身。
// 所以调用时要先读 fn_ptr，再传 env+args 调它。

/// 处理函数 trampoline: (env, app, req_bytes_handle, client_handle) → resp_bytes_handle
type HandlerTrampoline = extern "C" fn(*const c_void, *const c_void, i64, i64) -> i64;

/// 从 closure 对象的 offset 0 读出 trampoline 函数指针
unsafe fn closure_trampoline<T>(closure_obj: *const c_void) -> T
where
    T: Copy,
{
    debug_assert_eq!(std::mem::size_of::<T>(), std::mem::size_of::<*const c_void>());
    let fn_ptr = *(closure_obj as *const *const c_void);
    *(&fn_ptr as *const *const c_void as *const T)
}

#[inline]
fn invoke_handler(closure_obj: usize, app: *const c_void, req: i64, client: i64) -> i64 {
    let env = closure_obj as *const c_void;
    unsafe {
        let trampoline: HandlerTrampoline = closure_trampoline(env);
        trampoline(env, app, req, client)
    }
}

/// 启动异步服务器：takes ownership of the listener at server_handle，
/// 用 tokio 接管 accept 循环 + 每条连接的 IO，调 qi 侧的 handler_fn 处理请求。
///
/// HTTP 请求完整性（headers + Content-Length）在 Rust 侧直接检测 —— 比每个
/// chunk 调一次 Qi closure trampoline 便宜得多，对小请求基本是零开销路径。
///
/// 注意：handler_fn 是 Qi 的 *closure 对象* 指针，不是裸函数指针；
/// 调用它要走 trampoline（见 invoke_handler）。
/// 收到 SIGINT 时返回 0；其他错误返回 -1。
#[no_mangle]
pub extern "C" fn qi_runtime_async_serve(
    server_handle: i64,
    handler_fn: *const c_void,
    app_ptr: *const c_void,
) -> i64 {
    use std::os::fd::{AsRawFd, FromRawFd};

    // 把 listener 从同步池里取出来 —— tokio 要 owning std listener。
    let listener_arc = match 获取服务器池().remove(&server_handle) {
        Some((_, arc)) => arc,
        None => return -1,
    };

    // 由于 Arc 可能还有别的引用（理论上不该，因为我们刚 remove 出来唯一持有方），
    // 用 try_unwrap，失败就拷一份新 fd。
    let std_listener = match std::sync::Arc::try_unwrap(listener_arc) {
        Ok(l) => l,
        Err(arc) => {
            // 退路：用 raw fd 复制一个 listener
            let raw = arc.as_raw_fd();
            unsafe { std::net::TcpListener::from_raw_fd(libc::dup(raw)) }
        }
    };

    if std_listener.set_nonblocking(true).is_err() {
        return -1;
    }

    // 指针跨线程 —— 包成 usize 走 Send。
    let app_addr = app_ptr as usize;
    let handler_addr = handler_fn as usize;

    异步运行时().block_on(async move {
        let tokio_listener = match tokio::net::TcpListener::from_std(std_listener) {
            Ok(l) => l,
            Err(_) => return,
        };
        loop {
            tokio::select! {
                accept_result = tokio_listener.accept() => {
                    match accept_result {
                        Ok((stream, _addr)) => {
                            let _ = stream.set_nodelay(true);
                            tokio::spawn(handle_conn_async(stream, handler_addr, app_addr));
                        }
                        Err(_) => break,
                    }
                }
                _ = 关闭信号_watcher() => break,
            }
        }
    });

    0
}

/// 异步关闭信号 watcher：100ms 周期轮询关闭标志。
async fn 关闭信号_watcher() {
    loop {
        if crate::runtime::stdlib::signal_ffi::qi_signal_should_shutdown() != 0 {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

async fn handle_conn_async(
    mut stream: tokio::net::TcpStream,
    handler_addr: usize,
    app_addr: usize,
) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // PROBE: 是否启用纯 Rust 硬编码响应（跳过 qi handler）。
    // 用于诊断"瓶颈是 qi handler 还是 IO 层"。
    let probe_rust_only = std::env::var("QI_BENCH_RUST_ONLY").is_ok();
    const HARDCODED_RESPONSE: &[u8] =
        b"HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: keep-alive\r\nContent-Length: 2\r\n\r\nok";

    let mut read_buf = vec![0u8; 16384];
    let mut accumulated: Vec<u8> = Vec::with_capacity(16384);

    loop {
        // 读到完整请求为止 —— 完整性检测在 Rust 内联，不走 Qi 回调
        loop {
            match stream.read(&mut read_buf).await {
                Ok(0) => return, // peer close
                Ok(n) => accumulated.extend_from_slice(&read_buf[..n]),
                Err(_) => return,
            }
            if http_request_complete(&accumulated) {
                break;
            }
        }

        // 决定 keep-alive（在 move 之前 borrow 一下）
        let keep_alive = !request_has_connection_close(&accumulated);

        if probe_rust_only {
            // 纯 Rust 路径：跳过 qi handler，直接写硬编码响应
            accumulated.clear();
            if stream.write_all(HARDCODED_RESPONSE).await.is_err() {
                return;
            }
            if !keep_alive {
                return;
            }
            continue;
        }

        // 整个 buffer move 进字节池，不 clone。下一轮请求重新 alloc。
        let req_bytes = std::mem::take(&mut accumulated);
        let req_handle = crate::runtime::stdlib::bytes_ffi::register_bytes(req_bytes);

        // 同步调 qi handler — 这是 μs 级 CPU 工作，短暂占用 tokio worker 没事
        let resp_handle = invoke_handler(
            handler_addr,
            app_addr as *const c_void,
            req_handle,
            0,
        );
        // handler 可能已经释放了；no-op 再来一次
        crate::runtime::stdlib::bytes_ffi::free_bytes(req_handle);

        if resp_handle <= 0 {
            return;
        }

        // 取响应字节，async 写回
        let resp = match crate::runtime::stdlib::bytes_ffi::take_bytes(resp_handle) {
            Some(v) => v,
            None => return,
        };
        if stream.write_all(&resp).await.is_err() {
            return;
        }
        let _ = stream.flush().await;

        if !keep_alive {
            return;
        }
        // accumulated 已经被 take 走，下轮循环重新积累
        accumulated.reserve(16384);
    }
}

/// 判断 HTTP/1.1 请求是否完整：headers 找到 \r\n\r\n + body 字节数 ≥ Content-Length。
/// 没 Content-Length 头视为无 body 请求（GET/HEAD）。
fn http_request_complete(bytes: &[u8]) -> bool {
    let header_end = match bytes.windows(4).position(|w| w == b"\r\n\r\n") {
        Some(p) => p,
        None => return false,
    };
    let headers = &bytes[..header_end];

    // 找 Content-Length（不区分大小写）
    let cl_needle = b"content-length:";
    let mut idx = 0;
    while idx + cl_needle.len() <= headers.len() {
        if headers[idx..idx + cl_needle.len()].eq_ignore_ascii_case(cl_needle) {
            // 行内取值
            let line_end = headers[idx..]
                .windows(2)
                .position(|w| w == b"\r\n")
                .map(|p| idx + p)
                .unwrap_or(headers.len());
            let value_bytes = &headers[idx + cl_needle.len()..line_end];
            // 解析 i64
            let value_str = match std::str::from_utf8(value_bytes) {
                Ok(s) => s.trim(),
                Err(_) => return true, // 解析失败保守判完整
            };
            let cl: usize = match value_str.parse() {
                Ok(v) => v,
                Err(_) => return true,
            };
            let body_received = bytes.len() - header_end - 4;
            return body_received >= cl;
        }
        idx += 1;
    }
    // 没 Content-Length —— GET/HEAD 等无 body 请求，complete
    true
}

fn request_has_connection_close(bytes: &[u8]) -> bool {
    let header_end = bytes
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .unwrap_or(bytes.len());
    let headers = &bytes[..header_end];

    // 大小写无关搜 "connection:" 然后看其行内是否带 "close"
    let needle = b"connection:";
    let mut i = 0;
    while i + needle.len() <= headers.len() {
        if headers[i..i + needle.len()].eq_ignore_ascii_case(needle) {
            // 找到 connection: header，往后到行尾扫 close
            let line_end = headers[i..]
                .windows(2)
                .position(|w| w == b"\r\n")
                .map(|p| i + p)
                .unwrap_or(headers.len());
            let value = &headers[i + needle.len()..line_end];
            return value
                .windows(5)
                .any(|w| w.eq_ignore_ascii_case(b"close"));
        }
        i += 1;
    }
    false
}
