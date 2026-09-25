//! 守卫：不可逆 `do` 的唯一放行点（J-08；`20` v2 §2.3 `guard.rs::release`，步 16）。
//!
//! 守卫栈只记 `GuardEv`：`if` 入口压条件值自带的证据，`handle` 各臂入口压 `GuardEv::from_exit`。
//! 证据随 `Bool` 值走，不按名字、不按求值窗口另记（B121；原 `guard_of`/`walk_conjuncts` 与 let
//! 旁路表已删）。依据：B121（地基/附注/2026-09-25-B121守卫证据裁定.md）

use super::*;

impl<'a> Interp<'a> {
    /// 不可逆 `do` 的唯一放行点（J-08；`20` v2 §2.3 `guard.rs::release`，步 16）。
    /// 读守卫栈上的 `GuardEv`：任一层守卫有放行证据即放行；栈空（无条件执行）不受管。
    pub(crate) fn release(&self, name: &str, reversible: bool, sp: Span) -> R<()> {
        // J-08（12:265）：放行**不可逆** do 的守卫表达式中，至少一个合取项来自 taint=trusted
        // 的状态；untrusted 项的数量不改变这一要求；或经 ask。
        //
        // 「守卫表达式」在一个有 if 的语言里就是**包着这个 do 的那些条件**——不必是 do 的一个参数。
        // `00-宪法.md:44` 说 IFC 的纪律只有这一条：不可信材料上的判断不得单独放行不可逆 do。
        //
        // 不查的：**这个 trusted 是不是真的可信**（12:649 Nature 裁定：taint_out="trusted" 是作者的
        // 显式标记，语言保证它可见可追，**不设审核方**）。所以这里问的是「守卫里有没有一个 trusted
        // 合取项」，不是「那个 trusted 配不配」。
        //
        // 无条件执行的 do 不受管：它没有守卫可查，作者直接写 do 是他自己的决定。
        if !reversible && !self.guards.is_empty() {
            let 放行 = self.guards.iter().any(|g| g.releases());
            if !放行 {
                // B33 第 8 点：守卫出口所在状态含成分不可信的计算值材料时，说明原因。
                // 近似：看当前各帧产生过的出口（守卫就是由它们折出来的）。
                let 计算值 = {
                    let set = self.computed_untrusted_states.borrow();
                    self.frames
                        .iter()
                        .flat_map(|f| f.exits.iter())
                        .any(|e| set.contains(&e.state_hash))
                };
                let mut 补充 = if 计算值 {
                    "。该材料由计算值构成，成分含不可信内容".to_string()
                } else {
                    String::new()
                };
                // B105：守卫出口所在状态含宿主入口材料（未声明可信）时，说出入口名。
                // 依据：B105 / B106（地基/附注/2026-09-25-B105-B106裁定.md §一·2）
                let 入口 = {
                    let map = self.input_untrusted_states.borrow();
                    self.frames
                        .iter()
                        .flat_map(|f| f.exits.iter())
                        .find_map(|e| map.get(&e.state_hash).cloned())
                };
                if let Some(名) = 入口 {
                    // 步 14b-1（B108）：CLI 声明可信的开关名字点出来，不止说「宿主未声明可信」
                    补充.push_str(&format!(
                        "。该材料是宿主入口 {名}，宿主未声明可信（CLI：--input-trusted）"
                    ));
                }
                // B72-4：守卫出口的材料由不放行的祖先出口选出时，说出那一级。依据：B72（附注 2026-09-24-评估①裁定.md）
                let 谱系 = {
                    let 断 = self.谱系断.borrow();
                    self.frames
                        .iter()
                        .flat_map(|f| f.exits.iter())
                        .find_map(|e| 断.get(&e.id).cloned())
                };
                if let Some(说明) = 谱系 {
                    补充.push_str(&format!("。{说明}（谱系放行，B72-4）"));
                }
                return err(
                    Some("J-08"),
                    format!(
                        "不可逆动作 {name} 的守卫里没有一个来自可信状态的合取项：不可信材料上的判断不得**单独**放行不可逆动作（宪法 IFC / 12 §5 J-08）。修法：在条件里再合取一个来自 trusted 状态的判断，或改走 ask 让人拍板，或把这个动作登记成可逆。注意：凭夹具线（W-fixture-line，B29）或停岗候选线（W-suspend-candidate，B25）得到的出口不算可信合取项；类线（B75）、试用线（B72、B89）、临时上岗线（B19 修订）、认证范围外或范围未知（B68、B104）、证书未记录认证带宽（B104）、判据未测（J-15）的出口同样不算（报告 exits 表 releases: false）{补充}"
                    ),
                    sp,
                );
            }
        }
        Ok(())
    }

