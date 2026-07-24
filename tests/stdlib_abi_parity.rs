use std::path::{Path, PathBuf};

use qi_compiler::codegen::module_registry::ModuleRegistry;
use qi_compiler::codegen::stdlib_abi::{LLM_ABI, TOOL_CONTROL_ABI, WEB_RUNTIME_ABI};

fn canonical_runtime_source(relative_path: &str) -> PathBuf {
    std::env::var_os("QI_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .expect("compiler repository must have a parent directory")
                .join("qi-runtime")
        })
        .join(relative_path)
}

fn normalize_type(ty: &str) -> String {
    ty.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn runtime_export(source: &str, symbol: &str) -> Option<(Vec<String>, String)> {
    let marker = format!(r#"pub extern "C" fn {symbol}"#);
    let declaration = source
        .match_indices(&marker)
        .map(|(index, _)| &source[index + marker.len()..])
        .find(|suffix| suffix.trim_start().starts_with('('))?;
    let params_start = declaration.find('(')? + 1;
    let params_end = declaration[params_start..].find(')')? + params_start;
    let params = declaration[params_start..params_end]
        .split(',')
        .filter_map(|param| param.rsplit_once(':').map(|(_, ty)| normalize_type(ty)))
        .collect();
    let after_params = &declaration[params_end + 1..];
    let body_start = after_params.find('{')?;
    let signature_tail = &after_params[..body_start];
    let return_type = match signature_tail.find("->") {
        Some(return_arrow) => {
            let return_start = return_arrow + 2;
            normalize_type(&signature_tail[return_start..])
        }
        None => "()".to_string(),
    };
    Some((params, return_type))
}

fn assert_runtime_abi(
    relative_path: &str,
    declarations: &[qi_compiler::codegen::stdlib_abi::StdlibAbiFunction],
) {
    let path = canonical_runtime_source(relative_path);
    let source = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "cannot read canonical qi-runtime declarations at {}: {error}; set QI_RUNTIME_DIR when repositories are not siblings",
            path.display()
        )
    });
    for declaration in declarations {
        let (params, return_type) = runtime_export(&source, declaration.runtime_name)
            .unwrap_or_else(|| {
                panic!(
                    "qi-runtime does not export compiler-declared symbol {}",
                    declaration.runtime_name
                )
            });
        assert_eq!(
            &params,
            &declaration
                .c_param_types
                .iter()
                .map(|ty| ty.to_string())
                .collect::<Vec<_>>(),
            "parameter ABI drift for {}",
            declaration.runtime_name
        );
        assert_eq!(
            return_type, declaration.c_return_type,
            "return ABI drift for {}",
            declaration.runtime_name
        );
    }
}

#[test]
fn llm_registry_matches_compiler_abi_manifest() {
    let registry = ModuleRegistry::new();

    for declaration in LLM_ABI {
        let function = registry
            .get_function("标准库.大模型", declaration.qi_name)
            .unwrap_or_else(|| panic!("标准库.大模型 missing {}", declaration.qi_name));
        assert_eq!(function.runtime_name, declaration.runtime_name);
        assert_eq!(function.param_types, declaration.param_types);
        assert_eq!(function.return_type, declaration.return_type);
    }
}

#[test]
fn llm_compiler_abi_matches_canonical_qi_runtime_source() {
    assert_runtime_abi("src/stdlib/llm_ffi.rs", LLM_ABI);
}

#[test]
fn tool_control_registry_matches_compiler_abi_manifest() {
    let registry = ModuleRegistry::new();

    for declaration in TOOL_CONTROL_ABI {
        let function = registry
            .get_function("标准库.工具控制", declaration.qi_name)
            .unwrap_or_else(|| panic!("标准库.工具控制 missing {}", declaration.qi_name));
        assert_eq!(function.runtime_name, declaration.runtime_name);
        assert_eq!(function.param_types, declaration.param_types);
        assert_eq!(function.return_type, declaration.return_type);
    }
}

#[test]
fn tool_control_compiler_abi_matches_canonical_qi_runtime_source() {
    assert_runtime_abi("src/tool_control.rs", TOOL_CONTROL_ABI);
}

#[test]
fn web_runtime_registry_matches_compiler_abi_manifest() {
    let registry = ModuleRegistry::new();

    for declaration in WEB_RUNTIME_ABI {
        let function = registry
            .get_function("标准库.Web运行时", declaration.qi_name)
            .unwrap_or_else(|| panic!("标准库.Web运行时 missing {}", declaration.qi_name));
        assert_eq!(function.runtime_name, declaration.runtime_name);
        assert_eq!(function.param_types, declaration.param_types);
        assert_eq!(function.return_type, declaration.return_type);
    }
}

#[test]
fn web_runtime_compiler_abi_matches_canonical_qi_runtime_source() {
    assert_runtime_abi("src/stdlib/web_ffi.rs", WEB_RUNTIME_ABI);
}
