//! 单元检查器·表达式层：表达式类型推断 + 调用/成员访问的宽容检查。
//!
//! 与 单元检查.rs 同属一个检查器（本文件是其子模块，直接访问其私有状态）。
//! 设计原则（宽容默认/字面量来源门槛/沉默面）见父模块文档。

use super::*;

impl 单元检查器 {
    // ---------------------------------------------------------------------
    // 表达式
    // ---------------------------------------------------------------------

    pub(super) fn 推断表达式(&mut self, node: &AstNode) -> 推断 {
        match node {
            AstNode::字面量表达式(l) => Some(match &l.value {
                LiteralValue::整数(_) => TypeNode::基础类型(BasicType::整数),
                LiteralValue::浮点数(_) => TypeNode::基础类型(BasicType::浮点数),
                LiteralValue::字符串(_) => TypeNode::基础类型(BasicType::字符串),
                LiteralValue::布尔(_) => TypeNode::基础类型(BasicType::布尔),
                LiteralValue::字符(_) => TypeNode::基础类型(BasicType::字符),
            }),
            AstNode::标识符表达式(id) => self.解析标识符(id, true),
            AstNode::二元操作表达式(b) => self.推断二元(b),
            AstNode::一元操作表达式(u) => {
                let t = self.推断表达式(&u.operand);
                match u.operator {
                    UnaryOperator::非 => Some(TypeNode::基础类型(BasicType::布尔)),
                    _ => t,
                }
            }
            AstNode::类型转换表达式(c) => {
                self.推断表达式(&c.expression);
                // `x 作为 T`：codegen 容许很多转换，这里不校验，结果就是 T
                self.解析类型(&c.target_type)
            }
            AstNode::函数调用表达式(call) => self.检查函数调用(call),
            AstNode::方法调用表达式(mc) => self.检查方法调用(mc),
            AstNode::字段访问表达式(fa) => self.检查字段访问(fa),
            AstNode::数组访问表达式(aa) => {
                let t = self.推断表达式(&aa.array);
                self.推断表达式(&aa.index);
                match t {
                    Some(TypeNode::数组类型(a)) => self.解析类型(&a.element_type),
                    Some(TypeNode::列表类型(l)) => self.解析类型(&l.element_type),
                    Some(TypeNode::字典类型(d)) => self.解析类型(&d.value_type),
                    // 字符串下标/未知容器 → 未知（不报：字典句柄、切片等都合法）
                    _ => None,
                }
            }
            AstNode::数组字面量表达式(al) => {
                let mut 元素型: 推断 = None;
                let mut 一致 = true;
                let mut 各元素型: Vec<推断> = Vec::with_capacity(al.elements.len());
                for (i, e) in al.elements.iter().enumerate() {
                    let t = self.推断表达式(e);
                    各元素型.push(t.clone());
                    if i == 0 {
                        元素型 = t;
                    } else if t != 元素型 {
                        一致 = false;
                    }
                }
                // 纯字面量数组的混族元素（[1, "a"]）有把握报；
                // 任一元素非字面量 → 整体沉默（宽容）
                self.检查纯字面量数组(al, &各元素型);
                // 混合元素/未知元素 → 整体未知（不报错：宽容）
                match (一致, 元素型) {
                    (true, Some(t)) => Some(TypeNode::数组类型(ArrayType {
                        element_type: Box::new(t),
                        size: Some(al.elements.len()),
                    })),
                    _ => None,
                }
            }
            AstNode::区间表达式(r) => {
                self.推断表达式(&r.start);
                self.推断表达式(&r.end);
                None
            }
            AstNode::字符串连接表达式(sc) => {
                self.推断表达式(&sc.left);
                self.推断表达式(&sc.right);
                Some(TypeNode::基础类型(BasicType::字符串))
            }
            AstNode::赋值表达式(a) => self.检查赋值(a),
            AstNode::结构体实例化表达式(sl) => self.检查结构体字面量(sl),
            AstNode::等待表达式(w) => match self.推断表达式(&w.expression) {
                Some(TypeNode::未来类型(inner)) => self.解析类型(&inner),
                t => t, // eager future：等待非未来值 = 原值
            },
            AstNode::协程启动表达式(g) => {
                self.推断表达式(&g.expression);
                None // 启动 f() 的值语义（future/句柄）依门控而定 → 未知
            }
            AstNode::异步块表达式(b) => {
                self.进作用域();
                for st in &b.body {
                    self.走语句(st);
                }
                self.出作用域();
                None
            }
            AstNode::通道创建表达式(c) => {
                if let Some(cap) = &c.capacity {
                    self.推断表达式(cap);
                }
                Some(TypeNode::通道类型(ChannelType {
                    element_type: Box::new(c.element_type.clone()),
                }))
            }
            AstNode::通道发送表达式(s) => {
                self.推断表达式(&s.channel);
                self.推断表达式(&s.value);
                Some(TypeNode::基础类型(BasicType::空))
            }
            AstNode::通道接收表达式(r) => match self.推断表达式(&r.channel) {
                Some(TypeNode::通道类型(c)) => self.解析类型(&c.element_type),
                _ => None,
            },
            AstNode::选择表达式(sel) => {
                let mut 所有 = sel.cases.iter().collect::<Vec<_>>();
                if let Some(d) = &sel.default_case {
                    所有.push(d);
                }
                if let Some(t) = &sel.timeout_case {
                    所有.push(t);
                }
                for case in 所有 {
                    self.进作用域();
                    match &case.kind {
                        SelectCaseKind::通道接收 { channel, variable } => {
                            let et = match self.推断表达式(channel) {
                                Some(TypeNode::通道类型(c)) => {
                                    self.解析类型(&c.element_type)
                                }
                                _ => None,
                            };
                            if let Some(v) = variable {
                                self.绑定(v, et);
                            }
                        }
                        SelectCaseKind::通道发送 { channel, value } => {
                            self.推断表达式(channel);
                            self.推断表达式(value);
                        }
                        SelectCaseKind::超时 { 毫秒 } => {
                            self.推断表达式(毫秒);
                        }
                        SelectCaseKind::默认 => {}
                    }
                    for st in &case.body {
                        self.走语句(st);
                    }
                    self.出作用域();
                }
                None
            }
            AstNode::取地址表达式(a) => {
                let t = self.推断表达式(&a.expression);
                t.map(|inner| {
                    TypeNode::指针类型(PointerType {
                        target_type: Box::new(inner),
                    })
                })
            }
            AstNode::解引用表达式(d) => match self.推断表达式(&d.expression) {
                Some(TypeNode::指针类型(p)) => self.解析类型(&p.target_type),
                Some(TypeNode::引用类型(r)) => self.解析类型(&r.target_type),
                _ => None,
            },
            AstNode::闭包表达式(c) => {
                let 旧返回 = self.当前返回.take();
                self.当前返回 = c.return_type.as_ref().and_then(|t| self.解析类型(t));
                self.进作用域();
                for p in &c.parameters {
                    let t = p.type_annotation.as_ref().and_then(|a| self.解析类型(a));
                    self.绑定(&p.name, t);
                }
                for st in &c.body {
                    self.走语句(st);
                }
                self.出作用域();
                self.当前返回 = 旧返回;
                None // 闭包值类型（函数值）→ 未知（间接调用一律沉默）
            }
            AstNode::匹配表达式(m) => {
                self.推断表达式(&m.value);
                for arm in &m.arms {
                    self.进作用域();
                    self.绑定模式(&arm.pattern);
                    if let Some(g) = &arm.guard {
                        self.推断表达式(g);
                    }
                    for st in &arm.body {
                        self.走语句(st);
                    }
                    self.出作用域();
                }
                None // 匹配的值类型（各臂合一）→ 未知
            }
            AstNode::格式字符串表达式(f) => {
                // 洞内表达式走正常检查，但只保留「未定义变量/未定义函数」
                // （家族比对暂不开——逐步开火纪律，见 检查模板串）
                self.检查模板串(f);
                Some(TypeNode::基础类型(BasicType::字符串))
            }
            // 声明类节点出现在表达式位置（理论不发生）→ 交语句处理器，返回未知
            other @ (AstNode::变量声明(_) | AstNode::函数声明(_)) => {
                self.走语句(other);
                None
            }
            _ => None, // 其余未知节点 → 沉默
        }
    }

