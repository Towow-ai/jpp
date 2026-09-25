//! 字节存储（`20` v2 §2.3 `jpp::store::Blob`）：每类存储一个写入口，整份写入原子（临时文件 + rename），
//! 追加写入逐行落盘（B55 的 `append_durable`，步 18b 起账本用）。两个后端：内存与目录（D11.5）。

use std::collections::BTreeMap;
use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};

/// 存储错误：哪个键、什么原因。
#[derive(Clone, Debug, PartialEq)]
pub struct IoErr {
    pub key: String,
    pub reason: String,
}

impl fmt::Display for IoErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}：{}", self.key, self.reason)
    }
}

fn io_err(key: &str, e: impl fmt::Display) -> IoErr {
    IoErr {
        key: key.to_string(),
        reason: e.to_string(),
    }
}

/// 字节存储。键是 `/` 分隔的相对路径。
pub trait Blob {
    /// 读整份；不存在为 `None`。
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, IoErr>;
    /// 整份原子写：读者要么看到旧内容，要么看到新内容。
    fn put_atomic(&mut self, key: &str, bytes: &[u8]) -> Result<(), IoErr>;
    /// 追加一行并落盘后才返回（B55：不可逆动作的写前意向必须先于动作到盘）。`line` 不含换行符。
    fn append_durable(&mut self, key: &str, line: &[u8]) -> Result<(), IoErr>;
    /// 以 `prefix` 开头的键，排序。
    fn list(&self, prefix: &str) -> Result<Vec<String>, IoErr>;
}

/// 内存后端（测试与嵌入宿主）。
#[derive(Clone, Debug, Default)]
pub struct MemBlob {
    items: BTreeMap<String, Vec<u8>>,
}

impl MemBlob {
    pub fn new() -> MemBlob {
        MemBlob::default()
    }
}

impl Blob for MemBlob {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, IoErr> {
        Ok(self.items.get(key).cloned())
    }
    fn put_atomic(&mut self, key: &str, bytes: &[u8]) -> Result<(), IoErr> {
        self.items.insert(key.to_string(), bytes.to_vec());
        Ok(())
    }
    fn append_durable(&mut self, key: &str, line: &[u8]) -> Result<(), IoErr> {
        let v = self.items.entry(key.to_string()).or_default();
        v.extend_from_slice(line);
        v.push(b'\n');
        Ok(())
    }
    fn list(&self, prefix: &str) -> Result<Vec<String>, IoErr> {
        Ok(self
            .items
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect())
    }
}

/// 目录后端：键是目录下的相对路径。
#[derive(Clone, Debug)]
pub struct DirBlob {
    root: PathBuf,
}

impl DirBlob {
    pub fn new(root: impl Into<PathBuf>) -> DirBlob {
        DirBlob { root: root.into() }
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    fn path(&self, key: &str) -> PathBuf {
        self.root.join(key)
    }
    fn ensure_parent(&self, key: &str, p: &Path) -> Result<(), IoErr> {
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).map_err(|e| io_err(key, e))?;
        }
        Ok(())
    }
}

impl Blob for DirBlob {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, IoErr> {
        match std::fs::read(self.path(key)) {
            Ok(b) => Ok(Some(b)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(io_err(key, e)),
        }
    }
    fn put_atomic(&mut self, key: &str, bytes: &[u8]) -> Result<(), IoErr> {
        let p = self.path(key);
        self.ensure_parent(key, &p)?;
        let tmp = p.with_extension(format!(
            "{}.tmp",
            p.extension().and_then(|x| x.to_str()).unwrap_or("")
        ));
        {
            let mut f = std::fs::File::create(&tmp).map_err(|e| io_err(key, e))?;
            f.write_all(bytes).map_err(|e| io_err(key, e))?;
            f.sync_data().map_err(|e| io_err(key, e))?;
        }
        std::fs::rename(&tmp, &p).map_err(|e| io_err(key, e))
    }
    fn append_durable(&mut self, key: &str, line: &[u8]) -> Result<(), IoErr> {
        let p = self.path(key);
        self.ensure_parent(key, &p)?;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&p)
            .map_err(|e| io_err(key, e))?;
        let mut buf = line.to_vec();
        buf.push(b'\n');
        f.write_all(&buf).map_err(|e| io_err(key, e))?;
        f.sync_data().map_err(|e| io_err(key, e))
    }
    fn list(&self, prefix: &str) -> Result<Vec<String>, IoErr> {
        let mut out = vec![];
        let mut stack = vec![self.root.clone()];
        while let Some(d) = stack.pop() {
            let rd = match std::fs::read_dir(&d) {
                Ok(rd) => rd,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(io_err(prefix, e)),
            };
            for ent in rd {
                let p = ent.map_err(|e| io_err(prefix, e))?.path();
                if p.is_dir() {
                    stack.push(p);
                } else if let Ok(rel) = p.strip_prefix(&self.root) {
                    let k = rel.to_string_lossy().replace('\\', "/");
                    if k.starts_with(prefix) {
                        out.push(k);
                    }
                }
            }
        }
        out.sort();
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn 走一遍(b: &mut dyn Blob) {
        assert_eq!(b.get("a/x.json").unwrap(), None);
        b.put_atomic("a/x.json", b"{}").unwrap();
        b.put_atomic("a/x.json", b"{\"v\":1}").unwrap();
        assert_eq!(b.get("a/x.json").unwrap().unwrap(), b"{\"v\":1}");
        b.append_durable("l.jsonl", b"one").unwrap();
        b.append_durable("l.jsonl", b"two").unwrap();
        assert_eq!(b.get("l.jsonl").unwrap().unwrap(), b"one\ntwo\n");
        assert_eq!(b.list("a/").unwrap(), vec!["a/x.json".to_string()]);
        assert_eq!(b.list("").unwrap().len(), 2);
    }

    #[test]
    fn 内存后端() {
        走一遍(&mut MemBlob::new());
    }

    #[test]
    fn 目录后端_原子写不留临时文件() {
        let dir = std::env::temp_dir().join(format!("jpp-dirblob-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut b = DirBlob::new(&dir);
        走一遍(&mut b);
        assert!(b.list("").unwrap().iter().all(|k| !k.ends_with(".tmp")));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
