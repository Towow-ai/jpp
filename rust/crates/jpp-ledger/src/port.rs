//! 账本端口（`20` v2 §2.3 `jpp-ledger` 的 `LedgerPort`、`Durability`；B55，步 18b）。
//!
//! 运行时只经这个 trait 读写账本：读经 [`LedgerPort::view`]，条目只经 [`LedgerPort::append`] 写入，
//! 头在 [`LedgerPort::open_run`] 定稿，层末经 [`LedgerPort::end_layer`] 落盘。内存账本 [`Ledger`]
//! 实现它，落盘时机不起作用；逐行落盘的文件后端是 `jpp::store::LedgerFile`。
//!
//! 与 `20` v2 §2.3 草图的出入（步 18b 施工记录）：没有 `by_cache`（步 19）；`set_header` 取名
//! `open_run`，带校准视图（B124 把命中记录的比对定在这里）并返回 `W-header` 报文；多一个
//! `end_layer`（层末批量要有触发点）；`header()`、`get()` 经 `view()`。
//! 依据：B55（`20` v2 附录 B55 条、§4.3）；B124（`21` 步 18 注）

use std::fmt;

use serde_json::Value as Json;

use crate::{Entry, Header, HeaderCompare, Ledger};

/// 一条条目什么时候落盘（B55）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Durability {
    /// 追加即落盘：先把缓着的条目按序写出，再写这一条，落盘后才返回。不可逆 `do` 的意向与结果用它。
    Now,
    /// 先进内存，层末（[`LedgerPort::end_layer`]）批量落盘。其余条目用它。
    Layer,
}

/// 账本写不进存储：哪个键、什么原因（运行时报 `E-ledger-io`）。
#[derive(Clone, Debug, PartialEq)]
pub struct LedgerError(pub String);

impl fmt::Display for LedgerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// 账本端口：运行时对账本的全部读写。
pub trait LedgerPort {
    /// 读：索引、条目、头、命中记录的派生视图。
    fn view(&self) -> &Ledger;
    /// 追加一条（条目唯一的写入口）：同键已有即不写；已问未答的 `Ask` 得到答案时另起一条
    /// （[`Ledger::put_answer`] 的规则，对其余条目与 [`Ledger::put`] 相同）。
    fn append(&mut self, e: Entry, d: Durability) -> Result<(), LedgerError>;
    /// 运行入口换头并比对（[`Ledger::set_header_checked`]），头在这里定稿；返回 `W-header` 报文。
    /// 文件后端此时写出头行与已有条目（续接：新头、旧条目按新链重串）。
    fn open_run(
        &mut self,
        h: Header,
        mode: HeaderCompare,
        视图: &dyn Fn(&str) -> Option<Json>,
    ) -> Result<Option<String>, LedgerError>;
    /// 层末：把缓着的条目落盘。
    fn end_layer(&mut self) -> Result<(), LedgerError>;
    /// 记下一次校准记录命中（B124）：该键最后一条 `CalibUsed` 的哈希与本次相同则不追加。
    fn append_calib_used(
        &mut self,
        key: &str,
        hash: &str,
        record: Json,
    ) -> Result<(), LedgerError> {
        if self.view().calib_used_same(key, hash) {
            return Ok(());
        }
        self.append(
            Entry::CalibUsed {
                key: key.to_string(),
                hash: hash.to_string(),
                record,
            },
            Durability::Layer,
        )
    }
}

/// 内存账本：落盘时机不起作用，写入不会失败。
impl LedgerPort for Ledger {
    fn view(&self) -> &Ledger {
        self
    }
    fn append(&mut self, e: Entry, _d: Durability) -> Result<(), LedgerError> {
        self.put_answer(e);
        Ok(())
    }
    fn open_run(
        &mut self,
        h: Header,
        mode: HeaderCompare,
        视图: &dyn Fn(&str) -> Option<Json>,
    ) -> Result<Option<String>, LedgerError> {
        self.set_header_checked(h, mode, 视图);
        Ok(self.header_warning.take())
    }
    fn end_layer(&mut self) -> Result<(), LedgerError> {
        Ok(())
    }
}