    pub(super) fn 推断二元(&mut self, b: &BinaryExpression) -> 推断 {
        let l = self.推断表达式(&b.left);
        let r = self.推断表达式(&b.right);
        use BinaryOperator::*;
        match b.operator {
            等于 | 不等于 | 大于 | 小于 | 大于等于 | 小于等于 | 与 | 或 => {
                Some(TypeNode::基础类型(BasicType::布尔))
            }
            位与 | 位或 | 位异或 | 左移 | 右移 => {
                Some(TypeNode::基础类型(BasicType::整数))
            }
            加 | 减 | 乘 | 除 | 取余 => {
                let 是串 = |t: &推断| matches!(t, Some(TypeNode::基础类型(BasicType::字符串)));
                let 是浮 = |t: &推断| matches!(t, Some(TypeNode::基础类型(BasicType::浮点数)));
                if b.operator == 加 && (是串(&l) || 是串(&r)) {
                    // `+` 任一侧字符串 = 拼接（另一侧自动转字符串）
                    Some(TypeNode::基础类型(BasicType::字符串))
                } else if 是浮(&l) || 是浮(&r) {
                    Some(TypeNode::基础类型(BasicType::浮点数))
                } else if l.is_none() || r.is_none() {
                    None // 任一侧未知 → 结果未知（不猜整数，避免误导下游比对）
                } else {
                    Some(TypeNode::基础类型(BasicType::整数))
                }
            }
        }
    }

