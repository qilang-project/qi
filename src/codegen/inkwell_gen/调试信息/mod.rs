//! DWARF 调试信息生成（行号表 + 函数条目 + 局部变量 + 复合类型）。
//!
//! 本模块管框架：编译单元、文件/行号换算、每函数的 DISubprogram、每语句位置、
//! 局部变量条目、以及进出合成函数时的位置管理。
//! 结构体/数组/枚举展开成能在 lldb 里点开字段的形状在 [`复合类型`] 里。
//!
//! 为什么值得做：没有它，lldb 里既不能按 `文件.qi:行号` 下断点，也不能单步，
//! 崩溃栈全是 `_Z_E587B0…` 这样的 mangled 符号 —— 出了问题只能靠打印猜。
//! 有了行号表，qi 程序就能用所有人已经会用的调试器调，这是「敢不敢在大项目里
//! 用」的分界线。
//!
//! ## 设计取舍
//!
//! - **语言码标 DW_LANG_C99**。DWARF 没给 qi 分配语言码，随便编一个会让 lldb
//!   走「未知语言」的降级路径。标 C99 的效果是调试器按 C 的惯例解释我们的
//!   类型和调用约定 —— 而 qi 的 ABI 本来就是 C ABI，这是最贴近事实的谎。
//!
//! - **名字用中文原名，linkage name 用 mangled 符号**。DW_AT_name 是 UTF-8，
//!   lldb 显示中文没问题；DW_AT_linkage_name 保证符号能对回 nm 看到的东西。
//!
//! - **语句级位置，不做表达式级**。断点和单步只需要语句粒度；表达式级位置的
//!   收益是「一行里多个调用能分开」，代价是每个表达式降级点都要插钩子 ——
//!   现有 codegen 的表达式路径有几十个分支，改动面不成比例。
//!
//! - **默认开**。qi 没有 debug/release 之分，调试信息不影响运行期性能（只进
//!   .o 的 __DWARF 段，不进代码），只影响体积。`--无调试信息` 可关。
//!
//! ## 位置换算
//!
//! AST 的 `Span` 是**原始源码的 UTF-8 字节偏移**（parser 预处理是等字节替换，
//! 偏移不漂移）。这里按文件缓存一份「行首偏移表」，二分查行号、行内数字符查
//! 列号 —— 逐条语句调 `偏移转行列` 是 O(源码长度)，几千条语句的文件会明显变慢。
//!
//! ## 别处必须配合的一件事：进/出函数时的位置管理
//!
//! LLVM verifier 有两条硬规则：
//!   1. 带 !dbg 的指令，其 scope 必须属于**所在函数**的 DISubprogram；
//!   2. 有调试信息的函数里，调用另一个有调试信息的函数**必须**带 !dbg。
//!
//! builder 的「当前调试位置」是粘性的，不清就会漏到下一个函数去。所以：
//!   - 有调试信息的函数（用户函数/方法/main）：一进 entry 就设声明行位置，
//!     此后每条语句覆盖；这样形参 retain、剖析计时这些「语句之前」的指令
//!     也有位置（规则 2）。
//!   - **真正**由编译器凭空造出来的函数（闭包 env dtor、trampoline、
//!     qi.release.* 等）没有 DISubprogram，进去前 [`调试_暂离`]、出来后
//!     [`调试_归位`]（规则 1）。注意闭包**体**和协程**体**不在此列 ——
//!     它们是用户写的代码，各自有真的 DISubprogram（见 闭包.rs / 协程.rs），
//!     走的是和普通函数一样的 调试_进入函数 / 调试_离开函数。
//!     嵌套合成（语句中途建 trampoline）必须 **restore** 而不是只清，否则
//!     外层这条语句剩下的调用就没位置了（又违反规则 2）。

#[path = "复合类型.rs"]
mod 复合类型;

use super::后端;
use super::类型::Qi类型;
use crate::lexer::tokens::Span;
use crate::parser::ast::AstNode;
use inkwell::debug_info::{
    AsDIScope, DIBasicType, DICompileUnit, DIFile, DIFlagsConstants, DILocation, DISubprogram,
    DIType, DWARFEmissionKind, DWARFSourceLanguage, DebugInfoBuilder,
};
use inkwell::values::{FunctionValue, PointerValue};
use std::collections::HashMap;

