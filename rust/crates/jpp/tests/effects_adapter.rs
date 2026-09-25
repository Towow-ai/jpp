//! 步 9：观察端按效应实例服务（B60）；原路径的画像类型与 `jpp-effects` 是同一个类型。
//! 步 15c：旧 `Client` 删除，改由端口表给出实例。

use jpp::effects::{FixedPorts, NoCallPorts, Profile, Tri};

#[test]
fn 客户端按效应给出实例() {
    let mut c = FixedPorts::default();
    let v = c.ports().instances();
    let mut names: Vec<_> = v.iter().map(|i| jpp_effects::spec(i.effect).name).collect();
    names.sort();
    assert_eq!(names, ["ask", "gen", "judge"]);
    assert!(v.iter().all(|i| i.model == jpp::effects::FIXED_MODEL));
    assert_eq!(NoCallPorts::ports().instances().len(), 3);
}

#[test]
fn 原路径的画像就是新crate的画像() {
    let p: jpp_effects::Profile = Profile::untested();
    assert_eq!(p.arithmetic_capable(), jpp_effects::Tri::未测);
    let _: Tri = p.arithmetic_capable();
}
