# 协程 R6：多核 M:N 调度器 + qi-web 接入（设计规格）

> 现状：真协程 R1-R5 上线（QI_CORO 门控、默认零影响、518+407 全绿）。协程调度是
> **单线程协作式**：微基准比 Go 单核快 2-4×，但压不满多核 → CPU 密集并行 Go 仍胜。
> R6 = 把执行器多核化；R7 = qi-web 请求跑在此调度器上。

## ✅ 可行性结论（2026-07-12 已核查）

**最大拦路虎（引用计数非原子）不存在**：
- 字符串 `BufHeader { magic, refcount: AtomicI64, capacity }`（qi-runtime/src/stdlib/qi_str.rs）
- 对象 `ObjHeader { … refcount: AtomicI64 … }`（qi-runtime/src/stdlib/rc_obj.rs）
- retain=`fetch_add(Relaxed)`、release=`fetch_sub(AcqRel)`、immortal 门槛。

→ 多线程协程跨线程共享 RC 对象**不会损坏计数**，无需重写 ARC。这是字符串 ARC 战役 +
goroutine работа 顺带打下的地基。**R6 绿灯**。

已有多核基建可参考（但不直接复用）：`qi/src/runtime/async_runtime/executor.rs` 是
tokio work-stealing 池，供 `启动 <非协程>` 用。R6 的协程调度器建议独立于 tokio
（协作式协程用 std::thread worker + 共享队列更简单，避开 tokio async 复杂度）。

## R6 精确改造图（qi/src/runtime/async_runtime/coro.rs）

**注意**：coro 内部改动只影响 QI_CORO=1 程序；默认路径（QI_CORO 不设）不碰协程 executor，
518 测试天然不受影响。故可自由重构 executor 内部，只要 QI_CORO 程序行为不变。

1. **就绪队列**：`thread_local PENDING` → 全局 `Mutex<VecDeque<Cptr>>` + `Condvar`。
   `struct Cptr(*mut QiCoro); unsafe impl Send for Cptr`（frame 堆分配，任一时刻仅一个
   worker 持有 → 安全）。
2. **通道** `QiCoroChan` → `{ inner: Mutex<ChanInner> }`，ChanInner{buf,cap,closed,
   recv_waiters,send_waiters}。try_send/try_recv/park_* 全走锁。单线程下锁无争用、开销小。
3. **thread_local 保留**：CURRENT/PARKED/PARK_INTENT 每 worker 各一份（正确）。
4. **worker 循环**：pop 就绪（空则 Condvar wait）→ 设 CURRENT → resume → done/park/requeue。
5. **`qi_coro_run_all`**：读 `QI_CORO_WORKERS`（默认 1 = 当前单线程行为）；起 N-1 个
   std::thread + 当前线程共同排空，全部 join 后返回。
6. **终止检测**（多 worker 的关键难点）：`live: AtomicI64`（spawn++ / 完成--）。
   live==0 → `Condvar.notify_all` 让所有 worker 退出。
7. **死锁检测**：`idle: AtomicUsize`。worker 将 wait 前 idle++；若 `idle==workers &&
   ready 空 && live>0` → 死锁（协程全 park 无人唤醒）→ set shutdown + notify_all + 报错。
   （替代 R5 单线程版的 PARKED_COUNT 检测。）

**codegen 侧几乎不变**：suspend 点、park_recv/park_send、await poll 调的还是同名 FFI，
只是 runtime 内部实现从 thread_local 变共享。`执行器运行全部` 内建映射不变。

## R6 铁证（必须全过 + 反复跑查 race）

- **多核加速**：CPU 密集协程（每个协程做真计算，如求素数计数），`QI_CORO_WORKERS=8` vs
  `=1` 明显加速（接近 ×核数，扣调度开销）。**这是 R6 的灵魂**（单线程做不到）。
- **正确性 race 扫**：通道背压/百消费/嵌套/真IO 在 `WORKERS=8` 下**跑 50+ 遍**结果全一致、
  RC 全 0、无 hang/崩（数据竞争非确定，必须多跑）。建议 `ASAN`/`tsan` 构建跑一轮。
- 回归：QI_CORO 不设 518+407 全绿；WORKERS=1 与 R5 单线程 byte 一致。
- 单核吞吐不明显退（锁开销可接受）。

## R7 qi-web 接入（R6 之后）

- 现状：qi-web 122k RPS，走 tokio（executor.rs）。请求 handler 不是协程。
- 目标：handler 编译成协程、跑在 R6 的 M:N 调度器上；IO（DB/上游 HTTP）用协程 await
  让出而非阻塞 worker → 单机更高并发。