/// DWARF 基础类型编码（DW_ATE_*）。inkwell 只透传 u32，没给常量。
const DW_ATE_BOOLEAN: u32 = 0x02;
const DW_ATE_FLOAT: u32 = 0x04;
const DW_ATE_SIGNED: u32 = 0x05;
const DW_ATE_SIGNED_CHAR: u32 = 0x06;

/// 一个源文件的调试信息缓存：DIFile + 行首偏移表 + 源码（列号按字符数）。
struct 文件信息<'ctx> {
    文件: DIFile<'ctx>,
    源码: String,
    /// 每行首字节偏移（行号 = 下标 + 1）。二分查找。
    行首: Vec<usize>,
}

impl<'ctx> 文件信息<'ctx> {
    fn 新建(文件: DIFile<'ctx>, 源码: String) -> Self {
        let mut 行首 = vec![0usize];
        for (i, b) in 源码.bytes().enumerate() {
            if b == b'\n' {
                行首.push(i + 1);
            }
        }
        Self {
            文件, 源码, 行首
        }
    }

    /// 字节偏移 → (1-based 行, 1-based 列)。列按字符计（中文占 1 列）。
    fn 行列(&self, 偏移: usize) -> (u32, u32) {
        let 偏移 = 偏移.min(self.源码.len());
        // 最后一个 <= 偏移 的行首
        let 行下标 = match self.行首.binary_search(&偏移) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        let 行起 = self.行首[行下标];
        // 偏移可能落在多字节字符中间（合成节点的 span 端点），退到字符边界，
        // 不能直接切 &str（会 panic）。
        let mut 止 = 偏移;
        while 止 > 行起 && !self.源码.is_char_boundary(止) {
            止 -= 1;
        }
        let 列 = self.源码[行起..止].chars().count() as u32 + 1;
        (行下标 as u32 + 1, 列)
    }
}

/// 整个编译单元的调试信息状态。
pub(super) struct 调试上下文<'ctx> {
    生成器: DebugInfoBuilder<'ctx>,
    单元: DICompileUnit<'ctx>,
    /// 源文件路径 → 文件信息（懒加载；读不到文件的退回编译单元主文件）。
    文件表: HashMap<String, 文件信息<'ctx>>,
    /// 当前函数的 DISubprogram —— 语句位置挂在它下面。None = 当前在合成函数里。
    当前作用域: Option<DISubprogram<'ctx>>,
    /// 当前函数所属文件（位置换算查 文件表 用）。
    当前文件: Option<String>,
    /// 基础 DIType 缓存。键是 DW_ATE 编码 + 位宽，够区分我们用到的那几个。
    基础类型: HashMap<(u32, u64), DIBasicType<'ctx>>,
    /// 指针类 DIType（字符串统一近似成 `char*`；深度耗尽的复合类型退回 `void*`）。
    字符指针: Option<DIType<'ctx>>,
    不透明指针: Option<DIType<'ctx>>,
    /// 复合 DIType 缓存：(类型键, 剩余展开深度) → 引用类型。见 复合类型.rs。
    /// 键里必须带深度 —— 同一结构体在不同深度是不同的条目（深层那份字段更浅）。
    复合类型: HashMap<(String, u8), DIType<'ctx>>,
    /// 当前函数的中文显示名。闭包合成时拿它拼「外层·闭包N」，让 backtrace
    /// 里的闭包帧看得出是谁的闭包。
    当前函数显示名: Option<String>,
}

impl<'ctx> 调试上下文<'ctx> {
    /// 取（必要时读盘建立）某个源文件的信息。路径读不到时返回 None ——
    /// 调用方退回不发位置，宁可缺行号也不给 lldb 假行号。
    fn 取文件(&mut self, 路径: &str) -> Option<&文件信息<'ctx>> {
        if !self.文件表.contains_key(路径) {
            let 源码 = std::fs::read_to_string(路径).ok()?;
            let p = std::path::Path::new(路径);
            let 名 = p.file_name()?.to_string_lossy().to_string();
            let 目录 = p
                .parent()
                .map(|d| d.to_string_lossy().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| ".".to_string());
            let f = self.生成器.create_file(&名, &目录);
            self.文件表
                .insert(路径.to_string(), 文件信息::新建(f, 源码));
        }
        self.文件表.get(路径)
    }
}

