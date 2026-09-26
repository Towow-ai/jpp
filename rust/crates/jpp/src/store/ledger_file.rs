//! 逐行落盘的账本（`20` v2 §2.3 `jpp::store::LedgerFile`；B55，步 18b）。
//!
//! 头在运行入口定稿（[`LedgerPort::open_run`]）时，把新头与已有条目（续接时是上一趟的条目，按新链
//! 重串）用 `put_atomic` 整份写出：写到与输入同一个键也是原子替换，不会先截断再写。之后每条经
//! `append_durable` 追加：`Durability::Now` 的条目先把缓着的条目按序写出、再写自己，落盘后才返回；
//! `Durability::Layer` 的条目先进内存，层末（[`LedgerPort::end_layer`]）写出。写出的文件与
//! [`Ledger::encode`] 逐字节相同（同一套 `encode_head` / `encode_entry`）。
//! 依据：B55（`20` v2 附录 B55 条、§4.3、§九「跨会话程序的一致性」）；`21` 步 18 注（18b）；B74（本模块
//! 只依赖 `jpp_ledger` 与 `store::Blob`）

use jpp_ledger::{
    Durability, Entry, Header, HeaderCompare, Ledger, LedgerError, LedgerPort, encode_head,
    line_hash,
};
use serde_json::Value as Json;

use super::blob::{Blob, IoErr};

/// 逐行落盘的账本：内存里一份 [`Ledger`]（索引与读），存储里一份 JSONL 文件。
pub struct LedgerFile<B: Blob> {
    blob: B,
    key: String,
    ledger: Ledger,
    /// 已写进存储的条目数与最后一行的链哈希；`None` = 头行还没写
    written: Option<(usize, String)>,
}

fn io(e: IoErr) -> LedgerError {
    LedgerError(e.to_string())
}

impl<B: Blob> LedgerFile<B> {
    /// `ledger` 可以带上一趟的条目（续接）：头定稿时连同它们整份写出。此前存储里什么都不写。
    pub fn new(blob: B, key: &str, ledger: Ledger) -> LedgerFile<B> {
        LedgerFile {
            blob,
            key: key.to_string(),
            ledger,
            written: None,
        }
    }

    pub fn ledger(&self) -> &Ledger {
        &self.ledger
    }

    pub fn blob(&self) -> &B {
        &self.blob
    }

    /// 整份原子写出：头行与全部条目。
    fn write_all(&mut self) -> Result<(), LedgerError> {
        let text = self.ledger.encode();
        self.blob
            .put_atomic(&self.key, text.as_bytes())
            .map_err(io)?;
        let head = encode_head(&self.ledger.header);
        let (_, last) = self.ledger.encode_from(0, &line_hash(&head));
        self.written = Some((self.ledger.len(), last));
        Ok(())
    }

    /// 把还没写的条目按序逐行追加并落盘；头行还没写时整份写出。每写成一行就记下，
    /// 中途失败时 `written` 仍指向已落盘的最后一行。
    fn sync(&mut self) -> Result<(), LedgerError> {
        let Some((n, prev)) = self.written.clone() else {
            return self.write_all();
        };
        if n == self.ledger.len() {
            return Ok(());
        }
        let (lines, _) = self.ledger.encode_from(n, &prev);
        for (i, line) in lines.iter().enumerate() {
            self.blob
                .append_durable(&self.key, line.as_bytes())
                .map_err(io)?;
            self.written = Some((n + i + 1, line_hash(line)));
        }
        Ok(())
    }

    /// 写出余下的条目（头从未定稿时整份写出），交回内存账本与存储。
    pub fn finish(mut self) -> Result<(Ledger, B), LedgerError> {
        self.sync()?;
        Ok((self.ledger, self.blob))
    }
}

