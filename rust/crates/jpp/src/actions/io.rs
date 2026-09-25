//! 读写与记录：`record_check`、`read_json`、`write_json`（C-1 从 CLI 的注册闭包原样搬来）。

use super::Ctx;
use crate::interp::json_to_value;
use crate::value::Value;
use serde_json::Value as Json;

// The source computes validity. This action only records and returns its value.
pub(super) fn record_check(ctx: &Ctx, args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("record_check expects one source-computed record".into());
    }
    ctx.checks.borrow_mut().push(args[0].to_json());
    Ok(args[0].clone())
}

pub(super) fn read_json(_: &Ctx, args: &[Value]) -> Result<Value, String> {
    let [Value::Text(path, _)] = args else {
        return Err("read_json expects one file path".into());
    };
    let bytes = std::fs::read(path.as_ref()).map_err(|e| format!("{path}: {e}"))?;
    let value: Json = serde_json::from_slice(&bytes).map_err(|e| format!("{path}: {e}"))?;
    // Reject unsigned integers that the core's JSON adapter cannot represent as Int.
    super::validate_numbers(&value)?;
    Ok(json_to_value(&value))
}

pub(super) fn write_json(_: &Ctx, args: &[Value]) -> Result<Value, String> {
    let [Value::Text(path, _), value] = args else {
        return Err("write_json expects a file path and a value".into());
    };
    let bytes = serde_json::to_vec_pretty(&value.to_json()).map_err(|e| e.to_string())?;
    std::fs::write(path.as_ref(), bytes).map_err(|e| format!("{path}: {e}"))?;
    Ok(value.clone())
}
