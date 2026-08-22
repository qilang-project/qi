//! 差分模糊测试 —— 随机生成**良类型**的 qi 程序，比对三份答案。
//!
//! # 为什么是差分，不是崩溃 fuzz
//!
//! 这个编译器最难查的错不是崩，是**跑完了、退出码 0、答案是错的**。崩溃
//! fuzz 抓不到这一类：程序没崩，它就认为过了。所以这里换个路子 —— 生成
//! 程序的同时用一个 Rust 参考求值器算出**期望输出**，构造上就知道正确答案。
//!
//! 三份答案两两必须一致：
//!
//!   1. 参考求值器（纯 Rust，不碰编译器）
//!   2. `优化=无` 编译出来的可执行文件
//!   3. `优化=最大` 编译出来的可执行文件
//!
//! 1 vs 2 抓 codegen 本身译错；**2 vs 3 抓优化管线**（同一份源码两个优化级别
//! 跑出不同结果 = 一定有 miscompile，不需要知道谁对）。第二条特别便宜，
//! 却是一整类 bug 唯一的自动化出口。
//!
//! # 生成的子集，以及每条限制的理由
//!
//! 只生成 整数 与 布尔。**浮点数故意不生成**：`-O0` 和 `-O2` 下浮点结果
//! 合法地可以不同（重结合、FMA 合并），拿它做差分只会刷假红。
//!
//! - **除数只取非零字面量，且排除 -1**。LLVM 的 sdiv 除零是 UB，
//!   `i64::MIN / -1` 也是 UB —— UB 上的差分毫无意义，编译器怎么译都不算错。
//! - **函数只能调编号更小的函数**（f2 可以调 f0/f1，反之不行）。没有递归，
//!   构造上保证停机。
//! - **循环是计数器形式**：`当 i < 上界 { …; i = i + 步长; }`，上界和步长都是
//!   正字面量，迭代次数有上限。求值器再加一道 3 万次的保险，撞上就丢弃这个
//!   程序（不算失败）—— 生成器有 bug 时不至于把测试挂死。
//! - **整数字面量一律非负**，负数由减法产生。省掉「qi 的一元负号怎么写」
//!   这个跟本测试无关的变量。
//!
//! `与`/`或` 短路，求值器用 Rust 的 `&&`/`||` 对齐 —— 这是本差分器跑出的
//! 第一个真发现（种子 777024：qi 当时**不短路**，教科书守卫
//! `如果 (d != 0 与 (100 / d) > 5)` 直接除零崩掉），编译器随后改了。
//!
//! 溢出**照生成不误**：实测 qi 的 `+ - *` 是二补数回绕
//! （`9223372036854775807 + 1` → `-9223372036854775808`），
//! 求值器用 `wrapping_*` 精确对齐，所以回绕路径也在覆盖里。
//!
//! # 跑
//!
//! 默认 `#[ignore]`，因为每个程序要编译链接两次，比整套单测还慢，
//! 不该拖慢 `cargo test`。走 `make fuzz`（已挂进 `make ci`），或者：
//!
//!   cargo test --release --test 差分模糊 -- --ignored --nocapture
//!
//! 程序个数用 `QI_FUZZ_COUNT` 调（默认 12），起始种子用 `QI_FUZZ_SEED`。
//! **种子固定** —— CI 每次跑同一批程序，红了就能原样复现；想要新覆盖面
//! 就换种子，而不是让它每次随机、红一次再也复现不出来。
//!
//! 失败时会把源码、期望、两个实际输出全打出来，并保留 .qi 文件路径。

use qi_compiler::config::{CompilerConfig, OptimizationLevel};
use qi_compiler::QiCompiler;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ─────────────────────────── 随机数 ───────────────────────────

/// xorshift64*，自带而不用 rand crate：种子→程序必须**完全确定**，
/// 跨机器跨版本都是同一批程序，否则「CI 红了本地复现不出来」。
struct 骰子(u64);

