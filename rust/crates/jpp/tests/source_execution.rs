//! End-to-end source execution, not host-language implementations of the examples.
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn run(args: &[&str]) -> Value {
    let result = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(root())
        .args(args)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}

#[test]
fn composed_methods_execute_without_a_fixture() {
    let report = run(&["run", "examples/composition.jpp"]);
    assert_eq!(report["value"]["result"], 43);
    assert_eq!(report["cost"]["calls"], 0);
}

#[test]
fn adaptive_source_constructs_ten_questions() {
    let report = run(&[
        "run",
        "examples/adaptive.jpp",
        "--fixtures",
        "examples/fixtures/adaptive.json",
    ]);
    let value = &report["value"];
    assert_eq!(value["target"], 731);
    assert_eq!(value["candidates_left"], 1);
    assert_eq!(value["questions"], 10);
    assert_eq!(value["pending"], json!([]));
    assert_eq!(report["cost"]["calls"], 10);
    let evidence = value["evidence"].as_array().unwrap();
    let answers: Vec<_> = evidence.iter().map(|o| o["value"].clone()).collect();
    assert_eq!(
        answers,
        vec![
            json!(false),
            json!(true),
            json!(false),
            json!(false),
            json!(false),
            json!(true),
            json!(false),
            json!(false),
            json!(true),
            json!(false)
        ]
    );
}

