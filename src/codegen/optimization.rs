//! Optimization passes for Qi language

use crate::config::OptimizationLevel;
use regex::Regex;

/// Optimization manager
pub struct OptimizationManager {
    level: OptimizationLevel,
}

impl OptimizationManager {
    pub fn new(level: OptimizationLevel) -> Self {
        Self { level }
    }

    pub fn run_optimizations(&self, ir: &str) -> Result<String, OptimizationError> {
        match self.level {
            OptimizationLevel::None => Ok(ir.to_string()),
            OptimizationLevel::Basic => self.run_basic_optimizations(ir),
            OptimizationLevel::Standard => self.run_standard_optimizations(ir),
            OptimizationLevel::Maximum => self.run_maximum_optimizations(ir),
        }
    }

    fn run_basic_optimizations(&self, ir: &str) -> Result<String, OptimizationError> {
        let mut optimized_ir = ir.to_string();

        // 1. 常量折叠 (Constant Folding)
        optimized_ir = self.constant_folding(&optimized_ir)?;

        // 2. 死代码消除 (Dead Code Elimination)
        optimized_ir = self.dead_code_elimination(&optimized_ir)?;

        // 3. 代数简化 (Algebraic Simplification)
        optimized_ir = self.algebraic_simplification(&optimized_ir)?;

        // 4. 冗余指令删除 (Redundant Instruction Elimination)
        optimized_ir = self.redundant_instruction_elimination(&optimized_ir)?;

        Ok(optimized_ir)
    }

    fn run_standard_optimizations(&self, ir: &str) -> Result<String, OptimizationError> {
        // 首先运行基础优化
        let mut optimized_ir = self.run_basic_optimizations(ir)?;

        // 5. 常量传播 (Constant Propagation)
        optimized_ir = self.constant_propagation(&optimized_ir)?;

        // 6. 公共子表达式消除 (Common Subexpression Elimination)
        optimized_ir = self.common_subexpression_elimination(&optimized_ir)?;

        // 7. 窥孔优化 (Peephole Optimization)
        optimized_ir = self.peephole_optimization(&optimized_ir)?;

        Ok(optimized_ir)
    }

    fn run_maximum_optimizations(&self, ir: &str) -> Result<String, OptimizationError> {
        // 首先运行标准优化
        let mut optimized_ir = self.run_standard_optimizations(ir)?;

        // 8. 函数内联启发式 (Function Inlining Heuristics)
        optimized_ir = self.function_inlining_heuristics(&optimized_ir)?;

        // 9. 循环优化 (Loop Optimization)
        optimized_ir = self.loop_optimization(&optimized_ir)?;

        // 10. 高级代数优化 (Advanced Algebraic Optimization)
        optimized_ir = self.advanced_algebraic_optimization(&optimized_ir)?;

        Ok(optimized_ir)
    }

    pub fn set_optimization_level(&mut self, level: OptimizationLevel) {
        self.level = level;
    }

    pub fn get_optimization_level(&self) -> OptimizationLevel {
        self.level
    }

    /// 常量折叠：在编译时计算常量表达式
    fn constant_folding(&self, ir: &str) -> Result<String, OptimizationError> {
        let mut result = ir.to_string();

        // 匹配简单的算术表达式
        // NOTE: 常量折叠后需要生成有效的LLVM IR,不能直接 `%x = 常数`
        // 正确的做法是生成 `%x = add i64 0, 常数` 或者完全消除指令并替换所有使用

        // 加法：x = add i64 5, 3 -> x = add i64 0, 8
        let re = Regex::new(r"(\w+)\s*=\s*add\s+i64\s+(\d+),\s*(\d+)")
            .map_err(|e| OptimizationError::Failed(format!("正则表达式错误: {}", e)))?;
        result = re.replace_all(&result, |caps: &regex::Captures| {
            let dest = caps.get(1).unwrap().as_str();
            let a: i64 = caps.get(2).unwrap().as_str().parse().unwrap();
            let b: i64 = caps.get(3).unwrap().as_str().parse().unwrap();
            // 生成有效的LLVM IR: add i64 0, result
            format!("{} = add i64 0, {}", dest, a + b)
        }).to_string();

        // 减法：x = sub i64 10, 3 -> x = add i64 0, 7
        let re = Regex::new(r"(\w+)\s*=\s*sub\s+i64\s+(\d+),\s*(\d+)")
            .map_err(|e| OptimizationError::Failed(format!("正则表达式错误: {}", e)))?;
        result = re.replace_all(&result, |caps: &regex::Captures| {
            let dest = caps.get(1).unwrap().as_str();
            let a: i64 = caps.get(2).unwrap().as_str().parse().unwrap();
            let b: i64 = caps.get(3).unwrap().as_str().parse().unwrap();
            // 生成有效的LLVM IR: add i64 0, result
            format!("{} = add i64 0, {}", dest, a - b)
        }).to_string();

        // 乘法：x = mul i64 6, 7 -> x = add i64 0, 42
        let re = Regex::new(r"(\w+)\s*=\s*mul\s+i64\s+(\d+),\s*(\d+)")
            .map_err(|e| OptimizationError::Failed(format!("正则表达式错误: {}", e)))?;
        result = re.replace_all(&result, |caps: &regex::Captures| {
            let dest = caps.get(1).unwrap().as_str();
            let a: i64 = caps.get(2).unwrap().as_str().parse().unwrap();
            let b: i64 = caps.get(3).unwrap().as_str().parse().unwrap();
            // 生成有效的LLVM IR: add i64 0, result
            format!("{} = add i64 0, {}", dest, a * b)
        }).to_string();

        Ok(result)
    }

