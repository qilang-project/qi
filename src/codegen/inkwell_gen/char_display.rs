//! 字符值的显示：插值 `"${c}"`、`打印行(c)`、`"前缀" + c` 按字符输出，而不是码点数字。
//!
//! 后端类型格里没有「字符」—— `'中'` 降成 i64 码点 20013，类型是 `Qi类型::整数`，
//! 算术、比较、存数组、传参都按整数走，这是故意的（给格加一个变体要改上百处
//! `Qi类型::整数` 的匹配和相等判断，换来的只是显示这一处的区别）。
//!
//! 所以「是不是字符」放在旁边单独记，只在**显示**的那几处查：
//!   - 字符字面量 `'中'`；
//!   - 局部槽：注解为 `字符` 的变量 / 形参，或无注解但初值是字符表达式的变量
//!     （按 alloca 记，块作用域遮蔽的同名变量各有各的槽，不会串）；
//!   - 调用声明返回 `字符` 的函数（同名函数里只要有一个返回别的类型就不认，
//!     宁可显示成数字也不把整数错显示成字符）。
//!
//! 认出来之后交给运行时 `qi_string_format_parts` 的字符段（FMT_PART_CHAR）
//! 编码成 UTF-8；非法码点（代理区 / 超过 U+10FFFF）显示成 U+FFFD。

use super::后端;
use crate::parser::ast::{AstNode, BasicType, LiteralValue, TypeNode};

/// 类型注解是不是 `字符`。
pub(super) fn is_char_annotation(t: Option<&TypeNode>) -> bool {
    matches!(t, Some(TypeNode::基础类型(BasicType::字符)))
}

impl<'ctx> 后端<'ctx> {
    /// 登记函数声明的返回类型（声明原型时调用）。
    pub(super) fn note_return_type(&mut self, name: &str, ret: Option<&TypeNode>) {
        if is_char_annotation(ret) {
            self.char_returning_fns.insert(name.to_string());
        } else {
            self.non_char_returning_fns.insert(name.to_string());
        }
    }

    /// 把刚声明的局部槽标成「存的是字符」。
    pub(super) fn mark_char_slot(&mut self, slot: inkwell::values::PointerValue<'ctx>) {
        self.char_slots.insert(slot);
    }

    /// 这个表达式的值是不是字符（见模块注释的三种来源）。
    pub(super) fn is_char_expr(&self, node: &AstNode) -> bool {
        match node {
            AstNode::字面量表达式(lit) => matches!(lit.value, LiteralValue::字符(_)),
            AstNode::标识符表达式(id) => self
                .变量表
                .get(&id.name)
                .is_some_and(|(slot, _)| self.char_slots.contains(slot)),
            AstNode::函数调用表达式(c) => {
                !self.变量表.contains_key(&c.callee)
                    && self.char_returning_fns.contains(&c.callee)
                    && !self.non_char_returning_fns.contains(&c.callee)
            }
            _ => false,
        }
    }
}