impl 骰子 {
    fn 新(种子: u64) -> Self {
        // 先用 splitmix64 打散再进 xorshift。
        //
        // 第一版写的是 `骰子(种子 | 1)`（只为躲开「xorshift 卡死在 0」），
        // 结果 `777000 | 1 == 777001 | 1` —— **相邻的偶奇种子生成完全相同的
        // 程序**，一批 400 个实际只有 200 个不同的，白烧一半机时。
        // 逐程序打印耗时那行日志才让它现原形（连续两行源码行数一模一样）。
        let mut z = 种子.wrapping_add(0x9E3779B97F4A7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^= z >> 31;
        骰子(if z == 0 { 1 } else { z })
    }
    fn 下一个(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    /// [0, n) —— n 必须 > 0
    fn 取(&mut self, n: u64) -> u64 {
        self.下一个() % n
    }
    fn 抛硬币(&mut self, 分之一: u64) -> bool {
        self.取(分之一) == 0
    }
}

// ─────────────────────────── 语法树 ───────────────────────────

#[derive(Clone)]
enum 整表达式 {
    字面量(i64),
    变量(String),
    二元(Box<整表达式>, 算符, Box<整表达式>),
    /// 除数只能是非零、非 -1 的字面量 —— 见模块文档
    除(Box<整表达式>, i64),
    取余(Box<整表达式>, i64),
    调用(usize, Vec<整表达式>),
}

#[derive(Clone, Copy)]
enum 算符 {
    加,
    减,
    乘,
}

impl 算符 {
    fn 符号(self) -> &'static str {
        match self {
            算符::加 => "+",
            算符::减 => "-",
            算符::乘 => "*",
        }
    }
    fn 算(self, 甲: i64, 乙: i64) -> i64 {
        match self {
            算符::加 => 甲.wrapping_add(乙),
            算符::减 => 甲.wrapping_sub(乙),
            算符::乘 => 甲.wrapping_mul(乙),
        }
    }
}

#[derive(Clone)]
enum 布表达式 {
    比较(整表达式, 比较符, 整表达式),
    与(Box<布表达式>, Box<布表达式>),
    或(Box<布表达式>, Box<布表达式>),
}

#[derive(Clone, Copy)]
enum 比较符 {
    等于,
    不等,
    小于,
    大于,
    不大于,
    不小于,
}

impl 比较符 {
    fn 符号(self) -> &'static str {
        match self {
            比较符::等于 => "==",
            比较符::不等 => "!=",
            比较符::小于 => "<",
            比较符::大于 => ">",
            比较符::不大于 => "<=",
            比较符::不小于 => ">=",
        }
    }
    fn 判(self, 甲: i64, 乙: i64) -> bool {
        match self {
            比较符::等于 => 甲 == 乙,
            比较符::不等 => 甲 != 乙,
            比较符::小于 => 甲 < 乙,
            比较符::大于 => 甲 > 乙,
            比较符::不大于 => 甲 <= 乙,
            比较符::不小于 => 甲 >= 乙,
        }
    }
}

#[derive(Clone)]
enum 语句 {
    声明(String, 整表达式),
    赋值(String, 整表达式),
    分支(布表达式, Vec<语句>, Vec<语句>),
    /// 计数循环：`当 <计数器> < <上界> { <体>; <计数器> = <计数器> + <步长>; }`
    循环 {
        计数器: String,
        上界: i64,
        步长: i64,
        体: Vec<语句>,
    },
    打印(整表达式),
}

struct 函数定义 {
    序号: usize,
    形参: Vec<String>,
    体: Vec<语句>,
    返回: 整表达式,
}

struct 程序 {
    函数表: Vec<函数定义>,
    主体: Vec<语句>,
}

// ─────────────────────────── 生成 ───────────────────────────

struct 生成器<'a> {
    随机: &'a mut 骰子,
    /// 当前作用域里可读的整数变量名
    可见变量: Vec<String>,
    /// **可读但不许赋值**的名字 —— 目前只有循环计数器。
    ///
    /// 循环体里给计数器赋值是合法 qi，但那样循环可以永不终止，而永不终止的
    /// 程序对差分测试没有任何价值：参考求值器撞保险丝丢弃，编译出来的那份
    /// 却会把 harness 挂死在 Command::output 上。与其事后靠超时收拾，
    /// 不如构造上就不生成。
    禁赋值: Vec<String>,
    /// 已经生成好的函数（序号 → 形参个数），只允许调用这些 —— 保证无递归
    已有函数: Vec<usize>,
    下一个变量号: usize,
}