impl<'ctx> 后端<'ctx> {
    /// 建立编译单元。`主文件` 是 entry 的源码路径（没有就退化成占位名）。
    ///
    /// 必须同时打 "Debug Info Version" 模块标志，否则 LLVM 在写目标文件时
    /// 会把整份调试元数据当成过期格式直接丢掉 —— 表现是「一切正常但产物里
    /// 什么都没有」，最难查的那种。
    pub(super) fn 调试_建立单元(&mut self, 主文件: Option<&str>) {
        if !self.调试开 {
            return;
        }
        let 三 = self.ctx.i32_type().const_int(3, false);
        self.module.add_basic_value_flag(
            "Debug Info Version",
            inkwell::module::FlagBehavior::Warning,
            三,
        );
        // DWARF 版本：macOS 的 ld64/lldb 与 Linux 的 gdb 都稳吃 4；5 在部分
        // 老 dsymutil 上会退化。行号表这一层 4 和 5 没有差别，取稳的。
        let 四 = self.ctx.i32_type().const_int(4, false);
        self.module.add_basic_value_flag(
            "Dwarf Version",
            inkwell::module::FlagBehavior::Warning,
            四,
        );

        let (名, 目录) = match 主文件.map(std::path::Path::new) {
            Some(p) => (
                p.file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "qi_program.qi".to_string()),
                p.parent()
                    .map(|d| d.to_string_lossy().to_string())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| ".".to_string()),
            ),
            None => ("qi_program.qi".to_string(), ".".to_string()),
        };
        let 产出者 = format!("qi 编译器 {}", env!("CARGO_PKG_VERSION"));
        let (生成器, 单元) = self.module.create_debug_info_builder(
            true,
            DWARFSourceLanguage::C99,
            &名,
            &目录,
            &产出者,
            false, // is_optimized：qi 默认 -O1，但标 true 只会让调试器提示「值可能被优化掉」
            "",
            0,
            "",
            DWARFEmissionKind::Full,
            0,
            false,
            false,
            "",
            "",
        );
        self.调试 = Some(调试上下文 {
            生成器,
            单元,
            文件表: HashMap::new(),
            当前作用域: None,
            当前文件: None,
            基础类型: HashMap::new(),
            字符指针: None,
            不透明指针: None,
            复合类型: HashMap::new(),
            当前函数显示名: None,
        });
    }

    /// Qi 类型 → DIType 的统一入口。复合类型（结构体/数组/枚举）按
    /// [`复合类型::最大展开深度`] 展开成可在 lldb 里点开字段的形状，见 复合类型.rs。
    fn 调试_类型(&mut self, t: Qi类型) -> Option<DIType<'ctx>> {
        self.调试_类型深度(t, 复合类型::最大展开深度)
    }

    /// 标量与指针近似类型：整数/浮点/布尔/字符串/裸指针/函数值/通道/未来。
    /// 复合类型不走这里（走 调试_类型深度）。
    fn 调试_标量类型(&mut self, t: Qi类型) -> Option<DIType<'ctx>> {
        // 闭包/通道/未来/裸指针：内部布局是运行时的私事（fat obj / 调度器句柄 /
        // coro frame），展开出来的字段对写 qi 的人没有意义 —— 统一 void*，
        // 与深度耗尽时的兜底共用同一份构造。
        if matches!(
            t,
            Qi类型::指针 | Qi类型::函数值(_) | Qi类型::通道(_) | Qi类型::未来(_)
        ) {
            return self.调试_不透明指针();
        }
        let d = self.调试.as_mut()?;
        let 基础 =
            |d: &mut 调试上下文<'ctx>, 名: &str, 位宽: u64, 编码: u32| -> Option<DIType<'ctx>> {
                if let Some(t) = d.基础类型.get(&(编码, 位宽)) {
                    return Some(t.as_type());
                }
                let t = d
                    .生成器
                    .create_basic_type(名, 位宽, 编码, DIFlagsConstants::PUBLIC)
                    .ok()?;
                d.基础类型.insert((编码, 位宽), t);
                Some(t.as_type())
            };
        match t {
            Qi类型::空 => None,
            Qi类型::浮点数 => 基础(d, "浮点数", 64, DW_ATE_FLOAT),
            // LLVM 里布尔是 i1，但 DWARF 的最小可寻址单位是字节；标 8 位
            // boolean，lldb 打印出 true/false 而不是 0/1。
            Qi类型::布尔 => 基础(d, "布尔", 8, DW_ATE_BOOLEAN),
            Qi类型::字符串 => {
                if d.字符指针.is_none() {
                    let c = d
                        .生成器
                        // 基础类型名故意用英文 `char`：lldb 判断「这个指针要不要
                        // 按 C 字符串打印」是**按 DW_AT_name 匹配内建类型名**的
                        // （DWARFASTParserClang::GetBuiltinTypeForDWARFEncodingAndBitSize），
                        // 叫「字符」它认不出来，`名字` 就只打一个地址。
                        // 外层指针类型名仍是「字符串」，用户看到的是 (字符串) 名字 = "小明"。
                        .create_basic_type("char", 8, DW_ATE_SIGNED_CHAR, DIFlagsConstants::PUBLIC)
                        .ok()?;
                    let p = d.生成器.create_pointer_type(
                        "字符串",
                        c.as_type(),
                        64,
                        64,
                        inkwell::AddressSpace::default(),
                    );
                    d.字符指针 = Some(p.as_type());
                }
                d.字符指针
            }
            // 整数 / 未知 都是 i64（复合类型在 复合类型.rs 里真展开，不落到这里）
            _ => 基础(d, "整数", 64, DW_ATE_SIGNED),
        }
    }

    /// 给一个正在生成的函数建 DISubprogram 并挂上去，同时把 builder 的当前
    /// 位置设到声明行。
    ///
    /// `显示名` 是中文原名（进 DW_AT_name），`符号名` 是 mangled（进
    /// DW_AT_linkage_name）。`参数类型` 只用来填 DW_AT_type 的签名，缺了也不
    /// 影响断点。span 落不到有效行时（合成节点）整条跳过 —— 宁可这个函数没有
    /// 调试条目，也不要给 lldb 一个 0 行。
    pub(super) fn 调试_进入函数(
        &mut self,
        func: FunctionValue<'ctx>,
        显示名: &str,
        符号名: &str,
        span: Span,
        参数类型: &[Qi类型],
        返回类型: Qi类型,
    ) {
        if self.调试.is_none() {
            return;
        }
        let 文件路径 = match self.当前文件.clone() {
            Some(p) => p,
            None => return,
        };
        let (文件, 行) = {
            let d = match self.调试.as_mut() {
                Some(d) => d,
                None => return,
            };
            match d.取文件(&文件路径) {
                Some(fi) => (fi.文件, fi.行列(span.start).0),
                None => return,
            }
        };
        if 行 == 0 {
            return;
        }

        let 返回di = self.调试_类型(返回类型);
        let 参数di: Vec<DIType<'ctx>> =
            参数类型.iter().filter_map(|t| self.调试_类型(*t)).collect();

        let d = match self.调试.as_mut() {
            Some(d) => d,
            None => return,
        };
        let 子程序类型 =
            d.生成器
                .create_subroutine_type(文件, 返回di, &参数di, DIFlagsConstants::PUBLIC);
        let sp = d.生成器.create_function(
            d.单元.as_debug_info_scope(),
            显示名,
            Some(符号名),
            文件,
            行,
            子程序类型,
            false, // is_local_to_unit：qi 的符号跨文件可见，标 false
            true,  // is_definition
            行,    // scope_line
            DIFlagsConstants::PUBLIC,
            false,
        );
        func.set_subprogram(sp);
        d.当前作用域 = Some(sp);
        d.当前文件 = Some(文件路径);
        d.当前函数显示名 = Some(显示名.to_string());

        // 立刻设一个位置：形参落地 / ARC retain / 剖析计时都排在第一条语句
        // 之前，它们里面有 call —— 没位置会被 verifier 拦下（规则 2）。
        let loc = d
            .生成器
            .create_debug_location(self.ctx, 行, 1, sp.as_debug_info_scope(), None);
        self.builder.set_current_debug_location(loc);
    }

    /// 当前正在生成的函数的中文显示名。闭包登记时拿它拼「外层·闭包N」。
    /// 关调试信息时恒为 None（闭包显示名退回合成符号，不影响生成的代码）。
    pub(super) fn 调试_当前函数名(&self) -> Option<String> {
        self.调试.as_ref()?.当前函数显示名.clone()
    }

    /// 离开一个有调试信息的函数：清作用域 + 清 builder 位置。
    /// 不清的话，下一个**没有** DISubprogram 的合成函数会继承这个位置，
    /// verifier 报「!dbg attachment points at wrong subprogram」。
    pub(super) fn 调试_离开函数(&mut self) {
        if let Some(d) = self.调试.as_mut() {
            d.当前作用域 = None;
            d.当前文件 = None;
            d.当前函数显示名 = None;
            self.builder.unset_current_debug_location();
        }
    }

    /// 语句级位置。`生成语句` 一进来就调，一条语句内生成的所有指令共用它。
    pub(super) fn 调试_语句(&mut self, node: &AstNode) {
        let span = match 语句span(node) {
            Some(s) => s,
            None => return,
        };
        self.调试_位置(span);
    }

    /// 按 span 设当前调试位置。span 落不到有效行就保持原位置不动 ——
    /// 发 0 行会让 lldb 的单步在「未知行」上打转。
    pub(super) fn 调试_位置(&mut self, span: Span) {
        let ctx = self.ctx;
        let d = match self.调试.as_mut() {
            Some(d) => d,
            None => return,
        };
        let (作用域, 文件路径) = match (d.当前作用域, d.当前文件.clone()) {
            (Some(s), Some(f)) => (s, f),
            _ => return,
        };
        let (行, 列) = match d.取文件(&文件路径) {
            Some(fi) => fi.行列(span.start),
            None => return,
        };
        if 行 == 0 {
            return;
        }
        let loc = d
            .生成器
            .create_debug_location(ctx, 行, 列, 作用域.as_debug_info_scope(), None);
        self.builder.set_current_debug_location(loc);
    }

    /// 登记一个局部变量 / 形参，让 lldb 的 `frame variable` 认得出名字。
    ///
    /// `形参序号` 为 Some(n)（1-based）时建 DW_TAG_formal_parameter，
    /// None 时建 DW_TAG_variable。`槽` 必须是变量的 alloca。
    pub(super) fn 调试_局部变量(
        &mut self,
        名: &str,
        t: Qi类型,
        槽: PointerValue<'ctx>,
        span: Span,
        形参序号: Option<u32>,
    ) {
        if self.调试.is_none() {
            return;
        }
        let dt = match self.调试_类型(t) {
            Some(t) => t,
            None => return,
        };
        let ctx = self.ctx;
        let 块 = match self.builder.get_insert_block() {
            Some(b) => b,
            None => return,
        };
        let d = match self.调试.as_mut() {
            Some(d) => d,
            None => return,
        };
        let (作用域, 文件路径) = match (d.当前作用域, d.当前文件.clone()) {
            (Some(s), Some(f)) => (s, f),
            _ => return,
        };
        let (文件, 行) = match d.取文件(&文件路径) {
            Some(fi) => (fi.文件, fi.行列(span.start).0),
            None => return,
        };
        if 行 == 0 {
            return;
        }
        let 变量 = match 形参序号 {
            Some(n) => d.生成器.create_parameter_variable(
                作用域.as_debug_info_scope(),
                名,
                n,
                文件,
                行,
                dt,
                true,
                DIFlagsConstants::ZERO,
            ),
            None => d.生成器.create_auto_variable(
                作用域.as_debug_info_scope(),
                名,
                文件,
                行,
                dt,
                true,
                DIFlagsConstants::ZERO,
                0,
            ),
        };
        let loc = d
            .生成器
            .create_debug_location(ctx, 行, 1, 作用域.as_debug_info_scope(), None);
        // 挂在当前块末尾：调用点刚 alloca+store 完，dbg.declare 紧随其后。
        d.生成器
            .insert_declare_at_end(槽, Some(变量), None, loc, 块);
    }

    /// 进合成函数（闭包 dtor / trampoline / qi.release.*）前调：摘下当前位置并返回。
    /// 与 [`调试_归位`] 成对使用 —— 见模块头「位置管理」。
    pub(super) fn 调试_暂离(&self) -> Option<DILocation<'ctx>> {
        if self.调试.is_none() {
            return None;
        }
        let 旧 = self.builder.get_current_debug_location();
        self.builder.unset_current_debug_location();
        旧
    }

    /// 与 [`调试_暂离`] 配对：把位置放回去（None 表示原本就没有）。
    pub(super) fn 调试_归位(&self, 旧: Option<DILocation<'ctx>>) {
        if self.调试.is_none() {
            return;
        }
        match 旧 {
            Some(l) => self.builder.set_current_debug_location(l),
            None => self.builder.unset_current_debug_location(),
        }
    }

    /// 收尾：把延迟构造的元数据落实。必须在 module.verify() 和写目标文件之前，
    /// 否则 verifier 看到的是半成品（inkwell 的 Drop 也会调，但那时已经太晚）。
    pub(super) fn 调试_收尾(&self) {
        if let Some(d) = self.调试.as_ref() {
            d.生成器.finalize();
        }
    }
}

