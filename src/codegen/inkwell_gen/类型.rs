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
    /// 函数值 / 函数指针。u32 是签名在 函数值签名 注册表中的索引。按指针传递。
    函数值(u32),
    /// 尚未确定 / 不关心的类型（如指针语义的复合值）。默认按 i64 处理。
    未知,
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

    /// 是否函数值，返回其签名索引。
    pub fn 函数值索引(&self) -> Option<u32> {
        match self {
            Qi类型::函数值(i) => Some(*i),
            _ => None,
        }
    }
}

impl<'ctx> 后端<'ctx> {
    /// Qi 类型 → LLVM 基础类型。`空` 无基础类型，返回 None。
    /// 结构体按指针传递（不透明 ptr）。
    pub(super) fn llvm基础类型(&self, t: Qi类型) -> Option<BasicTypeEnum<'ctx>> {
        Some(match t {
            Qi类型::整数 | Qi类型::未知 => self.ctx.i64_type().into(),
            Qi类型::浮点数 => self.ctx.f64_type().into(),
            Qi类型::布尔 => self.ctx.bool_type().into(),
            Qi类型::字符串 | Qi类型::结构体(_) | Qi类型::函数值(_) => {
                self.ctx.ptr_type(AddressSpace::default()).into()
            }
            Qi类型::空 => return None,
        })
    }
}