impl 生成器<'_> {
    fn 新变量名(&mut self) -> String {
        let n = self.下一个变量号;
        self.下一个变量号 += 1;
        format!("v{}", n)
    }

    /// 非零、非 -1 的除数
    fn 安全除数(&mut self) -> i64 {
        let a = (self.随机.取(60) + 2) as i64; // 2..=61
        if self.随机.抛硬币(2) {
            -a
        } else {
            a
        }
    }

    fn 整式(&mut self, 深: u32) -> 整表达式 {
        // 到底了就只出叶子，否则表达式会指数膨胀
        if 深 == 0 || self.随机.抛硬币(3) {
            return if !self.可见变量.is_empty() && self.随机.抛硬币(2) {
                let i = self.随机.取(self.可见变量.len() as u64) as usize;
                整表达式::变量(self.可见变量[i].clone())
            } else {
                // 偶尔来个极大值，把回绕路径打进去
                let v = if self.随机.抛硬币(12) {
                    (i64::MAX as u64 - self.随机.取(3)) as i64
                } else {
                    self.随机.取(200) as i64
                };
                整表达式::字面量(v)
            };
        }
        match self.随机.取(10) {
            0..=4 => {
                let 符 = match self.随机.取(3) {
                    0 => 算符::加,
                    1 => 算符::减,
                    _ => 算符::乘,
                };
                let 左 = self.整式(深 - 1);
                let 右 = self.整式(深 - 1);
                整表达式::二元(Box::new(左), 符, Box::new(右))
            }
            5..=6 => {
                let 左 = self.整式(深 - 1);
                let d = self.安全除数();
                整表达式::除(Box::new(左), d)
            }
            7 => {
                let 左 = self.整式(深 - 1);
                let d = self.安全除数();
                整表达式::取余(Box::new(左), d)
            }
            _ => {
                if self.已有函数.is_empty() {
                    let 左 = self.整式(深 - 1);
                    let 右 = self.整式(深 - 1);
                    return 整表达式::二元(Box::new(左), 算符::加, Box::new(右));
                }
                let i = self.随机.取(self.已有函数.len() as u64) as usize;
                let (序号, 元数) = (i, self.已有函数[i]);
                let 实参: Vec<整表达式> = (0..元数).map(|_| self.整式(深 - 1)).collect();
                整表达式::调用(序号, 实参)
            }
        }
    }

    fn 布式(&mut self, 深: u32) -> 布表达式 {
        if 深 == 0 || self.随机.抛硬币(2) {
            let 符 = match self.随机.取(6) {
                0 => 比较符::等于,
                1 => 比较符::不等,
                2 => 比较符::小于,
                3 => 比较符::大于,
                4 => 比较符::不大于,
                _ => 比较符::不小于,
            };
            let 左 = self.整式(2);
            let 右 = self.整式(2);
            return 布表达式::比较(左, 符, 右);
        }
        let 左 = self.布式(深 - 1);
        let 右 = self.布式(深 - 1);
        if self.随机.抛硬币(2) {
            布表达式::与(Box::new(左), Box::new(右))
        } else {
            布表达式::或(Box::new(左), Box::new(右))
        }
    }

    fn 语句块(&mut self, 条数: usize, 深: u32) -> Vec<语句> {
        let 存档 = self.可见变量.clone();
        let mut 出: Vec<语句> = Vec::new();
        for _ in 0..条数 {
            出.push(self.一条语句(深));
        }
        // 块内声明的变量不该泄漏到块外
        self.可见变量 = 存档;
        出
    }

    /// 可以出现在赋值左边的名字：可见的，减去计数器
    fn 可赋值名单(&self) -> Vec<String> {
        self.可见变量
            .iter()
            .filter(|n| !self.禁赋值.contains(n))
            .cloned()
            .collect()
    }

    fn 一条语句(&mut self, 深: u32) -> 语句 {
        let 可赋值 = self.可赋值名单();
        match self.随机.取(10) {
            0..=3 => {
                let e = self.整式(3);
                let 名 = self.新变量名();
                self.可见变量.push(名.clone());
                语句::声明(名, e)
            }
            4..=5 if !可赋值.is_empty() => {
                let i = self.随机.取(可赋值.len() as u64) as usize;
                let 名 = 可赋值[i].clone();
                let e = self.整式(3);
                语句::赋值(名, e)
            }
            6..=7 if 深 > 0 => {
                let c = self.布式(1);
                let 真条数 = 1 + self.随机.取(2) as usize;
                let 真支 = self.语句块(真条数, 深 - 1);
                let 假条数 = 1 + self.随机.取(2) as usize;
                let 假支 = self.语句块(假条数, 深 - 1);
                语句::分支(c, 真支, 假支)
            }
            8 if 深 > 0 => {
                // 计数器在发射时是循环**外面**那一层的 变量 声明，所以它出了
                // 循环还看得见；但**体内不许给它赋值**，否则循环可能不终止。
                let 计数器 = self.新变量名();
                self.可见变量.push(计数器.clone());
                self.禁赋值.push(计数器.clone());
                let 体条数 = 1 + self.随机.取(2) as usize;
                let 体 = self.语句块(体条数, 深 - 1);
                self.禁赋值.pop();
                语句::循环 {
                    计数器,
                    上界: (self.随机.取(12) + 1) as i64,
                    步长: (self.随机.取(3) + 1) as i64,
                    体,
                }
            }
            _ => {
                let e = self.整式(3);
                语句::打印(e)
            }
        }
    }
}

