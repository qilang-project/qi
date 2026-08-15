#!/usr/bin/env python3
"""假 qi 包注册中心 —— docs/包管理设计.md 五个端点的最小内存实现。

存在的理由：包管理的核心契约（sha256 对得上、409 版本不可变、401 坏 token、
装完真能 import）**只有对面站着一个服务端才验得出来**。客户端自测「我发出去了」
永远是绿的，等真注册中心上线才发现打包排除规则或 percent-encode 错了就晚了。

刻意不做的：PG、token 签发、下载计数持久化。全在内存里，进程一死就干净。

用法：
    假注册中心.py --port-file /tmp/端口 [--token 令牌] [--state-file /tmp/状态.json]

--state-file 会在每次 PUT 后把 {包名: {版本: {sha256, size}}} 写盘，
供 bash 断言脚本读（服务端到底收没收到、算出来的 sha256 是多少）。
"""
from __future__ import annotations

import argparse
import base64
import hashlib
import json
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import unquote

# 包名 -> 版本 -> {"body": bytes, "sha256": str, "size": int, "description": str}
STORE: dict[str, dict[str, dict]] = {}
LOCK = threading.Lock()
TOKEN = "测试令牌"
STATE_FILE: Path | None = None
# 最近一次成功 PUT 的上行形态。断言脚本靠它确认「body 真的是 base64(tar.gz)」——
# 否则客户端哪天退回裸字节，只会表现成一条难查的 sha256 不符。
LAST_PUT: dict = {}


def dump_state() -> None:
    """把当前库存写盘（不含包体字节），给 bash 断言看。"""
    if STATE_FILE is None:
        return
    snapshot = {
        name: {
            ver: {"sha256": meta["sha256"], "size": meta["size"]}
            for ver, meta in versions.items()
        }
        for name, versions in STORE.items()
    }
    STATE_FILE.write_text(
        json.dumps({"包": snapshot, "上行": LAST_PUT}, ensure_ascii=False, indent=2),
        encoding="utf-8",
    )