    pub(super) fn 检查赋值(&mut self, a: &AssignmentExpression) -> 推断 {
        let 值型 = self.推断表达式(&a.value);
        let 值是字面量 = self.是字面量来源(&a.value);
        let 目标型: 推断 = match a.target.as_ref() {
            AstNode::标识符表达式(id) => {
                let t = self.解析标识符(id, true);
                // 常量重赋值：目标解析到 `常量` 绑定 → 报错（重入声明不走这里）
                if self.是常量(&id.name) {
                    self.报(TypeError::General {
                        message: format!("不能给常量 `{}` 重新赋值", id.name),
                        span: a.span,
                    });
                }
                // 维护字面量标记：字面量赋值 → 标记；句柄/调用结果覆写 → 清除
                self.设字面量(&id.name, 值是字面量);
                t
            }
            other => self.推断表达式(other),
        };
        if let (Some(t), Some(v), true) = (&目标型, &值型, 值是字面量) {
            if !家族相容(t, v) {
                self.报(TypeError::TypeMismatch {
                    expected: format!("赋值目标类型是 {}", 类型显示(t)),
                    actual: format!("赋的值却是 {}", 类型显示(v)),
                    span: a.span,
                });
            }
        }
        值型
    }

    pub(super) fn 绑定模式(&mut self, pat: &MatchPattern) {
        match pat {
            MatchPattern::变量绑定(n) => self.绑定(n, None),
            MatchPattern::结构体模式 { fields, .. } => {
                for (_, p) in fields {
                    self.绑定模式(p);
                }
            }
            MatchPattern::枚举变体模式 { bindings, .. } => {
                for p in bindings {
                    self.绑定模式(p);
                }
            }
            MatchPattern::元组模式(ps) | MatchPattern::数组模式(ps) | MatchPattern::或模式(ps) => {
                for p in ps {
                    self.绑定模式(p);
                }
            }
            MatchPattern::范围模式 { start, end, .. } => {
                if let Some(s) = start {
                    self.推断表达式(s);
                }
                if let Some(e) = end {
                    self.推断表达式(e);
                }
            }
            MatchPattern::字面量(_) | MatchPattern::通配符 => {}
        }
    }

    // ---------------------------------------------------------------------
    // 调用 / 成员访问
    // ---------------------------------------------------------------------