fn 生成程序(骰: &mut 骰子) -> 程序 {
    let 函数个数 = 骰.取(3) as usize; // 0..=2
    let mut 函数表: Vec<函数定义> = Vec::new();
    let mut 已有函数: Vec<usize> = Vec::new();

    for 序号 in 0..函数个数 {
        let 元数 = 骰.取(3) as usize; // 0..=2
        let 形参: Vec<String> = (0..元数).map(|i| format!("p{}", i)).collect();
        let mut g = 生成器 {
            随机: 骰,
            可见变量: 形参.clone(),
            禁赋值: Vec::new(),
            已有函数: 已有函数.clone(),
            下一个变量号: 0,
        };
        let 体条数 = 1 + g.随机.取(3) as usize;
        let 体 = g.语句块(体条数, 2);
        // 返回值在形参环境里算（块内声明的已经出栈了）
        let mut g2 = 生成器 {
            随机: 骰,
            可见变量: 形参.clone(),
            禁赋值: Vec::new(),
            已有函数: 已有函数.clone(),
            下一个变量号: 100,
        };
        let 返回 = g2.整式(3);
        函数表.push(函数定义 {
            序号,
            形参,
            体,
            返回,
        });
        已有函数.push(元数);
    }

    let mut g = 生成器 {
        随机: 骰,
        可见变量: Vec::new(),
        禁赋值: Vec::new(),
        已有函数,
        下一个变量号: 0,
    };
    // 至少有一条打印，否则这个程序什么都没验到
    let 主体条数 = 3 + g.随机.取(4) as usize;
    let mut 主体 = g.语句块(主体条数, 2);
    let e = g.整式(3);
    主体.push(语句::打印(e));
    程序 { 函数表, 主体 }
}

// ─────────────────────────── 发射 qi 源码 ───────────────────────────

fn 发射整式(e: &整表达式, 出: &mut String) {
    match e {
        整表达式::字面量(v) => {
            // 只发非负字面量（生成器保证），负数由减法产生
            出.push_str(&v.to_string());
        }
        整表达式::变量(名) => 出.push_str(名),
        整表达式::二元(左, 符, 右) => {
            出.push('(');
            发射整式(左, 出);
            出.push_str(&format!(" {} ", 符.符号()));
            发射整式(右, 出);
            出.push(')');
        }
        整表达式::除(左, d) => {
            出.push('(');
            发射整式(左, 出);
            出.push_str(" / ");
            发射除数(*d, 出);
            出.push(')');
        }
        整表达式::取余(左, d) => {
            出.push('(');
            发射整式(左, 出);
            出.push_str(" % ");
            发射除数(*d, 出);
            出.push(')');
        }
        整表达式::调用(序号, 实参) => {
            出.push_str(&format!("f{}(", 序号));
            for (i, a) in 实参.iter().enumerate() {
                if i > 0 {
                    出.push_str(", ");
                }
                发射整式(a, 出);
            }
            出.push(')');
        }
    }
}

