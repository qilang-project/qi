//! 运行时反射元数据 emit —— 编译期把用户函数 / 结构体 / 枚举 的定义登记进
//! 运行时反射注册表，让运行中的 Qi 程序（尤其内置 AI Agent）能自省「有哪些工具」。
//!
//! ## 怎么 emit
//! 在 `main` 序言里（用户 `入口()` 体运行之前）为每个用户函数生成一条
//! `qi_reflect_register_function(名, 签名文本)` 调用；结构体 / 枚举同理。名字与描述
//! 文本用 `emit_immortal_string`（immortal 全局常量、按内容去重），运行时侧
//! （reflect_ffi.rs）`to_string` 各存一份自有 String，不持有生成代码的指针。
//!
//! ## 为什么放 main 序言而非 global_ctors
//! 可执行程序当前不生成 `@llvm.global_ctors`（那是反向 FFI 库模式的自初始化用），
//! 单 module 里 `global_ctors` 只能有一条，与库模式冲突。放 main 序言零冲突、
//! 时序明确（早于任何用户代码），且库模式（插件）自身不需要自省，天然不受影响。
//!
//! ## 登记范围与确定性
//! 登记 `符号.函数` 里的用户函数（排除 `外部` C 函数与 `入口` 自身），按名字排序
//! 保证跨运行输出稳定。结构体 / 枚举按注册表索引序（即声明序）。

use super::后端;
use super::类型::{元素类型, Qi类型};

impl<'ctx> 后端<'ctx> {
    /// 声明反射注册运行时原型（3 个 `(ptr, ptr) -> void`）。幂等，随 声明运行时 调用。
    pub(super) fn 声明反射运行时(&self) {
        let ptrt = self.ctx.ptr_type(inkwell::AddressSpace::default());
        let 收两指针 = self
            .ctx
            .void_type()
            .fn_type(&[ptrt.into(), ptrt.into()], false);
        for 名 in [
            "qi_reflect_register_function",
            "qi_reflect_register_struct",
            "qi_reflect_register_enum",
        ] {
            if self.module.get_function(名).is_none() {
                self.module.add_function(名, 收两指针, None);
            }
        }
    }

    /// 在 main 入口序言里 emit 全部反射登记调用（须已 position 到 main 的 entry 块）。
    pub(super) fn 生成反射注册(&mut self) -> Result<(), String> {
        // ── 函数：名 + "参数类型, ... -> 返回类型" ──
        // HashMap 迭代无序 → 收集后按名字排序，输出跨运行确定。
        let mut 函数项: Vec<(String, String)> = Vec::new();
        {
            let mut 名单: Vec<String> = self
                .符号
                .函数
                .keys()
                .filter(|n| n.as_str() != "入口" && !self.符号.外部函数.contains(*n))
                .cloned()
                .collect();
            名单.sort();
            for 名 in 名单 {
                if let Some(sig) = self.符号.函数.get(&名) {
                    let 参数: Vec<String> = sig.参数.iter().map(|t| self.类型显示名(*t)).collect();
                    let 返回 = self.类型显示名(sig.返回);
                    let 文本 = if 参数.is_empty() {
                        format!("() -> {}", 返回)
                    } else {
                        format!("{} -> {}", 参数.join(", "), 返回)
                    };
                    函数项.push((名, 文本));
                }
            }
        }

        // ── 结构体：名 + "字段:类型, ..." ──（按注册表索引序 = 声明序）
        let mut 结构体项: Vec<(String, String)> = Vec::new();
        for s in &self.符号.结构体 {
            let 字段: Vec<String> = s
                .字段名
                .iter()
                .zip(s.字段类型.iter())
                .map(|(名, t)| format!("{}:{}", 名, self.类型显示名(*t)))
                .collect();
            结构体项.push((s.名字.clone(), 字段.join(", ")));
        }

        // ── 枚举：名 + "变体, ..." ──
        let mut 枚举项: Vec<(String, String)> = Vec::new();
        for e in &self.符号.枚举 {
            let 变体: Vec<String> = e.变体.iter().map(|v| v.名字.clone()).collect();
            枚举项.push((e.名字.clone(), 变体.join(", ")));
        }

        if 函数项.is_empty() && 结构体项.is_empty() && 枚举项.is_empty() {
            return Ok(());
        }

        self.发射登记调用("qi_reflect_register_function", &函数项)?;
        self.发射登记调用("qi_reflect_register_struct", &结构体项)?;
        self.发射登记调用("qi_reflect_register_enum", &枚举项)?;
        Ok(())
    }

    fn 发射登记调用(
        &mut self,
        运行时名: &str,
        项: &[(String, String)],
    ) -> Result<(), String> {
        if 项.is_empty() {
            return Ok(());
        }
        let f = self
            .module
            .get_function(运行时名)
            .ok_or_else(|| format!("反射注册原型缺失: {}", 运行时名))?;
        for (名, 描述) in 项 {
            let np = self.emit_immortal_string(名, "qi.reflect.name")?;
            let dp = self.emit_immortal_string(描述, "qi.reflect.desc")?;
            self.builder
                .build_call(f, &[np.into(), dp.into()], "")
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Qi 类型 → 可读中文名（反射描述文本用）。复合类型给近似可读名。
    pub(super) fn 类型显示名(&self, t: Qi类型) -> String {
        match t {
            Qi类型::整数 => "整数".to_string(),
            Qi类型::浮点数 => "浮点数".to_string(),
            Qi类型::布尔 => "布尔".to_string(),
            Qi类型::字符串 => "字符串".to_string(),
            Qi类型::空 => "空".to_string(),
            Qi类型::指针 => "指针".to_string(),
            Qi类型::结构体(i) => self
                .符号
                .结构体
                .get(i as usize)
                .map(|s| s.名字.clone())
                .unwrap_or_else(|| "结构体".to_string()),
            Qi类型::枚举(i) | Qi类型::装箱枚举(i) => self
                .符号
                .枚举
                .get(i as usize)
                .map(|e| e.名字.clone())
                .unwrap_or_else(|| "枚举".to_string()),
            Qi类型::函数值(_) => "函数".to_string(),
            Qi类型::数组(e) => format!("数组<{}>", self.元素显示名(e)),
            Qi类型::通道(e) => format!("通道<{}>", self.元素显示名(e)),
            Qi类型::未来(e) => format!("未来<{}>", self.元素显示名(e)),
            Qi类型::未知 => "未知".to_string(),
        }
    }

    fn 元素显示名(&self, e: 元素类型) -> String {
        self.类型显示名(e.标量())
    }
}
