//! ARC 循环引用静态检测 Linter —— 编译期警告可能的引用环。
//!
//! Qi 用纯引用计数（ARC，同 Swift）回收堆对象，**无环收集器**。一旦对象图里
//! 出现强引用环（A 强持有 B、B 又强持有 A），环上每个对象的引用计数永远 ≥1，
//! 到不了 0，就永久泄漏。这是文档记载的头号弱点。
//!
//! 本模块在「类型全部登记后、codegen 主体前」跑一趟静态分析：
//!   1) 构建 **RC 强引用有向图**：节点 = 每个 RC 类型（结构体 / 装箱枚举）；
//!      边 A→B = A 通过一个强引用字段/载荷直接持有 B。
//!   2) 用 Tarjan 求强连通分量（SCC）。含 ≥1 条边的 SCC（自环或多节点环）即为环。
//!   3) 每个成环 SCC 报一条诊断，列出一条规范化的环路径（类型名链）。
//!
//! 开关（环境变量 `QI_LINT`）：
//!   - 默认（未设 / 其它值）：开启，环以 **警告** 打到 stderr，不阻断编译。
//!   - `QI_LINT=0` / `false`：完全关闭。
//!   - `QI_LINT=strict`：环升为 **编译错误**（返回 Err，供 CI 卡关）。
//!
//! 入图规则（保守，只报真环，避免误报）：
//!   - **节点**：结构体、装箱枚举。标量（整数/浮点/布尔）与字符串是叶子，不入图
//!     （字符串虽是 immortal-or-RC，但不能再持有别的 RC 类型，不会成环）。
//!     无载荷枚举是纯 i64 tag、非 RC，也不入图。
//!   - **边**（强引用）：
//!       * 结构体字段类型是 结构体(i)          → 出边到该结构体
//!       * 结构体字段类型是 装箱枚举(i)        → 出边到该枚举
//!       * 结构体字段类型是 数组<结构体(i)>    → 出边到该结构体（自引用容器算真环）
//!       * 装箱枚举每个变体的每个载荷同上（选项<T>/结果<T>/用户泛型枚举均含在内）
//!   - 数组元素若是「指针」语义（装箱枚举/字符串/函数值）——`元素类型` 丢失了具体
//!     索引，无法追踪，保守 **不建边**（局限，见文件末尾说明）。
//!   - 通道/未来/函数值字段 **不建边**（通道/未来非 RC 拥有语义；函数值→闭包 env
//!     捕获类型在此阶段拿不到，见局限说明）。

use super::类型::{元素类型, Qi类型};
use super::类型检查::符号表;
use std::collections::{HashMap, HashSet, VecDeque};

/// Linter 模式（由 QI_LINT 环境变量决定）。
enum 模式 {
    关闭,
    警告,
    严格,
}

fn 读模式() -> 模式 {
    match std::env::var("QI_LINT") {
        Ok(v) if v == "0" || v.eq_ignore_ascii_case("false") || v.eq_ignore_ascii_case("off") => {
            模式::关闭
        }
        Ok(v) if v.eq_ignore_ascii_case("strict") || v.eq_ignore_ascii_case("error") => 模式::严格,
        _ => 模式::警告,
    }
}

/// RC 强引用图。节点按连续下标编号；`名字[i]` 为节点 i 的人类可读类型名。
struct 引用图 {
    邻接: Vec<Vec<usize>>,
    名字: Vec<String>,
}

/// 结构体/装箱枚举 在图里的键（用于去重建节点）。
#[derive(PartialEq, Eq, Hash, Clone, Copy)]
enum 键 {
    结构体(u32),
    枚举(u32),
}

/// 由一个字段/载荷类型求它强持有的目标节点键（无强引用则 None）。
/// 枚举一律查注册表的 `装箱` 标志判定 RC —— 因为自引用枚举的载荷类型在
/// `解析枚举变体` 阶段可能记成 `枚举(i)`（彼时 i 的装箱标志尚未回填），
/// 不能只信类型 tag 的 枚举/装箱枚举 之分。
fn 出边目标(t: Qi类型, 符号: &符号表) -> Option<键> {
    match t {
        Qi类型::结构体(i) => Some(键::结构体(i)),
        Qi类型::枚举(i) | Qi类型::装箱枚举(i) => {
            if 符号.枚举.get(i as usize).map(|e| e.装箱).unwrap_or(false) {
                Some(键::枚举(i))
            } else {
                None // 无载荷枚举 = 纯 i64 tag，非 RC
            }
        }
        // 数组<结构体>：自引用容器（树/图节点的经典环）要报。
        // 数组<指针语义元素>（装箱枚举/字符串/函数值）元素类型已丢索引，不建边。
        Qi类型::数组(元素类型::结构体(i)) => Some(键::结构体(i)),
        _ => None,
    }
}

