//! `fit` 注册表。（步 4c 自 `effects.rs` 原样搬出）

use std::collections::HashMap;
use std::rc::Rc;

// 步 14a：`FitRecord` 搬到 `jpp-effects::views`（运行时经 `FitView` 读，不依赖本 crate）；原路径重导出。
pub use jpp_effects::views::FitRecord;

#[derive(Default)]
pub struct FitRegistry {
    pub fits: HashMap<String, Rc<FitRecord>>,
}

impl FitRegistry {
    pub fn new() -> FitRegistry {
        FitRegistry::default()
    }
    /// 不核约束的登记（测试与内部用）
    pub fn register(
        &mut self,
        name: &str,
        features: &[(&str, &str)],
        n: u64,
        trained_from: &str,
        f: impl Fn(&[f64]) -> f64 + 'static,
    ) {
        let features = features
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        self.fits.insert(
            name.to_string(),
            Rc::new(FitRecord {
                features,
                n,
                trained_from: trained_from.into(),
                f: Rc::new(f),
            }),
        );
    }
    /// 核 J-16 的样本数约束再登记：`n ≥ max(50, 20×特征数)`。
    /// **样本不够就不该用它下结论**——这条在登记时拦，比在调用时拦早。
    pub fn register_checked(
        &mut self,
        name: &str,
        features: &[(&str, &str)],
        n: u64,
        trained_from: &str,
        f: impl Fn(&[f64]) -> f64 + 'static,
    ) -> Result<(), String> {
        let need = std::cmp::max(50, 20 * features.len() as u64);
        if n < need {
            return Err(format!(
                "J-16: fit {name} 的训练样本 n={n} 不足 max(50, 20×{}) = {need}",
                features.len()
            ));
        }
        self.register(name, features, n, trained_from, f);
        Ok(())
    }
}

/// `FitRegistry` 是运行时读的 `fit` 视图（`20` §2.3 `Ports.fits`；步 14a 起运行时经它读，不依赖本 crate）。
impl jpp_effects::views::FitView for FitRegistry {
    type Record = Rc<FitRecord>;
    fn get(&self, fit_ref: &str) -> Option<&Rc<FitRecord>> {
        self.fits.get(fit_ref)
    }
}
