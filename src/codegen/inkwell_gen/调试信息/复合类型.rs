//! 复合类型的 DWARF 条目 —— 让 `frame variable` 能展开字段，而不是只给一个地址。
//!
//! 上一轮把结构体/数组/枚举一律近似成 `void*`：lldb 打得出 `0x600000c0d340`，
//! 但看不见 `年龄 = 18`。调试的价值有一多半在这儿 —— 看得到地址而看不到内容，
//! 等于还是得靠 `打印` 猜。
//!
//! ## 为什么用 DW_TAG_reference_type 而不是 pointer
//!
//! qi 的结构体/数组/装箱枚举**在语义上就是引用**：堆分配、按指针传参、赋值即别名、
//! 没有值语义的拷贝。DWARF 的 reference 恰好是这个意思，标它是照实说，不是取巧。
//! 附带好处很实际：lldb 对 reference 会自动透视到被引对象，`frame variable 某学生`
//! 直接列字段；标成 pointer 的话得记得敲 `frame variable --ptr-depth 1`，
//! 而 `frame variable`（无参数，最常用的那条）只会打一行地址。
//!
//! ## 递归怎么终止（这是最容易死循环的地方）
//!
//! `结构体 节点 { 值: 整数, 下一个: 节点 }` —— 建 节点 的 DIType 需要 节点 的 DIType。
//! LLVM 的正解是 createReplaceableCompositeType 占位 + RAUW，但 inkwell 0.8 没导出
//! 它（也没导出 forward decl），走 llvm-sys 裸指针得新加一个必须与 inkwell 版本
//! 对齐的依赖。
//!
//! 这里改用**限深展开**：类型按 (类型键, 剩余深度) 缓存，字段递归时深度 -1，
//! 归零就退回不透明 `void*`。深度严格递减 ⇒ 一定终止，且因为缓存键含深度，
//! 生成的 DICompositeType 总数上限是 `类型数 × (最大展开深度+1)` —— 不会指数爆炸。
//! 代价是链表在 lldb 里能连续点开 [`最大展开深度`] 层，再深显示成地址；
//! 对「看一眼这个节点长什么样」这个真实需求，够用。
//!
//! ## 数组只展开长度 + 首元素
//!
//! qi 数组的内存形状是 `[长度@槽0, 元素0@槽1, 元素1@槽2, …]`，元素个数是**运行期**
//! 才知道的。DWARF 表达「长度由同结构里另一个成员决定」要 DW_AT_count 指向那个
//! 成员的 DIE，inkwell 的 `create_array_type` 只接受编译期常量区间，做不到。
//! 所以这里建成 `{ 长度, 首元素 }` 两个成员：长度是真的，首元素让人一眼看到
//! 元素类型和第一个值。**整个数组的逐元素展开做不了**，别期待。

use super::super::后端;
use super::super::类型::{Qi类型, 元素类型};
use super::DW_ATE_SIGNED_CHAR;
use inkwell::debug_info::{AsDIScope, DIFlagsConstants, DIType};

/// DW_TAG_reference_type。inkwell 只透传 u32，没给常量。
const DW_TAG_REFERENCE_TYPE: u32 = 0x10;

/// 复合类型往下展开几层 —— 自引用结构体靠它终止。见模块头。
/// 3 层的意思：`链表.头.下一个.下一个` 看得到字段，再下一层是地址。
pub(super) const 最大展开深度: u8 = 3;

/// 缓存键的类型前缀。字符串拼接而非 enum，纯粹因为键还要带深度，
/// 组一个 (String, u8) 比多定义一个派生 Hash 的 enum 短。
fn 结构体键(idx: u32) -> String {
    format!("s{}", idx)
}
fn 数组键(e: 元素类型) -> String {
    format!("a{:?}", e)
}
fn 枚举键(idx: u32, 装箱: bool) -> String {
    format!("{}{}", if 装箱 { "be" } else { "e" }, idx)
}

