//! 并行机器码生成：优化后的模块按函数切成几份，各自在独立的 LLVMContext 里
//! 做指令选择 + 寄存器分配 + 发射，得到几个目标文件一起链接。
//!
//! ## 为什么
//!
//! 机器码生成（SelectionDAG + 贪心寄存器分配 + …）是整条编译里最大的一块，
//! 而且天然按函数独立 —— 串行跑白白闲着其它核。
//!
//! ## 怎么切（产物仍然确定）
//!
//! 1. 本地符号（internal / private）改成 external + hidden：分到不同分片的函数
//!    还要互相调用、共享全局。hidden 保证不出这个可执行文件 —— 最终链接时
//!    链接器把 hidden 符号转成本地符号，可执行文件的符号表跟串行时一样
//!    （实测 nm 各类符号计数完全相同）。只读、地址无关的局部常量（字符串字面量）
//!    不改，各分片各留一份用到的。
//!    无名全局（`@0`）起个固定的名字，否则各分片会各自编号撞名。
//! 2. 按指令数做贪心装箱（大的先放、放进最轻的分片，平局取编号小的）——
//!    分片数是**固定常数**、不随机器核数变，同一份输入切出来永远一样。
//! 3. 模块写成 bitcode，每个分片在自己的线程、自己的 Context 里读回来，
//!    不归自己的函数只留声明（删函数体 + 清元数据），全局变量只有 0 号分片保留定义，
//!    `llvm.global_ctors` 这类 appending 全局只留在 0 号分片。
//! 4. 0 号分片写到原来那个 `.o`，其余写到源码树**之外**的
//!    `<缓存根>/parts/<.o 绝对路径的摘要>/<名>.qi-part<k>.o`（见 `parts_dir`），一起交给链接器。
//!
//! 为什么不 `ld -r` 合成一个：macOS 的 `ld -r` 不合并 DWARF，只在产物里留一张
//! 指回输入 .o 的 debug map —— 分片文件一删，调试信息就没了（实测）。所以分片
//! 目标文件跟原来那个 .o 一样留在源文件旁边，可执行文件的 debug map 指向它们，
//! lldb 照常按源码断点。分片放在稳定位置（不是临时目录），跨次编译一直在；
//! 往用户项目目录里放是不行的，那是在别人的源码树里乱丢文件。
//!
//! 要求切了也不切（直接走原来的串行路径）的情形：不是宿主原生目标、宿主不是 macOS/Linux、模块里有别名/ifunc/comdat 或
//! 我们不认识的链接类型。
//!
//! **默认不切**，只有 `QI_CODEGEN_PARTITIONS=N` 才切成 N 份。
//!
//! 原来优化后指令数过 6000 就自动切 8 份，2026.10.08-1 发版时 macOS arm64
//! 的 Release 编 qipkg 段错误，本机冷编也约 15% 复现：崩在分片线程的
//! `DwarfDebug::endModule → DwarfCompileUnit::finishEntityDefinition`（实体没有
//! DIE）。各分片 bitcode 过得了 verifier，单独 `llc` 48 次不崩，同进程单线程跑
//! 8 片也不崩 —— 只在多线程同时发射时出现，根因还在 LLVM 里没找到。
//! 并行只省 0.3–0.5 秒（约 20%），不值得拿编译器偶发段错误换。

use inkwell::context::Context;
use inkwell::memory_buffer::MemoryBuffer;
use inkwell::module::{Linkage, Module};
use inkwell::targets::{FileType, Target, TargetMachine, TargetTriple};
use inkwell::values::{AsValueRef, FunctionValue, GlobalValue};
use inkwell::GlobalVisibility;
use std::ffi::c_void;
use std::path::{Path, PathBuf};

/// 强制分片数的上限（也是清理旧分片文件时扫的范围）。
const MAX_PARTITIONS: usize = 64;

