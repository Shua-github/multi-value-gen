use walrus::ir::{LoadKind, MemArg};
use walrus::{InstrSeqBuilder, LocalId, MemoryId, ValType};

pub fn calculate_size(results: &[ValType]) -> u32 {
    let mut size: u32 = 0;
    for ty in results {
        size = match ty {
            ValType::I32 | ValType::F32 => size + 4,
            ValType::I64 | ValType::F64 => ((size + 7) & !7) + 8,
            ValType::V128 => ((size + 15) & !15) + 16,
            ValType::Ref(_) => unreachable!("引用类型不应出现在此处"),
        };
    }
    size
}

pub fn load_value(
    body: &mut InstrSeqBuilder,
    memory: MemoryId,
    return_pointer_local: LocalId,
    ty: ValType,
    offset: &mut u32,
) {
    body.local_get(return_pointer_local);
    match ty {
        ValType::I32 => {
            body.load(
                memory,
                LoadKind::I32 { atomic: false },
                MemArg {
                    align: 4,
                    offset: *offset,
                },
            );
            *offset += 4;
        }
        ValType::I64 => {
            *offset = (*offset + 7) & !7;
            body.load(
                memory,
                LoadKind::I64 { atomic: false },
                MemArg {
                    align: 8,
                    offset: *offset,
                },
            );
            *offset += 8;
        }
        ValType::F32 => {
            body.load(
                memory,
                LoadKind::F32,
                MemArg {
                    align: 4,
                    offset: *offset,
                },
            );
            *offset += 4;
        }
        ValType::F64 => {
            *offset = (*offset + 7) & !7;
            body.load(
                memory,
                LoadKind::F64,
                MemArg {
                    align: 8,
                    offset: *offset,
                },
            );
            *offset += 8;
        }
        ValType::V128 => {
            *offset = (*offset + 15) & !15;
            body.load(
                memory,
                LoadKind::V128,
                MemArg {
                    align: 16,
                    offset: *offset,
                },
            );
            *offset += 16;
        }
        ValType::Ref(_) => unreachable!("引用类型不应出现在此处"),
    }
}