    pub(super) fn 检查函数调用(&mut self, call: &FunctionCallExpression) -> 推断 {
        // 实参先各自推断一遍（收集类型 + 报实参内部的错）
        let 实参型: Vec<推断> = call.arguments.iter().map(|a| self.推断表达式(a)).collect();
        let n = call.arguments.len();
        let callee = call.callee.trim_start_matches(':');

        // 模块限定调用（语法动作合成，如 数学.最大值）：查注册表，miss 沉默
        if let Some(q) = &call.module_qualifier {
            let 模块 = self.模块别名.get(q).cloned().unwrap_or_else(|| q.clone());
            let reg = 注册表();
            if let Some(f) = reg
                .get_function(&模块, callee)
                .or_else(|| reg.get_function(&format!("标准库.{}", 模块), callee))
            {
                return 注册表返回保守(&f.return_type);
            }
            return None; // 模块限定：未播种来源，一律沉默
        }

        // 1) callee 是局部变量/全局变量（函数值间接调用）→ 沉默
        if self.查局部(callee).is_some() || self.全局变量.contains_key(callee) {
            return match self.查局部(callee).or_else(|| self.全局变量.get(callee)) {
                Some(Some(TypeNode::函数类型(f))) => self.解析类型(&f.return_type),
                _ => None,
            };
        }

        // 2) 内置函数（打印族/转换/同步/协程/AI 糖/构造子）。
        //    用户也定义了同名函数时两边解析有歧义（codegen 拦截顺序因名而异）
        //    → 整体沉默，只在两边都认识时不做任何硬校验。
        let 内置 = 内置函数(callee);
        let 用户 = self.函数表.get(callee).cloned();
        match (内置, 用户) {
            (Some(_), Some(_)) => return None, // 同名歧义 → 沉默
            (Some(ret), None) => return ret,
            (None, Some(重载集)) => {
                // 3) 用户函数（全单元合并的重载集，按元数匹配）
                return self.按签名检查(
                    callee,
                    &重载集,
                    n,
                    &实参型,
                    &call.arguments,
                    call.span,
                );
            }
            (None, None) => {}
        }

        // 4) 裸枚举变体构造：`有些(5)` → 枚举类型（载荷元数+类型可查：跨枚举重名沉默）
        if let Some(枚举名) = self.变体归属.get(callee).cloned() {
            if !self.歧义变体.contains(callee) {
                if let Some(&元数) = self.变体元数.get(&(枚举名.clone(), callee.to_string()))
                {
                    if n != 元数 {
                        self.报(TypeError::FunctionCallError {
                            message: format!(
                                "枚举变体 '{}.{}' 载荷元数不匹配: 期望 {}, 实际 {}",
                                枚举名, callee, 元数, n
                            ),
                            span: call.span,
                        });
                    } else {
                        self.检查变体载荷类型(
                            &枚举名,
                            callee,
                            &实参型,
                            &call.arguments,
                            call.span,
                        );
                    }
                }
            }
            return Some(TypeNode::自定义类型(枚举名));
        }

        // 5) 类型名当构造器/静态调用、导入符号、类型参数 → 沉默
        if self.结构体表.contains_key(callee)
            || self.枚举表.contains_key(callee)
            || self.联合体集.contains(callee)
            || self.导入符号.contains(callee)
            || self.当前类型参数.contains(callee)
        {
            return None;
        }

        // 6) 无限定标准库回退（镜像 codegen 尝试无限定标准库）
        if let Some(ret) = 注册表任意模块(callee) {
            return ret;
        }

        // 7) 哪个来源都不认识 → 有把握报未定义
        self.报(TypeError::FunctionCallError {
            message: format!("未定义的函数 '{}'", callee),
            span: call.span,
        });
        None
    }