// inkwell 没包的几个 C API。它们和 inkwell 用到的函数在同一个目标文件
// （LLVM Core.cpp）里，静态链接时一并进来。
extern "C" {
    fn LLVMSetInitializer(global: *mut c_void, value: *mut c_void);
    fn LLVMGlobalClearMetadata(global: *mut c_void);
    fn LLVMDeleteGlobal(global: *mut c_void);
    fn LLVMGetFirstGlobalAlias(module: *mut c_void) -> *mut c_void;
    fn LLVMGetFirstGlobalIFunc(module: *mut c_void) -> *mut c_void;
    fn LLVMHasPersonalityFn(func: *mut c_void) -> i32;
    fn LLVMSetPersonalityFn(func: *mut c_void, personality: *mut c_void);
    fn LLVMGetFirstBasicBlock(func: *mut c_void) -> *mut c_void;
    fn LLVMGetNextBasicBlock(block: *mut c_void) -> *mut c_void;
    fn LLVMGetFirstInstruction(block: *mut c_void) -> *mut c_void;
    fn LLVMGetNextInstruction(inst: *mut c_void) -> *mut c_void;
    fn LLVMGetFirstUse(value: *mut c_void) -> *mut c_void;
    fn LLVMTypeOf(value: *mut c_void) -> *mut c_void;
    fn LLVMGetTypeKind(ty: *mut c_void) -> u32;
    fn LLVMGetPoison(ty: *mut c_void) -> *mut c_void;
    fn LLVMReplaceAllUsesWith(old: *mut c_void, new: *mut c_void);
    fn LLVMInstructionEraseFromParent(inst: *mut c_void);
    fn LLVMDeleteBasicBlock(block: *mut c_void);
}

/// LLVMTypeKind::LLVMTokenTypeKind
const TOKEN_TYPE_KIND: u32 = 16;

fn raw(v: impl AsValueRef) -> *mut c_void {
    v.as_value_ref() as *mut c_void
}

/// 这次要切几份；None = 走串行。只认显式的 `QI_CODEGEN_PARTITIONS`（见模块注释）。
fn partition_count() -> Option<usize> {
    std::env::var("QI_CODEGEN_PARTITIONS")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|&n| n > 1)
        .map(|n| n.min(MAX_PARTITIONS))
}

fn functions<'ctx>(module: &Module<'ctx>) -> Vec<FunctionValue<'ctx>> {
    let mut out = Vec::new();
    let mut f = module.get_first_function();
    while let Some(func) = f {
        f = func.get_next_function();
        if func.count_basic_blocks() > 0 {
            out.push(func);
        }
    }
    out
}

fn globals<'ctx>(module: &Module<'ctx>) -> Vec<GlobalValue<'ctx>> {
    let mut out = Vec::new();
    let mut g = module.get_first_global();
    while let Some(gv) = g {
        g = gv.get_next_global();
        out.push(gv);
    }
    out
}

fn instruction_count(func: FunctionValue<'_>) -> usize {
    func.get_basic_blocks()
        .iter()
        .map(|bb| {
            let mut n = 0;
            let mut i = bb.get_first_instruction();
            while let Some(inst) = i {
                n += 1;
                i = inst.get_next_instruction();
            }
            n
        })
        .sum()
}

fn is_local(l: Linkage) -> bool {
    matches!(l, Linkage::Internal | Linkage::Private)
}

/// 局部、只读、地址无关的常量（字符串字面量那一大类）：每个分片各留一份，
/// 不必跨分片共享，也就不必改成外部符号。
fn is_duplicable_constant(g: GlobalValue<'_>) -> bool {
    is_local(g.get_linkage())
        && g.is_constant()
        && g.get_unnamed_address() != inkwell::values::UnnamedAddress::None
        && !g.is_declaration()
}