/// 负除数写成 `(0 - N)` —— 不去碰「一元负号该怎么写」这个跟本测试无关的问题
fn 发射除数(d: i64, 出: &mut String) {
    if d < 0 {
        出.push_str(&format!("(0 - {})", -d));
    } else {
        出.push_str(&d.to_string());
    }
}

fn 发射布式(b: &布表达式, 出: &mut String) {
    match b {
        布表达式::比较(左, 符, 右) => {
            发射整式(左, 出);
            出.push_str(&format!(" {} ", 符.符号()));
            发射整式(右, 出);
        }
        布表达式::与(左, 右) => {
            出.push('(');
            发射布式(左, 出);
            出.push_str(" 与 ");
            发射布式(右, 出);
            出.push(')');
        }
        布表达式::或(左, 右) => {
            出.push('(');
            发射布式(左, 出);
            出.push_str(" 或 ");
            发射布式(右, 出);
            出.push(')');
        }
    }
}

fn 发射语句(s: &语句, 缩进: usize, 出: &mut String) {
    let 空 = " ".repeat(缩进);
    match s {
        语句::声明(名, e) => {
            出.push_str(&format!("{}变量 {}: 整数 = ", 空, 名));
            发射整式(e, 出);
            出.push_str(";\n");
        }
        语句::赋值(名, e) => {
            出.push_str(&format!("{}{} = ", 空, 名));
            发射整式(e, 出);
            出.push_str(";\n");
        }
        语句::分支(c, 真支, 假支) => {
            出.push_str(&format!("{}如果 (", 空));
            发射布式(c, 出);
            出.push_str(") {\n");
            for s in 真支 {
                发射语句(s, 缩进 + 4, 出);
            }
            出.push_str(&format!("{}}} 否则 {{\n", 空));
            for s in 假支 {
                发射语句(s, 缩进 + 4, 出);
            }
            出.push_str(&format!("{}}}\n", 空));
        }
        语句::循环 {
            计数器,
            上界,
            步长,
            体,
        } => {
            出.push_str(&format!("{}变量 {}: 整数 = 0;\n", 空, 计数器));
            出.push_str(&format!("{}当 {} < {} {{\n", 空, 计数器, 上界));
            for s in 体 {
                发射语句(s, 缩进 + 4, 出);
            }
            出.push_str(&format!(
                "{}    {} = {} + {};\n{}}}\n",
                空, 计数器, 计数器, 步长, 空
            ));
        }
        语句::打印(e) => {
            出.push_str(&format!("{}打印行(", 空));
            发射整式(e, 出);
            出.push_str(");\n");
        }
    }
}

fn 发射程序(p: &程序) -> String {
    let mut 出 = String::from("包 主程序;\n\n");
    for f in &p.函数表 {
        let 形参: Vec<String> = f.形参.iter().map(|n| format!("{}: 整数", n)).collect();
        出.push_str(&format!(
            "函数 f{}({}) : 整数 {{\n",
            f.序号,
            形参.join(", ")
        ));
        for s in &f.体 {
            发射语句(s, 4, &mut 出);
        }
        出.push_str("    返回 ");
        发射整式(&f.返回, &mut 出);
        出.push_str(";\n}\n\n");
    }
    出.push_str("函数 入口() {\n");
    for s in &p.主体 {
        发射语句(s, 4, &mut 出);
    }
    出.push_str("}\n");
    出
}

// ─────────────────────────── 参考求值 ───────────────────────────

/// 迭代次数保险丝。撞上说明生成器出了 bug（构造上不该发生），
/// 这时丢弃这个程序而不是判失败 —— 免得测试挂死在这儿。
const 步数上限: u64 = 30_000;

struct 求值器<'a> {
    程序: &'a 程序,
    输出: Vec<i64>,
    步数: u64,
}

/// 求值中途放弃（撞保险丝）
struct 放弃;