    /// 按用户函数重载集检查元数与（有把握的）实参类型，返回推断的返回类型。
    pub(super) fn 按签名检查(
        &mut self,
        名: &str,
        重载集: &[函数签名],
        n: usize,
        实参型: &[推断],
        实参节点: &[AstNode],
        span: crate::lexer::Span,
    ) -> 推断 {
        let 匹配: Vec<&函数签名> = 重载集.iter().filter(|s| s.接受个数(n)).collect();
        if 匹配.is_empty() {
            // 全单元没有任何一个重载吃这个元数 → 有把握报（默认值/变参已计入）
            let 元数集: Vec<String> = 重载集.iter().map(|s| s.参数.len().to_string()).collect();
            self.报(TypeError::FunctionCallError {
                message: format!(
                    "函数 '{}' 参数数量不匹配: 期望 {}, 实际 {}",
                    名,
                    元数集.join("/"),
                    n
                ),
                span,
            });
            return None;
        }
        // 恰好一个候选时才做实参类型比对（多候选 → 类型检查沉默，只用返回类型交集）
        if 匹配.len() == 1 {
            let sig = 匹配[0];
            let 旧类型参数 = std::mem::replace(
                &mut self.当前类型参数,
                sig.类型参数.iter().cloned().collect(),
            );
            for (i, at) in 实参型.iter().enumerate().take(sig.参数.len()) {
                // 只对字面量来源的实参做家族比对（句柄惯用法沉默）
                if !实参节点
                    .get(i)
                    .map(|n| self.是字面量来源(n))
                    .unwrap_or(false)
                {
                    continue;
                }
                let 形参型 = sig.参数[i]
                    .type_annotation
                    .as_ref()
                    .and_then(|t| self.解析类型(t));
                if let (Some(pt), Some(a)) = (形参型, at) {
                    if !家族相容(&pt, a) {
                        self.报(TypeError::TypeMismatch {
                            expected: format!(
                                "函数 '{}' 第 {} 个参数期望 {}",
                                名,
                                i + 1,
                                类型显示(&pt)
                            ),
                            actual: format!("实参却是 {}", 类型显示(a)),
                            span,
                        });
                    }
                }
            }
            self.当前类型参数 = 旧类型参数;
        } else {
            // 多候选：仅当**全部**候选在某实参位都期望同一家族时才比对（谨慎开火）
            self.检查多候选实参(名, &匹配, 实参型, 实参节点, span);
        }
        // 返回类型：所有元数匹配的候选一致才采信
        let mut 返回: Option<推断> = None;
        for sig in &匹配 {
            let 旧类型参数 = std::mem::replace(
                &mut self.当前类型参数,
                sig.类型参数.iter().cloned().collect(),
            );
            let rt = sig.返回.as_ref().and_then(|t| self.解析类型(t));
            self.当前类型参数 = 旧类型参数;
            match &返回 {
                None => 返回 = Some(rt),
                Some(prev) if *prev == rt => {}
                Some(_) => return None, // 候选返回类型不一致 → 未知
            }
        }
        返回.flatten()
    }

    pub(super) fn 检查方法调用(&mut self, mc: &MethodCallExpression) -> 推断 {
        let method = mc.method_name.trim_start_matches(':');

        // 接收者是「裸标识符且不是变量」→ 模块/包/类型的限定调用
        if let AstNode::标识符表达式(id) = mc.object.as_ref() {
            let 是变量 = self.查局部(&id.name).is_some() || self.全局变量.contains_key(&id.name);
            if !是变量 {
                let 实参型: Vec<推断> = mc.arguments.iter().map(|a| self.推断表达式(a)).collect();
                return self.限定调用(&id.name, method, &mc.arguments, &实参型, mc.span);
            }
        }

        // 值接收者：obj.方法(...)
        let 对象型 = self.推断表达式(&mc.object);
        for a in &mc.arguments {
            self.推断表达式(a);
        }
        match 对象型 {
            Some(TypeNode::自定义类型(名)) => {
                match self.类型方法.get(&(名, method.to_string())) {
                    Some(重载集) => {
                        let 重载集 = 重载集.clone();
                        let n = mc.arguments.len();
                        // 本单元声明的方法：元数有把握可查（默认值/变参已计入）
                        if !重载集.iter().any(|s| s.接受个数(n)) {
                            let 元数集: Vec<String> =
                                重载集.iter().map(|s| s.参数.len().to_string()).collect();
                            self.报(TypeError::FunctionCallError {
                                message: format!(
                                    "方法 '{}' 参数数量不匹配: 期望 {}, 实际 {}",
                                    method,
                                    元数集.join("/"),
                                    n
                                ),
                                span: mc.span,
                            });
                            return None;
                        }
                        // 返回类型：元数匹配的候选一致才采信
                        let mut 返回: Option<推断> = None;
                        for sig in 重载集.iter().filter(|s| s.接受个数(n)) {
                            let rt = sig.返回.as_ref().and_then(|t| self.解析类型(t));
                            match &返回 {
                                None => 返回 = Some(rt),
                                Some(prev) if *prev == rt => {}
                                Some(_) => return None,
                            }
                        }
                        返回.flatten()
                    }
                    None => None, // 特性方法/嵌入方法/内建通用方法 → 沉默
                }
            }
            _ => None, // 字符串/数组/句柄上的内建方法（s.长度() 等）→ 沉默
        }
    }

