//! 真类型检查器 —— 作用域符号表 + 变量/函数/结构体/方法类型推断。
//!
//! 这是本次重写的核心：每个表达式都能问出「你是什么类型」，从而降级时
//! 选对 LLVM 指令 / 找到对应方法。检查器收集函数签名、结构体布局、方法签名，
//! 并按块作用域推断变量类型；表达式类型由 `推断表达式类型` 递归求出。
//!
//! 阶段8 关键：接收者可以是**任意表达式**（含上一次方法调用的结果），
//! 因此链式 `创建().放(1).放(2)` 天生可解析 —— 每步返回类型已知。

use super::类型::Qi类型;
use crate::parser::ast::{AstNode, BinaryOperator, LiteralValue};
use std::collections::HashMap;

/// 函数签名：参数类型 + 返回类型。
#[derive(Debug, Clone)]
pub struct 函数签名 {
    pub 参数: Vec<Qi类型>,
    pub 返回: Qi类型,
}

/// 结构体布局：字段名 → (索引, 类型)，按声明顺序。
#[derive(Debug, Clone)]
pub struct 结构体信息 {
    pub 名字: String,
    pub 字段名: Vec<String>,
    pub 字段类型: Vec<Qi类型>,
}

impl 结构体信息 {
    /// 字段索引 + 类型。
    pub fn 查字段(&self, name: &str) -> Option<(u32, Qi类型)> {
        self.字段名
            .iter()
            .position(|f| f == name)
            .map(|i| (i as u32, self.字段类型[i]))
    }
}

/// 作用域符号表。函数/结构体/方法签名全局共享；变量类型按块作用域压栈。
#[derive(Default)]
pub struct 符号表 {
    pub 函数: HashMap<String, 函数签名>,
    /// 结构体注册表：索引即 Qi类型::结构体(idx) 的 idx。
    pub 结构体: Vec<结构体信息>,
    结构体索引: HashMap<String, u32>,
    /// 方法签名：(结构体名, 方法名) → 签名（参数不含接收者）。
    pub 方法: HashMap<(String, String), 函数签名>,
    /// 函数值签名注册表：索引即 Qi类型::函数值(idx) 的 idx。
    pub 函数值签名: Vec<函数签名>,
    /// 顶层函数名 → 其函数值签名索引（登记函数时预填，供「函数名当值」）。
    函数值索引表: HashMap<String, u32>,
    作用域: Vec<HashMap<String, Qi类型>>,
}

impl 符号表 {
    pub fn new() -> Self {
        符号表 {
            函数: HashMap::new(),
            结构体: Vec::new(),
            结构体索引: HashMap::new(),
            方法: HashMap::new(),
            函数值签名: Vec::new(),
            函数值索引表: HashMap::new(),
            作用域: vec![HashMap::new()],
        }
    }

    /// 登记一个函数值签名，返回索引。
    pub fn 登记函数值签名(&mut self, sig: 函数签名) -> u32 {
        let idx = self.函数值签名.len() as u32;
        self.函数值签名.push(sig);
        idx
    }

    /// 按索引拿函数值签名。
    pub fn 函数值签名(&self, idx: u32) -> Option<&函数签名> {
        self.函数值签名.get(idx as usize)
    }

    /// 为一个顶层函数预登记「作为值」的签名索引（登记函数时调用）。
    pub fn 预登记函数值(&mut self, name: &str) {
        if self.函数值索引表.contains_key(name) {
            return;
        }
        if let Some(sig) = self.函数.get(name).cloned() {
            let idx = self.登记函数值签名(sig);
            self.函数值索引表.insert(name.to_string(), idx);
        }
    }

    /// 顶层函数名 → 其函数值类型（供「函数名当值」用）。immutable。
    pub fn 函数为值(&self, name: &str) -> Option<Qi类型> {
        self.函数值索引表.get(name).copied().map(Qi类型::函数值)
    }

    pub fn 进入作用域(&mut self) {
        self.作用域.push(HashMap::new());
    }

    pub fn 退出作用域(&mut self) {
        self.作用域.pop();
    }

    /// 在当前作用域声明变量类型。
    pub fn 声明变量(&mut self, name: &str, t: Qi类型) {
        if let Some(scope) = self.作用域.last_mut() {
            scope.insert(name.to_string(), t);
        }
    }

    /// 在全局作用域（栈底 作用域[0]）声明变量类型。全局无论何时可见。
    pub fn 声明全局(&mut self, name: &str, t: Qi类型) {
        if let Some(scope) = self.作用域.first_mut() {
            scope.insert(name.to_string(), t);
        }
    }

    /// 由内向外查找变量类型。
    pub fn 查变量(&self, name: &str) -> Option<Qi类型> {
        for scope in self.作用域.iter().rev() {
            if let Some(t) = scope.get(name) {
                return Some(*t);
            }
        }
        None
    }

    /// 登记一个结构体，返回其索引（幂等）。
    pub fn 登记结构体(&mut self, 信息: 结构体信息) -> u32 {
        if let Some(idx) = self.结构体索引.get(&信息.名字) {
            return *idx;
        }
        let idx = self.结构体.len() as u32;
        self.结构体索引.insert(信息.名字.clone(), idx);
        self.结构体.push(信息);
        idx
    }

    /// 按名字查结构体索引。
    pub fn 结构体索引(&self, name: &str) -> Option<u32> {
        self.结构体索引.get(name).copied()
    }