impl 求值器<'_> {
    fn 记一步(&mut self) -> Result<(), 放弃> {
        self.步数 += 1;
        if self.步数 > 步数上限 {
            return Err(放弃);
        }
        Ok(())
    }

    fn 算整式(
        &mut self, e: &整表达式, 环境: &HashMap<String, i64>
    ) -> Result<i64, 放弃> {
        self.记一步()?;
        Ok(match e {
            整表达式::字面量(v) => *v,
            // 生成器只在变量可见时引用它，取不到就是生成器的 bug
            整表达式::变量(名) => *环境.get(名).ok_or(放弃)?,
            整表达式::二元(左, 符, 右) => {
                let a = self.算整式(左, 环境)?;
                let b = self.算整式(右, 环境)?;
                符.算(a, b)
            }
            整表达式::除(左, d) => {
                let a = self.算整式(左, 环境)?;
                // 生成器保证 d ∉ {0, -1}，wrapping_div 与 LLVM sdiv 同为向零截断
                a.wrapping_div(*d)
            }
            整表达式::取余(左, d) => {
                let a = self.算整式(左, 环境)?;
                a.wrapping_rem(*d)
            }
            整表达式::调用(序号, 实参) => {
                let mut 值: Vec<i64> = Vec::with_capacity(实参.len());
                for a in 实参 {
                    值.push(self.算整式(a, 环境)?);
                }
                let f = self.程序.函数表.get(*序号).ok_or(放弃)?;
                let mut 内环境: HashMap<String, i64> = HashMap::new();
                for (名, v) in f.形参.iter().zip(值) {
                    内环境.insert(名.clone(), v);
                }
                // 函数体和返回式的 AST 借着 self.程序 的生命周期，先克隆出来
                // 免得跟 &mut self 打架（程序都很小，克隆的代价无所谓）
                let 体 = f.体.clone();
                let 返回 = f.返回.clone();
                for s in &体 {
                    self.走语句(s, &mut 内环境)?;
                }
                self.算整式(&返回, &内环境)?
            }
        })
    }

    fn 算布式(
        &mut self, b: &布表达式, 环境: &HashMap<String, i64>
    ) -> Result<bool, 放弃> {
        self.记一步()?;
        Ok(match b {
            布表达式::比较(左, 符, 右) => {
                let a = self.算整式(左, 环境)?;
                let c = self.算整式(右, 环境)?;
                符.判(a, c)
            }
            // 短路 —— 跟 qi 的语义一致（2026-08-22 起）。
            //
            // 这两行是有历史的：本差分器第一次真跑（种子 777024）就发现
            // qi 的 与/或 **不短路**，生成的程序比参考求值器多打印一次副作用。
            // 当时先把求值器改成两侧都求值来对齐现状，随后编译器改成了短路
            // （见 inkwell_gen/表达式.rs 的 短路.右/短路.汇合），这里跟着改回来。
            //
            // 生成的表达式目前没有副作用，所以短路与否不影响**答案**；
            // 真正的差异靠 tests/codegen回归/31 那条守卫用例盯着。
            布表达式::与(左, 右) => self.算布式(左, 环境)? && self.算布式(右, 环境)?,
            布表达式::或(左, 右) => self.算布式(左, 环境)? || self.算布式(右, 环境)?,
        })
    }

    fn 走语句(&mut self, s: &语句, 环境: &mut HashMap<String, i64>) -> Result<(), 放弃> {
        self.记一步()?;
        match s {
            语句::声明(名, e) | 语句::赋值(名, e) => {
                let v = self.算整式(e, 环境)?;
                环境.insert(名.clone(), v);
            }
            // ── 出块**不回滚**环境 ────────────────────────────────────
            //
            // 第一版在这儿存档 + `*环境 = 存档`，想模拟块作用域。那是错的，
            // 而且错得很贵：qi 里块内 `变量 x` 确实出块失效，但**块内对外层
            // 变量的赋值是保留的**（实测 `变量 外 = 1; 如果(…){ 外 = 99; }`
            // 打出 99）。回滚把这些赋值一起吞了。
            //
            // 后果不只是答案对不上：分支里对循环计数器的赋值被吞掉之后，
            // 求值器算出「循环会停」，真程序却停不下来 —— 于是 harness 卡死在
            // Command::output 上，300 个程序跑了 35 分钟还没完，看着像编译器慢。
            //
            // 不回滚是安全的：生成器只会引用当前 可见变量 里的名字，而变量名
            // 在同一作用域里唯一，所以出块后残留的绑定不会被任何表达式读到。
            语句::分支(c, 真支, 假支) => {
                let 走真 = self.算布式(c, 环境)?;
                let 支 = if 走真 { 真支 } else { 假支 };
                for s in 支 {
                    self.走语句(s, 环境)?;
                }
            }
            语句::循环 {
                计数器,
                上界,
                步长,
                体,
            } => {
                // 计数器在发射时是**循环外**那一层的 `变量 计数器: 整数 = 0;`，
                // 所以它在环境里的生命周期也到外层为止
                环境.insert(计数器.clone(), 0);
                loop {
                    self.记一步()?;
                    let i = *环境.get(计数器).ok_or(放弃)?;
                    if i >= *上界 {
                        break;
                    }
                    for s in 体 {
                        self.走语句(s, 环境)?;
                    }
                    let 现计数 = *环境.get(计数器).ok_or(放弃)?;
                    环境.insert(计数器.clone(), 现计数.wrapping_add(*步长));
                }
            }
            语句::打印(e) => {
                let v = self.算整式(e, 环境)?;
                self.输出.push(v);
            }
        }
        Ok(())
    }
}