/// 语句节点的 span。只覆盖会出现在函数体里的那些 —— 表达式落到 `表达式语句`
/// 这一层就够（表达式级位置见模块头的取舍说明）。
///
/// 拿不到 span 的返回 None：调用方保持上一条语句的位置，不发假行号。
fn 语句span(node: &AstNode) -> Option<Span> {
    use AstNode::*;
    let s = match node {
        变量声明(x) => x.span,
        如果语句(x) => x.span,
        循环语句(x) => x.span,
        当语句(x) => x.span,
        对于语句(x) => x.span,
        返回语句(x) => x.span,
        跳出语句(x) => x.span,
        继续语句(x) => x.span,
        表达式语句(x) => x.span,
        块语句(x) => x.span,
        尝试语句(x) => x.span,
        抛出语句(x) => x.span,
        函数声明(x) => x.span,
        结构体声明(x) => x.span,
        方法声明(x) => x.span,
        枚举声明(x) => x.span,
        _ => return None,
    };
    // span(0,0) 是个别合成节点的默认值 —— 换算出来是第 1 行第 1 列，
    // 而那里通常是 `包 xxx;`，单步会莫名其妙跳回文件头。跳过。
    if s.start == 0 && s.end == 0 {
        None
    } else {
        Some(s)
    }
}