/// 模块里有没有我们不会切的东西。有就整体走串行。
fn unsupported(module: &Module<'_>) -> Option<String> {
    let m = module.as_mut_ptr() as *mut c_void;
    unsafe {
        if !LLVMGetFirstGlobalAlias(m).is_null() {
            return Some("有全局别名".into());
        }
        if !LLVMGetFirstGlobalIFunc(m).is_null() {
            return Some("有 ifunc".into());
        }
    }
    for f in functions(module) {
        let g = f.as_global_value();
        if g.get_comdat().is_some() {
            return Some("有 comdat".into());
        }
        if !matches!(
            f.get_linkage(),
            Linkage::External | Linkage::Internal | Linkage::Private
        ) {
            return Some(format!("函数链接类型 {:?}", f.get_linkage()));
        }
    }
    for g in globals(module) {
        if g.get_comdat().is_some() {
            return Some("有 comdat".into());
        }
        let l = g.get_linkage();
        let name = g.get_name().to_bytes();
        let ok = matches!(l, Linkage::External | Linkage::Internal | Linkage::Private)
            || (l == Linkage::Appending && name.starts_with(b"llvm."));
        if !ok {
            return Some(format!("全局链接类型 {:?}", l));
        }
    }
    None
}

/// 贪心装箱：大的先放、放进当前最轻的分片（平局取编号小的）。
/// 返回 函数名 → 分片号。
fn assign(module: &Module<'_>, parts: usize) -> std::collections::HashMap<Vec<u8>, usize> {
    let mut items: Vec<(usize, usize, Vec<u8>)> = functions(module)
        .into_iter()
        .enumerate()
        .map(|(i, f)| (instruction_count(f), i, f.get_name().to_bytes().to_vec()))
        .collect();
    items.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let mut load = vec![0usize; parts];
    let mut owner = std::collections::HashMap::new();
    for (size, _, name) in items {
        let (k, _) = load
            .iter()
            .enumerate()
            .min_by(|a, b| a.1.cmp(b.1).then(a.0.cmp(&b.0)))
            .unwrap();
        load[k] += size.max(1);
        owner.insert(name, k);
    }
    owner
}

/// 本地符号改 external + hidden，无名全局起名。在切片前对主模块做一次。
fn externalize(module: &Module<'_>) {
    for (i, f) in functions(module).into_iter().enumerate() {
        let g = f.as_global_value();
        if g.get_name().to_bytes().is_empty() {
            g.set_name(&format!("__qi_anon_fn.{}", i));
        }
        if is_local(f.get_linkage()) {
            f.set_linkage(Linkage::External);
            g.set_visibility(GlobalVisibility::Hidden);
        }
    }
    for (i, g) in globals(module).into_iter().enumerate() {
        if is_duplicable_constant(g) {
            continue;
        }
        if g.get_name().to_bytes().is_empty() {
            g.set_name(&format!("__qi_anon_gv.{}", i));
        }
        if is_local(g.get_linkage()) {
            g.set_linkage(Linkage::External);
            g.set_visibility(GlobalVisibility::Hidden);
        }
    }
}

/// 把函数变成声明：先把体内每条有使用者的指令替换成 poison（使用者都在同一函数里），
/// 再逐条删指令、删基本块 —— 直接删块会留下悬空的 Use。
unsafe fn strip_body(func: *mut c_void) -> Result<(), String> {
    let mut insts = Vec::new();
    let mut bb = LLVMGetFirstBasicBlock(func);
    let mut blocks = Vec::new();
    while !bb.is_null() {
        blocks.push(bb);
        let mut i = LLVMGetFirstInstruction(bb);
        while !i.is_null() {
            insts.push(i);
            i = LLVMGetNextInstruction(i);
        }
        bb = LLVMGetNextBasicBlock(bb);
    }
    for &i in &insts {
        if !LLVMGetFirstUse(i).is_null() {
            let ty = LLVMTypeOf(i);
            if LLVMGetTypeKind(ty) == TOKEN_TYPE_KIND {
                return Err("token 类型的值有使用者".into());
            }
            LLVMReplaceAllUsesWith(i, LLVMGetPoison(ty));
        }
    }
    for &i in &insts {
        LLVMInstructionEraseFromParent(i);
    }
    for &b in &blocks {
        LLVMDeleteBasicBlock(b);
    }
    if LLVMHasPersonalityFn(func) != 0 {
        LLVMSetPersonalityFn(func, std::ptr::null_mut());
    }
    LLVMGlobalClearMetadata(func);
    Ok(())
}