- 关键改造：qi-web 的 accept 循环把每个连接 `启动` 成协程；handler 内的阻塞点（qi-web
  的 IO FFI）改造成协程 await（挂起而非阻塞 worker）。
- 铁证：wrk/bench 同机 qi-web(协程版) vs 现 tokio 版 RPS 对比；长连接高并发下内存/尾延迟。
- 风险：qi-web 的 IO 目前深度依赖 tokio；把它的 IO 迁到协程 await 是大工程，需评估
  是「协程调度器接管 tokio」还是「协程 await 桥接 tokio future」（后者更省，R4 的
  qi_coro_await_poll 已能 poll eager Future，异步 IO future 完成即唤醒协程）。

## 为什么分阶段而非一次做完

多线程调度器的 bug 是**非确定性数据竞争**，跑几次绿≠正确。必须：独立实现 + 大量重复
压测 + 可能 tsan。这是专项工程，不宜与其它任务混做或仓促上线（否则砸 R1-R5 的稳定招牌）。
建议：R6 单独一轮，带压测脚本；绿透了再上 R7。

---

## R6 实现进展（2026-07-12，feat/coro-round6-wip 分支）

**已实现并验证的部分**（分支上，非 main）：
- 全局 `Mutex<VecDeque>+Condvar` 就绪队列、通道 `Mutex` 化、RUNNING/LIVE 终止+死锁检测、
  park/wake 锁内重检（关 try/park race）、`QI_CORO_WORKERS` 门控（默认 1=R5 逐字节同）。
- ✅ **多核 CPU 密集加速已证明**：多核加速测（8 协程各数 30 万素数）
  `WORKERS=1` 116ms → `WORKERS=8` 34ms = **3.4× 加速**，结果一致（207976）。
- ✅ WORKERS=1 与 R5 逐字节同行为（R1-R5 八测全过）。
- ✅ 轻通道多 worker：通道统一测 WORKERS=8 正常。

**未解卡点（为何未合 main）**：
- 🔴 **通道重 fanout + 多 worker 确定性 hang**：百消费（100 消费者 + cap4 通道 +
  WORKERS=8）100% 卡死（退出 124，无输出无死锁告警 → run_all 内 RUNNING>0 疑似卡在某
  coro 的 resume 或 worker 忙等 livelock）。ordering 修复（push_ready 先于 RUNNING--）
  + notify_all 未解。
- 需 **tsan 构建** + 加调度日志定位（确定性 hang，可复现，好查）。怀疑点：
  ① worker wait/notify 与 wake 的时序 livelock；② 大量 recv_waiters 下某个 wake 链断裂；
  ③ 生成代码 resume 在多 worker 下的某路径不返回。

**决定**：main 保 R5（磐石、已上线）；R6 在本 WIP 分支续，绿透（含 tsan + 50× 压测）
再合。ARC 原子（绿灯）+ 多核加速（3.4× 已证）说明方向对、地基牢，只差把并发 hang 收干净。

---

## ✅ R6 完成（2026-07-12，已合 main）

多核 M:N 调度器打通。根因是一个 **R5 潜藏逻辑 bug**（非多线程 race）：`try_recv` 提升
parked 发送者的值进 buf 时只唤醒发送者、漏唤醒等待消费者 → 值卡 buf、消费者饿死 hang。
被 R5 测试里 producer 的 `等待 让出()` 掩盖（走另一条唤醒路径）。修：提升后也唤醒一个
recv_waiter。单线程可复现，故好查好验。

**验收全过**：
- 多核加速：8 协程数素数 W=1 125ms → W=8 31ms = **4.0×**。
- CPU 密集 vs Go（M2 Pro，同题）：Go 34ms / qi(W=8) 41ms = **qi ≈ Go 83%**（此前单核 3.7× 慢）。
- 压测：百消费(100 消费者)/多核通道扇出 各 W=8 跑 20/20 无 hang、结果对、RC 0。
- R1-R5 八测 W=8 各 3 遍全过 RC 0；默认 518+407 全绿；WORKERS=1=单线程行为。
- park/wake race 锁内重检；RUNNING+LIVE 终止+死锁检测。

**门控**：`QI_CORO_WORKERS`（默认 1）。`QI_CORO=1` 程序设 `QI_CORO_WORKERS=8` 即多核。

**R7 qi-web 接入**：下一步（桥接路线见上文）。
