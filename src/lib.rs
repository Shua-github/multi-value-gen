use std::collections::HashMap;
use walrus::ir::{BinaryOp, LoadKind, MemArg};
use walrus::{FunctionBuilder, FunctionId, GlobalId, LocalId, MemoryId, Module, ValType};

pub fn parse(wasm: Vec<u8>, funcs: HashMap<String, Vec<ValType>>) -> Result<Vec<u8>, String> {
    let mut module = Module::from_buffer(&wasm).map_err(|e| format!("无法解析WASM模块: {}", e))?;
    let memory = module
        .memories
        .iter()
        .next()
        .map(|m| m.id())
        .ok_or_else(|| "未找到内存".to_string())?;
    let stack_pointer = module
        .globals
        .iter()
        .find(|g| g.name.as_deref() == Some("__stack_pointer"))
        .map(|g| g.id())
        .ok_or_else(|| "未找到栈指针全局变量".to_string())?;
    let mut to_xform = Vec::new();
    for (name, results) in funcs {
        let func_id = find_func(&module, &name)?;
        to_xform.push((func_id, 0, results));
    }
    if to_xform.is_empty() {
        return Err("没有需要转换的函数".to_string());
    }
    let mut wrappers = Vec::with_capacity(to_xform.len());
    for (func_id, return_pointer_index, results) in to_xform.iter() {
        let wrapper = new_wrapper_func(
            &mut module,
            memory,
            stack_pointer,
            *func_id,
            *return_pointer_index,
            results,
        )?;
        wrappers.push(wrapper);
    }
    replace_export(&mut module, to_xform.iter().map(|(id, _, _)| *id), wrappers);
    let result = module.emit_wasm();
    Ok(result)
}

fn find_func(module: &Module, name: &str) -> Result<FunctionId, String> {
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
    let (params, results) = module.types.params_results(ty);
    if !results.is_empty() {
        return Err(format!("函数 '{}' 已有返回结果", name));
    }
    if params.first() != Some(&ValType::I32) {
        return Err(format!(
            "函数 '{}' 的第一个参数必须是i32类型(返回指针)",
            name
        ));
    }
    Ok(func_id)
}

fn new_wrapper_func(
    module: &mut Module,
    memory: MemoryId,
    stack_pointer: GlobalId,
    func_id: FunctionId,
    return_pointer_index: usize,
    results: &[ValType],
) -> Result<FunctionId, String> {
    if module.globals.get(stack_pointer).ty != ValType::I32 {
        return Err("栈指针全局变量不是i32类型".to_string());
    }
    let func = module.funcs.get(func_id);
    let ty = func.ty();
    let (params, original_results) = module.types.params_results(ty);
    if !original_results.is_empty() {
        return Err("只能转换没有返回结果的函数".to_string());
    }
    match params.get(return_pointer_index) {
        Some(ValType::I32) => {}
        None => return Err("返回指针参数不存在".to_string()),
        Some(_) => return Err("返回指针参数不是i32类型".to_string()),
    }
    let mut size: u32 = 0;
    for ty in results {
        size = match ty {
            ValType::I32 | ValType::F32 => size + 4,
            ValType::I64 | ValType::F64 => ((size + 7) & !7) + 8,
            ValType::V128 => ((size + 15) & !15) + 16,
            ValType::Ref(_) => unreachable!("引用类型不应出现在此处"),
        };
    }
    let results_size = size as i32;
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
    let param_locals: Vec<LocalId> = new_params.iter().map(|ty| module.locals.add(*ty)).collect();
    let return_pointer_local = module.locals.add(ValType::I32);
    let mut builder = FunctionBuilder::new(&mut module.types, &new_params, results);
    let mut body = builder.func_body();
    body.global_get(stack_pointer)
        .i32_const(results_size)
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
    for ty in results {
        body.local_get(return_pointer_local);
        match ty {
            ValType::I32 => {
                body.load(
                    memory,
                    LoadKind::I32 { atomic: false },
                    MemArg { align: 4, offset },
                );
                offset += 4;
            }
            ValType::I64 => {
                offset = (offset + 7) & !7;
                body.load(
                    memory,
                    LoadKind::I64 { atomic: false },
                    MemArg { align: 8, offset },
                );
                offset += 8;
            }
            ValType::F32 => {
                body.load(memory, LoadKind::F32, MemArg { align: 4, offset });
                offset += 4;
            }
            ValType::F64 => {
                offset = (offset + 7) & !7;
                body.load(memory, LoadKind::F64, MemArg { align: 8, offset });
                offset += 8;
            }
            ValType::V128 => {
                offset = (offset + 15) & !15;
                body.load(memory, LoadKind::V128, MemArg { align: 16, offset });
                offset += 16;
            }
            ValType::Ref(_) => unreachable!("引用类型不应出现在此处"),
        }
    }
    body.local_get(return_pointer_local)
        .i32_const(results_size)
        .binop(BinaryOp::I32Add)
        .global_set(stack_pointer);
    let wrapper_id = builder.finish(param_locals, &mut module.funcs);
    if let Some(name) = &module.funcs.get(func_id).name {
        module.funcs.get_mut(wrapper_id).name = Some(format!("{}_wrapper", name));
    }
    Ok(wrapper_id)
}

fn replace_export(
    module: &mut Module,
    original_funcs: impl Iterator<Item = FunctionId>,
    wrappers: Vec<FunctionId>,
) {
    let mut wrapper_iter = wrappers.into_iter();
    for original_id in original_funcs {
        if let Some(export) = module.exports.iter_mut().find(|e| match e.item {
            walrus::ExportItem::Function(f) => f == original_id,
            _ => false,
        }) {
            export.item = walrus::ExportItem::Function(wrapper_iter.next().unwrap());
        }
    }
}