/// 类型名的人类可读渲染：去掉 `结构体N_` 内部前缀、把 `模板$实参` 还原成 `模板<实参>`。
/// 如 `选项$结构体0_节点` → `选项<节点>`，`对$整数` → `对<整数>`，`账户` → `账户`。
fn 显示名(名: &str) -> String {
    match 名.split_once('$') {
        Some((模板, 实参)) => format!("{}<{}>", 清理段(模板), 显示名(实参)),
        None => 清理段(名),
    }
}

fn 清理段(段: &str) -> String {
    // 结构体实例名片段形如 `结构体{N}_{原名}`（见 符号表::类型名）；还原为原名。
    if let Some(rest) = 段.strip_prefix("结构体") {
        if let Some(us) = rest.find('_') {
            return rest[us + 1..].to_string();
        }
    }
    段.to_string()
}

/// 从符号表构建 RC 强引用有向图。
fn 构建图(符号: &符号表) -> 引用图 {
    // 1) 建节点：所有结构体 + 所有「装箱」枚举（无载荷枚举非 RC，跳过）。
    let mut 键到号: HashMap<键, usize> = HashMap::new();
    let mut 名字: Vec<String> = Vec::new();

    for (i, s) in 符号.结构体.iter().enumerate() {
        键到号.insert(键::结构体(i as u32), 名字.len());
        名字.push(显示名(&s.名字));
    }
    for (j, e) in 符号.枚举.iter().enumerate() {
        if e.装箱 {
            键到号.insert(键::枚举(j as u32), 名字.len());
            名字.push(显示名(&e.名字));
        }
    }

    let mut 邻接: Vec<Vec<usize>> = vec![Vec::new(); 名字.len()];
    let 加边 = |邻接: &mut Vec<Vec<usize>>, 源键: 键, 类型: Qi类型| {
        if let (Some(&u), Some(目标)) = (键到号.get(&源键), 出边目标(类型, 符号)) {
            if let Some(&v) = 键到号.get(&目标) {
                if !邻接[u].contains(&v) {
                    邻接[u].push(v);
                }
            }
        }
    };

    // 2) 结构体字段边
    for (i, s) in 符号.结构体.iter().enumerate() {
        for &ft in &s.字段类型 {
            加边(&mut 邻接, 键::结构体(i as u32), ft);
        }
    }
    // 3) 装箱枚举载荷边（含 选项/结果/用户泛型枚举的具体实例）
    for (j, e) in 符号.枚举.iter().enumerate() {
        if !e.装箱 {
            continue;
        }
        for 变体 in &e.变体 {
            for &pt in &变体.载荷 {
                加边(&mut 邻接, 键::枚举(j as u32), pt);
            }
        }
    }

    引用图 { 邻接, 名字 }
}

/// Tarjan 求强连通分量。返回每个 SCC 的节点下标列表。
fn 求scc(图: &引用图) -> Vec<Vec<usize>> {
    let n = 图.邻接.len();
    let mut 序号 = vec![usize::MAX; n];
    let mut 低链 = vec![0usize; n];
    let mut 在栈 = vec![false; n];
    let mut 栈: Vec<usize> = Vec::new();
    let mut 计数 = 0usize;
    let mut 结果: Vec<Vec<usize>> = Vec::new();

    // 迭代式 Tarjan（避免深图递归爆栈）。帧 = (节点, 下一个待访问邻居下标)。
    for 起点 in 0..n {
        if 序号[起点] != usize::MAX {
            continue;
        }
        let mut 帧栈: Vec<(usize, usize)> = vec![(起点, 0)];
        while let Some(&(v, ei)) = 帧栈.last() {
            if ei == 0 {
                序号[v] = 计数;
                低链[v] = 计数;
                计数 += 1;
                栈.push(v);
                在栈[v] = true;
            }
            if ei < 图.邻接[v].len() {
                let w = 图.邻接[v][ei];
                帧栈.last_mut().unwrap().1 += 1;
                if 序号[w] == usize::MAX {
                    帧栈.push((w, 0));
                } else if 在栈[w] {
                    低链[v] = 低链[v].min(序号[w]);
                }
            } else {
                // v 处理完：若是 SCC 根，弹出整簇
                if 低链[v] == 序号[v] {
                    let mut 簇 = Vec::new();
                    loop {
                        let x = 栈.pop().unwrap();
                        在栈[x] = false;
                        簇.push(x);
                        if x == v {
                            break;
                        }
                    }
                    结果.push(簇);
                }
                帧栈.pop();
                if let Some(&(父, _)) = 帧栈.last() {
                    低链[父] = 低链[父].min(低链[v]);
                }
            }
        }
    }
    结果
}

/// 一个 SCC 是否真的成环：多节点必成环；单节点仅当带自环才算。
fn 是环(图: &引用图, 簇: &[usize]) -> bool {
    if 簇.len() > 1 {
        return true;
    }
    let v = 簇[0];
    图.邻接[v].contains(&v)
}