    /// 按索引拿结构体信息。
    pub fn 结构体信息(&self, idx: u32) -> Option<&结构体信息> {
        self.结构体.get(idx as usize)
    }

    /// 解析类型注解为 Qi 类型，自定义类型解析成 结构体(idx)，函数类型解析成 函数值(idx)。
    /// 需 &mut 因为函数类型会登记新签名。
    pub fn 解析类型(&mut self, t: &crate::parser::ast::TypeNode) -> Qi类型 {
        use crate::parser::ast::TypeNode;
        match t {
            TypeNode::自定义类型(name) | TypeNode::结构体类型(crate::parser::ast::StructType { name, .. }) => {
                self.结构体索引(name)
                    .map(Qi类型::结构体)
                    .unwrap_or(Qi类型::未知)
            }
            TypeNode::函数类型(ft) => {
                let 参数: Vec<Qi类型> = ft.parameters.iter().map(|p| self.解析类型(p)).collect();
                let 返回 = self.解析类型(&ft.return_type);
                let idx = self.登记函数值签名(函数签名 { 参数, 返回 });
                Qi类型::函数值(idx)
            }
            _ => Qi类型::从注解(t),
        }
    }

    /// 从内建/用户函数名推断返回类型。
    pub fn 查函数返回(&self, callee: &str) -> Option<Qi类型> {
        if let Some(sig) = self.函数.get(callee) {
            return Some(sig.返回);
        }
        内建返回类型(callee)
    }
}

/// 内建（运行时）函数的返回类型。
pub fn 内建返回类型(callee: &str) -> Option<Qi类型> {
    Some(match callee {
        "整数转字符串" | "int_to_string" | "浮点数转字符串" | "float_to_string" => {
            Qi类型::字符串
        }
        "字符串转整数" | "string_to_int" => Qi类型::整数,
        "字符串转浮点数" | "string_to_float" => Qi类型::浮点数,
        "整数转浮点数" | "int_to_float" => Qi类型::浮点数,
        "浮点数转整数" | "float_to_int" => Qi类型::整数,
        _ => return None,
    })
}

/// 推断表达式的 Qi 类型（不产生 IR，仅用于选指令 / 声明变量 / 找方法）。
pub fn 推断表达式类型(node: &AstNode, 表: &符号表) -> Qi类型 {
    match node {
        AstNode::字面量表达式(lit) => match &lit.value {
            LiteralValue::整数(_) => Qi类型::整数,
            LiteralValue::浮点数(_) => Qi类型::浮点数,
            LiteralValue::字符串(_) => Qi类型::字符串,
            LiteralValue::布尔(_) => Qi类型::布尔,
            LiteralValue::字符(_) => Qi类型::整数,
        },
        AstNode::标识符表达式(id) => 表
            .查变量(&id.name)
            .or_else(|| 表.函数为值(&id.name)) // 变量没有则看是不是顶层函数名（当值用）
            .unwrap_or(Qi类型::未知),
        AstNode::二元操作表达式(b) => {
            use BinaryOperator::*;
            match b.operator {
                等于 | 不等于 | 大于 | 小于 | 大于等于 | 小于等于 | 与 | 或 => Qi类型::布尔,
                加 | 减 | 乘 | 除 | 取余 => {
                    let l = 推断表达式类型(&b.left, 表);
                    let r = 推断表达式类型(&b.right, 表);
                    if b.operator == 加 && (l == Qi类型::字符串 || r == Qi类型::字符串) {
                        Qi类型::字符串
                    } else if l == Qi类型::浮点数 || r == Qi类型::浮点数 {
                        Qi类型::浮点数
                    } else {
                        Qi类型::整数
                    }
                }
            }
        }
        AstNode::字符串连接表达式(_) => Qi类型::字符串,
        AstNode::一元操作表达式(u) => 推断表达式类型(&u.operand, 表),
        AstNode::函数调用表达式(call) => {
            // 优先：callee 是局部函数值变量 → 用其签名返回类型（间接调用）
            if let Some(t) = 表.查变量(&call.callee) {
                if let Some(idx) = t.函数值索引() {
                    if let Some(sig) = 表.函数值签名(idx) {
                        return sig.返回;
                    }
                }
            }
            表.查函数返回(&call.callee).unwrap_or(Qi类型::未知)
        }
        AstNode::赋值表达式(a) => 推断表达式类型(&a.value, 表),
        AstNode::结构体实例化表达式(lit) => 表
            .结构体索引(&lit.struct_name)
            .map(Qi类型::结构体)
            .unwrap_or(Qi类型::未知),
        AstNode::字段访问表达式(fa) => {
            let obj = 推断表达式类型(&fa.object, 表);
            if let Some(idx) = obj.结构体索引() {
                if let Some(info) = 表.结构体信息(idx) {
                    if let Some((_, ft)) = info.查字段(&fa.field) {
                        return ft;
                    }
                }
            }
            Qi类型::未知
        }
        AstNode::方法调用表达式(mc) => {
            // 接收者是任意表达式（链式关键）：先定其类型，再查方法返回。
            let recv = 推断表达式类型(&mc.object, 表);
            if let Some(idx) = recv.结构体索引() {
                if let Some(info) = 表.结构体信息(idx) {
                    if let Some(sig) = 表.方法.get(&(info.名字.clone(), mc.method_name.clone())) {
                        return sig.返回;
                    }
                }
            }
            Qi类型::未知
        }
        _ => Qi类型::未知,
    }
}