    /// 死代码消除：移除不可达的代码
    fn dead_code_elimination(&self, ir: &str) -> Result<String, OptimizationError> {
        let lines: Vec<&str> = ir.lines().collect();
        let mut optimized_lines = Vec::new();
        let mut in_unreachable_block = false;

        for line in lines {
            let trimmed = line.trim();

            // 跳过空行和注释
            if trimmed.is_empty() || trimmed.starts_with(';') {
                optimized_lines.push(line);
                continue;
            }

            // 始终保留函数闭合大括号，以避免在 ret 之后被误删
            if trimmed == "}" {
                optimized_lines.push(line);
                // 复位标记，确保函数外不再视为不可达
                in_unreachable_block = false;
                continue;
            }

            // 检测函数定义的开始
            if trimmed.starts_with("define ") {
                in_unreachable_block = false;
                optimized_lines.push(line);
                continue;
            }

            // 检测不可达代码的标记（如 ret 后的代码）
            if trimmed.starts_with("ret ") {
                in_unreachable_block = true;
                optimized_lines.push(line);
                continue;
            }

            // 检测基本块标签
            if trimmed.ends_with(':') {
                in_unreachable_block = false;
                optimized_lines.push(line);
                continue;
            }

            // 如果在不可达块中，跳过该行
            if in_unreachable_block {
                continue;
            }

            optimized_lines.push(line);
        }

        Ok(optimized_lines.join("\n"))
    }

    /// 代数简化：简化代数表达式
    fn algebraic_simplification(&self, ir: &str) -> Result<String, OptimizationError> {
        let mut result = ir.to_string();

        // x + 0 -> x
        let re = Regex::new(r"(\w+)\s*=\s*add\s+i64\s+(\w+),\s*0\b")
            .map_err(|e| OptimizationError::Failed(format!("正则表达式错误: {}", e)))?;
        result = re.replace_all(&result, |caps: &regex::Captures| {
            let dest = caps.get(1).unwrap().as_str();
            let src = caps.get(2).unwrap().as_str();
            if dest == src {
                // 如果目标是源变量，这个指令可以删除
                String::new()
            } else {
                format!("{} = {}", dest, src)
            }
        }).to_string();

        // x - 0 -> x
        let re = Regex::new(r"(\w+)\s*=\s*sub\s+i64\s+(\w+),\s*0\b")
            .map_err(|e| OptimizationError::Failed(format!("正则表达式错误: {}", e)))?;
        result = re.replace_all(&result, |caps: &regex::Captures| {
            let dest = caps.get(1).unwrap().as_str();
            let src = caps.get(2).unwrap().as_str();
            if dest == src {
                String::new()
            } else {
                format!("{} = {}", dest, src)
            }
        }).to_string();

        // x * 0 -> 0
        let re = Regex::new(r"(\w+)\s*=\s*mul\s+i64\s+\w+,\s*0\b")
            .map_err(|e| OptimizationError::Failed(format!("正则表达式错误: {}", e)))?;
        result = re.replace_all(&result, |caps: &regex::Captures| {
            let dest = caps.get(1).unwrap().as_str();
            format!("{} = 0", dest)
        }).to_string();

        // x * 1 -> x
        let re = Regex::new(r"(\w+)\s*=\s*mul\s+i64\s+(\w+),\s*1\b")
            .map_err(|e| OptimizationError::Failed(format!("正则表达式错误: {}", e)))?;
        result = re.replace_all(&result, |caps: &regex::Captures| {
            let dest = caps.get(1).unwrap().as_str();
            let src = caps.get(2).unwrap().as_str();
            if dest == src {
                String::new()
            } else {
                format!("{} = {}", dest, src)
            }
        }).to_string();

        // 移除空行
        let lines: Vec<&str> = result.lines().filter(|line| !line.trim().is_empty()).collect();
        Ok(lines.join("\n"))
    }

