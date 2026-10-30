//! 轻量硬约束求解器。
//!
//! 叙事中的可形式化硬约束（先后、同时、移动耗时、销毁后持有）都可以写成
//! 差分约束 `t_to - t_from <= weight`，即简单时间网络（STN）。STN 可满足
//! 当且仅当约束图中没有负环；负环上的约束就是一组冲突核心——去掉其中任意
//! 一条，这个环就不再成立。这样无需打包 Z3，也能给出可映射回事实和原文的
//! 冲突证书。

use std::collections::HashMap;

#[derive(Debug, Clone)]
struct Edge {
    from: usize,
    to: usize,
    weight: i64,
    label: String,
}

/// 简单时间网络：时间点 + 带标签的差分约束。
#[derive(Debug, Default)]
pub struct TemporalNetwork {
    points: HashMap<String, usize>,
    edges: Vec<Edge>,
}

impl TemporalNetwork {
    pub fn new() -> Self {
        Self::default()
    }

    fn point(&mut self, name: &str) -> usize {
        let next = self.points.len();
        *self.points.entry(name.to_string()).or_insert(next)
    }

    /// 约束 `t(to) - t(from) <= weight`。
    pub fn at_most(&mut self, from: &str, to: &str, weight: i64, label: impl Into<String>) {
        let from = self.point(from);
        let to = self.point(to);
        self.edges.push(Edge {
            from,
            to,
            weight,
            label: label.into(),
        });
    }

    /// `later` 至少比 `earlier` 晚 `gap` 个时间单位。
    pub fn after(&mut self, earlier: &str, later: &str, gap: i64, label: impl Into<String>) {
        self.at_most(later, earlier, -gap, label);
    }

    /// 两个时间点相同；两条边共用同一个标签，冲突核心里只出现一次。
    pub fn same_time(&mut self, a: &str, b: &str, label: impl Into<String>) {
        let label = label.into();
        self.at_most(a, b, 0, label.clone());
        self.at_most(b, a, 0, label);
    }

    /// 可满足返回 `Ok(())`；否则返回负环上的约束标签（按出现顺序去重）。
    pub fn check(&self) -> Result<(), Vec<String>> {
        let n = self.points.len();
        if n == 0 {
            return Ok(());
        }
        // 虚拟源点到所有点距离为 0，等价于把 dist 全部初始化为 0。
        let mut dist = vec![0_i64; n];
        let mut pred: Vec<Option<usize>> = vec![None; n];
        let mut touched = None;
        for _ in 0..n {
            touched = None;
            for (index, edge) in self.edges.iter().enumerate() {
                let candidate = dist[edge.from].saturating_add(edge.weight);
                if candidate < dist[edge.to] {
                    dist[edge.to] = candidate;
                    pred[edge.to] = Some(index);
                    touched = Some(edge.to);
                }
            }
            if touched.is_none() {
                return Ok(());
            }
        }
        let Some(mut node) = touched else {
            return Ok(());
        };
        // 回溯 n 步保证落在环上，再沿前驱走一圈收集约束。
        for _ in 0..n {
            node = self.edges[pred[node].expect("relaxed node has predecessor")].from;
        }
        let start = node;
        let mut cycle = Vec::new();
        loop {
            let edge = &self.edges[pred[node].expect("cycle node has predecessor")];
            cycle.push(edge.label.clone());
            node = edge.from;
            if node == start {
                break;
            }
        }
        cycle.reverse();
        let mut seen = std::collections::HashSet::new();
        cycle.retain(|label| seen.insert(label.clone()));
        Err(cycle)
    }
}

fn conflict_core(network: &TemporalNetwork) -> Option<Vec<String>> {
    network.check().err()
}

/// 死亡后现实行动：死亡时间点、行动时间点与“死者不能在死亡后行动”构成负环。
pub fn dead_after_death_core(
    death_id: &str,
    event_id: &str,
    rule_code: &str,
) -> Option<Vec<String>> {
    let mut network = TemporalNetwork::new();
    // 死亡事实：存活区间最迟在死亡时刻结束。
    network.at_most("death", "alive_end", 0, format!("claim:{death_id}:dead"));
    // 规则：现实行动必须发生在存活区间内。
    network.at_most("alive_end", "action", 0, format!("rule:{rule_code}"));
    // 事件：行动发生在死亡之后。
    network.after(
        "death",
        "action",
        1,
        format!("event:{event_id}:real_action"),
    );
    conflict_core(&network)
}

/// 同一时刻出现在两个地点：同时约束与“换地点至少需要移动时间”构成负环。
pub fn two_places_core(claim_a: &str, claim_b: &str) -> Option<Vec<String>> {
    let mut network = TemporalNetwork::new();
    network.same_time("at_a", "at_b", "constraint:same_story_time");
    network.after("at_a", "at_b", 1, "rule:exclusive_location");
    let mut core = conflict_core(&network)?;
    core.insert(0, format!("claim:{claim_b}:location"));
    core.insert(0, format!("claim:{claim_a}:location"));
    Some(core)
}

/// 物品被销毁后仍被持有：持有晚于销毁，而销毁后不得再持有。
pub fn item_after_destroy_core(destroyed: &str, held: &str) -> Option<Vec<String>> {
    let mut network = TemporalNetwork::new();
    network.after("destroyed", "held", 1, "constraint:held_after_destroyed");
    network.after("held", "destroyed", 0, "rule:item_exclusive");
    let mut core = conflict_core(&network)?;
    core.insert(0, format!("claim:{held}:held"));
    core.insert(0, format!("claim:{destroyed}:destroyed"));
    Some(core)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn death_then_action_returns_core() {
        let core = dead_after_death_core("c1", "e1", "dead_character_acts").unwrap();
        assert!(core.iter().any(|item| item.contains("claim:c1")));
        assert!(core.iter().any(|item| item.contains("event:e1")));
    }

    #[test]
    fn location_and_item_conflicts_return_cores() {
        assert!(two_places_core("c1", "c2").is_some());
        assert!(item_after_destroy_core("c3", "c4").is_some());
    }

    #[test]
    fn consistent_network_is_satisfiable() {
        let mut network = TemporalNetwork::new();
        network.after("birth", "exile", 10, "e1");
        network.after("exile", "return", 3, "e2");
        network.at_most("birth", "return", 20, "window");
        assert!(network.check().is_ok());
    }

    #[test]
    fn core_contains_only_cycle_constraints() {
        let mut network = TemporalNetwork::new();
        network.after("a", "b", 2, "a_before_b");
        network.after("b", "c", 2, "b_before_c");
        network.at_most("a", "c", 3, "c_within_3_of_a");
        network.after("x", "y", 1, "unrelated");
        let core = network.check().unwrap_err();
        assert_eq!(core.len(), 3);
        assert!(!core.contains(&"unrelated".to_string()));
    }
}
