use std::path::PathBuf;

pub const HELP: &str = "J++ native source tools\nUsage:\n  jpp parse <file.jpp> [--ast]\n  jpp check <file.jpp> [--json] [--input <file.json>] [--input-trusted] [--questions-out <file.json>]\n  jpp run <file.jpp> [--json] [--input <file.json>] [--input-trusted] [--fixtures <file.json>] [--output <report.json>]\n          [--ledger-out <ledger.json>] [--replay <ledger.json> | --resume <ledger.json>]
          [--profile <profile.json>] [--calib <calib-dir>] [--calib-out <calib-dir>]
          [--backend fixed|live|stub] [--model <name>] [--profiles-dir <dir>]\n  jpp calib-import <labels.jsonl> --calib-out <calib-dir> [--calib <calib-dir>] [--profile <profile.json>]\n          [--alpha 0.1] [--conf-delta 0.1] [--spot-check-min 0.9] [--spot-check-conf 0.95] [--abstain-warn 0.1] [--seed 20260923]\n          [--extent-min-disagree 3] [--extent-same-dir 0.8] [--extent-same-tier 0.667] [--scope-quantiles 0.01,0.99] [--scope-margins 2,0.10] [--class-min-sources 2] [--alpha-trial 0.25]\n          [--certify fixed-sequence|split|sequential] [--step <n>] [--cost fp,fn]\n          [--batch 10] [--order random|two-ends] [--coverage-target <tau>] [--mix-weights 0.8,0.1,0.05,0.05] [--from-ledger <ledger>]\n  jpp calib-import --from-ledger <ledger.jsonl> --key <key> --list-out <list.jsonl> [--materials <texts.json>] [--report <report.json>] [--seed <n>]\n  jpp calib-import <labels.jsonl> --calib <calib-dir> --extend-scope <key> --calib-out <calib-dir> [--alpha-trial 0.25] [--scope-quantiles 0.01,0.99] [--scope-margins 2,0.10]
  jpp calib-confirm <calib-dir> <key> --suspend|--keep\n  jpp ledger-migrate <v2-ledger> <v3-out>\n\nLeading relative imports load source libraries. --json makes diagnostics machine-readable, one JSON object each: {code, level, span {file, line, col, start, end}, message, fix, applicability manual|wiring|null, count}, plus explain for runtime codes E-rt-<name>; check --json prints one document {file, ok, errors, warnings, diagnostics} on stdout, run --json writes diagnostics as JSON lines (each starting with {) on stderr and leaves the report unchanged. Diagnostics with the same code, location and message are folded into one with a count (text output appends （同码同址 ×N）). --input binds the JSON file's value to the name input in the program (a host binding outside the program's outermost block; a program's own let input shadows it); its content is untrusted by default (every leaf, as for read_json), and the hash of the whole entry (name, canonical JSON and declared taint) goes into the ledger header as entry_hash, so a replay or resume must be given the same --input (a different one reports W-header: entry_hash). --input-trusted declares that file's content trusted (B108; requires --input, else a usage error); it only says the file comes from a source the host trusts, not that its content is correct, and it goes into entry_hash like any other declared taint, so replaying with a different --input-trusted state reports W-header: entry_hash. check accepts --input and --input-trusted only to know that input is bound and how it is declared. check --questions-out <file.json> writes the program's literal questions (test, select and measure whose question text, label and declarations are literals), grouped by calibration label, each with its zero-slot form hash, for migrating label-named calibration records (B116); it is written once the program lowers, even if the check then reports errors, and calls whose parts are not literals are listed as skipped with a reason. Material entries and purpose are given only through Session (the Rust API). Run defaults to fixed generation/judgment/response records; no model API requests are made. --backend live switches to the real JEV backend (JevClient::live), reading the API key only from ~/.typesafe-key (never logged or written to any report); it requires jpp to be built with `--features live` and is mutually exclusive with --fixtures. --backend stub is a deterministic stand-in judge (model stub-0) for checking that a program runs unchanged on a second judge: its readings come from a hash of the state and the question, it makes no requests, does not generate, and needs a profile like any backend (the repository keeps a stand-in profile at tests/profile_swap/stub-0.json; it is not a measurement). --model names the model for --backend live (default jev-1.13.0). Registered actions: {actions}. --profile loads a model profile (lines, deltas, windows, prices, class-assumption fields). A live run (first run or --resume) must have a profile (B73): --profile <file>, else --profiles-dir <dir>/<model>.json, else profiles/<model>.json next to the jpp executable (releases ship profiles/); if none is found the run stops with E-profile-missing listing the paths tried, and never falls back to code defaults. The profile hash goes into the ledger header, and the price comes only from the profile's cost field (no price: cost is reported as Unknown with W-cost-unknown). Replay makes no requests. A certificate records the delta it was certified with (B104): cut uses the stricter of that delta and the run-time delta when a profile is loaded, and the certificate's delta when none is, so a line is never widened by a delta mismatch; either way a mismatch reports W-delta-mismatch. To reproduce a live run byte for byte, replay with the same profile. A fixed-observation run without a profile still runs until step 15d and always prints that no profile is loaded and lines and deltas are code defaults. --calib loads calibration records from a directory of per-key JSON files. --calib-out folds this run's readings into those records and writes them back, which is the only way the calibration loop closes: J-03 forbids a program from writing a line itself. File paths use the working directory. Resume may perform unrecorded actions; replay rejects them. Replay restores the calibration records the ledger recorded as used when they are not supplied again. Ledgers are format v3 (the calibration records a run used are CalibUsed entries, the last one per key counts); a v2 ledger is migrated in memory when read (a note is printed, the file is not rewritten), and ledger-migrate rewrites it as v3. calib-import is the truth channel: it folds labelled readings (JSONL: key or form, item, p, label true|false|\"ambiguous\", source human|computed|model:<name>, optional spot_check review-batch id, optional generator, optional q and fill, optional kind or slot_shape one|pair with over_kind; conflicting kinds on one key are rejected with E-kind-conflict) into calibration records and certifies them two-sided. --certify picks the method (new imports only; existing records are left as they are). --cost fp,fn (B129) certifies a cost line instead: the labelled rows are split in half (B85 stratified alternating split, same as split; --seed picks where each segment starts), the line minimizes fp × false releases + fn × missed releases on the selection half, and the certificate checks that line against --alpha (then --alpha-trial) on the certification half only, not the rows used to pick it; it takes test rows only, is one-sided (below the line exits are unsure(band), not ignore), cannot be combined with --certify, --order, --step, --batch, --coverage-target, --mix-weights, --extend-scope or --list-out, and is refused on a key that already holds a certificate not made from a cost; cut(r, {cost: [fp, fn]}) uses exactly that certificate. Because certification only sees half the labelled rows, roughly twice as many are needed to reach the same sample-size floor as an unsplit line. fixed-sequence (default, B86) does not split the rows: candidate thresholds come from the readings alone (the upper side's j-th candidate is the lower edge of the n_needed + j*s highest readings, ties widened to the group edge, taking the group value and the midpoint to the next value; the lower side mirrors it; s = --step, default 5% of the rows, at least 1), they are tested from strictest to widest with the Clopper-Pearson bound, testing stops at the first failure, and the pair with the most decided rows is taken from the two passing prefixes; the family error per side stays at most --conf-delta, the same level as split certification, without spending half the rows. (Choosing the widest passing threshold on the same rows without this order is not proven; on nested threshold families its measured inflation is only about twofold, so most of what the split's discarded half bought was a guarantee with a proof, not protection from gross overfitting — the fixed order gives the proof for free.) The certificate records the method, the candidate rule version, s, delta, how many candidates each side generated and where it stopped, so a later load can rerun it from the labels. split (B85) is the older split-sample method, now with a stratified alternating split: in canonical order the negative rows and the positive rows each alternate between the selection half and the certification half, --seed only picks where each segment starts (per source for class rows); a pair is chosen on one half and certified once on the other. sequential (B87) lets you label in batches and import as you go (label, import, and stop when it is enough): rows arrive in --batch sized batches, every candidate threshold keeps a mixture e-process (alternatives 0, alpha/4, alpha/2, 3alpha/4 weighted by --mix-weights), a candidate is rejected once its e-value reaches 1/--conf-delta, which stays valid under any stopping time; by default it stops where labelling more could not widen the line (settled; the pair is never wider than fixed-sequence's, and a zero-error side needs 24 rows at alpha 0.1, 10 at 0.25), or at the narrowest legal pair whose coverage of the sampled readings reaches --coverage-target; if it has not stopped the gate reports how many rows are labelled, each side's e-value, the current and reachable coverage and roughly how many more zero-error rows are needed. The arrival order is random (a seeded permutation, --order random) unless the rows come from a to-label list. --from-ledger <ledger> --key <key> --list-out <list> writes that list (B88) from a first run's ledger: every reading of that key is the sampling frame, rows are grouped from both ends inward (group upper1, lower1, upper2, ..., rest; random within a group, seeded), and each row carries only item (the material's state hash), q (the question's hash: one material can be asked several questions, B107), group and, with --materials, the material text, with --report <report.json> (the run's report) the question's template and fill - never the reading or an exit, so the labeller cannot lean on the judge; a key asked with more than one question and no --report warns W-list-no-question. Importing labels with --from-ledger joins each row's reading back from the ledger by (item, q) (a row without q whose item has more than one reading under that key is rejected with E-list-ambiguous), and with --report takes the question kind from the report (B120), defaults to --certify sequential with the two-ends order, and requires the labelled rows to be a prefix of the list; with that order only the first group's candidates are judged on partial labels, a wider candidate is judged only once all of its rows are labelled. A sequential import must give all labelled rows of the key at once (import the cumulative file each time, without --calib). truth per item is taken in the order computed > human > review row > annotation row (B36; a review row is any row carrying spot_check, human or model:<name>; a review from the same source as that item's annotation or from the material generator is rejected); model-only truth is certified only when a same-key review batch reaches --spot-check-min, and the gate names model reviewers: the point estimate below it keeps the record pending; a point estimate at or above it whose one-sided --spot-check-conf lower bound is still below it certifies the line provisionally (gate \"临时上岗\", W-provisional at use) and reports how many more all-agreeing checks would confirm it. When review disagreements reach --extent-min-disagree and their direction agrees at --extent-same-dir or higher (or, with fill_tier on the rows, --extent-same-tier or more fall in one fill tier), the question's scope is judged undetermined (B36 5(c)): the record stays pending with the reason in the gate and no further review is requested; rewrite the question first. select and measure rows (B63) add op select|measure (or use a form with that op), p = the winning candidate's or level's probability, pick = the reading's argmax index, and label = the true candidate index (over order) or level index (scale order); they are certified one-sided on p_max (Pick/At only when p_max >= hi + delta; there is no low side). Two certification grades (B72): the key is first certified at --alpha (formal grade); only if that fails (certification refused or too few rows) is it certified again at --alpha-trial (default 0.25; a value not above --alpha disables it), and the certificate is marked trial. A trial line routes act/ignore like any line but reports W-trial-line and never releases an irreversible action; a trial import never overwrites a key that already holds a certified formal line. When model labels supply the truth (B89) the certificate records alpha_eff, the false-release bound relative to the reviewer: alpha when review rows cover every certified row in the decided region, alpha + (1 - a_lb) when the reviewed rows inside the decided region give the one-sided agreement lower bound a_lb, alpha + (1 - a_lb)/c for older records whose review batch was not drawn from that region (c = the share of certified rows in it); a line whose alpha_eff exceeds its alpha is graded trial (routes, never releases an irreversible action), and the record names its truth baseline (model:<reviewer> or human). To put model labels on a formal line, have the review cover the certification set. Review rows (rows with spot_check) must not carry p, pick, exit or reading - the reviewer may not see the judge's answer - and are rejected with E-review-leak; their reading is joined from the same item's annotation row. Sample size (fixed-sequence; each side's first candidate needs 22 zero-error decided rows at alpha 0.1 and 9 at alpha 0.25): a formal line (may release an irreversible do) needs about 60 labelled rows for a literal question form and about 60–80 for a semantic one — a form whose scope is undetermined should be rewritten before labelling; a trial line (routes only) needs about 32 rows, about 40 for a form whose readings are spread out. --certify split needs about 2–3 times as many. Measured offline: 地基/评估/2026-09-24-新题标注门槛-对照/results.md and 裁定复算/recompute.out.txt. When every certified row carries text (the judged material), the record stores a material fingerprint of the certification set (B68: character count, Chinese / Latin / digit / punctuation-and-space ratios, line count, mean line length, each as a --scope-quantiles interval widened by --scope-margins k,m: count-like quantities are divided/multiplied by k, ratios are widened by m and clipped to [0, 1]; the import prints how many certification rows fall outside their own range, which must be 0, else W-scope-self); at run time a line used on material outside that range still routes but reports W-calib-scope and cannot release an irreversible action. --extend-scope <key> (B91) extends that range to a new material style: label rows of the new style (p, label, text on every row) test the record's existing line pair once per side, without choosing a new line; each side needs as many zero-error decided rows as certification does (22 formal, 9 trial) and a binomial bound at most alpha, first at the record's alpha, else at --alpha-trial. If it passes, the batch's fingerprint is added to the record's scope.extensions; exits on material in an extension no longer count as out of scope, and a trial-level extension grades them Trial (W-scope-extension). A 10-row review never clears out-of-scope. Re-importing the key replaces the line and drops its extensions. A record without a fingerprint has an unknown scope (B104): its exits still route but never release an irreversible action (W-scope-unknown); when only some certified rows carry text, the fingerprint is built from those rows and the record notes how many (n_text). A certificate whose line was shifted by the certification bandwidth but that did not record it (an old split-sample certificate) likewise routes but never releases (W-delta-unknown) until it is re-imported or a later load reruns it. A row with class <label> (B34) goes to the class record of that calibration class instead of its own key; the sample's source is the row's form (its form hash), else the hash of its question text (a hand-written question), else its key — different fills of one form are one source (B75). A class record is certified only when the batch mixes at least --class-min-sources distinct sources and each source has at least the grade's zero-error row count (22 formal, 9 trial); under --certify split the halves are also stratified by source, and the record lists its sources (else it stays pending with the reason in the gate). At run time a question whose own key and form have no certified line borrows the class line of the key it was written with (W-class-line); a class line routes but does not release an irreversible action (B75). calib-confirm is the human confirmation of a suspension candidate (B25): a drift signal marks a certified line as a candidate (its exits still route but cannot release an irreversible action), --calib-out writes the candidate status, and --suspend or --keep settles it.";

/// 帮助文本：[`HELP`] 模板里的 `{actions}` 填入动作表的清单（比赛块 C-1：动作清单与注册同源）。
pub fn help() -> String {
    HELP.replace("{actions}", &jpp::actions::usage_list())
}

// 默认模型名步 15g-0 起在注册表条目里（`jpp::backends::jev::SPEC.default_model`）。

#[derive(Debug, PartialEq)]
pub enum Command {
    Help,
    Parse {
        source: PathBuf,
        ast: bool,
    },
    Check {
        source: PathBuf,
        input: Option<PathBuf>,
        /// 宿主声明 `--input` 可信（步 14b-1，B108）：只在给了 `input` 时有意义
        input_trusted: bool,
    },
    Run(RunOptions),
    CalibImport(ImportArgs),
    CalibConfirm {
        dir: PathBuf,
        key: String,
        suspend: bool,
    },
    /// 账本 v2 → v3（步 18a，B124 Q3）
    LedgerMigrate {
        from: PathBuf,
        to: PathBuf,
    },
}

#[derive(Debug, PartialEq)]
pub struct ImportArgs {
    /// 标注文件；只导出待标清单（`--list-out`）时没有
    pub labels: Option<PathBuf>,
    pub calib: Option<PathBuf>,
    /// 导入时必给；只导出清单时没有
    pub calib_out: Option<PathBuf>,
    pub profile: Option<PathBuf>,
    pub alpha: f64,
    pub conf_delta: f64,
    pub spot_check_min: f64,
    pub spot_check_conf: f64,
    pub abstain_warn: f64,
    pub seed: u64,
    pub extent_min_disagree: usize,
    pub extent_same_dir: f64,
    pub extent_same_tier: f64,
    pub scope_quantiles: (f64, f64),
    pub scope_margins: (f64, f64),
    pub class_min_sources: usize,
    pub alpha_trial: f64,
    /// B86 / B85：`fixed-sequence`（缺省）或 `split`
    pub certify: jpp::truth::CertifyMethod,
    /// B86：固定序步长；`None` = 池的 5%
    pub step: Option<usize>,
    /// 显式给了 `--certify`（`--from-ledger` 回填时缺省改为 sequential）
    pub certify_explicit: bool,
    /// B87：每批条数、顺序（random / two-ends）、覆盖目标、混合权重
    pub batch: usize,
    pub order: Option<String>,
    pub coverage_target: Option<f64>,
    pub weights: [f64; 4],
    /// B88：首跑账本（抽样框）、键、待标清单输出、材料文本（JSON 字符串数组）
    pub from_ledger: Option<PathBuf>,
    pub key: Option<String>,
    pub list_out: Option<PathBuf>,
    pub materials: Option<PathBuf>,
    /// B107（步 20h-2）：首跑的报告（`questions` 表）：清单行附题面与填法，回填时标注行附题类（B120 (a)）
    pub report: Option<PathBuf>,
    /// B91（步 20d-2）：范围扩展认证的键（`--calib` 里已有的认证线）
    pub extend_scope: Option<String>,
}

fn parse_import(args: &[String]) -> Result<Command, String> {
    let labels = args
        .get(1)
        .filter(|s| !s.starts_with("--"))
        .map(PathBuf::from);
    let (mut calib, mut calib_out, mut profile) = (None, None, None);
    let (mut alpha, mut conf_delta, mut spot_check_min, mut abstain_warn) = (0.1, 0.1, 0.9, 0.1);
    let mut spot_check_conf = 0.95;
    let mut seed: u64 = 20260923;
    let (mut extent_min_disagree, mut extent_same_dir, mut extent_same_tier) =
        (3usize, 0.8, 2.0 / 3.0);
    let mut scope_quantiles = (0.01, 0.99);
    let mut scope_margins = (2.0, 0.10);
    // B75：类记录「混合样本」的来源数下限（来源 = 题式，填法不算不同来源）
    let mut class_min_sources = 2usize;
    // B72：试用 α（正式 α 不过时再认证一次；不大于 --alpha 即不试）
    let mut alpha_trial = 0.25;
    // B86：缺省固定序认证（不拆分）；B85：split 为分层交替分半；序贯（B87）落步 20h
    let mut certify = jpp::truth::CertifyMethod::FixedSequence;
    let mut step: Option<usize> = None;
    let mut certify_explicit = false;
    // B87：每批 10 条、混合权重 (0.8, 0.1, 0.05, 0.05) 于 p₁ ∈ {0, α/4, α/2, 3α/4}；缺省停在 settled
    let (mut batch, mut order, mut coverage_target) = (10usize, None::<String>, None::<f64>);
    let mut weights = [0.8, 0.1, 0.05, 0.05];
    // B88：首跑账本作抽样框
    let (mut from_ledger, mut key, mut list_out, mut materials) = (None, None, None, None);
    let mut report = None;
    let mut extend_scope = None;
    // B129（步 20a-2a）：`--cost fp,fn` 是一种认证方式（代价线）；给了哪些与它无关的认证参数要记下来，
    // 同给即用法错误（静默忽略就是吞字段）
    let mut cost: Option<(f64, f64)> = None;
    let mut 给了: Vec<String> = vec![];
    let mut i = if labels.is_some() { 2 } else { 1 };
    while i < args.len() {
        let v = args
            .get(i + 1)
            .filter(|s| !s.starts_with("--"))
            .ok_or_else(|| format!("{} requires a value", args[i]))?;
        let num = |x: &str| {
            x.parse::<f64>()
                .map_err(|_| format!("{} expects a number, got {x}", args[i]))
        };
        给了.push(args[i].clone());
        match args[i].as_str() {
            "--calib" => calib = Some(PathBuf::from(v)),
            "--calib-out" => calib_out = Some(PathBuf::from(v)),
            "--profile" => profile = Some(PathBuf::from(v)),
            "--cost" => {
                let bad = || {
                    format!(
                        "--cost expects two positive numbers fp,fn (the cost of a false release and of a missed one), got {v}"
                    )
                };
                let (a, b) = v.split_once(',').ok_or_else(bad)?;
                let (a, b) = (
                    a.trim().parse::<f64>().map_err(|_| bad())?,
                    b.trim().parse::<f64>().map_err(|_| bad())?,
                );
                if !(a > 0.0 && b > 0.0 && a.is_finite() && b.is_finite()) {
                    return Err(bad());
                }
                cost = Some((a, b));
            }
            "--alpha" => alpha = num(v)?,
            "--conf-delta" => conf_delta = num(v)?,
            "--spot-check-min" => spot_check_min = num(v)?,
            "--spot-check-conf" => spot_check_conf = num(v)?,
            "--abstain-warn" => abstain_warn = num(v)?,
            "--extent-min-disagree" => {
                extent_min_disagree = v.parse::<usize>().map_err(|_| {
                    format!("--extent-min-disagree expects a non-negative integer, got {v}")
                })?
            }
            "--extent-same-dir" => extent_same_dir = num(v)?,
            "--extent-same-tier" => extent_same_tier = num(v)?,
            "--scope-quantiles" => {
                let bad = || {
                    format!(
                        "--scope-quantiles expects two numbers lo,hi in [0, 1] with lo < hi, got {v}"
                    )
                };
                let (a, b) = v.split_once(',').ok_or_else(bad)?;
                let (a, b) = (
                    a.trim().parse::<f64>().map_err(|_| bad())?,
                    b.trim().parse::<f64>().map_err(|_| bad())?,
                );
                if !(0.0 <= a && a < b && b <= 1.0) {
                    return Err(bad());
                }
                scope_quantiles = (a, b);
            }
            "--scope-margins" => {
                let bad =
                    || format!("--scope-margins expects k,m with k >= 1 and 0 <= m < 1, got {v}");
                let (a, b) = v.split_once(',').ok_or_else(bad)?;
                let (a, b) = (
                    a.trim().parse::<f64>().map_err(|_| bad())?,
                    b.trim().parse::<f64>().map_err(|_| bad())?,
                );
                if !(a >= 1.0 && (0.0..1.0).contains(&b)) {
                    return Err(bad());
                }
                scope_margins = (a, b);
            }
            "--class-min-sources" => {
                class_min_sources = v.parse::<usize>().map_err(|_| {
                    format!("--class-min-sources expects a non-negative integer, got {v}")
                })?
            }
            "--alpha-trial" => alpha_trial = num(v)?,
            "--certify" => {
                certify_explicit = true;
                certify = match v.as_str() {
                    "fixed-sequence" => jpp::truth::CertifyMethod::FixedSequence,
                    "split" => jpp::truth::CertifyMethod::Split,
                    "sequential" => jpp::truth::CertifyMethod::Sequential,
                    other => {
                        return Err(format!(
                            "--certify expects fixed-sequence|split|sequential, got {other}"
                        ));
                    }
                }
            }
            "--step" => {
                step = Some(
                    v.parse::<usize>()
                        .ok()
                        .filter(|n| *n >= 1)
                        .ok_or_else(|| format!("--step expects a positive integer, got {v}"))?,
                )
            }
            "--batch" => {
                batch = v
                    .parse::<usize>()
                    .ok()
                    .filter(|n| *n >= 1)
                    .ok_or_else(|| format!("--batch expects a positive integer, got {v}"))?
            }
            "--order" => {
                if v != "random" && v != "two-ends" {
                    return Err(format!("--order expects random|two-ends, got {v}"));
                }
                order = Some(v.clone());
            }
            "--coverage-target" => {
                let t = num(v)?;
                if !(0.0..=1.0).contains(&t) {
                    return Err(format!(
                        "--coverage-target expects a number in [0, 1], got {v}"
                    ));
                }
                coverage_target = Some(t);
            }
            "--mix-weights" => {
                let bad = || {
                    format!("--mix-weights expects four non-negative numbers summing to 1, got {v}")
                };
                let xs: Vec<f64> = v
                    .split(',')
                    .map(|x| x.trim().parse::<f64>())
                    .collect::<Result<_, _>>()
                    .map_err(|_| bad())?;
                if xs.len() != 4
                    || xs.iter().any(|x| *x < 0.0)
                    || (xs.iter().sum::<f64>() - 1.0).abs() > 1e-9
                {
                    return Err(bad());
                }
                weights = [xs[0], xs[1], xs[2], xs[3]];
            }
            "--from-ledger" => from_ledger = Some(PathBuf::from(v)),
            "--key" => key = Some(v.clone()),
            "--list-out" => list_out = Some(PathBuf::from(v)),
            "--materials" => materials = Some(PathBuf::from(v)),
            "--report" => report = Some(PathBuf::from(v)),
            "--extend-scope" => extend_scope = Some(v.clone()),
            "--seed" => {
                seed = v
                    .parse::<u64>()
                    .map_err(|_| format!("--seed expects a non-negative integer, got {v}"))?
            }
            other => return Err(format!("unknown calib-import option '{other}'")),
        }
        i += 2;
    }
    if let Some((fp, fn_)) = cost {
        // 依据：B129（代价线是自己的认证方式；这些参数对它无消费者）
        const 不相干: [&str; 8] = [
            "--certify",
            "--order",
            "--step",
            "--batch",
            "--coverage-target",
            "--mix-weights",
            "--extend-scope",
            "--list-out",
        ];
        if let Some(o) = 给了.iter().find(|o| 不相干.contains(&o.as_str())) {
            return Err(format!(
                "--cost cannot be combined with {o}: a cost line is its own certification method (B129)"
            ));
        }
        certify = jpp::truth::CertifyMethod::Cost(fp, fn_);
        certify_explicit = true;
    }
    if list_out.is_some() {
        if from_ledger.is_none() || key.is_none() {
            return Err(
                "calib-import --list-out requires --from-ledger <ledger> and --key <key>".into(),
            );
        }
    } else {
        if labels.is_none() {
            return Err("calib-import requires a labels file (JSONL)".into());
        }
        if calib_out.is_none() {
            return Err("calib-import requires --calib-out <dir>".into());
        }
    }
    Ok(Command::CalibImport(ImportArgs {
        labels,
        calib,
        calib_out,
        profile,
        alpha,
        conf_delta,
        spot_check_min,
        spot_check_conf,
        abstain_warn,
        seed,
        extent_min_disagree,
        extent_same_dir,
        extent_same_tier,
        scope_quantiles,
        scope_margins,
        class_min_sources,
        alpha_trial,
        certify,
        step,
        certify_explicit,
        batch,
        order,
        coverage_target,
        weights,
        from_ledger,
        key,
        list_out,
        materials,
        report,
        extend_scope,
    }))
}

/// 观察后端：默认固定观察（不越界）；其余取值来自后端注册表（步 15g-0，`jpp::backends::REGISTRY`；
/// `live` 接真机 `JevClient`）。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Backend {
    #[default]
    Fixed,
    Registered(&'static jpp::backends::BackendSpec),
}

impl Backend {
    /// 注册后端的条目；固定观察为 `None`
    pub fn spec(&self) -> Option<&'static jpp::backends::BackendSpec> {
        match self {
            Backend::Fixed => None,
            Backend::Registered(s) => Some(s),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct RunOptions {
    pub source: PathBuf,
    pub fixtures: Option<PathBuf>,
    pub output: Option<PathBuf>,
    pub ledger_out: Option<PathBuf>,
    pub replay: Option<PathBuf>,
    pub resume: Option<PathBuf>,
    pub profile: Option<PathBuf>,
    pub calib: Option<PathBuf>,
    pub calib_out: Option<PathBuf>,
    pub backend: Backend,
    pub model: Option<String>,
    /// 真机画像目录（B73）：没给 `--profile` 时按 `<目录>/<model>.json` 找；缺省为可执行文件旁的 `profiles/`。
    pub profiles_dir: Option<PathBuf>,
    /// 宿主入口材料（步 14b-0）：JSON 文件，以名字 `input` 绑定给程序，叶子 untrusted
    pub input: Option<PathBuf>,
    /// 宿主声明 `--input` 可信（步 14b-1，B108）：只在 `input` 给了时有意义
    pub input_trusted: bool,
}

/// 取出 `--json`（步 9a）：只有 `check` 与 `run` 收它，出现一次；其余参数原样交给 [`parse`]。
pub fn take_json(args: &mut Vec<String>) -> Result<bool, String> {
    let n = args.iter().skip(1).filter(|a| *a == "--json").count();
    if n == 0 {
        return Ok(false);
    }
    if !matches!(args.first().map(String::as_str), Some("check" | "run")) {
        return Err("--json is accepted only by check and run".into());
    }
    if n > 1 {
        return Err("--json was supplied twice".into());
    }
    let i = args.iter().skip(1).position(|a| a == "--json").unwrap() + 1;
    args.remove(i);
    Ok(true)
}

/// `check --questions-out <file.json>`（步 20a-2b，B116 (5)）：导出程序里的字面题，供校准键迁移
/// （20a-2e，`jpp calib-migrate --questions`）算零槽题式哈希。与 `--json` 同法在解析前取出，
/// `check` 其余参数的解析与报文不变；只有 `check` 收它。
pub fn take_questions_out(args: &mut Vec<String>) -> Result<Option<PathBuf>, String> {
    let n = args
        .iter()
        .skip(1)
        .filter(|a| *a == "--questions-out")
        .count();
    if n == 0 {
        return Ok(None);
    }
    if args.first().map(String::as_str) != Some("check") {
        return Err("--questions-out is accepted only by check".into());
    }
    if n > 1 {
        return Err("--questions-out was supplied twice".into());
    }
    let i = args
        .iter()
        .skip(1)
        .position(|a| a == "--questions-out")
        .unwrap()
        + 1;
    let path = args
        .get(i + 1)
        .filter(|p| !p.starts_with("--"))
        .cloned()
        .ok_or("--questions-out requires a value <file.json>")?;
    args.drain(i..=i + 1);
    Ok(Some(PathBuf::from(path)))
}

pub fn parse(args: &[String]) -> Result<Command, String> {
    if args.is_empty() || args == ["--help"] || args == ["-h"] {
        return Ok(Command::Help);
    }
    let verb = args[0].as_str();
    if verb == "calib-import" {
        return parse_import(args);
    }
    if verb == "ledger-migrate" {
        let (Some(from), Some(to), None) = (args.get(1), args.get(2), args.get(3)) else {
            return Err("ledger-migrate requires <v2-ledger> <v3-out>".into());
        };
        return Ok(Command::LedgerMigrate {
            from: from.into(),
            to: to.into(),
        });
    }
    if verb == "calib-confirm" {
        let dir = args
            .get(1)
            .ok_or("calib-confirm requires <calib-dir> <key> --suspend|--keep")?;
        let key = args
            .get(2)
            .ok_or("calib-confirm requires <calib-dir> <key> --suspend|--keep")?;
        let suspend = match args.get(3).map(String::as_str) {
            Some("--suspend") => true,
            Some("--keep") => false,
            _ => return Err("calib-confirm requires --suspend or --keep".into()),
        };
        return Ok(Command::CalibConfirm {
            dir: dir.into(),
            key: key.clone(),
            suspend,
        });
    }
    if !matches!(verb, "parse" | "check" | "run") {
        return Err(format!("unknown command '{verb}'"));
    }
    let source = args
        .get(1)
        .filter(|s| !s.starts_with("--"))
        .ok_or_else(|| format!("{verb} requires a .jpp source file"))?;
    if verb == "parse" {
        return match &args[2..] {
            [] => Ok(Command::Parse {
                source: source.into(),
                ast: false,
            }),
            [flag] if flag == "--ast" => Ok(Command::Parse {
                source: source.into(),
                ast: true,
            }),
            _ => Err("parse accepts only --ast after the source file".into()),
        };
    }
    if verb == "check" {
        return match &args[2..] {
            [] => Ok(Command::Check {
                source: source.into(),
                input: None,
                input_trusted: false,
            }),
            [flag, path] if flag == "--input" && !path.starts_with("--") => Ok(Command::Check {
                source: source.into(),
                input: Some(path.into()),
                input_trusted: false,
            }),
            [flag, path, trusted]
                if flag == "--input" && !path.starts_with("--") && trusted == "--input-trusted" =>
            {
                Ok(Command::Check {
                    source: source.into(),
                    input: Some(path.into()),
                    input_trusted: true,
                })
            }
            [trusted] if trusted == "--input-trusted" => {
                Err("check --input-trusted requires --input <file.json>".into())
            }
            _ => Err(
                "check accepts only a source file, --input <file.json> and --input-trusted".into(),
            ),
        };
    }
    let mut options = RunOptions {
        source: source.into(),
        fixtures: None,
        output: None,
        ledger_out: None,
        replay: None,
        resume: None,
        profile: None,
        calib: None,
        calib_out: None,
        backend: Backend::Fixed,
        model: None,
        profiles_dir: None,
        input: None,
        input_trusted: false,
    };
    let mut backend_set = false;
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--backend" => {
                if backend_set {
                    return Err("--backend was supplied twice".into());
                }
                let value = args
                    .get(i + 1)
                    .filter(|s| !s.starts_with("--"))
                    .ok_or_else(|| {
                        format!(
                            "--backend requires a value (fixed|{})",
                            jpp::backends::names()
                        )
                    })?;
                options.backend = match value.as_str() {
                    "fixed" => Backend::Fixed,
                    other => match jpp::backends::by_name(other) {
                        Some(s) => Backend::Registered(s),
                        None => {
                            return Err(format!(
                                "unknown --backend '{other}' (expected fixed|{})",
                                jpp::backends::names()
                            ));
                        }
                    },
                };
                backend_set = true;
                i += 2;
            }
            "--model" => {
                if options.model.is_some() {
                    return Err("--model was supplied twice".into());
                }
                let value = args
                    .get(i + 1)
                    .filter(|s| !s.starts_with("--"))
                    .ok_or_else(|| "--model requires a value".to_string())?;
                options.model = Some(value.clone());
                i += 2;
            }
            "--input-trusted" => {
                if options.input_trusted {
                    return Err("--input-trusted was supplied twice".into());
                }
                options.input_trusted = true;
                i += 1;
            }
            _ => {
                let target = match args[i].as_str() {
                    "--fixtures" => &mut options.fixtures,
                    "--output" => &mut options.output,
                    "--ledger-out" => &mut options.ledger_out,
                    "--replay" => &mut options.replay,
                    "--resume" => &mut options.resume,
                    "--profile" => &mut options.profile,
                    "--calib" => &mut options.calib,
                    "--calib-out" => &mut options.calib_out,
                    "--profiles-dir" => &mut options.profiles_dir,
                    "--input" => &mut options.input,
                    other => return Err(format!("unknown run option '{other}'")),
                };
                if target.is_some() {
                    return Err(format!("{} was supplied twice", args[i]));
                }
                let value = args
                    .get(i + 1)
                    .filter(|s| !s.starts_with("--"))
                    .ok_or_else(|| format!("{} requires a file path", args[i]))?;
                *target = Some(value.into());
                i += 2;
            }
        }
    }
    if options.input_trusted && options.input.is_none() {
        return Err("run --input-trusted requires --input <file.json>".into());
    }
    if options.replay.is_some() && options.resume.is_some() {
        return Err(
            "use either --replay (no new requests) or --resume (continue with the client)".into(),
        );
    }
    if let (Some(_), Some(s)) = (&options.fixtures, options.backend.spec()) {
        return Err(format!(
            "--fixtures and --backend {} are mutually exclusive",
            s.name
        ));
    }
    // B127 过渡守卫（20a-2 合入前）：校准记录没有模型分量，只有 fixed 与 live 能用现有校准记录；
    // 其他后端读（`--calib`）或写（`--calib-out`，主会话 2026-09-25 决定一并守住）都会跨判断器借线。
    // 依据：B127（地基/附注/2026-09-25-待补批量裁定-2.md §七）
    if let Some(s) = options.backend.spec().filter(|s| !s.calib)
        && (options.calib.is_some() || options.calib_out.is_some())
    {
        return Err(format!(
            "E-calib-model: --backend {} 不能带 --calib 或 --calib-out：校准记录尚无模型分量（B60，20a-2），不得跨判断器借线",
            s.name
        ));
    }
    if options.model.is_some() && options.backend == Backend::Fixed {
        return Err(format!(
            "--model requires --backend {}",
            jpp::backends::names()
        ));
    }
    if options.profiles_dir.is_some() && options.backend == Backend::Fixed {
        return Err(format!(
            "--profiles-dir requires --backend {}",
            jpp::backends::names()
        ));
    }
    Ok(Command::Run(options))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 比赛块 C-1、C-1b（R）：帮助文本的动作句与改前手写的原文逐字相同，模板占位已填；
    /// R2b 起表里每加一行动作，这里的期望按表序追加。
    #[test]
    fn 帮助文本的动作清单与原文相同() {
        assert_eq!(
            jpp::actions::usage_list(),
            "record_check, read_json(path), write_json(path,value), graph:matching(graph), graph:shortest_path(graph), graph:max_clique(graph), graph:components(graph), graph:set_cover(graph), graph:max_flow(graph), exec_py(code,stdin,timeout_s), check_tests(code,tests,timeout_s), embed_topk(texts,query,k), bm25_topk(query,corpus,k), exec_sql(db,sql)"
        );
        let h = help();
        assert_eq!(
            h.matches("Registered actions: record_check, read_json(path), write_json(path,value), graph:matching(graph), graph:shortest_path(graph), graph:max_clique(graph), graph:components(graph), graph:set_cover(graph), graph:max_flow(graph), exec_py(code,stdin,timeout_s), check_tests(code,tests,timeout_s), embed_topk(texts,query,k), bm25_topk(query,corpus,k), exec_sql(db,sql).")
                .count(),
            1
        );
        assert!(!h.contains("{actions}"));
    }
    fn args(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn accepts_combined_fixture_replay_and_report_paths() {
        let Command::Run(run) = parse(&args(&[
            "run",
            "a file.jpp",
            "--fixtures",
            "fixture.json",
            "--replay",
            "ledger.json",
            "--output",
            "report.json",
        ]))
        .unwrap() else {
            panic!("run")
        };
        assert_eq!(run.source, PathBuf::from("a file.jpp"));
        assert_eq!(run.replay, Some("ledger.json".into()));
        assert_eq!(run.output, Some("report.json".into()));
    }

    #[test]
    fn reports_usage_mistakes_before_execution() {
        for argv in [
            vec!["run"],
            vec!["run", "a.jpp", "--output"],
            vec!["run", "a.jpp", "--output", "--replay", "x"],
            vec!["run", "a.jpp", "--fixtures", "a", "--fixtures", "b"],
            vec!["check", "a.jpp", "--ast"],
            vec!["run", "a.jpp", "--resume", "a", "--replay", "b"],
            vec!["run", "a.jpp", "--backend", "bogus"],
            vec!["run", "a.jpp", "--backend", "live", "--fixtures", "f.json"],
            vec!["run", "a.jpp", "--model", "jev-1.13.0"],
            vec!["run", "a.jpp", "--backend", "live", "--backend", "fixed"],
            vec!["run", "a.jpp", "--profiles-dir", "profiles"],
        ] {
            assert!(parse(&args(&argv)).is_err(), "{argv:?}");
        }
    }

    #[test]
    fn backend_defaults_to_fixed_and_live_needs_explicit_opt_in() {
        let Command::Run(run) = parse(&args(&["run", "a.jpp"])).unwrap() else {
            panic!("run")
        };
        assert_eq!(run.backend, Backend::Fixed);
        assert_eq!(run.model, None);
    }

    #[test]
    fn backend_live_accepts_a_model_name() {
        let Command::Run(run) = parse(&args(&[
            "run",
            "a.jpp",
            "--backend",
            "live",
            "--model",
            "jev-1.13.0",
        ]))
        .unwrap() else {
            panic!("run")
        };
        assert_eq!(run.backend, Backend::Registered(&jpp::backends::jev::SPEC));
        assert_eq!(run.model, Some("jev-1.13.0".into()));
    }

    #[test]
    fn run_and_check_accept_an_input_file() {
        let Command::Run(run) = parse(&args(&["run", "a.jpp", "--input", "m.json"])).unwrap()
        else {
            panic!("run")
        };
        assert_eq!(run.input, Some("m.json".into()));
        assert_eq!(
            parse(&args(&["check", "a.jpp", "--input", "m.json"])).unwrap(),
            Command::Check {
                source: "a.jpp".into(),
                input: Some("m.json".into()),
                input_trusted: false,
            }
        );
        for argv in [
            vec!["run", "a.jpp", "--input", "a.json", "--input", "b.json"],
            vec!["run", "a.jpp", "--input"],
            vec!["check", "a.jpp", "--input"],
            vec!["check", "a.jpp", "--fixtures", "f.json"],
        ] {
            assert!(parse(&args(&argv)).is_err(), "{argv:?}");
        }
    }

    /// 步 14b-1（B108）：`--input-trusted` 在 `run`/`check` 都收，且都要求先有 `--input`。
    #[test]
    fn input_trusted_requires_input() {
        let Command::Run(run) = parse(&args(&[
            "run",
            "a.jpp",
            "--input",
            "m.json",
            "--input-trusted",
        ]))
        .unwrap() else {
            panic!("run")
        };
        assert!(run.input_trusted);
        assert_eq!(
            parse(&args(&[
                "check",
                "a.jpp",
                "--input",
                "m.json",
                "--input-trusted"
            ]))
            .unwrap(),
            Command::Check {
                source: "a.jpp".into(),
                input: Some("m.json".into()),
                input_trusted: true,
            }
        );
        for argv in [
            vec!["run", "a.jpp", "--input-trusted"],
            vec!["check", "a.jpp", "--input-trusted"],
            vec![
                "run",
                "a.jpp",
                "--input",
                "m.json",
                "--input-trusted",
                "--input-trusted",
            ],
        ] {
            assert!(parse(&args(&argv)).is_err(), "{argv:?}");
        }
    }

    #[test]
    fn backend_live_accepts_a_profiles_dir() {
        let Command::Run(run) = parse(&args(&[
            "run",
            "a.jpp",
            "--backend",
            "live",
            "--profiles-dir",
            "p",
        ]))
        .unwrap() else {
            panic!("run")
        };
        assert_eq!(run.profiles_dir, Some("p".into()));
    }
}