fn 参考求值(p: &程序) -> Option<Vec<i64>> {
    let mut 器 = 求值器 {
        程序: p,
        输出: Vec::new(),
        步数: 0,
    };
    let mut 环境: HashMap<String, i64> = HashMap::new();
    for s in &p.主体 {
        if 器.走语句(s, &mut 环境).is_err() {
            return None;
        }
    }
    Some(器.输出)
}

// ─────────────────────────── 编译并跑 ───────────────────────────

fn 定位运行时归档() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("QI_RUNTIME_LIB") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    ["release", "debug"]
        .iter()
        .map(|c| {
            manifest
                .join("../qi-runtime/target")
                .join(c)
                .join("libqi_runtime.a")
        })
        .find(|p| p.exists())
}

/// 跑出来的程序最多给这么久。生成器构造上保证停机（循环有界、无递归、
/// 计数器不可赋值），所以撞上超时**本身就是个发现** —— 要么编译器把循环
/// 译错了，要么生成器有漏洞，两种都得看见，不能挂在那儿。
///
/// 这一条是踩出来的：第一版没有超时，参考求值器又错误地回滚了块内赋值，
/// 于是「求值器说会停、真程序停不下来」，`Command::output` 直接阻塞，
/// 300 个程序跑了 35 分钟毫无产出，表面上还像是编译慢。
const 单程序超时: std::time::Duration = std::time::Duration::from_secs(20);