    /// 「模块名/包别名/类型名 . 方法」限定调用的解析（实参已由调用方推断）。
    pub(super) fn 限定调用(
        &mut self,
        接收者: &str,
        method: &str,
        实参节点: &[AstNode],
        实参型: &[推断],
        span: crate::lexer::Span,
    ) -> 推断 {
        let n = 实参节点.len();
        let reg = 注册表();
        // ① 标准库：别名 → 模块名；或裸模块名直用（无需导入，镜像 codegen）
        let 模块 = self
            .模块别名
            .get(接收者)
            .cloned()
            .unwrap_or_else(|| 接收者.to_string());
        if let Some(f) = reg
            .get_function(&模块, method)
            .or_else(|| reg.get_function(&format!("标准库.{}", 模块), method))
        {
            return 注册表返回保守(&f.return_type);
        }
        // 模块在注册表里存在但函数没查到 → 注册表可能不全，沉默
        if reg.has_module(&模块) || reg.has_module(&format!("标准库.{}", 模块)) {
            return None;
        }
        // ② 用户包限定：`家庭.建库(...)` / `检索.构建(...)` —— 包函数已随
        //    collect_programs 进入编译单元 → 查合并函数表；miss 沉默（可能是再导出）
        if self.不透明别名.contains(接收者) || self.包名集.contains(接收者) {
            if let Some(重载集) = self.函数表.get(method) {
                // 只取返回类型（限定路径不做元数/类型硬校验：跨包重名合并有歧义）
                if 重载集.len() == 1 && 重载集[0].类型参数.is_empty() {
                    return 重载集[0].返回.clone();
                }
            }
            return None;
        }
        // ③ 枚举静态：`枚举名.变体(...)`（带载荷构造，载荷元数可查）
        if self.枚举表.contains_key(接收者) {
            let 是变体 = self
                .枚举表
                .get(接收者)
                .map(|变体集| 变体集.contains(method))
                .unwrap_or(false);
            if 是变体 {
                let 元数 = self
                    .变体元数
                    .get(&(接收者.to_string(), method.to_string()))
                    .copied();
                if let Some(元数) = 元数 {
                    if n != 元数 {
                        self.报(TypeError::FunctionCallError {
                            message: format!(
                                "枚举变体 '{}.{}' 载荷元数不匹配: 期望 {}, 实际 {}",
                                接收者, method, 元数, n
                            ),
                            span,
                        });
                    } else {
                        self.检查变体载荷类型(接收者, method, 实参型, 实参节点, span);
                    }
                }
            }
            return Some(TypeNode::自定义类型(接收者.to_string()));
        }
        // ④ 类型静态方法：`结构体::关联函数(...)` / `未来::就绪(...)` 等 → 沉默
        //   （含一切解析不到的裸接收者：可能是未登记包的别名——绝不报未定义变量）
        None
    }

