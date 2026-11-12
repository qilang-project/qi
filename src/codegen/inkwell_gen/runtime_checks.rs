//! 运行期安全检查：整数除零、数组下标越界。
//!
//! 两类错误以前都是「静默给垃圾」：
//!   - `sdiv/srem` 除数为 0 在 LLVM 里是 UB（poison），优化后打印出一个每次都
//!     不一样、像指针的数；`尝试/捕获` 也接不住。
//!   - 数组下标直接 GEP + load，`[3]` 读到下一块内存、`[-1]` 读到长度头、
//!     大下标段错误。
//!
//! 现在两者都抛普通的 qi 异常（与 `抛出` 同一套 qi_exc_* 机制）：在 `尝试` 里
//! 能被 `捕获`，没人接就是常规的「未捕获的异常」abort。
//!
//! ## 热路径代价
//!
//! 下标：一次长度头 load + 一次**无符号**比较（负下标按无符号看是巨大的数，
//! 一条比较同时挡住 <0 和 >=长度）+ 一条分支。除法：一次与 0 的比较 + 分支，
//! 再加两条 select 处理 -1。抛异常的代码放在独立的 noinline/cold/noreturn
//! 辅助函数里（每个模块一份），调用点只多一条 call + unreachable 的冷块；
//! LLVM 见到 noreturn + unreachable 会把冷块排到函数末尾、分支概率压到最低。
//!
//! ## INT64_MIN / -1
//!
//! 结果不可表示，LLVM 同样当 UB。取 Go 的语义：商**回绕**成 INT64_MIN，
//! 余数为 0，不抛异常（与 `+ - *` 的回绕一致）。实现上把除数 -1 换成 1 去做
//! sdiv/srem（永远合法），再用 select 挑回 `0 - 被除数`（回绕减法）/ 0 ——
//! 无分支。
//!
//! ## 抛出前释放本帧局部
//!
//! 与显式 `抛出` 相同（见 异常.rs）：try 深度 0 时先释放本帧 RC 局部再抛。
//! 释放序列在每个检查点的冷块里各展开一份 —— 只占代码体积，不进热路径。

use super::后端;
use inkwell::attributes::{Attribute, AttributeLoc};
use inkwell::module::Linkage;
use inkwell::values::{FunctionValue, IntValue, PointerValue};
use inkwell::IntPredicate;

/// 下标越界辅助函数：`void qi.rt.index_oob(i64 下标, i64 长度)`，不返回。
const INDEX_OOB_HELPER: &str = "qi.rt.index_oob";
/// 除零辅助函数：`void qi.rt.div_zero()`，不返回。
const DIV_ZERO_HELPER: &str = "qi.rt.div_zero";

impl<'ctx> 后端<'ctx> {
    /// 给辅助函数挂 noreturn / cold / noinline。
    /// 不挂 nounwind：goroutine 里未捕获时 qi_exc_throw 会 panic 展开穿过它。
    fn mark_cold_noreturn(&self, f: FunctionValue<'ctx>) {
        for name in ["noreturn", "cold", "noinline"] {
            let kind = Attribute::get_named_enum_kind_id(name);
            f.add_attribute(
                AttributeLoc::Function,
                self.ctx.create_enum_attribute(kind, 0),
            );
        }
    }

    /// 取（必要时现造）一个辅助函数。造的时候要离开当前插入点与调试位置，
    /// 造完原样放回 —— 调用方正在某个函数体中间。
    fn runtime_check_helper(&mut self, name: &str) -> Result<FunctionValue<'ctx>, String> {
        if let Some(f) = self.module.get_function(name) {
            return Ok(f);
        }
        let i64t = self.ctx.i64_type();
        let voidt = self.ctx.void_type();
        let is_oob = name == INDEX_OOB_HELPER;
        let signature = if is_oob {
            voidt.fn_type(&[i64t.into(), i64t.into()], false)
        } else {
            voidt.fn_type(&[], false)
        };
        let f = self
            .module
            .add_function(name, signature, Some(Linkage::Internal));
        self.mark_cold_noreturn(f);

