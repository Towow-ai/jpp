//! 确定性文本与数据内置（B157，`地基/附注/2026-09-26-批6裁定.md` §五）与带种子伪随机（B158，同附注 §六）。
//! 步 7t（`21` 原文「步 7c · 文本与数据内置」，序 P16；与已造的旧步 7c——B83 续接命中记录——撞号改称）。
//!
//! **为什么在这里、不在 prelude、不在动作表**：`split`、正则、日期解析这类原语无法由现有内置组合出来
//! （只能 `fold` 逐字符写，行数回到手写水平）；宿主动作是效应——记账、计费、按键重放，纯函数收进去会把
//! 文本处理算进 `effect_requests`，让报告的效应数失真（B157 §一）。
//!
//! **taint 统一算法（B33 第 3 条：每一个把值搬过去的边界，默认都会把「这个值怎么来的」留在原地）**：
//! 本文件每个函数最后一步都是 `raw.with_prov(&join_args(&args))`——不给任何一个函数单独写 taint 分支。
//! 唯一的例外是 `Value::Fail`：`with_prov` 没有 `Fail` 分支（见 `jpp_value::value::Value::with_prov`），
//! 落到 `other => other` 原样返回，所以 `parse_json`/`date_parse` 失败时必须手工把 `join_args(&args)`
//! 塞进 `Value::Fail(reason, p)` 的构造，不能先建 trusted 的 `Fail` 再 `.with_prov()`（否则不可信输入解析
//! 失败后仍是可信 `Fail`，K-182/K-203 同一类问题）。
//!
//! `now`、无种子随机、环境变量、文件读取都不是内置（B157 (2)）：不确定性只经端口进入。

use super::*;
use jpp_value::prov::join as prov_join;

/// 带种子随机的算法版本（B158 (1)）：换算法或换常量要换版本号、加新测试，不能静默改变同 seed 的输出。
pub const RAND_VERSION: &str = "s1";

const GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;