    pub(super) fn 检查字段访问(&mut self, fa: &FieldAccessExpression) -> 推断 {
        // 接收者是裸标识符且非变量：枚举变体 / 模块成员 / 类型静态 → 特殊处理
        if let AstNode::标识符表达式(id) = fa.object.as_ref() {
            let 是变量 = self.查局部(&id.name).is_some() || self.全局变量.contains_key(&id.name);
            if !是变量 {
                // 枚举变体值：颜色.红
                if self.枚举表.contains_key(&id.name) {
                    return Some(TypeNode::自定义类型(id.name.clone()));
                }
                // 模块常量/包成员/类型静态/未知裸名 → 沉默未知
                return None;
            }
        }
        let 对象型 = self.推断表达式(&fa.object);
        match 对象型 {
            Some(TypeNode::自定义类型(名)) => {
                if let Some(info) = self.结构体表.get(&名) {
                    // 字段集封闭（无嵌入、非泛型）时才做字段名检查
                    if let Some(f) = info.字段.iter().find(|f| f.name == fa.field) {
                        let t = f.type_annotation.clone();
                        // 泛型结构体的字段类型可能含类型参数 → 借类型参数环境判定
                        let 旧 = std::mem::replace(
                            &mut self.当前类型参数,
                            info.类型参数.iter().cloned().collect(),
                        );
                        let r = self.解析类型(&t);
                        self.当前类型参数 = 旧;
                        r
                    } else if !info.有嵌入 && info.类型参数.is_empty() {
                        self.报(TypeError::General {
                            message: format!("结构体 '{}' 没有字段 '{}'", 名, fa.field),
                            span: fa.span,
                        });
                        None
                    } else {
                        None // 嵌入字段/泛型 → 字段集不封闭，沉默
                    }
                } else {
                    None // 枚举值的 .字段？（如变体载荷访问）→ 沉默
                }
            }
            // 数组/列表/字符串的 .长度 属性
            Some(
                TypeNode::数组类型(_)
                | TypeNode::列表类型(_)
                | TypeNode::基础类型(BasicType::字符串),
            ) if fa.field == "长度" => Some(TypeNode::基础类型(BasicType::整数)),
            _ => None,
        }
    }

    pub(super) fn 检查结构体字面量(&mut self, sl: &StructLiteralExpression) -> 推断 {
        // spread 基值先推断一遍（捕获未定义变量等；类型是否一致交由 codegen 报错）。
        if let Some(base) = &sl.spread_base {
            let _ = self.推断表达式(base);
        }
        // 字段值先各自推断
        let 值型: Vec<推断> = sl
            .fields
            .iter()
            .map(|f| self.推断表达式(&f.value))
            .collect();

        let info = match self.结构体表.get(&sl.struct_name) {
            Some(i) => i,
            None => {
                // 导入符号/联合体/类型参数/内建泛型包装 → 沉默；其余报未定义
                if self.导入符号.contains(&sl.struct_name)
                    || self.联合体集.contains(&sl.struct_name)
                    || self.当前类型参数.contains(&sl.struct_name)
                    || matches!(sl.struct_name.as_str(), "选项" | "结果" | "未来" | "通道")
                {
                    return None;
                }
                self.报(TypeError::General {
                    message: format!("未定义的结构体类型 '{}'", sl.struct_name),
                    span: sl.span,
                });
                return None;
            }
        };

        // 字段集封闭（无嵌入）时检查字段名；类型比对只对具体基础类型字段做
        let 封闭 = !info.有嵌入;
        let 字段表: Vec<(String, TypeNode)> = info
            .字段
            .iter()
            .map(|f| (f.name.clone(), f.type_annotation.clone()))
            .collect();
        let 结构类型参数: HashSet<String> = info.类型参数.iter().cloned().collect();

        for (fv, vt) in sl.fields.iter().zip(值型.iter()) {
            match 字段表.iter().find(|(n, _)| *n == fv.name) {
                Some((_, ft)) => {
                    // 字段类型含该结构体的类型参数 → 沉默
                    let 旧 = std::mem::replace(&mut self.当前类型参数, 结构类型参数.clone());
                    let 已知字段型 = self.解析类型(ft);
                    self.当前类型参数 = 旧;
                    // 只对字面量来源的字段值做家族比对（句柄惯用法沉默）
                    if !self.是字面量来源(&fv.value) {
                        continue;
                    }
                    if let (Some(ft), Some(vt)) = (已知字段型, vt) {
                        if !家族相容(&ft, vt) {
                            self.报(TypeError::TypeMismatch {
                                expected: format!(
                                    "结构体 '{}' 字段 '{}' 声明为 {}",
                                    sl.struct_name,
                                    fv.name,
                                    类型显示(&ft)
                                ),
                                actual: format!("字面量值却是 {}", 类型显示(vt)),
                                span: fv.span,
                            });
                        }
                    }
                }
                None if 封闭 => {
                    self.报(TypeError::General {
                        message: format!("结构体 '{}' 没有字段 '{}'", sl.struct_name, fv.name),
                        span: fv.span,
                    });
                }
                None => {}
            }
        }

        Some(TypeNode::自定义类型(sl.struct_name.clone()))
    }
}