        let saved_block = self.builder.get_insert_block();
        let saved_loc = self.调试_暂离();
        let entry = self.ctx.append_basic_block(f, "entry");
        self.builder.position_at_end(entry);

        let built = (|| -> Result<(), String> {
            if is_oob {
                let index = f.get_nth_param(0).unwrap().into_int_value();
                let len = f.get_nth_param(1).unwrap().into_int_value();
                let prefix = self.emit_immortal_string("下标越界：下标 ", "oob.msg0")?;
                let infix = self.emit_immortal_string("，长度 ", "oob.msg1")?;
                let index_str =
                    self.调用返回指针("qi_runtime_int_to_string", &[index.into()])?;
                let len_str = self.调用返回指针("qi_runtime_int_to_string", &[len.into()])?;
                let part1 = self.调用返回指针(
                    "qi_runtime_string_concat",
                    &[prefix.into(), index_str.into()],
                )?;
                let part2 = self.调用返回指针(
                    "qi_runtime_string_concat",
                    &[part1.into(), infix.into()],
                )?;
                let message = self.调用返回指针(
                    "qi_runtime_string_concat",
                    &[part2.into(), len_str.into()],
                )?;
                // stage 把消息拷进运行时，之后这几条临时串就可以放掉了
                let stage = self.取异常运行时("qi_exc_stage")?;
                self.builder
                    .build_call(stage, &[message.into()], "")
                    .map_err(|e| e.to_string())?;
                // wasm 运行时没有 qi_rc_release_any
                if !self.目标是Wasm() {
                    if let Some(rel) = self.module.get_function("qi_rc_release_any") {
                        for s in [index_str, len_str, part1, part2, message] {
                            self.builder
                                .build_call(rel, &[s.into()], "")
                                .map_err(|e| e.to_string())?;
                        }
                    }
                }
                let throw = self.取异常运行时("qi_exc_throw_staged")?;
                self.builder
                    .build_call(throw, &[], "")
                    .map_err(|e| e.to_string())?;
            } else {
                let message = self.emit_immortal_string("整数除以零", "divzero.msg")?;
                let throw = self.取异常运行时("qi_exc_throw")?;
                self.builder
                    .build_call(throw, &[message.into()], "")
                    .map_err(|e| e.to_string())?;
            }
            self.builder
                .build_unreachable()
                .map_err(|e| e.to_string())?;
            Ok(())
        })();

