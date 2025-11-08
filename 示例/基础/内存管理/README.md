# 内存管理示例

本目录包含Qi语言内存管理系统的测试示例。

## 测试示例

### 1. 基本类型测试.qi
- **目的**: 验证基本类型(整数、浮点数、布尔)使用栈分配
- **预期**: 所有基本类型变量在栈上分配,性能高效
- **验证方法**: 检查生成的LLVM IR,应该看到`alloca`指令

### 2. 小数组栈分配.qi
- **目的**: 验证小数组(≤64元素)使用栈分配
- **预期**: 小数组使用栈分配,避免堆分配开销
- **阈值**: SMALL_ARRAY_THRESHOLD = 64
- **验证方法**: 检查LLVM IR,应该看到`alloca [10 x i64]`

### 3. 大数组堆分配.qi
- **目的**: 验证大数组(>64元素)使用堆分配
- **预期**: 大数组通过`@qi_runtime_alloc`在堆上分配
- **GC触发**: 超大数组(>1MB)会触发GC检查
- **验证方法**: 检查LLVM IR,应该看到`call ptr @qi_runtime_alloc`

## 内存分配策略

### 栈分配 (Stack Allocation)
- 小型基础类型: i64, f64, i1
- 小数组: ≤64元素
- 优点: 快速分配/释放,自动管理
- 缺点: 大小受限,不能逃逸

### 堆分配 (Heap Allocation)
- 大数组: >64元素
- 字符串: 通过运行时函数
- 结构体: 复杂类型
- 优点: 灵活,可逃逸,大小不限
- 缺点: 需要GC,性能开销

## GC策略

### 触发条件
1. **内存压力**: 使用率超过阈值(默认gc_threshold)
2. **大分配**: 单次分配>1MB时检查GC

### GC流程
```llvm
%should_gc = call i64 @qi_runtime_gc_should_collect()
%need_gc = icmp ne i64 %should_gc, 0
br i1 %need_gc, label %do_gc, label %skip_gc

do_gc:
    call void @qi_runtime_gc_collect()
    br label %skip_gc

skip_gc:
    %ptr = call ptr @qi_runtime_alloc(i64 %size)
```

## 运行示例

```bash
# 编译并运行基本类型测试
cargo run -- run 示例/基础/内存管理/基本类型测试.qi

# 编译并查看LLVM IR
cargo run -- compile 示例/基础/内存管理/小数组栈分配.qi -o output.ll
cat output.ll | grep -A5 "alloca"
```

## 验证内存管理

### 1. 检查分配指令
```bash
# 栈分配 - 应该看到 alloca
grep "alloca" output.ll

# 堆分配 - 应该看到 qi_runtime_alloc
grep "qi_runtime_alloc" output.ll
```

### 2. 检查GC调用
```bash
# GC检查
grep "qi_runtime_gc_should_collect" output.ll

# GC执行
grep "qi_runtime_gc_collect" output.ll
```

## 注意事项

1. **向后兼容**: 现有代码继续正常工作
2. **性能优化**: 智能选择栈/堆,平衡性能和灵活性
3. **自动管理**: 作用域退出时自动清理堆分配
4. **GC透明**: 用户无需手动管理内存