def read_manifest_name_version(body: bytes) -> tuple[str | None, str | None]:
    """从 tar.gz 里抠出包根 qi.toml 的 名称/版本。

    服务端**必须**校验包内清单与 URL 一致（协议第 50 行），所以这里真解包。
    用极简的行扫描而不是 TOML 库：标准库没有 tomllib 之前的 Python 也能跑，
    而我们只关心 [包] 段里两个键。
    """
    import io
    import tarfile

    try:
        with tarfile.open(fileobj=io.BytesIO(body), mode="r:gz") as tf:
            member = tf.getmember("qi.toml")
            fh = tf.extractfile(member)
            if fh is None:
                return None, None
            text = fh.read().decode("utf-8", "replace")
    except Exception:
        return None, None

    name = version = None
    in_package = False
    for raw in text.splitlines():
        line = raw.strip()
        if line.startswith("["):
            in_package = line in ("[包]", "[package]", '["包"]')
            continue
        if not in_package or "=" not in line or line.startswith("#"):
            continue
        key, _, value = line.partition("=")
        key = key.strip().strip('"')
        value = value.split("#")[0].strip().strip('"')
        if key in ("名称", "name"):
            name = value
        elif key in ("版本", "version"):
            version = value
    return name, version


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, fmt: str, *args: object) -> None:
        return

    # ── 响应小工具 ──
    def send_json(self, code: int, payload: dict) -> None:
        body = json.dumps(payload, ensure_ascii=False).encode("utf-8")
        self.send_response(code)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def send_error_json(self, code: int, reason: str) -> None:
        # 协议规定错误响应形状是 {"error": "人话原因"}
        self.send_json(code, {"error": reason})

    def send_bytes(self, code: int, body: bytes, content_type: str) -> None:
        self.send_response(code)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def split_path(self) -> list[str]:
        """路径段解 percent-encode。中文包名就是靠这一步还原的。"""
        path = self.path.split("?")[0]
        return [unquote(seg) for seg in path.strip("/").split("/") if seg]

    # ── GET ──
    def do_GET(self) -> None:
        parts = self.split_path()
        # /api/v1/packages…
        if len(parts) < 3 or parts[:3] != ["api", "v1", "packages"]:
            self.send_error_json(404, "没有这个端点")
            return
        rest = parts[3:]

        with LOCK:
            if not rest:
                packages = [
                    {
                        "name": name,
                        "latest": sorted(versions)[-1] if versions else None,
                        "description": next(iter(versions.values()))["description"]
                        if versions
                        else "",
                        "downloads": sum(v["downloads"] for v in versions.values()),
                    }
                    for name, versions in sorted(STORE.items())
                ]
                self.send_json(200, {"packages": packages})
                return

            name = rest[0]
            versions = STORE.get(name)
            if versions is None:
                self.send_error_json(404, f"没有名为 {name} 的包")
                return

            if len(rest) == 1:
                self.send_json(
                    200,
                    {
                        "name": name,
                        "description": next(iter(versions.values()))["description"],
                        "versions": [
                            {
                                "version": ver,
                                "sha256": meta["sha256"],
                                "size": meta["size"],
                                "uploaded_at": meta["uploaded_at"],
                            }
                            for ver, meta in sorted(versions.items())
                        ],
                    },
                )
                return

            version = rest[1]
            meta = versions.get(version)
            if meta is None:
                self.send_error_json(404, f"{name} 没有 {version} 这个版本")
                return

            if len(rest) == 2:
                self.send_json(
                    200,
                    {
                        "version": version,
                        "sha256": meta["sha256"],
                        "size": meta["size"],
                        "uploaded_at": meta["uploaded_at"],
                    },
                )
                return

            if len(rest) == 3 and rest[2] == "download":
                meta["downloads"] += 1
                self.send_bytes(200, meta["body"], "application/gzip")
                return

        self.send_error_json(404, "没有这个端点")

    # ── PUT（发布）──
    def do_PUT(self) -> None:
        parts = self.split_path()
        if len(parts) != 5 or parts[:3] != ["api", "v1", "packages"]:
            self.send_error_json(404, "发布地址应为 /api/v1/packages/{名称}/{版本}")
            return
        name, version = parts[3], parts[4]

        length = int(self.headers.get("Content-Length", "0"))
        raw = self.rfile.read(length) if length else b""

        # http.server 用 latin-1 解请求头，非 ASCII 的 token 会变成乱码。
        # 按 latin-1 还原成字节再当 UTF-8 读，这样中文 token 也能正确比对。
        auth = self.headers.get("Authorization", "")
        try:
            auth = auth.encode("latin-1").decode("utf-8")
        except (UnicodeEncodeError, UnicodeDecodeError):
            pass
        if not auth.startswith("Bearer ") or auth[len("Bearer "):].strip() != TOKEN:
            self.send_error_json(401, "发布 token 无效")
            return

        # 发布 body 是 base64(tar.gz)，不是裸字节 —— 注册中心用 qi-web 写，
        # qi-runtime 的 web FFI 收请求体时会把内嵌 0x00 换成空格（C 串约定），
        # tar.gz 裸传上去会**长度不变、只坏字节**地悄悄损坏。这里跟真服务端
        # 一样先解 base64；解不动就按 400 回，免得测试里悄悄退化回裸字节。
        try:
            body = base64.b64decode(raw, validate=True)
        except Exception:
            self.send_error_json(400, "包体不是合法的 base64（发布 body 应为 base64(tar.gz)）")
            return

        if not body:
            self.send_error_json(400, "包体为空")
            return

        # 协议要求：解包校验 qi.toml 的 名称/版本 与 URL 一致
        got_name, got_version = read_manifest_name_version(body)
        if got_name is None:
            self.send_error_json(400, "包体里没有可解析的包根 qi.toml")
            return
        if got_name != name or got_version != version:
            self.send_error_json(
                400,
                f"包内 qi.toml 是 {got_name} {got_version}，与发布地址 {name} {version} 不符",
            )
            return

        with LOCK:
            versions = STORE.setdefault(name, {})
            if version in versions:
                # 版本不可变
                self.send_error_json(409, f"{name} 的 {version} 版本已存在，版本不可变")
                return
            versions[version] = {
                "body": body,
                "sha256": hashlib.sha256(body).hexdigest(),
                "size": len(body),
                "uploaded_at": "2026-08-15T00:00:00Z",
                "description": f"{name} 的测试包",
                "downloads": 0,
            }
            LAST_PUT["was_base64"] = True
            LAST_PUT["encoded_len"] = len(raw)
            LAST_PUT["decoded_len"] = len(body)
            dump_state()

        self.send_json(201, {"name": name, "version": version,
                             "sha256": STORE[name][version]["sha256"]})


def main() -> None:
    global TOKEN, STATE_FILE
    parser = argparse.ArgumentParser()
    parser.add_argument("--port-file", required=True)
    parser.add_argument("--token", default="测试令牌")
    parser.add_argument("--state-file")
    args = parser.parse_args()

    TOKEN = args.token
    if args.state_file:
        STATE_FILE = Path(args.state_file)
        STATE_FILE.write_text("{}", encoding="utf-8")

    # 端口 0 = 内核挑一个空闲高位端口再写给调用方。固定端口在 CI 上会撞，
    # 而且撞了报的是「连接被拒」，很难指向真因。
    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    port = server.server_address[1]
    Path(args.port_file).write_text(str(port), encoding="utf-8")
    server.serve_forever()


if __name__ == "__main__":
    main()