/// 在一个成环 SCC 内找一条规范化的简单环路径（从字典序最小的节点名起）。
/// 返回节点下标序列 `[起, …, 终]`，环即 起→…→终→起。
fn 环路径(图: &引用图, 簇: &[usize]) -> Vec<usize> {
    let 簇集: HashSet<usize> = 簇.iter().copied().collect();
    // 起点：SCC 内类型名字典序最小者（路径规范化，同一环只报一种链）
    let 起 = *簇
        .iter()
        .min_by(|&&a, &&b| 图.名字[a].cmp(&图.名字[b]))
        .unwrap();

    // 自环
    if 图.邻接[起].contains(&起) {
        return vec![起];
    }

    // BFS 求 起 → 起 的最短环：记前驱，找一条 起→…→u 且 u→起 的最短路径。
    let mut 前驱: HashMap<usize, usize> = HashMap::new();
    let mut 队: VecDeque<usize> = VecDeque::new();
    let mut 已访: HashSet<usize> = HashSet::new();
    队.push_back(起);
    已访.insert(起);
    let mut 闭合于: Option<usize> = None;
    'bfs: while let Some(v) = 队.pop_front() {
        for &w in &图.邻接[v] {
            if !簇集.contains(&w) {
                continue;
            }
            if w == 起 {
                // v → 起 闭合成环
                闭合于 = Some(v);
                break 'bfs;
            }
            if 已访.insert(w) {
                前驱.insert(w, v);
                队.push_back(w);
            }
        }
    }

    let mut 路径 = vec![起];
    if let Some(mut u) = 闭合于 {
        let mut 反: Vec<usize> = Vec::new();
        while u != 起 {
            反.push(u);
            u = *前驱.get(&u).unwrap();
        }
        反.reverse();
        路径.extend(反);
    }
    路径
}

/// 把一条环路径渲染成 `账户 → 交易 → 账户` 形式（结尾回到起点，直观呈现闭环）。
fn 渲染环(图: &引用图, 路径: &[usize]) -> String {
    let mut 链: Vec<&str> = 路径.iter().map(|&i| 图.名字[i].as_str()).collect();
    链.push(图.名字[路径[0]].as_str()); // 回到起点
    链.join(" → ")
}

/// 收集版（供 `qi doctor` 用）：跑同一套 图→SCC→成环判定，但**只返回**渲染好的
/// 环链列表（如 `["账户 → 交易 → 账户"]`），既不打 stderr 也不受 QI_LINT 开关影响。
/// 无环时返回空 Vec。与 `检测循环引用` 共用底层 `构建图/求scc/是环/环路径/渲染环`，
/// 保证 doctor 报告与编译期警告口径一致。
pub(super) fn 收集循环引用环(符号: &符号表) -> Vec<String> {
    let 图 = 构建图(符号);
    let sccs = 求scc(&图);

    let mut 环链: Vec<String> = Vec::new();
    for 簇 in &sccs {
        if 是环(&图, 簇) {
            let 路径 = 环路径(&图, 簇);
            环链.push(渲染环(&图, &路径));
        }
    }
    环链.sort();
    环链.dedup();
    环链
}

/// 主入口：检测循环引用。按 QI_LINT 模式打警告 / 报错 / 静默。
/// 在 mod.rs 里类型全部登记后、codegen 主体前调用一次。
pub(super) fn 检测循环引用(符号: &符号表) -> Result<(), String> {
    let 模式 = match 读模式() {
        模式::关闭 => return Ok(()),
        m => m,
    };

    let 图 = 构建图(符号);
    let sccs = 求scc(&图);

    let mut 环链: Vec<String> = Vec::new();
    for 簇 in &sccs {
        if 是环(&图, 簇) {
            let 路径 = 环路径(&图, 簇);
            环链.push(渲染环(&图, &路径));
        }
    }
    if 环链.is_empty() {
        return Ok(());
    }

    // 去重（不同 SCC 渲染出相同链的极端情形）+ 稳定排序
    环链.sort();
    环链.dedup();

    let 前缀 = match 模式 {
        模式::严格 => "[环检测] 错误",
        _ => "[环检测] 警告",
    };
    let mut 报告 = String::new();
    for 链 in &环链 {
        报告.push_str(&format!(
            "{}: 检测到可能的循环引用（引用环 → 内存泄漏）: {}\n",
            前缀, 链
        ));
    }
    报告.push_str(
        "  提示: Qi 使用纯引用计数（ARC）回收，无环收集器，引用环上的对象计数永不归零会永久泄漏。\n         考虑把环上其中一条边改用弱引用打破环（暂无 weak 语法，可用整数 id 间接引用替代直接持有）。",
    );

    match 模式 {
        模式::严格 => Err(报告),
        _ => {
            eprintln!("{}", 报告);
            Ok(())
        }
    }
}