        if let Some(b) = saved_block {
            self.builder.position_at_end(b);
        }
        self.调试_归位(saved_loc);
        built?;
        Ok(f)
    }

    /// 在当前插入点分出一条冷路径：`cond` 为真继续往下，为假调 `helper(args)` 后
    /// unreachable。返回时插入点在 ok 块。
    fn emit_guard(
        &mut self,
        cond: IntValue<'ctx>,
        helper_name: &str,
        args: &[inkwell::values::BasicMetadataValueEnum<'ctx>],
        block_name: &str,
    ) -> Result<(), String> {
        let helper = self.runtime_check_helper(helper_name)?;
        let func = self
            .builder
            .get_insert_block()
            .and_then(|bb| bb.get_parent())
            .ok_or_else(|| format!("{}：不在函数体内", block_name))?;
        let ok_bb = self
            .ctx
            .append_basic_block(func, &format!("{}.ok", block_name));
        let fail_bb = self
            .ctx
            .append_basic_block(func, &format!("{}.fail", block_name));
        self.builder
            .build_conditional_branch(cond, ok_bb, fail_bb)
            .map_err(|e| e.to_string())?;
        self.builder.position_at_end(fail_bb);
        // 与显式 `抛出` 同一条规矩（见 异常.rs 生成抛出）：本函数里没有压着的
        // 异常帧时，这次抛出必然跳出本函数，先释放本帧 RC 局部，否则被外层
        // 接住后每次都漏（实测：循环里反复越界取参数数组，数组永远回收不掉）。
        // 消息只由整数拼成，不依赖任何局部，先放后抛安全。
        if self.弧开() && self.try深度 == 0 && self.协程当前.is_none() {
            self.弧释放局部()?;
        }
        self.builder
            .build_call(helper, args, "")
            .map_err(|e| e.to_string())?;
        self.builder
            .build_unreachable()
            .map_err(|e| e.to_string())?;
        self.builder.position_at_end(ok_bb);
        Ok(())
    }

    /// 数组下标检查：`0 <= 下标 < 长度`，否则抛「下标越界：下标 i，长度 n」。
    /// 返回加宽到 i64 的下标（调用方接着算槽偏移）。
    pub(super) fn emit_index_check(
        &mut self,
        base: PointerValue<'ctx>,
        index: IntValue<'ctx>,
    ) -> Result<IntValue<'ctx>, String> {
        let i64t = self.ctx.i64_type();
        let index = if index.get_type().get_bit_width() < 64 {
            self.整数加宽到i64(index)?
        } else {
            index
        };
        let head = self.槽指针(base, i64t.into(), 0)?;
        let len = self
            .builder
            .build_load(i64t, head, "arr.len")
            .map_err(|e| e.to_string())?
            .into_int_value();
        // 无符号比较：负下标看作巨大的无符号数，一条比较同时挡住两头
        let in_bounds = self
            .builder
            .build_int_compare(IntPredicate::ULT, index, len, "idx.inb")
            .map_err(|e| e.to_string())?;
        self.emit_guard(
            in_bounds,
            INDEX_OOB_HELPER,
            &[index.into(), len.into()],
            "idx",
        )?;
        Ok(index)
    }

    /// 整数 `/` 与 `%`：除数为 0 抛「整数除以零」；INT64_MIN / -1 回绕（见模块头）。
    pub(super) fn emit_int_division(
        &mut self,
        dividend: IntValue<'ctx>,
        divisor: IntValue<'ctx>,
        is_rem: bool,
    ) -> Result<IntValue<'ctx>, String> {
        let t = divisor.get_type();
        let plain = |s: &Self, rhs: IntValue<'ctx>| -> Result<IntValue<'ctx>, String> {
            if is_rem {
                s.builder
                    .build_int_signed_rem(dividend, rhs, "irem")
                    .map_err(|e| e.to_string())
            } else {
                s.builder
                    .build_int_signed_div(dividend, rhs, "idiv")
                    .map_err(|e| e.to_string())
            }
        };
        // 常量除数（最常见：`n / 2`、`i % 10`）：编译期就知道安全，零额外指令
        if let Some(c) = divisor.get_sign_extended_constant() {
            if c != 0 && c != -1 {
                return plain(self, divisor);
            }
            if c == -1 {
                return if is_rem {
                    Ok(t.const_zero())
                } else {
                    self.builder
                        .build_int_sub(t.const_zero(), dividend, "ineg")
                        .map_err(|e| e.to_string())
                };
            }
            // c == 0：落到下面的通用路径，条件恒假，运行到这里必抛
        }

        let nonzero = self
            .builder
            .build_int_compare(IntPredicate::NE, divisor, t.const_zero(), "div.nz")
            .map_err(|e| e.to_string())?;
        self.emit_guard(nonzero, DIV_ZERO_HELPER, &[], "div")?;

        let minus_one = t.const_all_ones();
        let is_minus_one = self
            .builder
            .build_int_compare(IntPredicate::EQ, divisor, minus_one, "div.m1")
            .map_err(|e| e.to_string())?;
        let safe_divisor = self
            .builder
            .build_select(is_minus_one, t.const_int(1, false), divisor, "div.safe")
            .map_err(|e| e.to_string())?
            .into_int_value();
        let quotient = plain(self, safe_divisor)?;
        let minus_one_result = if is_rem {
            t.const_zero()
        } else {
            self.builder
                .build_int_sub(t.const_zero(), dividend, "ineg")
                .map_err(|e| e.to_string())?
        };
        Ok(self
            .builder
            .build_select(is_minus_one, minus_one_result, quotient, "div.res")
            .map_err(|e| e.to_string())?
            .into_int_value())
    }
}
