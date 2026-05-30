#!/usr/bin/env python3
"""
P2 验证脚本：测试 MCP 服务器推送通知（stdio 传输）

流程:
1. 编译 通知服务.qi 为原生二进制（qi compile）
2. 启动该二进制进程（stdio MCP 服务器）
3. 发送 initialize + initialized
4. 调用 "计算" 工具（传 a=7, b=8）
5. 读取所有输出行，断言看到：
   - tools/call 响应 (result.content[0].text 含 "sum":15)
   - notifications/message (level=info, data="工具被调用了")
   - notifications/progress
"""
import subprocess, json, sys, os, time, tempfile

QI_BIN = os.path.join(os.path.dirname(__file__), "../../../../target/debug/qi")
QI_FILE = os.path.join(os.path.dirname(__file__), "通知服务.qi")

QI_BIN = os.path.abspath(QI_BIN)
QI_FILE = os.path.abspath(QI_FILE)

# 编译到临时二进制
BIN_OUT = os.path.join(tempfile.gettempdir(), "qi_notify_server_test")
print(f"[test] 编译: {QI_BIN} compile {QI_FILE} -o {BIN_OUT}", flush=True)
compile_result = subprocess.run(
    [QI_BIN, "compile", QI_FILE, "-o", BIN_OUT],
    capture_output=True, text=True, timeout=60
)
if compile_result.returncode != 0:
    print(f"[FAIL] 编译失败:\n{compile_result.stdout}\n{compile_result.stderr}")
    sys.exit(1)
print(f"[test] 编译成功", flush=True)

print(f"[test] 启动服务器: {BIN_OUT}", flush=True)

proc = subprocess.Popen(
    [BIN_OUT],
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
)

def send(obj):
    line = json.dumps(obj) + "\n"
    proc.stdin.write(line.encode())
    proc.stdin.flush()

def read_line(timeout=5):
    import select
    ready, _, _ = select.select([proc.stdout], [], [], timeout)
    if not ready:
        return None
    line = proc.stdout.readline()
    return line.decode().strip() if line else None

# 1. initialize
send({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
    "protocolVersion":"2025-06-18",
    "capabilities":{},
    "clientInfo":{"name":"test-client","version":"0.1"}
}})

resp_str = read_line()
assert resp_str, "initialize 无响应"
resp = json.loads(resp_str)
print(f"[test] initialize 响应: {json.dumps(resp, ensure_ascii=False)}", flush=True)
assert "result" in resp, f"initialize 应有 result 字段，实际: {resp}"
caps = resp["result"].get("capabilities", {})
print(f"[test] capabilities: {json.dumps(caps, ensure_ascii=False)}", flush=True)
assert "logging" in caps, f"capabilities 应包含 logging，实际: {caps}"
assert caps.get("tools", {}).get("listChanged") == True, f"tools.listChanged 应为 true，实际: {caps}"

# 2. initialized notification (client→server, no response)
send({"jsonrpc":"2.0","method":"notifications/initialized"})
time.sleep(0.1)

# 3. tools/call "计算"
send({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{
    "name":"计算",
    "arguments":{"a":7,"b":8}
}})

# 收集接下来的输出行（可能包含 通知 + 响应，顺序不确定）
received_lines = []
deadline = time.time() + 5
while time.time() < deadline:
    line = read_line(timeout=0.5)
    if line:
        received_lines.append(line)
        print(f"[test] 收到行: {line}", flush=True)
    else:
        # 如果已收到工具调用响应就停
        if any('"id":2' in l or '"id": 2' in l for l in received_lines):
            # 再等一点收通知
            for _ in range(6):
                line2 = read_line(timeout=0.3)
                if line2:
                    received_lines.append(line2)
                    print(f"[test] 收到后续行: {line2}", flush=True)
            break

proc.stdin.close()
proc.wait(timeout=3)

# 分析
tool_resp = None
log_notif = None
progress_notifs = []

for raw in received_lines:
    try:
        obj = json.loads(raw)
    except Exception:
        continue
    method = obj.get("method", "")
    if obj.get("id") == 2:
        tool_resp = obj
    elif method == "notifications/message":
        log_notif = obj
    elif method == "notifications/progress":
        progress_notifs.append(obj)

print("\n===== 结果断言 =====", flush=True)

# 断言1: 工具调用响应正确
assert tool_resp is not None, f"未收到 tools/call 响应(id=2)，收到行: {received_lines}"
content_text = tool_resp["result"]["content"][0]["text"]
assert '"sum": 15' in content_text or '"sum":15' in content_text, \
    f"工具结果应含 sum:15，实际: {content_text}"
print(f"[PASS] tools/call 响应正确: {content_text}", flush=True)

# 断言2: 日志消息通知
assert log_notif is not None, f"未收到 notifications/message 通知，收到行: {received_lines}"
assert log_notif["params"]["level"] == "info", f"日志级别应为 info，实际: {log_notif}"
assert log_notif["params"]["data"] == "工具被调用了", f"日志内容错误: {log_notif}"
print(f"[PASS] notifications/message 通知: {json.dumps(log_notif, ensure_ascii=False)}", flush=True)

# 断言3: 进度通知
assert len(progress_notifs) > 0, f"未收到 notifications/progress 通知，收到行: {received_lines}"
tokens = [p["params"]["progressToken"] for p in progress_notifs]
assert "calc-1" in tokens, f"进度通知 token 应含 calc-1，实际: {tokens}"
print(f"[PASS] notifications/progress 通知 ({len(progress_notifs)} 条): token={tokens}", flush=True)

print("\n[PASS] 全部断言通过 ✓", flush=True)
sys.exit(0)