impl<'ctx> 后端<'ctx> {
    /// Qi 类型 → DIType，带展开深度预算。深度是**结构层数**，不是指针跳数。
    pub(super) fn 调试_类型深度(&mut self, t: Qi类型, 深度: u8) -> Option<DIType<'ctx>> {
        match t {
            Qi类型::结构体(idx) => self.调试_结构体类型(idx, 深度),
            Qi类型::数组(e) => self.调试_数组类型(e, 深度),
            Qi类型::枚举(idx) => self.调试_枚举tag类型(idx),
            Qi类型::装箱枚举(idx) => self.调试_装箱枚举类型(idx, 深度),
            // 其余（基础类型 / 字符串 / 函数值 / 通道 / 未来 / 裸指针）走原有近似。
            _ => self.调试_标量类型(t),
        }
    }

    /// 结构体 → `引用 → DW_TAG_structure_type{ 逐字段 DW_TAG_member }`。
    /// 字段名是中文原名，偏移/大小从 TargetData 取（`{ptr, i1, i64}` 的第三个字段
    /// 在 16 不在 9 —— 手算这个迟早算错，问 LLVM 最省事）。
    fn 调试_结构体类型(&mut self, idx: u32, 深度: u8) -> Option<DIType<'ctx>> {
        if 深度 == 0 {
            return self.调试_不透明指针();
        }
        let 键 = (结构体键(idx), 深度);
        if let Some(t) = self.调试.as_ref()?.复合类型.get(&键) {
            return Some(*t);
        }
        // 结构体信息要先复制出来：下面递归建字段 DIType 会 &mut self。
        let (名字, 字段名, 字段类型) = {
            let 信息 = self.符号.结构体信息(idx)?;
            (
                信息.名字.clone(),
                信息.字段名.clone(),
                信息.字段类型.clone(),
            )
        };
        let st = self.取结构体llvm(idx).ok()?;
        // 字段名/字段类型 长度不一致是登记期的内部错误，这里宁可不发条目也不越界。
        if 字段名.len() != 字段类型.len() {
            return self.调试_不透明指针();
        }

        // 先把字段的 DIType 全部建好（递归，期间不能持有 self.调试 的借用）。
        let mut 字段di: Vec<DIType<'ctx>> = Vec::with_capacity(字段类型.len());
        for t in &字段类型 {
            let dt = self
                .调试_类型深度(*t, 深度 - 1)
                .or_else(|| self.调试_不透明指针())?;
            字段di.push(dt);
        }

        let (整体位宽, 对齐位) = self.调试_布局(&st);
        let 偏移: Vec<u64> = (0..字段名.len())
            .map(|i| self.调试_字段偏移(st, i as u32))
            .collect();

        let d = self.调试.as_mut()?;
        let 文件 = d.单元.get_file();
        let 作用域 = d.单元.as_debug_info_scope();
        let 成员: Vec<DIType<'ctx>> = 字段名
            .iter()
            .enumerate()
            .map(|(i, 名)| {
                d.生成器
                    .create_member_type(
                        作用域,
                        名,
                        文件,
                        0,
                        字段di[i].get_size_in_bits(),
                        字段di[i].get_align_in_bits(),
                        偏移[i] * 8,
                        DIFlagsConstants::PUBLIC,
                        字段di[i],
                    )
                    .as_type()
            })
            .collect();
        let 复合 = d.生成器.create_struct_type(
            作用域,
            &名字,
            文件,
            0,
            整体位宽,
            对齐位,
            DIFlagsConstants::PUBLIC,
            None,
            &成员,
            0,
            None,
            // unique_id 带深度：同一结构体的不同深度副本是**不同**的类型条目，
            // 共用 id 会被 LLVM 的类型去重合并掉，深层那份就没了。
            &format!("qi.s{}.d{}", idx, 深度),
        );
        let 引用 = d
            .生成器
            .create_reference_type(复合.as_type(), DW_TAG_REFERENCE_TYPE)
            .as_type();
        d.复合类型.insert(键, 引用);
        Some(引用)
    }

    /// 数组 → `引用 → { 长度: 整数, 首元素: T }`。只有这两个成员，见模块头。
    fn 调试_数组类型(&mut self, e: 元素类型, 深度: u8) -> Option<DIType<'ctx>> {
        if 深度 == 0 {
            return self.调试_不透明指针();
        }
        let 键 = (数组键(e), 深度);
        if let Some(t) = self.调试.as_ref()?.复合类型.get(&键) {
            return Some(*t);
        }
        let 元素di = self
            .调试_类型深度(e.标量(), 深度 - 1)
            .or_else(|| self.调试_不透明指针())?;
        let 长度di = self.调试_标量类型(Qi类型::整数)?;
        // 首元素的字节偏移 = 一个「槽」的宽度。数组访问走
        // `GEP(元素llvm类型, base, 下标+1)`，所以槽宽就是元素 LLVM 类型的 ABI 大小
        // —— 这里照抄那份真相，不假设它是 8（布尔元素目前确实不是，见报告）。
        let 槽宽 = {
            let et = self.元素llvm类型(e);
            self.调试_布局(&et).0 / 8
        };
        let 元素名 = 元素类型名(e);

        let d = self.调试.as_mut()?;
        let 文件 = d.单元.get_file();
        let 作用域 = d.单元.as_debug_info_scope();
        let 长度成员 = d
            .生成器
            .create_member_type(
                作用域,
                "长度",
                文件,
                0,
                64,
                64,
                0,
                DIFlagsConstants::PUBLIC,
                长度di,
            )
            .as_type();
        let 首成员 = d
            .生成器
            .create_member_type(
                作用域,
                "首元素",
                文件,
                0,
                元素di.get_size_in_bits(),
                元素di.get_align_in_bits(),
                槽宽 * 8,
                DIFlagsConstants::PUBLIC,
                元素di,
            )
            .as_type();
        let 复合 = d.生成器.create_struct_type(
            作用域,
            &format!("数组<{}>", 元素名),
            文件,
            0,
            (槽宽 + 8) * 8,
            64,
            DIFlagsConstants::PUBLIC,
            None,
            &[长度成员, 首成员],
            0,
            None,
            &format!("qi.arr.{:?}.d{}", e, 深度),
        );
        let 引用 = d
            .生成器
            .create_reference_type(复合.as_type(), DW_TAG_REFERENCE_TYPE)
            .as_type();
        d.复合类型.insert(键, 引用);
        Some(引用)
    }

    /// 无载荷枚举 → DW_TAG_enumeration_type（i64 底层）。
    /// 这层是纯赚：变量本来打出来是 `2`，现在打出来是 `绿`。
    fn 调试_枚举tag类型(&mut self, idx: u32) -> Option<DIType<'ctx>> {
        self.调试_枚举tag(idx)
    }

    /// 建（或取缓存）某枚举的 tag 枚举类型。装箱枚举的 `标记` 成员也用它。
    fn 调试_枚举tag(&mut self, idx: u32) -> Option<DIType<'ctx>> {
        let 键 = (枚举键(idx, false), 0u8);
        if let Some(t) = self.调试.as_ref()?.复合类型.get(&键) {
            return Some(*t);
        }
        let (名字, 变体): (String, Vec<(String, i64)>) = {
            let 信息 = self.符号.枚举.get(idx as usize)?;
            (
                super::super::枚举::枚举显示名(&信息.名字),
                信息.变体.iter().map(|v| (v.名字.clone(), v.tag)).collect(),
            )
        };
        let 底层 = self.调试_标量类型(Qi类型::整数)?;
        let d = self.调试.as_mut()?;
        let 文件 = d.单元.get_file();
        let 作用域 = d.单元.as_debug_info_scope();
        let 枚举项: Vec<_> = 变体
            .iter()
            .map(|(名, tag)| d.生成器.create_enumerator(名, *tag, false))
            .collect();
        let t = d
            .生成器
            .create_enumeration_type(作用域, &名字, 文件, 0, 64, 64, &枚举项, 底层)
            .as_type();
        d.复合类型.insert(键, t);
        Some(t)
    }

    /// 装箱枚举 → `引用 → { 标记: <枚举>, 载荷0..n: 整数 }`。
    ///
    /// 载荷槽按**变体**才知道真类型（`有(字符串)` 和 `有(整数)` 共用槽 1），
    /// DWARF 表达这个要 variant part（DW_TAG_variant_part，DWARF 5 且 LLVM 侧
    /// 只有 Rust 在用，lldb 对 C 语言 CU 不认）。所以载荷一律标成 整数，
    /// 显示的是**位模式**：整数载荷直接可读，浮点/字符串载荷得自己解释。
    /// 至少 `标记` 是准的 —— 知道当前是哪个变体，比什么都没有强得多。
    fn 调试_装箱枚举类型(&mut self, idx: u32, 深度: u8) -> Option<DIType<'ctx>> {
        if 深度 == 0 {
            return self.调试_不透明指针();
        }
        let 键 = (枚举键(idx, true), 深度);
        if let Some(t) = self.调试.as_ref()?.复合类型.get(&键) {
            return Some(*t);
        }
        let (名字, 载荷槽数) = {
            let 信息 = self.符号.枚举.get(idx as usize)?;
            (super::super::枚举::枚举显示名(&信息.名字), 信息.最大载荷槽)
        };
        let tagdi = self.调试_枚举tag(idx)?;
        let 整数di = self.调试_标量类型(Qi类型::整数)?;

        let d = self.调试.as_mut()?;
        let 文件 = d.单元.get_file();
        let 作用域 = d.单元.as_debug_info_scope();
        let mut 成员: Vec<DIType<'ctx>> = vec![d
            .生成器
            .create_member_type(
                作用域,
                "标记",
                文件,
                0,
                64,
                64,
                0,
                DIFlagsConstants::PUBLIC,
                tagdi,
            )
            .as_type()];
        for i in 0..载荷槽数 {
            成员.push(
                d.生成器
                    .create_member_type(
                        作用域,
                        &format!("载荷{}", i),
                        文件,
                        0,
                        64,
                        64,
                        ((i as u64) + 1) * 64,
                        DIFlagsConstants::PUBLIC,
                        整数di,
                    )
                    .as_type(),
            );
        }
        let 复合 = d.生成器.create_struct_type(
            作用域,
            &名字,
            文件,
            0,
            ((载荷槽数 as u64) + 1) * 64,
            64,
            DIFlagsConstants::PUBLIC,
            None,
            &成员,
            0,
            None,
            &format!("qi.be{}.d{}", idx, 深度),
        );
        let 引用 = d
            .生成器
            .create_reference_type(复合.as_type(), DW_TAG_REFERENCE_TYPE)
            .as_type();
        d.复合类型.insert(键, 引用);
        Some(引用)
    }

    /// LLVM 类型的 (位宽, 对齐位)。没有 TargetData 时（单测路径）退回 64/64
    /// —— qi 的槽本来就都是 8 字节，退化值不会把常见形状算错。
    fn 调试_布局(&self, t: &dyn inkwell::types::AnyType<'ctx>) -> (u64, u32) {
        match self.目标数据.as_ref() {
            Some(td) => (td.get_bit_size(t), td.get_abi_alignment(t) * 8),
            None => (64, 64),
        }
    }

    /// 结构体第 i 个字段的**字节**偏移。没有 TargetData 时退回 i*8。
    fn 调试_字段偏移(&self, st: inkwell::types::StructType<'ctx>, i: u32) -> u64 {
        match self.目标数据.as_ref() {
            Some(td) => td.offset_of_element(&st, i).unwrap_or((i as u64) * 8),
            None => (i as u64) * 8,
        }
    }

    /// 不透明 `void*`（深度耗尽 / 拿不到布局时的兜底）。与旧行为一致。
    pub(super) fn 调试_不透明指针(&mut self) -> Option<DIType<'ctx>> {
        let d = self.调试.as_mut()?;
        if d.不透明指针.is_none() {
            let u = d
                .生成器
                .create_basic_type("字节", 8, DW_ATE_SIGNED_CHAR, DIFlagsConstants::PUBLIC)
                .ok()?;
            let p = d.生成器.create_pointer_type(
                "指针",
                u.as_type(),
                64,
                64,
                inkwell::AddressSpace::default(),
            );
            d.不透明指针 = Some(p.as_type());
        }
        d.不透明指针
    }
}

/// 元素类型的显示名（进 `数组<…>` 的类型名）。
fn 元素类型名(e: 元素类型) -> &'static str {
    match e {
        元素类型::整数 => "整数",
        元素类型::浮点数 => "浮点数",
        元素类型::布尔 => "布尔",
        元素类型::指针 => "字符串",
        元素类型::结构体(_) => "结构体",
    }
}