/// 在独立 Context 里读回 bitcode，只保留第 `k` 份，生成目标文件字节。
fn build_partition(
    bitcode: &[u8],
    k: usize,
    owner: &std::collections::HashMap<Vec<u8>, usize>,
    triple: &str,
    tm_opt: inkwell::OptimizationLevel,
) -> Result<Vec<u8>, String> {
    let ctx = Context::create();
    let buf = MemoryBuffer::create_from_memory_range_copy(bitcode, "qi-partition");
    let module = Module::parse_bitcode_from_buffer(&buf, &ctx).map_err(|e| e.to_string())?;

    for f in functions(&module) {
        let name = f.get_name().to_bytes();
        let mine = owner.get(name).copied().unwrap_or(0) == k;
        if !mine {
            unsafe { strip_body(raw(f))? };
        }
    }
    if k != 0 {
        for g in globals(&module) {
            if g.get_linkage() == Linkage::Appending {
                unsafe { LLVMDeleteGlobal(raw(g)) };
            } else if !g.is_declaration() && !is_duplicable_constant(g) {
                unsafe {
                    LLVMSetInitializer(raw(g), std::ptr::null_mut());
                    LLVMGlobalClearMetadata(raw(g));
                }
            }
        }
    }

    let triple = TargetTriple::create(triple);
    let target = Target::from_triple(&triple).map_err(|e| e.to_string())?;
    let tm = target
        .create_target_machine(
            &triple,
            "generic",
            "",
            tm_opt,
            inkwell::targets::RelocMode::PIC,
            inkwell::targets::CodeModel::Default,
        )
        .ok_or_else(|| "无法创建 target machine".to_string())?;
    // 去掉本分片用不到的局部常量和声明
    module
        .run_passes(
            "globaldce",
            &tm,
            inkwell::passes::PassBuilderOptions::create(),
        )
        .map_err(|e| e.to_string())?;
    let obj = tm
        .write_to_memory_buffer(&module, FileType::Object)
        .map_err(|e| e.to_string())?;
    Ok(obj.as_slice().to_vec())
}

/// 分片目标文件放在源码树之外：`<缓存根>/parts/<out 绝对路径的摘要>/`。
///
/// 不能放 `.o` 旁边 —— 那会往用户的项目目录里塞一堆 `.qi-partN.o`。
/// 也不能放临时目录：macOS 的调试信息留在目标文件里，可执行文件的 debug map
/// 按**绝对路径**指回它们，删了 lldb 就看不到源码。所以要一个稳定、按输出
/// 路径区分的位置，跨次编译一直在。拿不到缓存根（没有 HOME 之类）就返回 None，
/// 调用方退回串行，不切。
pub(crate) fn parts_dir(out: &Path) -> Option<PathBuf> {
    use sha2::Digest;
    let root = crate::build_cache::cache_root()?;
    let abs = if out.is_absolute() {
        out.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(out)
    };
    let digest = sha2::Sha256::digest(abs.to_string_lossy().as_bytes());
    let name: String = digest
        .iter()
        .take(8)
        .map(|b| format!("{:02x}", b))
        .collect();
    Some(root.join("parts").join(name))
}

/// 第 k 个分片的目标文件路径（k ≥ 1；0 号就是 `out` 本身）。
pub(crate) fn part_object_path(out: &Path, k: usize) -> Option<PathBuf> {
    let stem = out
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    Some(parts_dir(out)?.join(format!("{}.qi-part{}.o", stem, k)))
}