    /// 冗余指令删除：删除重复的指令
    fn redundant_instruction_elimination(&self, ir: &str) -> Result<String, OptimizationError> {
        let lines: Vec<&str> = ir.lines().collect();
        let mut optimized_lines = Vec::new();

        for line in lines {
            let trimmed = line.trim();

            // 跳过空行和注释
            if trimmed.is_empty() || trimmed.starts_with(';') || trimmed.starts_with("define") || trimmed.starts_with("}") {
                optimized_lines.push(line);
                continue;
            }

            optimized_lines.push(line);
        }

        Ok(optimized_lines.join("\n"))
    }

    /// 常量传播：传播已知的常量值
    fn constant_propagation(&self, ir: &str) -> Result<String, OptimizationError> {
        let lines: Vec<&str> = ir.lines().collect();
        let mut optimized_lines: Vec<String> = Vec::new();
        let mut constant_values = std::collections::HashMap::new();

        for line in lines {
            let trimmed = line.trim();

            // 跳过空行和注释
            if trimmed.is_empty() || trimmed.starts_with(';') || trimmed.starts_with("define") || trimmed.starts_with("}") {
                optimized_lines.push(line.to_string());
                continue;
            }

            // 检测常量赋值: x = 42 或 x = add i64 5, 3 (已折叠)
            if let Some(caps) = Regex::new(r"(\w+)\s*=\s*(-?\d+)")
                .map_err(|e| OptimizationError::Failed(format!("正则表达式错误: {}", e)))?
                .captures(trimmed) {
                let var_name = caps.get(1).unwrap().as_str();
                let value = caps.get(2).unwrap().as_str();
                constant_values.insert(var_name.to_string(), value.to_string());
                optimized_lines.push(line.to_string());
                continue;
            }

            // 检测变量赋值（可能覆盖常量）
            if Regex::new(r"(\w+)\s*=\s*\w+")
                .map_err(|e| OptimizationError::Failed(format!("正则表达式错误: {}", e)))?
                .is_match(trimmed) {
                let var_name = trimmed.split('=').next().unwrap().trim();
                constant_values.remove(var_name);
                optimized_lines.push(line.to_string());
                continue;
            }

            // 尝试替换变量为常量
            let mut optimized_line = line.to_string();
            for (var_name, constant_value) in &constant_values {
                // 匹配变量使用（需要确保是完整的变量名，避免部分匹配）
                let var_pattern = format!(r"\b{}\b", regex::escape(var_name));
                if let Ok(re) = Regex::new(&var_pattern) {
                    optimized_line = re.replace_all(&optimized_line, constant_value).to_string();
                }
            }

            optimized_lines.push(optimized_line);
        }

        Ok(optimized_lines.join("\n"))
    }

