# Qi → WebAssembly

```bash
qi --target wasm compile 程序.qi -o 程序.wasm
wasmtime run 程序.wasm
```

一步到位：编译器出 wasm32 目标文件，再自己调 wasm-ld 链上 wasi libc 和 wasm 版运行时。
产物是标准的 **wasm32-wasip1** 模块，wasmtime / wasmer / 浏览器（配一个 WASI shim）都能跑。

## 要装什么

| 东西 | 从哪来 | 用途 |
|---|---|---|
| wasm-ld | `rustup target add wasm32-wasip1`（自带 rust-lld）；或 `brew install lld` / `apt install lld` | 链接 |
| wasi sysroot | 同上 rustup 目标（crt1-command.o + libc.a）；或 `apt install wasi-libc` | libc |
| wasm 运行时归档 | **发布包自带**（`lib/qi/libqi_runtime_wasm.a`，2026.09.06-1 起）；源码构建：`cd qi-runtime/wasm && cargo build --release --target wasm32-wasip1` | qi 运行时 |
| wasmtime | `brew install wasmtime` | 命令行里跑 |

编译器按上面的顺序自己找，找不到就报中文提示。也能用环境变量钉死：
`QI_WASM_LD` / `QI_WASM_SYSROOT` / `QI_WASM_RUNTIME_LIB`。

`-o 程序.o`（或不给 `-o`）只出目标文件、不链接，给想自己链的人用。

## 本目录

```
构建.sh        重建 wasm 运行时 → 编译 → wasmtime 跑 → 提示浏览器版
试炼.qi 你好.qi 两个演示程序
index.html    浏览器里跑同一个 .wasm
wasi-shim.js  浏览器用的最小 WASI 实现（fd_write / 时钟 / 随机 / 进程退出，没有文件系统）
```

```bash
./构建.sh                              # 试炼.qi → 试炼.wasm，wasmtime 跑
python3 -m http.server 43510           # 然后开 http://localhost:43510/index.html
```

浏览器侧只要这一句：

```js
import { runQiWasm } from './wasi-shim.js';
const code = await runQiWasm(await (await fetch('程序.wasm')).arrayBuffer(), text => out.textContent += text);
```

## 运行时是怎么来的

`qi-runtime/wasm` 这个 crate 自己几乎没有代码：字符串 / JSON / 列表 / 哈希表 / 字节切片 /
数学 / 时间 / 闭包 / 反射 / 正则 / 加密 / 压缩 / 向量 / 词法索引，全部用 `#[path]` 把主运行时
`qi-runtime/src/stdlib/*.rs` **原样编进来**。在 wasm 里跑的就是原生那份代码，行为逐字节一致，
改一处两边都改。只有跟宿主强相关的几件是手写的：打印、分配、goroutine、Future、异常。

为什么不直接给 qi-runtime 加个 feature：它的非可选依赖里有 rusqlite（bundled sqlite）和
reqwest → rustls → aws-lc-sys 两棵原生 C 库树，build script 在 wasm32 上直接失败，跟
feature 开关无关。

## wasm 里能用什么、不能用什么

**能**：整个语言核心（结构体 / 枚举 / 闭包 / 方法链 / 泛型 / 匹配 / 数组）、字符串、JSON、
列表、哈希表、字节切片、数学、时间、正则、加密、随机、压缩、向量、路径、文件读写
（wasmtime 要 `--dir .` 授权；浏览器 shim 里没有文件系统，读到的是「不存在」）。

**不能**（链接期报 `undefined symbol: qi_xxx`，编译器会附提示）：网络 / HTTP / WebSocket /
TLS / 数据库 / Redis / 大模型 / MCP / gRPC / 图形化 / 子进程 / 信号。

**语义上退化的**：

- `启动`（goroutine）**就地同步执行**。wasm32-wasip1 没有第二根线程。「启动几个任务再
  等待组.等待」结果一样只是不并行；「启动一个死循环的后台任务」会先把主线程占住。
- `未来<T>` / `等待`：Future 一出生就已完成，`等待` 立刻返回。
- `尝试 / 捕获`：**做不出来**。wasi libc 没有 setjmp/longjmp（要 wasm 异常处理提案 +
  `-mllvm -wasm-enable-sjlj` + libsetjmp，rustup 的 sysroot 里没有）。用到 `尝试` 的程序
  链接期报 `setjmp` 未定义；单独的 `抛出` 是打印消息然后退出码 1。
- 没有 fsync、没有环境变量（浏览器 shim 里）、`进程统计` 那组读到的都是 0。

## 验证方式

`qi/tests/wasm/断言.sh`：同一个 `.qi` 原生跑一遍、wasm 跑一遍，stdout 必须逐字节一致。
不维护期望文件 —— 期望就是原生的输出，测的是「与原生一致」而不是「与某天手抄的文本一致」。
缺 wasmtime / wasm 目标 / 运行时归档时整套跳过，不拖红主干。`make wasm` 是入口。

2026-09-05 第一次拿全部示例语料（410 个能编到 wasm 的程序）做原生 vs wasm 差分，抓到两个
真 bug：数组槽位按元素类型做 GEP（x86_64 上指针和 i64 都是 8 字节所以从没暴露，wasm32 上
`数组<字符串>` 第 1 个元素压在长度头的高 32 位上），以及浮点打印少 `.0`。都修了。

## 已知边界

- 单线程；数据集受 wasm 线性内存约束（默认 4GB 上限，wasmtime 可配）
- `尝试/捕获` 不可用（见上）
- 浏览器 shim 的 `poll_oneoff`（睡眠）是忙等，主线程会卡；要长时间跑放 Worker 里
- 编译器本身没有编到 wasm：浏览器里编译任意用户代码仍要服务端（见 qi-playground）