#[cfg(test)]
mod tests {
    use super::文件信息;
    use inkwell::context::Context;

    /// 行首表 + 二分：跟 位置.rs 的线性换算必须给出同样的行号，
    /// 否则调试器的行号和编译错误的行号会对不上。
    #[test]
    fn 行列换算与位置模块一致() {
        let ctx = Context::create();
        let m = ctx.create_module("t");
        let (di, _cu) = m.create_debug_info_builder(
            true,
            inkwell::debug_info::DWARFSourceLanguage::C99,
            "t.qi",
            ".",
            "test",
            false,
            "",
            0,
            "",
            inkwell::debug_info::DWARFEmissionKind::Full,
            0,
            false,
            false,
            "",
            "",
        );
        let f = di.create_file("t.qi", ".");
        let 源 = "包 主程序;\n\n函数 入口() {\n    变量 甲: 整数 = 1;\n    打印(甲);\n}\n";
        let fi = 文件信息::新建(f, 源.to_string());
        for 偏移 in 0..源.len() {
            let (行1, 列1) = fi.行列(偏移);
            let (行2, 列2) = crate::parser::位置::偏移转行列(源, 偏移);
            assert_eq!(
                (行1, 列1),
                (行2 as u32, 列2 as u32),
                "偏移 {} 换算不一致",
                偏移
            );
        }
    }

    #[test]
    fn 偏移超出末尾不panic() {
        let ctx = Context::create();
        let m = ctx.create_module("t");
        let (di, _cu) = m.create_debug_info_builder(
            true,
            inkwell::debug_info::DWARFSourceLanguage::C99,
            "t.qi",
            ".",
            "test",
            false,
            "",
            0,
            "",
            inkwell::debug_info::DWARFEmissionKind::Full,
            0,
            false,
            false,
            "",
            "",
        );
        let f = di.create_file("t.qi", ".");
        let fi = 文件信息::新建(f, "变量 甲 = 1;\n".to_string());
        let _ = fi.行列(9999);
        // 偏移落在多字节字符中间也不能切坏 UTF-8
        let _ = fi.行列(1);
        let _ = fi.行列(2);
    }
}