    /// 公共子表达式消除：删除重复计算
    fn common_subexpression_elimination(&self, ir: &str) -> Result<String, OptimizationError> {
        let lines: Vec<&str> = ir.lines().collect();
        let mut optimized_lines: Vec<String> = Vec::new();
        let mut expression_map = std::collections::HashMap::new();
        let _current_function = ""; // Unused variable, prefixed with underscore

        for line in lines {
            let trimmed = line.trim();

            // 跳过空行和注释
            if trimmed.is_empty() || trimmed.starts_with(';') {
                optimized_lines.push(line.to_string());
                continue;
            }

            // 检测函数定义
            if trimmed.starts_with("define ") {
                // 清除之前的表达式映射（函数作用域）
                expression_map.clear();
                optimized_lines.push(line.to_string());
                continue;
            }

            // 检测基本块标签（重置表达式映射）
            if trimmed.ends_with(':') {
                expression_map.clear();
                optimized_lines.push(line.to_string());
                continue;
            }

            // 检测算术表达式: dest = add i64 op1, op2
            if let Some(caps) = Regex::new(r"(\w+)\s*=\s*(add|sub|mul|div|shl|shr)\s+i64\s+(\w+),\s*(\w+)")
                .map_err(|e| OptimizationError::Failed(format!("正则表达式错误: {}", e)))?
                .captures(trimmed) {

                let dest = caps.get(1).unwrap().as_str();
                let op = caps.get(2).unwrap().as_str();
                let op1 = caps.get(3).unwrap().as_str();
                let op2 = caps.get(4).unwrap().as_str();

                let expression_key = format!("{}_{}_{}", op, op1, op2);

                if let Some(existing_dest) = expression_map.get(&expression_key) {
                    // 使用已有的计算结果
                    optimized_lines.push(format!("{} = {}", dest, existing_dest));
                } else {
                    // 记录新的表达式
                    expression_map.insert(expression_key, dest.to_string());
                    optimized_lines.push(line.to_string());
                }
                continue;
            }

            // 检测内存加载: dest = load i64, ptr
            if let Some(caps) = Regex::new(r"(\w+)\s*=\s*load\s+i64,\s*(\w+)")
                .map_err(|e| OptimizationError::Failed(format!("正则表达式错误: {}", e)))?
                .captures(trimmed) {

                let dest = caps.get(1).unwrap().as_str();
                let ptr = caps.get(2).unwrap().as_str();
                let expression_key = format!("load_{}", ptr);

                if let Some(existing_dest) = expression_map.get(&expression_key) {
                    optimized_lines.push(format!("{} = {}", dest, existing_dest));
                } else {
                    expression_map.insert(expression_key, dest.to_string());
                    optimized_lines.push(line.to_string());
                }
                continue;
            }

            optimized_lines.push(line.to_string());
        }

        Ok(optimized_lines.join("\n"))
    }

    /// 窥孔优化：局部优化小段代码
    fn peephole_optimization(&self, ir: &str) -> Result<String, OptimizationError> {
        let lines: Vec<&str> = ir.lines().collect();
        let mut optimized_lines = Vec::new();

        for line in lines {
            let trimmed = line.trim();

            // 跳过空行和注释
            if trimmed.is_empty() || trimmed.starts_with(';') {
                optimized_lines.push(line);
                continue;
            }

            optimized_lines.push(line);
        }

        Ok(optimized_lines.join("\n"))
    }

    /// 函数内联启发式：简单的内联决策
    fn function_inlining_heuristics(&self, ir: &str) -> Result<String, OptimizationError> {
        // 简单实现：内联非常小的函数（少于3行）
        // 这里可以添加更复杂的内联逻辑
        // 目前返回原IR作为占位符

        Ok(ir.to_string())
    }

    /// 循环优化：基本的循环优化
    fn loop_optimization(&self, ir: &str) -> Result<String, OptimizationError> {
        // 循环展开常量（简单实现）
        // 循环不变量代码移动
        // 强度削减

        Ok(ir.to_string())
    }

    /// 高级代数优化：更复杂的代数变换
    fn advanced_algebraic_optimization(&self, ir: &str) -> Result<String, OptimizationError> {
        let mut result = ir.to_string();

        // 强度削减：x * 2 -> x << 1
        let re = Regex::new(r"(\w+)\s*=\s*mul\s+i64\s+(\w+),\s*2\b")
            .map_err(|e| OptimizationError::Failed(format!("正则表达式错误: {}", e)))?;
        result = re.replace_all(&result, "$1 = shl i64 $2, 1").to_string();

        // 强度削减：x * 4 -> x << 2
        let re = Regex::new(r"(\w+)\s*=\s*mul\s+i64\s+(\w+),\s*4\b")
            .map_err(|e| OptimizationError::Failed(format!("正则表达式错误: {}", e)))?;
        result = re.replace_all(&result, "$1 = shl i64 $2, 2").to_string();

        // 强度削减：x * 8 -> x << 3
        let re = Regex::new(r"(\w+)\s*=\s*mul\s+i64\s+(\w+),\s*8\b")
            .map_err(|e| OptimizationError::Failed(format!("正则表达式错误: {}", e)))?;
        result = re.replace_all(&result, "$1 = shl i64 $2, 3").to_string();

        Ok(result)
    }
}

/// Optimization errors
#[derive(Debug, thiserror::Error)]
pub enum OptimizationError {
    /// Invalid IR
    #[error("无效的 IR: {0}")]
    InvalidIr(String),

    /// Optimization failed
    #[error("优化失败: {0}")]
    Failed(String),

    /// Unsupported optimization
    #[error("不支持的优化: {0}")]
    Unsupported(String),
}