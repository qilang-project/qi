//! Qi 类型 ↔ LLVM 类型映射。
//!
//! 这里是「类型化 IR」的地基：编译期确定每个值的 Qi 语义类型（`Qi类型`），
//! 再由它派生出唯一确定的 LLVM 基础类型。杜绝旧后端「统统当 i64/ptr」的猜测。

use super::后端;
use crate::parser::ast::{BasicType, TypeNode};
use inkwell::types::BasicTypeEnum;
use inkwell::AddressSpace;

/// Qi 的语义类型。类型检查器在各表达式/变量上标注它；
/// 降级时据它选择正确的 LLVM 指令（int vs float、i1 vs i64…）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Qi类型 {
    整数,   // i64
    浮点数, // double
    布尔,   // i1
    字符串, // ptr (i8*)
    空,     // void
    /// 结构体实例（按指针传递）。u32 是结构体在 结构体注册表 中的索引。
    结构体(u32),
    /// 无载荷枚举（纯 i64 tag，零分配，非 RC）。u32 是枚举注册表索引。
    枚举(u32),
    /// 带载荷枚举（堆指针：槽0=tag，槽1..=载荷）。RC 管理。u32 是枚举注册表索引。
    装箱枚举(u32),
    /// 函数值 / 函数指针。u32 是签名在 函数值签名 注册表中的索引。按指针传递。
    函数值(u32),
    /// 数组（连续堆分配，按指针传递）。参数是元素类型。
    数组(元素类型),
    /// 通道<T>（运行时通道句柄指针）。参数是元素类型 —— 收发两端据它做
    /// i64 位模式装/还原（浮点 bitcast、字符串/结构体 ptr↔int、布尔 zext/trunc）。
    /// 非 RC（句柄本体不回收，与旧行为一致）。
    通道(元素类型),
    /// 未来<T>（eager future 指针）。参数是内部值类型。按指针传递。
    未来(元素类型),
    /// 尚未确定 / 不关心的类型（如指针语义的复合值）。默认按 i64 处理。
    未知,
}

/// 数组元素类型（基础标量 + 结构体；嵌套数组/函数值按 指针 处理）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum 元素类型 {
    整数,
    浮点数,
    布尔,
    指针, // 字符串/函数值/类型不明的指针语义值
    /// 结构体元素（按指针存槽）。u32 是结构体注册表索引 —— Round E2：
    /// 已知元素是结构体时，数组释放走 qi.release.arr.s<idx> 类型化级联，
    /// 元素访问返回精确的 结构体(idx) 类型。
    结构体(u32),
}

impl 元素类型 {
    /// 元素的标量 Qi 类型（用于 数组访问 返回类型）。
    pub fn 标量(self) -> Qi类型 {
        match self {
            元素类型::整数 => Qi类型::整数,
            元素类型::浮点数 => Qi类型::浮点数,
            元素类型::布尔 => Qi类型::布尔,
            元素类型::指针 => Qi类型::字符串,
            元素类型::结构体(i) => Qi类型::结构体(i),
        }
    }

    /// 由标量 Qi 类型推元素类型。
    pub fn 从标量(t: Qi类型) -> 元素类型 {
        match t {
            Qi类型::浮点数 => 元素类型::浮点数,
            Qi类型::布尔 => 元素类型::布尔,
            Qi类型::结构体(i) => 元素类型::结构体(i),
            Qi类型::字符串 | Qi类型::函数值(_) => 元素类型::指针,
            _ => 元素类型::整数,
        }
    }
}

impl Qi类型 {
    /// 从类型注解节点推断 Qi 类型（不解析自定义类型 —— 那要注册表）。
    /// 自定义/结构体类型返回 未知，由 符号表::解析类型 补全为 结构体(idx)。
    pub fn 从注解(t: &TypeNode) -> Qi类型 {
        match t {
            TypeNode::基础类型(BasicType::整数)
            | TypeNode::基础类型(BasicType::长整数)
            | TypeNode::基础类型(BasicType::短整数)
            | TypeNode::基础类型(BasicType::字节) => Qi类型::整数,
            TypeNode::基础类型(BasicType::浮点数) => Qi类型::浮点数,
            TypeNode::基础类型(BasicType::布尔) => Qi类型::布尔,
            TypeNode::基础类型(BasicType::字符串) => Qi类型::字符串,
            TypeNode::基础类型(BasicType::空) => Qi类型::空,
            _ => Qi类型::未知,
        }
    }

    /// 是否浮点，用于选择 f-指令还是 i-指令。
    pub fn 是浮点(&self) -> bool {
        matches!(self, Qi类型::浮点数)
    }

    /// 是否结构体（指针语义）。
    pub fn 结构体索引(&self) -> Option<u32> {
        match self {
            Qi类型::结构体(i) => Some(*i),
            _ => None,
        }
    }

    /// 是否枚举（无载荷或装箱），返回其枚举注册表索引。
    pub fn 枚举索引(&self) -> Option<u32> {
        match self {
            Qi类型::枚举(i) | Qi类型::装箱枚举(i) => Some(*i),
            _ => None,
        }
    }

    /// 是否函数值，返回其签名索引。
    pub fn 函数值索引(&self) -> Option<u32> {
        match self {
            Qi类型::函数值(i) => Some(*i),
            _ => None,
        }
    }

    /// 是否数组，返回其元素类型。
    pub fn 数组元素(&self) -> Option<元素类型> {
        match self {
            Qi类型::数组(e) => Some(*e),
            _ => None,
        }
    }

    /// 是否未来，返回其内部值类型。
    pub fn 未来内部(&self) -> Option<元素类型> {
        match self {
            Qi类型::未来(e) => Some(*e),
            _ => None,
        }
    }

    /// 是否通道，返回其元素类型。
    pub fn 通道元素(&self) -> Option<元素类型> {
        match self {
            Qi类型::通道(e) => Some(*e),
            _ => None,
        }
    }
}

impl<'ctx> 后端<'ctx> {
    /// Qi 类型 → LLVM 基础类型。`空` 无基础类型，返回 None。
    /// 结构体按指针传递（不透明 ptr）。
    pub(super) fn llvm基础类型(&self, t: Qi类型) -> Option<BasicTypeEnum<'ctx>> {
        Some(match t {
            // 无载荷枚举 = i64 tag
            Qi类型::整数 | Qi类型::未知 | Qi类型::枚举(_) => self.ctx.i64_type().into(),
            Qi类型::浮点数 => self.ctx.f64_type().into(),
            Qi类型::布尔 => self.ctx.bool_type().into(),
            // 装箱枚举 = 堆指针；通道 = 运行时句柄指针
            Qi类型::字符串
            | Qi类型::结构体(_)
            | Qi类型::装箱枚举(_)
            | Qi类型::函数值(_)
            | Qi类型::数组(_)
            | Qi类型::通道(_)
            | Qi类型::未来(_) => self.ctx.ptr_type(AddressSpace::default()).into(),
            Qi类型::空 => return None,
        })
    }
}
