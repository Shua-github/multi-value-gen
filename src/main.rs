use multi_value_gen::parse;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::process;
use walrus::ValType;

fn main() {
    if let Err(e) = run() {
        eprintln!("错误: {}", e);
        process::exit(1);
    }
    process::exit(0);
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    let mut input = None;
    let mut output = None;
    let mut f_str = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-i" => {
                i += 1;
                if i >= args.len() {
                    return Err("缺少 -i 参数值".to_string());
                }
                input = Some(args[i].clone());
            }
            "-o" => {
                i += 1;
                if i >= args.len() {
                    return Err("缺少 -o 参数值".to_string());
                }
                output = Some(args[i].clone());
            }
            "-f" => {
                i += 1;
                if i >= args.len() {
                    return Err("缺少 -f 参数值".to_string());
                }
                f_str = Some(args[i].clone());
            }
            arg => return Err(format!("未知参数: {}", arg)),
        }
        i += 1;
    }

    let input = input.ok_or("缺少输入文件参数 -i".to_string())?;
    let output = output.ok_or("缺少输出文件参数 -o".to_string())?;
    let f_str = f_str
        .ok_or("缺少函数签名参数 -f".to_string())?
        .replace(' ', "");

    let mut funcs = HashMap::new();
    for sig in f_str.split(';').filter(|s| !s.is_empty()) {
        let parts: Vec<&str> = sig.split(':').collect();
        if parts.len() != 2 {
            return Err(format!(
                "无效的函数签名格式: '{}'，应为 '函数名:结果类型1,结果类型2'",
                sig
            ));
        }
        let name = parts[0].trim().to_string();
        if name.is_empty() {
            return Err("函数名不能为空".to_string());
        }
        let results_str = parts[1].trim();
        let results = if results_str.is_empty() {
            Vec::new()
        } else {
            results_str
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(|s| match s {
                    "i32" => Ok(ValType::I32),
                    "i64" => Ok(ValType::I64),
                    "f32" => Ok(ValType::F32),
                    "f64" => Ok(ValType::F64),
                    _ => Err(format!(
                        "不支持的类型: '{}'，支持的类型: i32, i64, f32, f64",
                        s
                    )),
                })
                .collect::<Result<Vec<_>, _>>()?
        };
        funcs.insert(name, results);
    }

    if funcs.is_empty() {
        return Err("未提供有效的函数签名".to_string());
    }

    let wasm_bytes = fs::read(&input).map_err(|e| format!("无法读取输入文件 {}: {}", input, e))?;
    let result = parse(wasm_bytes, funcs).map_err(|e| format!("WASM转换失败: {}", e))?;
    fs::write(&output, result).map_err(|e| format!("无法写入输出文件 {}: {}", output, e))?;

    println!("转换成功: {} -> {}", input, output);
    Ok(())
}