/// splitmix64 单发混合：给定 `(seed, k)`，产出一个与两者都相关的 64 位值（B158 (1)）。
/// 不是有状态的生成器——每次调用都是纯函数，同输入同输出，不需要作者手传状态（J-11 可交换性）。
fn mix(seed: i64, k: i64) -> u64 {
    let mut z = (seed as u64)
        .wrapping_add((k as u64).wrapping_mul(GAMMA))
        .wrapping_add(GAMMA);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// `[0, 1)` 上的浮点数：取高 53 位，与常见「u64 转均匀 double」写法相同。
fn rand_f64(seed: i64, k: i64) -> f64 {
    (mix(seed, k) >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
}

/// 通用算术错误
fn need(name: &str, n: usize, k: usize, sp: Span) -> R<()> {
    if n == k {
        Ok(())
    } else {
        err(
            Some("E-rt-arity"),
            format!("{name} 需要 {k} 个参数，收到 {n}"),
            sp,
        )
    }
}

/// B33 第 3 条的统一实现：结果携带全部输入的 taint（∨）与 sources（∪）。
/// `Value::prov()` 对 `List`/`Record` 已经是逐元素 join，这里再对参数列表 fold 一次即可。
fn join_args(args: &[Value]) -> Provenance {
    args.iter()
        .fold(Provenance::trusted(), |a, x| prov_join(&a, &x.prov()))
}

fn text_of(v: &Value) -> Option<Rc<str>> {
    match v {
        Value::Text(s, _) => Some(s.clone()),
        _ => None,
    }
}

/// 逐字符列表（`chars` 与 `split(s, "")` 共用）。
fn chars_list(s: &str) -> Vec<Value> {
    s.chars().map(|c| Value::text(&c.to_string())).collect()
}

/// 标量的排序秩：用来判定「同类型」，不用来比较（比较见 [`cmp_scalar`]）。
fn scalar_rank(v: &Value) -> Option<u8> {
    match v {
        Value::Int(..) => Some(0),
        Value::Float(..) => Some(1),
        Value::Text(..) => Some(2),
        Value::Bool(..) => Some(3),
        _ => None,
    }
}

/// 排序比较：只在类型秩相同时调用（调用方先用 [`scalar_rank`] 核过一遍）。
fn cmp_scalar(a: &Value, b: &Value, name: &str, sp: Span) -> R<std::cmp::Ordering> {
    use std::cmp::Ordering;
    match (a, b) {
        (Value::Int(x, _), Value::Int(y, _)) => Ok(x.cmp(y)),
        (Value::Float(x, _), Value::Float(y, _)) => x.partial_cmp(y).ok_or(()).or_else(|_| {
            err::<Ordering>(
                Some("E-rt-type"),
                format!("{name} 遇到 NaN，NaN 不可排序"),
                sp,
            )
        }),
        (Value::Text(x, _), Value::Text(y, _)) => Ok(x.as_ref().cmp(y.as_ref())),
        (Value::Bool(x, _, _), Value::Bool(y, _, _)) => Ok(x.cmp(y)),
        _ => err(
            Some("E-rt-type"),
            format!("{name} 只支持同类型标量键（Int / Float / Text / Bool）"),
            sp,
        ),
    }
}

/// `sort`/`sort_by` 的公共尾段：校验整批键同形（同长度、逐位同类型），无 NaN，然后稳定排序。
/// `keyed`：每个元素连同它的排序键（单值或复合键列表）。
fn stable_sort_by_keys(name: &str, mut keyed: Vec<(Value, Vec<Value>)>, sp: Span) -> R<Vec<Value>> {
    if let Some((_, first_key)) = keyed.first() {
        let arity = first_key.len();
        let ranks: Vec<Option<u8>> = first_key.iter().map(scalar_rank).collect();
        for (_, k) in &keyed {
            if k.len() != arity {
                return err(Some("E-rt-type"), format!("{name} 的键长度不一致"), sp);
            }
            for (i, v) in k.iter().enumerate() {
                if scalar_rank(v) != ranks[i] {
                    return err(
                        Some("E-rt-type"),
                        format!("{name} 的第 {} 位键类型不一致", i + 1),
                        sp,
                    );
                }
            }
        }
        // 提前发现 NaN / 不可比类型，报错点在排序之前（排序本身用 infallible 比较，
        // 依赖这一步已经把会出错的情形筛掉）。
        for i in 0..arity {
            for (_, k) in &keyed {
                let _ = cmp_scalar(&k[i], &k[i], name, sp)?;
            }
        }
    }
    keyed.sort_by(|(_, ka), (_, kb)| {
        for (a, b) in ka.iter().zip(kb.iter()) {
            match cmp_scalar(a, b, name, sp).unwrap_or(std::cmp::Ordering::Equal) {
                std::cmp::Ordering::Equal => continue,
                other => return other,
            }
        }
        std::cmp::Ordering::Equal
    });
    Ok(keyed.into_iter().map(|(v, _)| v).collect())
}

impl<'a> Interp<'a> {
    /// 22 个内置的总分派（B157 十九个 + B158 三个）。`host_builtins.rs::builtin()` 里一支多名转发到这里。
    pub(crate) fn text_builtin(
        &mut self,
        name: &'static str,
        args: Vec<Value>,
        sp: Span,
    ) -> R<Value> {
        match name {
            "split" => self.b_split(args, sp),
            "lower" => self.b_lower(args, sp),
            "upper" => self.b_upper(args, sp),
            "trim" => self.b_trim(args, sp),
            "replace" => self.b_replace(args, sp),
            "starts_with" => self.b_starts_with(args, sp),
            "ends_with" => self.b_ends_with(args, sp),
            "index_of" => self.b_index_of(args, sp),
            "chars" => self.b_chars(args, sp),
            "regex_match" => self.b_regex_match(args, sp),
            "regex_find" => self.b_regex_find(args, sp),
            "sort" => self.b_sort(args, sp),
            "sort_by" => self.b_sort_by(args, sp),
            "parse_json" => self.b_parse_json(args, sp),
            "to_json" => self.b_to_json(args, sp),
            "hash" => self.b_hash(args, sp),
            "date_parse" => self.b_date_parse(args, sp),
            "date_format" => self.b_date_format(args, sp),
            "date_add" => self.b_date_add(args, sp),
            "rand" => self.b_rand(args, sp),
            "rand_int" => self.b_rand_int(args, sp),
            "shuffle" => self.b_shuffle(args, sp),
            _ => err(Some("E-rt-name"), format!("未知内置 {name}"), sp),
        }
    }

    fn b_split(&mut self, args: Vec<Value>, sp: Span) -> R<Value> {
        need("split", args.len(), 2, sp)?;
        let (Some(s), Some(sep)) = (text_of(&args[0]), text_of(&args[1])) else {
            return err(Some("E-rt-arg"), "split(Text, Text)", sp);
        };
        let parts = if sep.is_empty() {
            chars_list(&s)
        } else {
            s.split(sep.as_ref())
                .map(|p| Value::text(p))
                .collect::<Vec<_>>()
        };
        Ok(Value::list(parts).with_prov(&join_args(&args)))
    }

    fn b_lower(&mut self, args: Vec<Value>, sp: Span) -> R<Value> {
        need("lower", args.len(), 1, sp)?;
        let Some(s) = text_of(&args[0]) else {
            return err(Some("E-rt-arg"), "lower(Text)", sp);
        };
        Ok(Value::text(&s.to_lowercase()).with_prov(&join_args(&args)))
    }

    fn b_upper(&mut self, args: Vec<Value>, sp: Span) -> R<Value> {
        need("upper", args.len(), 1, sp)?;
        let Some(s) = text_of(&args[0]) else {
            return err(Some("E-rt-arg"), "upper(Text)", sp);
        };
        Ok(Value::text(&s.to_uppercase()).with_prov(&join_args(&args)))
    }

    fn b_trim(&mut self, args: Vec<Value>, sp: Span) -> R<Value> {
        need("trim", args.len(), 1, sp)?;
        let Some(s) = text_of(&args[0]) else {
            return err(Some("E-rt-arg"), "trim(Text)", sp);
        };
        Ok(Value::text(s.trim()).with_prov(&join_args(&args)))
    }

    fn b_replace(&mut self, args: Vec<Value>, sp: Span) -> R<Value> {
        need("replace", args.len(), 3, sp)?;
        let (Some(s), Some(from), Some(to)) =
            (text_of(&args[0]), text_of(&args[1]), text_of(&args[2]))
        else {
            return err(Some("E-rt-arg"), "replace(Text, Text, Text)", sp);
        };
        // 空 `from`：原样返回，不用 Rust `str::replace("", to)` 的逐字符插入语义（见预注册 §五）。
        let out = if from.is_empty() {
            s.to_string()
        } else {
            s.replace(from.as_ref(), to.as_ref())
        };
        Ok(Value::text(&out).with_prov(&join_args(&args)))
    }

    fn b_starts_with(&mut self, args: Vec<Value>, sp: Span) -> R<Value> {
        need("starts_with", args.len(), 2, sp)?;
        let (Some(s), Some(sub)) = (text_of(&args[0]), text_of(&args[1])) else {
            return err(Some("E-rt-arg"), "starts_with(Text, Text)", sp);
        };
        Ok(Value::bool(s.starts_with(sub.as_ref())).with_prov(&join_args(&args)))
    }

    fn b_ends_with(&mut self, args: Vec<Value>, sp: Span) -> R<Value> {
        need("ends_with", args.len(), 2, sp)?;
        let (Some(s), Some(sub)) = (text_of(&args[0]), text_of(&args[1])) else {
            return err(Some("E-rt-arg"), "ends_with(Text, Text)", sp);
        };
        Ok(Value::bool(s.ends_with(sub.as_ref())).with_prov(&join_args(&args)))
    }

    fn b_index_of(&mut self, args: Vec<Value>, sp: Span) -> R<Value> {
        need("index_of", args.len(), 2, sp)?;
        let (Some(s), Some(sub)) = (text_of(&args[0]), text_of(&args[1])) else {
            return err(Some("E-rt-arg"), "index_of(Text, Text)", sp);
        };
        // 按字符计数（与 `len(Text)`/`chars` 同一口径），不是字节偏移。
        let idx = match s.find(sub.as_ref()) {
            Some(byte_i) => s[..byte_i].chars().count() as i64,
            None => -1,
        };
        Ok(Value::int(idx).with_prov(&join_args(&args)))
    }

    fn b_chars(&mut self, args: Vec<Value>, sp: Span) -> R<Value> {
        need("chars", args.len(), 1, sp)?;
        let Some(s) = text_of(&args[0]) else {
            return err(Some("E-rt-arg"), "chars(Text)", sp);
        };
        Ok(Value::list(chars_list(&s)).with_prov(&join_args(&args)))
    }

    fn b_regex_match(&mut self, args: Vec<Value>, sp: Span) -> R<Value> {
        need("regex_match", args.len(), 2, sp)?;
        let (Some(s), Some(re)) = (text_of(&args[0]), text_of(&args[1])) else {
            return err(Some("E-rt-arg"), "regex_match(Text, Text)", sp);
        };
        let re = regex::Regex::new(&re)
            .map_err(|e| RtError::new(Some("E-rt-regex"), format!("非法正则「{re}」：{e}"), sp))?;
        Ok(Value::bool(re.is_match(&s)).with_prov(&join_args(&args)))
    }

    fn b_regex_find(&mut self, args: Vec<Value>, sp: Span) -> R<Value> {
        need("regex_find", args.len(), 2, sp)?;
        let (Some(s), Some(re)) = (text_of(&args[0]), text_of(&args[1])) else {
            return err(Some("E-rt-arg"), "regex_find(Text, Text)", sp);
        };
        let re = regex::Regex::new(&re)
            .map_err(|e| RtError::new(Some("E-rt-regex"), format!("非法正则「{re}」：{e}"), sp))?;
        let found: Vec<Value> = re.find_iter(&s).map(|m| Value::text(m.as_str())).collect();
        Ok(Value::list(found).with_prov(&join_args(&args)))
    }

    fn b_sort(&mut self, args: Vec<Value>, sp: Span) -> R<Value> {
        need("sort", args.len(), 1, sp)?;
        let Value::List(l) = &args[0] else {
            return err(Some("E-rt-arg"), "sort(List)", sp);
        };
        for v in l.iter() {
            if matches!(v, Value::Reading(_)) {
                return err(Some("J-01"), "读数不可比，不能排序", sp);
            }
        }
        let keyed: Vec<(Value, Vec<Value>)> =
            l.iter().map(|v| (v.clone(), vec![v.clone()])).collect();
        let sorted = stable_sort_by_keys("sort", keyed, sp)?;
        Ok(Value::list(sorted).with_prov(&join_args(&args)))
    }

    fn b_sort_by(&mut self, args: Vec<Value>, sp: Span) -> R<Value> {
        need("sort_by", args.len(), 2, sp)?;
        let (Value::List(l), f) = (&args[0], &args[1]) else {
            return err(Some("E-rt-arg"), "sort_by(List, Fn)", sp);
        };
        let mut keyed: Vec<(Value, Vec<Value>)> = vec![];
        for it in l.iter() {
            let k = self.apply(f.clone(), vec![it.clone()], sp)?;
            if matches!(k, Value::Reading(_)) {
                return err(Some("J-01"), "读数不可比，不能作排序键", sp);
            }
            let kv = match &k {
                Value::List(ks) => ks.iter().cloned().collect(),
                other => vec![other.clone()],
            };
            keyed.push((it.clone(), kv));
        }
        let sorted = stable_sort_by_keys("sort_by", keyed, sp)?;
        Ok(Value::list(sorted).with_prov(&join_args(&args)))
    }

    fn b_parse_json(&mut self, args: Vec<Value>, sp: Span) -> R<Value> {
        need("parse_json", args.len(), 1, sp)?;
        let Some(s) = text_of(&args[0]) else {
            return err(Some("E-rt-arg"), "parse_json(Text)", sp);
        };
        let p = join_args(&args);
        match serde_json::from_str::<Json>(&s) {
            Ok(j) => Ok(json_to_value(&j).with_prov(&p)),
            // `with_prov` 没有 `Fail` 分支（`other => other`），手工把 taint/sources 塞进构造，
            // 否则不可信文本解析失败后仍是可信 `Fail`（K-182/K-203 同一类问题）。
            Err(e) => Ok(Value::Fail(
                Rc::from(format!("parse_json: {e}").as_str()),
                p,
            )),
        }
    }

    fn b_to_json(&mut self, args: Vec<Value>, sp: Span) -> R<Value> {
        need("to_json", args.len(), 1, sp)?;
        let out = canon(&args[0].to_json());
        Ok(Value::text(&out).with_prov(&join_args(&args)))
    }

    fn b_hash(&mut self, args: Vec<Value>, sp: Span) -> R<Value> {
        need("hash", args.len(), 1, sp)?;
        let Some(s) = text_of(&args[0]) else {
            return err(Some("E-rt-arg"), "hash(Text)", sp);
        };
        // 与账本同一函数（`jpp_ir::key::hash_of`），位数以它的实现为准（预注册 §一）。
        Ok(Value::text(&hash_of(&[&s])).with_prov(&join_args(&args)))
    }

    fn b_date_parse(&mut self, args: Vec<Value>, sp: Span) -> R<Value> {
        need("date_parse", args.len(), 2, sp)?;
        let (Some(s), Some(fmt)) = (text_of(&args[0]), text_of(&args[1])) else {
            return err(Some("E-rt-arg"), "date_parse(Text, Text)", sp);
        };
        let p = join_args(&args);
        let epoch = chrono::NaiveDateTime::parse_from_str(&s, &fmt)
            .map(|dt| dt.and_utc().timestamp())
            .or_else(|_| {
                chrono::NaiveDate::parse_from_str(&s, &fmt)
                    .ok()
                    .and_then(|d| d.and_hms_opt(0, 0, 0))
                    .map(|dt| dt.and_utc().timestamp())
                    .ok_or(())
            });
        match epoch {
            Ok(secs) => Ok(Value::int(secs).with_prov(&p)),
            Err(()) => Ok(Value::Fail(
                Rc::from(format!("date_parse: 「{s}」不匹配格式「{fmt}」").as_str()),
                p,
            )),
        }
    }

    fn b_date_format(&mut self, args: Vec<Value>, sp: Span) -> R<Value> {
        need("date_format", args.len(), 2, sp)?;
        let (Value::Int(secs, _), Some(fmt)) = (&args[0], text_of(&args[1])) else {
            return err(Some("E-rt-arg"), "date_format(Int, Text)", sp);
        };
        let Some(dt) = chrono::DateTime::from_timestamp(*secs, 0) else {
            return err(
                Some("E-rt-arg"),
                format!("date_format: {secs} 超出可表示范围"),
                sp,
            );
        };
        Ok(Value::text(&dt.format(&fmt).to_string()).with_prov(&join_args(&args)))
    }

    fn b_date_add(&mut self, args: Vec<Value>, sp: Span) -> R<Value> {
        need("date_add", args.len(), 2, sp)?;
        let Value::Int(secs, _) = &args[0] else {
            return err(
                Some("E-rt-arg"),
                "date_add(Int, {days?, hours?, minutes?})",
                sp,
            );
        };
        let field = |k: &str| -> R<i64> {
            match args[1].get(k) {
                None => Ok(0),
                Some(Value::Int(i, _)) => Ok(i),
                Some(other) => err(
                    Some("E-rt-type"),
                    format!("date_add 的 {k} 要是 Int，收到 {}", other.type_name()),
                    sp,
                ),
            }
        };
        let delta = field("days")? * 86_400 + field("hours")? * 3_600 + field("minutes")? * 60;
        Ok(Value::int(secs + delta).with_prov(&join_args(&args)))
    }

    fn b_rand(&mut self, args: Vec<Value>, sp: Span) -> R<Value> {
        need("rand", args.len(), 2, sp)?;
        let (Value::Int(seed, _), Value::Int(k, _)) = (&args[0], &args[1]) else {
            return err(Some("E-rt-arg"), "rand(seed: Int, k: Int)", sp);
        };
        Ok(Value::float(rand_f64(*seed, *k)).with_prov(&join_args(&args)))
    }

    fn b_rand_int(&mut self, args: Vec<Value>, sp: Span) -> R<Value> {
        need("rand_int", args.len(), 3, sp)?;
        let (Value::Int(seed, _), Value::Int(k, _), Value::Int(n, _)) =
            (&args[0], &args[1], &args[2])
        else {
            return err(Some("E-rt-arg"), "rand_int(seed: Int, k: Int, n: Int)", sp);
        };
        if *n <= 0 {
            return err(Some("E-rt-arg"), "rand_int 的 n 要 > 0", sp);
        }
        let v = (mix(*seed, *k) % (*n as u64)) as i64;
        Ok(Value::int(v).with_prov(&join_args(&args)))
    }

    fn b_shuffle(&mut self, args: Vec<Value>, sp: Span) -> R<Value> {
        need("shuffle", args.len(), 2, sp)?;
        let (Value::List(l), Value::Int(seed, _)) = (&args[0], &args[1]) else {
            return err(Some("E-rt-arg"), "shuffle(List, seed: Int)", sp);
        };
        let mut v: Vec<Value> = l.iter().cloned().collect();
        // Fisher–Yates：第 i 步用 `rand(seed, i)`（B158 (1) 字面）。
        let mut i = v.len();
        while i > 1 {
            i -= 1;
            let j = (rand_f64(*seed, i as i64) * (i as f64 + 1.0)) as usize;
            let j = j.min(i);
            v.swap(i, j);
        }
        Ok(Value::list(v).with_prov(&join_args(&args)))
    }
}
