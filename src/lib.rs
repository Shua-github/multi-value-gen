mod utils;

use std::collections::HashMap;
pub use walrus::ValType;
use walrus::ir::BinaryOp;
use walrus::{FunctionBuilder, FunctionId, GlobalId, LocalId, MemoryId, Module};

#[derive(Debug)]
struct Env {
    memory: MemoryId,
    stack_pointer: GlobalId,
    to_xform: Vec<(FunctionId, usize, Vec<ValType>)>,
}

fn setup_env(module: &Module, funcs: HashMap<String, Vec<ValType>>) -> Result<Env, String> {
    let memory = module
        .memories
        .iter()
        .next()
        .map(|m| m.id())
        .ok_or_else(|| "未找到内存".to_string())?;

    let mut stack_pointer = None;
    for g in module.globals.iter() {
        // 可变 + i32 + 第一个 或者 叫__stack_pointer
        if (g.mutable == true && g.ty == ValType::I32 && g.id().index() == 0) || g.name.as_deref() == Some("__stack_pointer") {
            stack_pointer = Some(g.id());
            break;
        }
    }

    let stack_pointer = stack_pointer.ok_or_else(|| "未找到栈指针全局变量".to_string())?;

    if module.globals.get(stack_pointer).ty != ValType::I32 {
        return Err("栈指针全局变量不是i32类型".to_string());
    }

    let mut to_xform = Vec::new();
    for (name, results) in funcs {
        let func_id = module
            .exports
            .iter()
            .find(|e| e.name == name)
            .and_then(|e| match e.item {
                walrus::ExportItem::Function(f) => Some(f),
                _ => None,
            })
            .ok_or_else(|| format!("未找到导出的函数: '{}'", name))?;

        let func = module.funcs.get(func_id);
        let ty = func.ty();

        let (params, original_results) = module.types.params_results(ty);
        if !original_results.is_empty() {
            return Err(format!("函数 '{}' 已有返回结果", name));
        }
        if params.first() != Some(&ValType::I32) {
            return Err(format!(
                "函数 '{}' 的第一个参数必须是i32类型(返回指针)",
                name
            ));
        }

        let return_pointer_index = 0;
        match params.get(return_pointer_index) {
            Some(ValType::I32) => {}
            None => return Err(format!("函数 '{}' 的返回指针参数不存在", name)),
            Some(_) => return Err(format!("函数 '{}' 的返回指针参数不是i32类型", name)),
        }
        to_xform.push((func_id, return_pointer_index, results));
    }
    if to_xform.is_empty() {
        return Err("没有需要转换的函数".to_string());
    }
    Ok(Env {
        memory,
        stack_pointer,
        to_xform,
    })
}

pub fn parse(wasm: Vec<u8>, funcs: HashMap<String, Vec<ValType>>) -> Result<Vec<u8>, String> {
    // 解析WASM模块
    let mut module = Module::from_buffer(&wasm).map_err(|e| format!("无法解析WASM模块: {}", e))?;
    let env = setup_env(&module, funcs)?;
    // 为每个需要转换的函数创建包装器
    for (func_id, return_pointer_index, results) in env.to_xform {
        let wrapper = new_wrapper_func(
            &mut module,
            env.memory,
            env.stack_pointer,
            func_id,
            return_pointer_index,
            &results,
        )?;
        replace_export(&mut module, func_id, wrapper)
    }
    let result = module.emit_wasm();
    Ok(result)
}

fn new_wrapper_func(
    module: &mut Module,
    memory: MemoryId,
    stack_pointer: GlobalId,
    func_id: FunctionId,
    return_pointer_index: usize,
    results: &[ValType],
) -> Result<FunctionId, String> {
    let func = module.funcs.get(func_id);
    let ty = func.ty();
    let (params, _) = module.types.params_results(ty);

    // 计算返回值所需的栈空间大小
    let size = utils::calculate_size(results);

    let new_params: Vec<ValType> = params
        .iter()
        .enumerate()
        .filter_map(|(i, ty)| {
            if i == return_pointer_index {
                None
            } else {
                Some(*ty)
            }
        })
        .collect();

    let mut builder = FunctionBuilder::new(&mut module.types, &new_params, results);
    let mut body = builder.func_body();
    let param_locals: Vec<LocalId> = new_params.iter().map(|ty| module.locals.add(*ty)).collect();
    let return_pointer_local = module.locals.add(ValType::I32);

    // 保存栈指针
    body.global_get(stack_pointer)
        .i32_const(size as i32)
        .binop(BinaryOp::I32Sub)
        .local_tee(return_pointer_local)
        .global_set(stack_pointer);

    for (i, local) in param_locals.iter().enumerate() {
        if i == return_pointer_index {
            body.local_get(return_pointer_local);
        }
        body.local_get(*local);
    }
    if return_pointer_index == param_locals.len() {
        body.local_get(return_pointer_local);
    }
    body.call(func_id);
    let mut offset: u32 = 0;

    // 从内存中加载返回值
    for ty in results {
        utils::load_value(&mut body, memory, return_pointer_local, *ty, &mut offset);
    }
    body.local_get(return_pointer_local)
        .i32_const(size as i32)
        .binop(BinaryOp::I32Add)
        .global_set(stack_pointer);
    let wrapper_id = builder.finish(param_locals, &mut module.funcs);
    if let Some(name) = &module.funcs.get(func_id).name {
        module.funcs.get_mut(wrapper_id).name = Some(format!("{}_wrapper", name));
    }
    Ok(wrapper_id)
}

fn replace_export(module: &mut Module, original_func: FunctionId, wrapper: FunctionId) {
    if let Some(export) = module.exports.iter_mut().find(|e| match e.item {
        walrus::ExportItem::Function(f) => f == original_func,
        _ => false,
    }) {
        export.item = walrus::ExportItem::Function(wrapper);
    }
}
