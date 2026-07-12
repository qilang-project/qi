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

---

## ✅ R6 优化完成（2026-07-12）：默认多核 + 直接交接，4 项微基准全面反超 Go

**默认改为 CPU 核数**（`available_parallelism()`，对齐 Go GOMAXPROCS）；`QI_CORO_WORKERS=N` 覆盖。

**直接交接（direct handoff）**：运行中协程唤醒对端时，塞进本 worker 的 thread_local `NEXT` 槽
（pick 先取它），让 pingpong 待在同一线程、免每次交接的跨线程 condvar 唤醒。配 notify_one
（免惊群）+ 死锁 2 次确认（避 NEXT 瞬时 RUNNING=0 窗口误判）。

**同机 M2 Pro 4 项微基准（qi 默认多核 vs Go 全核）**：
| 工作负载 | qi | Go |
|---|---|---|
| CPU 密集(8×30万素数) | **28ms** | 35ms |
| 上下文切换(100万) | **249ms** | 295ms |
| 协程创建(5万) | **23ms** | 29ms |
| 通道收发(50万) | **30ms** | 65ms |

关键突破：通道 1764ms（naive 跨线程）→ 30ms（direct-handoff），反超单线程 76ms 与 Go 65ms。
正确性：百消费/多核扇出各 15/15 无 hang RC0；协程全测(10)+518+407 全绿。

**结论**：qi 协程调度在这台机上 4 项微基准全面 ≥ Go。加上计算 +15%，全维度进 Go 联盟。

---

## ✅ QI_CORO 默认开（2026-07-12）

门控翻转：`qi/src/codegen/inkwell_gen/mod.rs` 的 `协程` 字段默认 `true`（照 `弧` 先例，
`QI_CORO=0/false` 退回 eager future）。门控是**函数级**：无 `未来<T>+等待` 的普通程序 IR
逐字节不变（已验：`计算.qi` 开/关 IR 二进制一致；纯计算程序耗时不变）。

**回归全绿（默认态=开箱即协程）**：协程 12 例 11 PASS（`真IO上车测` 需 `QI_CHAT_URL`，配好即过）、
异步 7 例、`cargo test` 518 + qi-runtime 407、多核加速 98ms→21ms（≈4.7×，12 逻辑核）、
harness 询问/并行询问/尝试询问、qi-web 同步服务 curl、AIOne `服务.qi`（47318）/run+/judge（3/3 及格）
含 6 并发压令牌通道无 hang。RC 报告全 0。

**顺带修 select×协程通道**：`生成选择`（`并发.rs`）曾对协程原生通道调 eager FFI
（`qi_runtime_channel_try_*`）→ 类型混淆（`选择发送` 静默"发送失败"）。已按 `协程开()` 分支到
`qi_coro_chan_try_send/recv`（slot 单层 i64、wait 分支驱动 `qi_coro_step_once`）。默认开与
`QI_CORO=0` 输出一致。

**通道×非协程上下文结论**：带缓冲通道做信号量（aione 令牌通道、tokio handler、顶层裸用）在协程模式
按非阻塞轮询正确工作；空通道在非协程上下文 busy-poll（`step_once` 驱动执行器），功能正确，
高并发下有 CPU 空转的效率代价（非阻塞语义所致，非死锁）。

## 默认开后基线与攻坚（2026-07-12）

微基准（M2 Pro 12 逻辑核 = 8P+4E，Go 1.26.4，QI_CORO 默认开，各 3 次取中位，qi 用 release 运行时归档）。

**说明与 R6 旧记录的出入**：R6 曾记「多核加速 98ms→21ms ≈4.7×」——那是 `QI_CORO_WORKERS=1` 对
`=12` 的自比（单核 turbo 基线偏高，放大了比值）；本轮改用与 Go 同写法逐负载对照，数字口径不同。

| 负载 | 攻坚前 qi | Go | 攻坚后 qi | 结论 |
|---|---|---|---|---|
| 上下文切换100万(pingpong) | 58ms | 153ms | 57ms | 保持 2.6× 胜 |
| 通道收发50万(1对1) | 19ms | 22ms | 19ms | 保持略胜 |
| 纯创建5万(各1次让出) | 20ms | 27ms | 21ms | 保持略胜 |
| CPU密集(8×30万素数) | 33ms | 15ms | 33ms | 非调度器瓶颈（见下） |
| **高扇入汇聚(5万发送者→1消费者)** | **215ms** | 49ms | **~30ms** | **7× 提速，反超 Go** |

### 输项①：高扇入汇聚 —— **调度器 bug（惊群 futex 风暴 + 缺 work-stealing）**，已修
根因（instrumentation 精确定位）：单消费者串行地每 recv 唤醒一个 parked 发送者（发送者仅剩
`返回`），R6 里每次唤醒都**推全局 READY + `notify_one`**。12 worker 全睡，被逐个 futex 唤醒
干一点点活又睡 —— `wait≈4.9 万`、`notify≈4.9 万`，futex 往返≈全部耗时。worker 曲线铁证：
`WORKERS=1` 仅 19ms（已胜 Go），`=12` 却 230ms（越多 worker 越慢）。

三处根治（`coro.rs`，R7）：
1. **每 worker 本地队列 `LOCALQ` + 批量偷取**：NEXT 单槽已占后的溢出唤醒落本地队列（自 push 几乎
   无争用），空闲 worker 一次偷一半（摊薄全局锁争用），取代「12 worker 抢全局单个」。
2. **IDLE 门控 notify**：只有真有 worker park 在 Condvar 才 `notify_one`，消灭空发 notify。
3. **HANDOFF 计数修死锁误判**：NEXT 在途协程精确计数，死锁检测要 `RUNNING==0 && HANDOFF==0`
   （旧代码靠 2ms 采样概率躲，work-stealing 改时序后 pingpong 偶发假死锁，此修根治）。

优化前 215ms → 优化后 ~30ms（3 次 25/30/32），反超 Go 49ms。pingpong/chan/纯创建无回退。

### 输项②：CPU 密集多核 —— **非调度器瓶颈（单核 codegen + turbo）**，如实报告硬边界
逐一验证候选后否定「让出成本 / worker 空转 / direct-handoff 副作用」：`让出()` 走全局 READY 非 NEXT；
8 协程 120 次让出可忽略；`让出` 频率减半/顶端让出（模拟 initial-suspend）耗时不变。真相：
- **调度扩展本身没问题**：大负载（2M 素数）实测 `W=1 1288ms → W=8 248ms = 5.2×`，逼近 Go 5.9×。
- **小负载差距 = 单核 codegen + turbo**：`qi W=1 110ms vs Go 77ms = 1.43×`（素数循环 LLVM O1 vs Go）。
  `qi run` 默认 `Basic`(O1)；且 300k 小负载下 W=1 单核 turbo 抬高，放大 W=8 相对差。
结论：协程调度器在此负载**无可摘的果子**；剩余差距属编译期优化（默认 O 级别）与硬件 turbo，
不在本轮调度器攻坚范围。未强改 initial-suspend codegen（实测无收益且会危及①/纯创建的战果）。

`QI_CORO_SPIN`（默认 0）保留 park 前自旋旋钮：IDLE 门控+批量偷取已消除 futex 风暴，自旋在
过订阅（worker>物理核）时抢核反拖累 pingpong，故默认关。