#[test]
fn partial_continuations_preserve_prior_evidence_and_checks() {
    let report = run(&[
        "run",
        "examples/partial.jpp",
        "--fixtures",
        "examples/fixtures/partial.json",
    ]);
    let value = &report["value"];
    assert_eq!(
        value["used_before_complete"],
        json!({"members":["A","B"],"cost":9})
    );
    assert_eq!(value["initial"]["pending"], json!(["C", "D"]));
    assert_eq!(value["improved"]["best"], json!({"members":["C"],"cost":2}));
    assert_eq!(value["improved"]["pending"], json!(["D"]));
    assert_eq!(value["final"]["best"], value["improved"]["best"]);
    assert_eq!(value["final"]["pending"], json!([]));
    assert_eq!(value["terminal"], true);
    assert_eq!(report["cost"]["calls"], 9, "6 次判断 + 3 次 do（B38）");
    let names: Vec<_> = report["local_checks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].clone())
        .collect();
    assert_eq!(names, vec![json!("A"), json!("B"), json!("C")]);
    let initial = value["initial"]["evidence"].as_array().unwrap();
    let improved = value["improved"]["evidence"].as_array().unwrap();
    let final_evidence = value["final"]["evidence"].as_array().unwrap();
    assert_eq!(initial.len(), 4);
    assert_eq!(improved.len(), 5);
    assert_eq!(final_evidence.len(), 6);
    assert_eq!(&improved[..4], initial);
    assert_eq!(&final_evidence[..5], improved);
}

#[test]
fn ledger_replay_reconstructs_source_methods_without_fresh_effects() {
    let ledger =
        std::env::temp_dir().join(format!("jpp-source-replay-{}.json", std::process::id()));
    let ledger_name = ledger.to_str().unwrap();
    let original = run(&[
        "run",
        "examples/partial.jpp",
        "--fixtures",
        "examples/fixtures/partial.json",
        "--ledger-out",
        ledger_name,
    ]);
    let replay = run(&[
        "run",
        "examples/partial.jpp",
        "--fixtures",
        "examples/fixtures/partial.json",
        "--replay",
        ledger_name,
    ]);
    assert_eq!(replay["value"], original["value"]);
    assert_eq!(replay["cost"]["calls"], 0);
    assert_eq!(replay["local_checks"], json!([]));
    std::fs::remove_file(ledger).unwrap();
}

#[test]
fn check_rejects_source_type_errors_before_run() {
    for file in [
        "examples/errors/type-mismatch.jpp",
        "examples/errors/missing-budget.jpp",
        "examples/errors/question-field-typo.jpp",
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_jpp"))
            .current_dir(root())
            .args(["check", file])
            .output()
            .unwrap();
        assert!(!result.status.success());
        let error = String::from_utf8_lossy(&result.stderr);
        assert!(error.contains(file), "{error}");
        assert!(!error.contains("panicked"), "{error}");
    }
}

#[test]
/// 步 22-0（B93）起预算为零是停发不是挂起：程序照常返回，一次也不请求，报告 `budget.exhausted`
fn zero_budget_returns_without_requesting_an_observation() {
    let source = std::fs::read_to_string(root().join("examples/adaptive.jpp"))
        .unwrap()
        .replacen("calls: 10", "calls: 0", 1);
    let path = std::env::temp_dir().join(format!("jpp-zero-budget-{}.jpp", std::process::id()));
    std::fs::write(&path, source).unwrap();
    let report = run(&[
        "run",
        path.to_str().unwrap(),
        "--fixtures",
        "examples/fixtures/adaptive.json",
    ]);
    assert_eq!(report["status"], "returned");
    assert_eq!(report["cost"]["calls"], 0);
    assert_eq!(report["budget"]["exhausted"], true);
    std::fs::remove_file(path).unwrap();
}

/// 施工件 b：题由题式按填法生成，字段可读，经判断得到出口；未决随返回值交出。
#[test]
fn forms_fill_questions_whose_fields_are_readable() {
    let report = run(&[
        "run",
        "examples/question-forms.jpp",
        "--fixtures",
        "examples/fixtures/question-forms.json",
    ]);
    let v = &report["value"];
    assert_eq!(v["forms"][0]["slots"], json!(["city"]));
    let first = &v["first"];
    assert_eq!(first["text"], "这段话是否提到了成都？");
    assert_eq!(first["template"], "这段话是否提到了{city}？");
    assert_eq!(first["fill"], json!({"city": "成都"}));
    assert_eq!(first["partition"], "binary");
    assert_eq!(first["request"], "whether");
    assert_eq!(first["subject"], "on");
    assert_eq!(first["from_form"], true);
    let trip: Vec<_> = v["trip"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["answer"]["answer"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(trip, vec!["是", "否", "否", "未决", "否"]);
    assert_eq!(report["returned_unsure"], json!(["unsure(band)"]));
}

/// 题作为数据：由题计算出新题（新题面 + 前提），题列表可 filter / map。
#[test]
fn questions_are_data_that_compute_new_questions() {
    let report = run(&[
        "run",
        "examples/question-as-data.jpp",
        "--fixtures",
        "examples/fixtures/question-as-data.json",
    ]);
    let v = &report["value"];
    assert_eq!(v["strict"]["presupposition"], "材料里至少提到一个地名");
    assert_eq!(v["strict"]["from_form"], false);
    assert_eq!(v["dropped"], 1);
    assert_eq!(v["answers"].as_array().unwrap().len(), 2);
    // 步 13b：`map(derived, fn(q){ … verdict(note, q) … })` 穿过 `verdict` 包装向量化，两题同状态合一次调用
    assert_eq!(report["cost"]["calls"], 1);
}

/// 施工件 c：三路过滤。完整输入时三流不漏、互斥，保持输入顺序；重复输入各占一席、只问一次；
/// 同一材料上的多道题合成一次调用（10 段材料去重后 9 次）。
#[test]
fn sieve_splits_every_item_into_exactly_one_stream() {
    let report = run(&[
        "run",
        "examples/sieve.jpp",
        "--fixtures",
        "examples/fixtures/sieve.json",
    ]);
    assert_eq!(report["cost"]["calls"], 9);
    for s in report["value"]["streams"].as_array().unwrap() {
        let idx = |k: &str| -> Vec<i64> {
            s[k].as_array()
                .unwrap()
                .iter()
                .map(|x| {
                    if x.is_object() {
                        x["index"].as_i64().unwrap()
                    } else {
                        x.as_i64().unwrap()
                    }
                })
                .collect()
        };
        let mut all: Vec<i64> = ["act", "ignore", "unsure", "unobserved"]
            .iter()
            .flat_map(|k| idx(k))
            .collect();
        all.sort();
        assert_eq!(all, (0..10).collect::<Vec<_>>(), "{s}");
        assert_eq!(s["complete"], true);
    }
    let chengdu = &report["value"]["streams"][0];
    assert_eq!(chengdu["act"], json!([0, 3, 8, 9]));
    let beijing = &report["value"]["streams"][1];
    assert_eq!(beijing["unsure"], json!([{"index": 2, "cause": "band"}]));
}

/// 预算提前停止：没问到的进 unobserved，不混进 ignore 或 unsure；停止原因写明。
#[test]
fn sieve_budget_stop_reports_unobserved_range() {
    let report = run(&[
        "run",
        "examples/sieve-budget.jpp",
        "--fixtures",
        "examples/fixtures/sieve.json",
    ]);
    assert_eq!(report["status"], "returned");
    assert_eq!(report["cost"]["calls"], 4);
    let s = &report["value"]["streams"][0];
    assert_eq!(s["unobserved"], json!([4, 5, 6, 7, 8]));
    assert!(s["stopped"].as_str().unwrap().contains("预算"));
    assert_eq!(s["ignore"], json!([1, 2]));
}

/// 「14 倍」消除：同一材料 14 道题，交给 sieve 1 次；逐题经函数判断的写法在步 13b 之前 14 次，
/// 之后向量化穿过 `verdict` 包装，同样 1 次（`16` §一；`地基/过程记录/工程-步13b.md`）。
#[test]
fn sieve_fuses_questions_on_one_state() {
    let old = run(&[
        "run",
        "examples/sieve-batch-old.jpp",
        "--fixtures",
        "examples/fixtures/sieve-batch.json",
    ]);
    let new = run(&[
        "run",
        "examples/sieve-batch-new.jpp",
        "--fixtures",
        "examples/fixtures/sieve-batch.json",
    ]);
    assert_eq!(old["cost"]["calls"], 1);
    assert_eq!(new["cost"]["calls"], 1);
}

/// 组合封闭：过滤的产物再过滤，trail 记下上一次的出口；重放新增 0 调用。
#[test]
fn sieve_output_is_sieve_input_and_replays_free() {
    let dir = std::env::temp_dir().join(format!("jpp-sieve-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let ledger = dir.join("ledger.json");
    let report = run(&[
        "run",
        "examples/sieve-compose.jpp",
        "--fixtures",
        "examples/fixtures/sieve.json",
        "--ledger-out",
        ledger.to_str().unwrap(),
    ]);
    let second = &report["value"]["second"];
    assert_eq!(second["act"].as_array().unwrap().len(), 2);
    assert_eq!(second["act"][0]["trail"], json!(["act"]));
    assert_eq!(
        second["ignore"],
        json!(["我们计划明年夏天去成都和北京旅游。"])
    );
    let replay = run(&[
        "run",
        "examples/sieve-compose.jpp",
        "--fixtures",
        "examples/fixtures/sieve.json",
        "--replay",
        ledger.to_str().unwrap(),
    ]);
    assert_eq!(replay["cost"]["calls"], 0);
    assert_eq!(replay["value"]["second"], report["value"]["second"]);
    std::fs::remove_dir_all(dir).unwrap();
}

/// 聚合是精确计算：5 项中 2 act、1 ignore、2 unsure → 计数 [2,4]；存在 = act，全部 = ignore。
/// 输入顺序中的第一个：第 0 项未决时，不能把第 1 项称为「第一个」。
#[test]
fn tally_counts_an_exact_interval_and_first_is_blocked_by_an_earlier_unsure() {
    let report = run(&[
        "run",
        "examples/tally.jpp",
        "--fixtures",
        "examples/fixtures/tally.json",
    ]);
    let v = &report["value"];
    assert_eq!(v["count"], json!([2, 4]));
    assert_eq!(v["complete"], false);
    assert_eq!(v["exists"]["exit"], "act");
    assert_eq!(v["all"]["exit"], "ignore");
    assert_eq!(v["first1"]["exit"]["exit"], "unsure(band)");
    assert_eq!(v["first1"]["blocked_at"], 0);
}

/// 预算停止时未观察的元素计入计数上界，不混进 ignore。
#[test]
fn tally_counts_unobserved_into_the_upper_bound() {
    let report = run(&[
        "run",
        "examples/tally-budget.jpp",
        "--fixtures",
        "examples/fixtures/tally.json",
    ]);
    let v = &report["value"];
    assert_eq!(v["unobserved"], json!([3, 4]));
    assert_eq!(v["count"], json!([1, 4]));
}

/// 配对保留两端原元素；组合作为新材料进入第二轮同一构造；重放新增 0 调用。
#[test]
fn pairs_keep_both_ends_and_a_combination_pairs_again() {
    let dir = std::env::temp_dir().join(format!("jpp-pair-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let ledger = dir.join("ledger.json");
    let report = run(&[
        "run",
        "examples/pair-team.jpp",
        "--fixtures",
        "examples/fixtures/pair-team.json",
        "--ledger-out",
        ledger.to_str().unwrap(),
    ]);
    let v = &report["value"];
    assert_eq!(v["round1"]["pairs"], 12);
    assert_eq!(v["round1"]["count"], json!([3, 5]));
    assert_eq!(
        v["round1"]["act"][0],
        json!({"need": "做一个带登录的网页后台", "person": "王工"})
    );
    assert_eq!(v["round2"]["combo_round1_exit"], json!(["act"]));
    assert_eq!(v["round2"]["act"], json!(["李工"]));
    let replay = run(&[
        "run",
        "examples/pair-team.jpp",
        "--fixtures",
        "examples/fixtures/pair-team.json",
        "--replay",
        ledger.to_str().unwrap(),
    ]);
    assert_eq!(replay["cost"]["calls"], 0);
    assert_eq!(replay["value"]["round2"]["act"], v["round2"]["act"]);
    std::fs::remove_dir_all(dir).unwrap();
}

/// 迭代的终止原因写进结果：材料不再变少时按第二条线停（noshrink），步数用完按 bound 停。
#[test]
fn iterate_reports_why_it_stopped() {
    let report = run(&[
        "run",
        "examples/iterate.jpp",
        "--fixtures",
        "examples/fixtures/iterate.json",
    ]);
    let v = &report["value"];
    assert_eq!(v["by_count"]["reason"], "noshrink");
    assert_eq!(v["by_count"]["measures"], json!([6, 4, 2, 2]));
    assert_eq!(v["by_tokens"]["reason"], "noshrink");
    assert_eq!(v["bound_only"]["reason"], "bound");
}

fn fails(args: &[&str]) -> String {
    let result = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(root())
        .args(args)
        .output()
        .unwrap();
    assert!(
        !result.status.success(),
        "应当失败：{}",
        String::from_utf8_lossy(&result.stdout)
    );
    String::from_utf8_lossy(&result.stderr).to_string()
}

/// 施工件 i（B17 不变量 1）：sieve / pair / 调用者构造的 outcome / tally 的产物是同一类型，
/// 可以直接交给下一个构造；调用者构造的契约值再进 pair 做第二轮；续接方法可调用；重放新增 0 调用。
#[test]
fn every_construction_returns_the_same_contract_and_composes_again() {
    let dir = std::env::temp_dir().join(format!("jpp-contract-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let ledger = dir.join("ledger.json");
    let report = run(&[
        "run",
        "examples/contract.jpp",
        "--fixtures",
        "examples/fixtures/contract.json",
        "--ledger-out",
        ledger.to_str().unwrap(),
    ]);
    let v = &report["value"];
    assert_eq!(v["same_type"], true);
    let kinds: Vec<&str> = v["stages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        [
            "sieve", "pair", "sieve", "outcome", "sieve", "sieve", "tally"
        ]
    );
    // 未决随包转移：第 1 步的未决（钱设计）经 pair、sieve 带到调用者构造的契约值里
    assert_eq!(v["stages"][1]["pending"], 1);
    assert_eq!(v["stages"][3]["pending"], 2);
    assert_eq!(v["second_round"]["act"], json!(["王工"]));
    assert_eq!(v["recheck"]["ignore"], 1);
    let replay = run(&[
        "run",
        "examples/contract.jpp",
        "--fixtures",
        "examples/fixtures/contract.json",
        "--replay",
        ledger.to_str().unwrap(),
    ]);
    assert_eq!(replay["cost"]["calls"], 0);
    assert_eq!(replay["value"], report["value"]);
    std::fs::remove_dir_all(dir).unwrap();
}

/// B17 不变量 2：带未决的契约值绑定后不用 = 静默丢弃，检查阶段报 J-05。
#[test]
fn dropping_a_contract_with_pending_is_rejected_by_check() {
    let err = fails(&["check", "examples/errors/outcome-dropped.jpp"]);
    assert!(err.contains("J-05") && err.contains("契约值 r"), "{err}");
}

/// B17 不变量 3：契约的证据只存账本键；给观察副本会被拒绝。
#[test]
fn contract_evidence_is_ledger_keys_only() {
    let report = run(&[
        "run",
        "examples/contract.jpp",
        "--fixtures",
        "examples/fixtures/contract.json",
    ]);
    assert!(report["value"]["stages"][0]["evidence"].as_i64().unwrap() > 0);
    let dir = std::env::temp_dir().join(format!("jpp-evidence-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("copy.jpp");
    std::fs::write(&src, "budget {calls: 1, cost: 0, depth: 64};\noutcome({value: 1, evidence: [{material: \"一段材料\", p: 0.9}]})\n").unwrap();
    let err = fails(&["run", src.to_str().unwrap()]);
    assert!(err.contains("E-evidence"), "{err}");
    std::fs::remove_dir_all(dir).unwrap();
}
