use z3::ast::Bool;
use z3::{SatResult, Solver};

fn core_names(named: &[(String, Bool)], solver: &Solver) -> Vec<String> {
    let core = solver.get_unsat_core();
    named
        .iter()
        .filter(|(_, tracker)| core.contains(tracker))
        .map(|(name, _)| name.clone())
        .collect()
}

/// 死亡后现实行动：死亡事实、行动事实与“死亡后不能行动”规则共同构成 unsat core。
pub fn dead_after_death_core(
    death_id: &str,
    event_id: &str,
    rule_code: &str,
) -> Option<Vec<String>> {
    let solver = Solver::new();
    let dead = Bool::new_const(format!("claim:{death_id}:dead"));
    let acts = Bool::new_const(format!("event:{event_id}:real_action"));
    let rule = Bool::new_const(format!("rule:{rule_code}"));
    solver.assert_and_track(dead.clone(), &dead);
    solver.assert_and_track(acts.clone(), &acts);
    solver.assert_and_track(rule.clone(), &rule);
    solver.assert_and_track(
        (dead.clone() & acts.clone() & rule.clone()).not(),
        &Bool::new_const("constraint:exclusive"),
    );
    if solver.check() == SatResult::Unsat {
        Some(core_names(
            &[
                (format!("claim:{death_id}:dead"), dead),
                (format!("event:{event_id}:real_action"), acts),
                (format!("rule:{rule_code}"), rule),
            ],
            &solver,
        ))
    } else {
        None
    }
}

pub fn two_places_core(claim_a: &str, claim_b: &str) -> Option<Vec<String>> {
    let solver = Solver::new();
    let at_a = Bool::new_const(format!("claim:{claim_a}:location"));
    let at_b = Bool::new_const(format!("claim:{claim_b}:location"));
    let same_time = Bool::new_const("constraint:same_story_time");
    solver.assert_and_track(at_a.clone(), &at_a);
    solver.assert_and_track(at_b.clone(), &at_b);
    solver.assert_and_track(same_time.clone(), &same_time);
    solver.assert_and_track(
        (at_a.clone() & at_b.clone() & same_time.clone()).not(),
        &Bool::new_const("rule:exclusive_location"),
    );
    if solver.check() == SatResult::Unsat {
        Some(vec![
            format!("claim:{claim_a}:location"),
            format!("claim:{claim_b}:location"),
            "constraint:same_story_time".into(),
            "rule:exclusive_location".into(),
        ])
    } else {
        None
    }
}

pub fn item_after_destroy_core(destroyed: &str, held: &str) -> Option<Vec<String>> {
    let solver = Solver::new();
    let destroyed_flag = Bool::new_const(format!("claim:{destroyed}:destroyed"));
    let held_flag = Bool::new_const(format!("claim:{held}:held"));
    let order = Bool::new_const("constraint:held_after_destroyed");
    solver.assert_and_track(destroyed_flag.clone(), &destroyed_flag);
    solver.assert_and_track(held_flag.clone(), &held_flag);
    solver.assert_and_track(order.clone(), &order);
    solver.assert_and_track(
        (destroyed_flag.clone() & held_flag.clone() & order.clone()).not(),
        &Bool::new_const("rule:item_exclusive"),
    );
    if solver.check() == SatResult::Unsat {
        Some(vec![
            format!("claim:{destroyed}:destroyed"),
            format!("claim:{held}:held"),
            "constraint:held_after_destroyed".into(),
            "rule:item_exclusive".into(),
        ])
    } else {
        None
    }
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
}