/// 编译到可执行文件并运行，返回 stdout 的每一行
fn 编译并跑(源文件: &Path, 优化: OptimizationLevel) -> Result<Vec<String>, String> {
    let mut 配置 = CompilerConfig::default();
    配置.optimization_level = 优化;
    let 编译器 = QiCompiler::with_config(配置);
    let 结果 = 编译器
        .compile(源文件.to_path_buf())
        .map_err(|e| format!("编译失败({}): {:?}", 优化, e))?;

    // 用 spawn + 轮询 try_wait 而不是 output()：标准库没有带超时的等待。
    // stdout/stderr 收进管道，程序小（几十行数字），装得下 64K 管道缓冲，
    // 不会因为写满而在退出前卡住。
    let mut 子 = std::process::Command::new(&结果.executable_path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("起不来({}): {}", 优化, e))?;
    let 起 = std::time::Instant::now();
    let 状态 = loop {
        match 子.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) => {
                if 起.elapsed() > 单程序超时 {
                    let _ = 子.kill();
                    let _ = 子.wait();
                    return Err(format!(
                        "跑不完({})：超过 {} 秒还没退出。生成的程序构造上一定停机，\n\
                         所以这要么是循环被译错了，要么是生成器漏了一条终止性约束。",
                        优化,
                        单程序超时.as_secs()
                    ));
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            Err(e) => return Err(format!("等不到({}): {}", 优化, e)),
        }
    };
    let 输出 = 子
        .wait_with_output()
        .map_err(|e| format!("收不到输出({}): {}", 优化, e))?;
    if !状态.success() {
        return Err(format!(
            "运行失败({}): 退出码 {:?}\nstderr: {}",
            优化,
            状态.code(),
            String::from_utf8_lossy(&输出.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&输出.stdout)
        .lines()
        .map(|l| l.trim_end_matches('\r').to_string())
        .collect())
}

fn 读环境数(名: &str, 默认: u64) -> u64 {
    std::env::var(名)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(默认)
}

#[test]
#[ignore = "要编译链接，比整套单测还慢；走 make fuzz"]
fn 差分模糊() {
    let Some(归档) = 定位运行时归档() else {
        eprintln!("跳过：未找到 qi-runtime 归档（先在 qi-runtime/ 跑 cargo build --release）");
        return;
    };
    std::env::set_var("QI_RUNTIME_LIB", &归档);

    let 数量 = 读环境数("QI_FUZZ_COUNT", 12);
    let 起始种子 = 读环境数("QI_FUZZ_SEED", 20260822);
    let 临时 = tempfile::tempdir().expect("建临时目录");

    let mut 跑过 = 0u64;
    let mut 丢弃 = 0u64;
    let mut 失败: Vec<String> = Vec::new();

    // 逐个程序报一行耗时。看着啰嗦，但没有它就是个黑盒：
    // 第一版跑 300 个程序 35 分钟没动静，光看 CPU 占用完全判断不出是
    // 「编译慢」还是「卡在某一个程序上」，白等了半小时。
    let 详细 = std::env::var("QI_FUZZ_QUIET").is_err();

    for i in 0..数量 {
        let 种子 = 起始种子.wrapping_add(i);
        let mut 骰 = 骰子::新(种子);
        let p = 生成程序(&mut 骰);

        let Some(期望) = 参考求值(&p) else {
            丢弃 += 1;
            if 详细 {
                eprintln!("种子 {} 丢弃（求值撞保险丝）", 种子);
            }
            continue;
        };
        let 源码 = 发射程序(&p);
        let 源文件 = 临时.path().join(format!("模糊{}.qi", 种子));
        std::fs::write(&源文件, &源码).expect("写源文件");

        let 计时 = std::time::Instant::now();
        let 无优化 = 编译并跑(&源文件, OptimizationLevel::None);
        let 最大优化 = 编译并跑(&源文件, OptimizationLevel::Maximum);
        if 详细 {
            eprintln!(
                "种子 {} 用时 {:.1}s（源码 {} 行，打印 {} 条）",
                种子,
                计时.elapsed().as_secs_f32(),
                源码.lines().count(),
                期望.len()
            );
        }

        let 报 = |因为: String| -> String {
            format!(
                "\n════ 种子 {} ════\n{}\n── 期望（参考求值器）──\n{:?}\n── {} ──\n源码留在 {}\n",
                种子,
                源码,
                期望,
                因为,
                源文件.display()
            )
        };

        match (无优化, 最大优化) {
            (Err(e), _) | (_, Err(e)) => 失败.push(报(e)),
            (Ok(甲), Ok(乙)) => {
                let 期望串: Vec<String> = 期望.iter().map(|v| v.to_string()).collect();
                // 先比两个优化级别：它俩不一致 = 一定有 miscompile，
                // 而且不需要知道谁对，这条最硬
                if 甲 != 乙 {
                    失败.push(报(format!(
                        "优化级别之间就不一致：无优化 {:?} vs 最大优化 {:?}",
                        甲, 乙
                    )));
                } else if 甲 != 期望串 {
                    失败.push(报(format!("与参考求值器不符：实际 {:?}", 甲)));
                } else {
                    跑过 += 1;
                }
            }
        }
    }

    eprintln!(
        "差分模糊：种子 {}..{}，通过 {}，丢弃 {}，失败 {}",
        起始种子,
        起始种子 + 数量 - 1,
        跑过,
        丢弃,
        失败.len()
    );
    assert!(失败.is_empty(), "{}", 失败.join("\n"));
    assert!(
        跑过 > 0,
        "一个程序都没真跑起来（丢弃 {}）—— 生成器或环境有问题",
        丢弃
    );
}