    /// 本趟出口表登记一个切出的出口（B72-4，步 17b；只由 `bridge.rs::cut` 调）。
    /// 记「已决且 `releases()`」；同一键多次登记取与，说明取第一个不放行者。
    pub(crate) fn 登记出口放行(&mut self, e: &Rc<Exit>) {
        let key = e.ledger_key.borrow().clone();
        if key.is_empty() {
            return;
        }
        let 等级 = e.grade.get().map(|g| g.name()).unwrap_or("无等级");
        let (ok, 说明) = if e.is_unsure() {
            (false, format!("该材料由 {等级} 线的未决出口选出"))
        } else if !e.releases() {
            (false, format!("该材料由 {等级} 线的出口选出"))
        } else {
            (true, String::new())
        };
        let slot = self
            .出口放行表
            .entry(key)
            .or_insert_with(|| (true, String::new()));
        if slot.0 && !ok {
            *slot = (false, 说明);
        }
    }

    /// 谱系放行（B72-4，步 17b）：出口 e 的被判断状态经账本 `parents` 传递闭包可达的祖先出口，
    /// 若有一个不是「已决且放行」，返回它的说明；全部放行返回 `None`。
    ///
    /// 「可达」= `State.parents`（槽材料的 sources ∪ 题的 sources），经账本 `Entry::Judge.parents` 传递（B84 补）。
    /// 祖先的等级从本趟出口表按账本键取（续接与重放从头重跑，祖先都在本趟重新切，B83）。
    /// 表里查不到的祖先键算「无法证明」：谱系断，并报 `W-lineage-unknown`（17b 解释登记 (b)，主会话改保守读法）。
    pub(crate) fn 谱系(&mut self, e: &Exit) -> R<Option<String>> {
        let start = e.ledger_key.borrow().clone();
        let mut 待查: Vec<String> = self.parents_of(&start);
        let mut 见过: HashSet<String> = HashSet::new();
        while let Some(k) = 待查.pop() {
            if !见过.insert(k.clone()) {
                continue;
            }
            // 惰性过桥（B94，审查修复 1）：这个读数切出的出口可能还没解析、放行表里还没有它的等级。
            // 先解析同键的全部出口再查表。依据：B72-4。解析出错往上传（复核修复 7）：那次刷新已取出
            // 别的待发判断，吞成「不放行」会丢掉它们；改前同样的错误在 `cut` 处报出并中止
            self.解析同键出口(&k)?;
            match self.出口放行表.get(&k) {
                Some((true, _)) => {}
                Some((false, 说明)) => return Ok(Some(说明.clone())),
                None => {
                    // 依据：B72（地基/附注/2026-09-24-评估①裁定.md，B72-4 谱系放行）；缺键按不放行是主会话 2026-09-25 的保守读法
                    if self.谱系缺键已报.insert(k.clone()) {
                        self.trace.warn(format!(
                            "W-lineage-unknown: 读数 {} 是被判断材料的来源，但本趟没有从它切出出口，谱系放行无法证明，按不放行处理（B72-4；17b 解释登记 (b)）",
                            k
                        ));
                    }
                    return Ok(Some(format!(
                        "该材料的来源读数 {k} 本趟没有切出出口，谱系无法证明"
                    )));
                }
            }
            待查.extend(self.parents_of(&k));
        }
        Ok(None)
    }

    fn parents_of(&self, key: &str) -> Vec<String> {
        match self.ledger.get(key) {
            Some(Entry::Judge { parents, .. }) => parents.clone(),
            _ => vec![],
        }
    }
}