/// 删掉上一次并行编译为 `out` 留下的分片目标文件（这次可能切得更少或不切）。
pub(crate) fn remove_stale_parts(out: &Path) {
    let Some(dir) = parts_dir(out) else {
        return;
    };
    if !dir.is_dir() {
        return;
    }
    for k in 1..=MAX_PARTITIONS {
        if let Some(p) = part_object_path(out, k) {
            if p.exists() {
                let _ = std::fs::remove_file(p);
            }
        }
    }
}

/// 分片目录里记一笔「这是给哪个 .o 的」，人翻缓存目录时看得懂。
const OWNER_NOTE: &str = "for.txt";

/// 一个月没动过的分片目录（对应的项目多半已删或挪走）顺手清掉，免得无限累积。
fn sweep_old_parts_dirs(parts_root: &Path) {
    let Ok(items) = std::fs::read_dir(parts_root) else {
        return;
    };
    let month = std::time::Duration::from_secs(30 * 24 * 3600);
    for item in items.flatten() {
        let old = item
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > month);
        if old {
            let _ = std::fs::remove_dir_all(item.path());
        }
    }
}

/// 并行生成目标文件。返回 Ok(None) = 这个模块不适合切，调用方走串行；
/// Ok(Some(全部目标文件)) = 已写好，0 号是 `out`。
/// 返回 Err = 切到一半出错（主模块已被改成 external+hidden，但串行路径照样能编它）。
pub(super) fn try_emit_parallel(
    module: &Module<'_>,
    tm: &TargetMachine,
    tm_opt: inkwell::OptimizationLevel,
    host_native: bool,
    out: &Path,
) -> Result<Option<Vec<PathBuf>>, String> {
    if !host_native || !(cfg!(target_os = "macos") || cfg!(target_os = "linux")) {
        return Ok(None);
    }
    let Some(parts) = partition_count() else {
        return Ok(None);
    };
    let Some(dir) = parts_dir(out) else {
        return Ok(None);
    };
    if let Some(why) = unsupported(module) {
        if std::env::var_os("QI_CODEGEN_PARTITIONS").is_some() {
            eprintln!("[qi 并行codegen] 不切分: {}", why);
        }
        return Ok(None);
    }

    let t = std::time::Instant::now();
    let owner = assign(module, parts);
    externalize(module);
    let bitcode = module.write_bitcode_to_memory().as_slice().to_vec();
    let triple = tm.get_triple().as_str().to_string_lossy().into_owned();
    let t = crate::phase_timer::mark("  切片准备", t);

    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(parts);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let results: Vec<std::sync::Mutex<Option<Result<Vec<u8>, String>>>> =
        (0..parts).map(|_| std::sync::Mutex::new(None)).collect();
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| loop {
                let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if k >= parts {
                    break;
                }
                let r = build_partition(&bitcode, k, &owner, &triple, tm_opt);
                *results[k].lock().unwrap() = Some(r);
            });
        }
    });
    crate::phase_timer::mark(&format!("  {}片并行", parts), t);

    if let Some(root) = dir.parent() {
        sweep_old_parts_dirs(root);
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("建 {} 失败: {}", dir.display(), e))?;
    let _ = std::fs::write(dir.join(OWNER_NOTE), out.display().to_string());
    let mut paths: Vec<PathBuf> = Vec::new();
    for (k, slot) in results.into_iter().enumerate() {
        let bytes = match slot.into_inner().unwrap() {
            Some(Ok(b)) => b,
            Some(Err(e)) => return Err(format!("分片 {} 失败: {}", k, e)),
            None => return Err(format!("分片 {} 没有结果", k)),
        };
        let p = if k == 0 {
            out.to_path_buf()
        } else {
            dir.join(
                part_object_path(out, k)
                    .and_then(|p| p.file_name().map(|n| n.to_owned()))
                    .ok_or("分片文件名")?,
            )
        };
        std::fs::write(&p, &bytes).map_err(|e| format!("写 {} 失败: {}", p.display(), e))?;
        paths.push(p);
    }
    Ok(Some(paths))
}