impl<B: Blob> LedgerPort for LedgerFile<B> {
    fn view(&self) -> &Ledger {
        &self.ledger
    }
    fn append(&mut self, e: Entry, d: Durability) -> Result<(), LedgerError> {
        self.ledger.put_answer(e);
        match d {
            Durability::Now => self.sync(),
            Durability::Layer => Ok(()),
        }
    }
    fn open_run(
        &mut self,
        h: Header,
        mode: HeaderCompare,
        视图: &dyn Fn(&str) -> Option<Json>,
    ) -> Result<Option<String>, LedgerError> {
        self.ledger.set_header_checked(h, mode, 视图);
        let w = self.ledger.header_warning.take();
        self.write_all()?;
        Ok(w)
    }
    fn end_layer(&mut self) -> Result<(), LedgerError> {
        self.sync()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::MemBlob;
    use crate::value::Answer;

    fn 头() -> Header {
        Header::new(3, 1.0, "m", "r1", "h")
    }

    fn 读(f: &LedgerFile<MemBlob>) -> String {
        String::from_utf8(f.blob().get("l.jsonl").unwrap().unwrap_or_default()).unwrap()
    }

    #[test]
    fn 头定稿才写_层末条目等到层末_即刻条目先带出缓着的() {
        let mut f = LedgerFile::new(MemBlob::new(), "l.jsonl", Ledger::new());
        assert_eq!(读(&f), "", "头定稿前什么都不写");
        f.open_run(头(), HeaderCompare::Resume, &|_| None).unwrap();
        assert_eq!(读(&f).lines().count(), 1, "头行");
        f.append(
            Entry::judge("k1", Answer::Noul(0.9), 0, 0.0, "m", 1),
            Durability::Layer,
        )
        .unwrap();
        assert_eq!(读(&f).lines().count(), 1, "层末条目先进内存");
        f.append(
            Entry::Intent {
                key: "intent:e".into(),
                at: 1,
            },
            Durability::Now,
        )
        .unwrap();
        assert_eq!(
            读(&f).lines().count(),
            3,
            "即刻条目先写出缓着的判断，再写自己"
        );
        assert_eq!(读(&f), f.ledger().encode(), "链不断，与整份编码相同");
        f.append(
            Entry::effect_keyed("e".into(), "do", Json::from(1), 0.0),
            Durability::Layer,
        )
        .unwrap();
        f.end_layer().unwrap();
        assert_eq!(读(&f), f.ledger().encode());
        let (d, t) = Ledger::decode(&读(&f)).unwrap();
        assert!(t.is_none());
        assert_eq!(d.len(), 3);
    }

    #[test]
    fn 续接_新头与旧条目整份重写_之后追加() {
        let mut old = Ledger::new();
        old.set_header(Header::new(1, 1.0, "m", "r1", "h"));
        old.put(Entry::judge("k1", Answer::Noul(0.9), 0, 0.0, "m", 1));
        let mut b = MemBlob::new();
        b.put_atomic("l.jsonl", old.encode().as_bytes()).unwrap();
        let (读回, _) = Ledger::decode(&old.encode()).unwrap();
        let mut f = LedgerFile::new(b, "l.jsonl", 读回);
        f.open_run(头(), HeaderCompare::Resume, &|_| None).unwrap();
        f.append(
            Entry::judge("k2", Answer::Noul(0.1), 0, 0.0, "m", 2),
            Durability::Layer,
        )
        .unwrap();
        let (l, b) = f.finish().unwrap();
        let text = String::from_utf8(b.get("l.jsonl").unwrap().unwrap()).unwrap();
        assert_eq!(text, l.encode());
        assert!(text.starts_with(&encode_head(&Some(头()))), "新头");
        assert_eq!(Ledger::decode(&text).unwrap().0.len(), 2);
    }

    #[test]
    fn 头从未定稿时_finish_整份写出() {
        let mut old = Ledger::new();
        old.put(Entry::judge("k1", Answer::Noul(0.9), 0, 0.0, "m", 1));
        let f = LedgerFile::new(MemBlob::new(), "l.jsonl", old.clone());
        let (_, b) = f.finish().unwrap();
        assert_eq!(
            String::from_utf8(b.get("l.jsonl").unwrap().unwrap()).unwrap(),
            old.encode()
        );
    }
}
