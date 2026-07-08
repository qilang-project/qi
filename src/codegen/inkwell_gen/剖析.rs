//! 插桩式性能剖析器 codegen —— QI_PROF=1 时在函数入口/出口注入计时调用。
//!
//! 语义与运行时（qi-runtime/src/stdlib/profiler.rs）配套：
//!   - 序言：`t = qi_prof_enter("显示名")`，把 t 存进 entry 块的 alloca 槽（跨块安全）；
//!   - 每个出口（return / 落地隐式返回）前：`qi_prof_exit("显示名", load(槽))`。
//!
//! 显示名用**中文原名**（非 mangle 名），emit 成 immortal 全局常量（复用
//! emit_immortal_string，按内容去重 ⇒ 同名函数同一指针，运行时据指针聚合）。
//!
//! 门控：`self.剖析`（QI_PROF 环境变量，new() 里读，默认关）。关时两个方法都是
//! 空操作，且 声明运行时 也不声明 qi_prof_ 原型 ⇒ 未开启时 IR 逐字节不变。
//!
//! 异常路径：setjmp/longjmp 跳过正常出口时 exit 不执行，被提前退出的函数计时不准
//! （已在运行时文档注明），此处不为它加清理。

use super::后端;

impl<'ctx> 后端<'ctx> {
    /// 函数序言注入：`qi_prof_enter(显示名) -> i64` 存入 entry 块 alloca 槽。
    ///
    /// 调用点：各函数/方法/闭包/入口 body 生成的最开头（已 position_at_end(entry)）。
    /// 每次进函数体都先清空 剖析名/剖析槽，再按需重设，避免跨函数串用。
    pub(super) fn 剖析入口(&mut self, 显示名: &str) -> Result<(), String> {
        self.剖析名 = None;
        self.剖析槽 = None;
        if !self.剖析 {
            return Ok(());
        }
        let name_ptr = self.emit_immortal_string(显示名, "qi.prof.name")?;
        let enter = self
            .module
            .get_function("qi_prof_enter")
            .ok_or_else(|| "qi_prof_enter 原型缺失（剖析未声明？）".to_string())?;
        let t = self
            .builder
            .build_call(enter, &[name_ptr.into()], "prof_t")
            .map_err(|e| e.to_string())?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| "qi_prof_enter 无返回值".to_string())?;
        // 进入时刻存 entry 块 alloca（此刻正位于 entry，直接 build_alloca 即在 entry）
        let i64t = self.ctx.i64_type();
        let slot = self
            .builder
            .build_alloca(i64t, "prof_slot")
            .map_err(|e| e.to_string())?;
        self.builder
            .build_store(slot, t)
            .map_err(|e| e.to_string())?;
        self.剖析名 = Some(name_ptr);
        self.剖析槽 = Some(slot);
        Ok(())
    }

    /// 出口注入：`qi_prof_exit(显示名, load(槽))`。每个 return / 落地出口前调一次。
    ///
    /// 关或未设名/槽（如剖析未开启的函数）时空操作。
    pub(super) fn 剖析出口(&mut self) -> Result<(), String> {
        if !self.剖析 {
            return Ok(());
        }
        let (name_ptr, slot) = match (self.剖析名, self.剖析槽) {
            (Some(n), Some(s)) => (n, s),
            _ => return Ok(()),
        };
        let i64t = self.ctx.i64_type();
        let t = self
            .builder
            .build_load(i64t, slot, "prof_t")
            .map_err(|e| e.to_string())?;
        let exit = self
            .module
            .get_function("qi_prof_exit")
            .ok_or_else(|| "qi_prof_exit 原型缺失（剖析未声明？）".to_string())?;
        self.builder
            .build_call(exit, &[name_ptr.into(), t.into()], "")
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
