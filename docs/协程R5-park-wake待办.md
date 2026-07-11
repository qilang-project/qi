# 协程 R5：park-wake 调度器重写（待办 · 交接 Fable）

> 现状：真协程 R1-R4 已上线（qi 7213a71→d7455cc / qi-runtime f39dd86→e8bae30），
> QI_CORO=1 门控、默认路径逐字节不变、518+407 全绿。正常并发场景全能跑、性能
> 9M 上下文切换/秒。本文件列的是 R4 尝试增量 park-wake 时暴露、随后**回退**的
> 边缘 bug —— 明天专项做。

## 背景：R5 park-wake 增量补丁为什么回退

R4 的通道是「协作式 spin-yield」：协程收空通道 → `qi_coro_yield_ready` + 挂起 →
下一轮 resume 重试。正确但空转、且无背压/死锁处理。

试图升级为真 park-wake（阻塞协程摘出 PENDING、挂到通道 `recv_waiters`/`send_waiters`、
send/recv 时唤醒）。实现见回退前的 diff（CURRENT/PARKED/PARKED_COUNT +
qi_coro_chan_park_recv/park_send + 背压 + 死锁检测）。**核心用例全过、常见场景是改进，
但边缘案例 hang**，故回退（`git checkout` 掉，未提交）。

**根因**：顶层 `启动 <协程调用>`（`入口` 非协程）会在 **CURRENT=null** 时同步跑
ramp 到首个挂起点。若首挂起是通道 park，park 因 CURRENT 空而失效 → 协程落 PENDING
自旋，与背压/唤醒链交互产生 lost-wakeup/hang。

**正解方向**：调度器级重写，`启动` 不在顶层同步跑 ramp body，而是把协程**延迟入执行器**
（ramp 只建 frame + 首次 resume 也由 step() 驱动，保证任何挂起点都在 CURRENT 有值的
上下文里发生）。这样 park-wake 才能干净落地，顺带解决下面 1-4。

## Bug 清单（按优先级）

1. **有界通道(cap>0)满时发送丢值** 🔴
   `ch <- v` 满返 1 但发送端不挂起（背压未做），值静默丢。
   复现：3 消费者 + 生产者向 cap-1 通道发 3 值 → 只 1 个消费者拿到。
   修法：send 满 → park 进 send_waiters（连值）；recv 腾空位时补值 + 唤醒 sender。

2. **协程永久阻塞通道 → 资源泄漏 + 无死锁处理** 🔴
   阻塞协程的 frame/RC 不回收（QI_RC_REPORT 活跃字符串/闭包 >0）；顶层 run_all
   直接返回或忙等，不报死锁。
   修法：park-wake 后，run_all 结束时 PENDING 空但 parked>0 = 死锁 → 报错 + 销毁
   parked 协程 frame（走 cleanup 释放 RC）。

3. **顶层 `启动` ramp CURRENT=null** 🟠
   见根因。park-wake 才暴露；当前 R4 spin 模型下表现为多自旋一轮，不致命，但重写必须解决。

4. **协作式 await/通道是轮询非真 park-wake** 🟡
   future 完成靠下轮 poll 命中（单线程语义正确，海量挂起有轮询开销）。park-wake 一并消除。

5. **`异步对话`→未来<字符串> 直接 `等待` 取值 null** 🟡（独立，与协程无关）
   `内部=指针 → qi_future_await_ptr`，但 eager future 存 FutureValue::String 变体不匹配
   → 返 null。修 codegen：`未来<字符串>` await 走 qi_future_await_string；或
   qi_future_await_ptr 对 String 变体返回 RC 字符串。结构化输出用 异步询问::<T> 已正常。

6. **`启动 <非协程表达式>`** 🟢 仍走 tokio 池，与协程原生通道不互通（跨世界，设计取舍，可不改）。

## 验收（park-wake 重写后必须全过）

- 回归：默认 QI_CORO 不设 518+407 全绿；R1-R4 七测（交错/挂起值/RC字符串/RC结构体/
  提前销毁/通道统一/嵌套等待/真IO上车）全不回退、RC 0、无 hang。
- 新铁证：①有界 cap-1 通道 + N 消费者 + 生产者 N 值 → N 个全收到、RC 0（背压 #1）。
- ②100 消费者阻塞单通道 + 1 生产者慢喂 → 全部唤醒、RC 0、无 hang（#2/#4）。
- ③死锁程序（收无人发的通道）→ 报「死锁」而非 hang（#2）。
- ④性能不退：纯让出吞吐 ≥ R4 的 9M 切换/秒（park 应更快，无空转）。

## 关键文件

- 执行器：`qi/src/runtime/async_runtime/coro.rs`（真身，编进 libqi_compiler.a）+
  `qi-runtime/src/async_runtime/coro.rs`（镜像，改完 `cp` 同步；**注意 module 路径
  qi 用 `crate::runtime::async_runtime::future`、qi-runtime 用 `crate::async_runtime::future`**）。
- 通道 codegen：`qi/src/codegen/inkwell_gen/并发.rs`（生成通道创建/发送/接收/协程启动）。
- await codegen：`qi/src/codegen/inkwell_gen/异步.rs`（生成等待 / 协程等待轮询）。
- 挂起点：`qi/src/codegen/inkwell_gen/协程.rs`（生成挂起点 pub(super)、协程函数集、调用是协程）。
- **改 runtime 后 debug+release 两个 .a 都要重建**（编译器 find_host_runtime_library 按
  ["debug","release"] 找，debug 优先，只建 release 会静默链旧库）。
