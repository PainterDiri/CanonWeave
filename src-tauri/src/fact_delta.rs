use std::collections::BTreeSet;

use regex::Regex;

use crate::domain::{FactChange, FactDelta};

fn sentences(text: &str) -> Vec<String> {
    text.split(['。', '！', '？', '；', '\n', '!', '?', ';'])
        .map(str::trim)
        .filter(|sentence| !sentence.is_empty())
        .map(str::to_string)
        .collect()
}

fn numbers(text: &str) -> BTreeSet<String> {
    let regex = Regex::new(r"\d+(?:\.\d+)?").expect("number regex");
    regex
        .find_iter(text)
        .map(|value| value.as_str().to_string())
        .collect()
}

fn matching_sentences(text: &str, keywords: &[&str]) -> BTreeSet<String> {
    sentences(text)
        .into_iter()
        .filter(|sentence| keywords.iter().any(|keyword| sentence.contains(keyword)))
        .collect()
}

fn possession_objects(text: &str) -> BTreeSet<String> {
    let regex = Regex::new(
        r"(?:持有|握着|拿着|手里|口袋|交出|销毁)([^，。；！？、\s]{1,8})",
    )
    .expect("possession regex");
    regex
        .captures_iter(text)
        .filter_map(|cap| cap.get(1))
        .map(|value| {
            value
                .as_str()
                .trim_end_matches(['的', '着', '了'])
                .to_string()
        })
        .collect()
}

pub fn compare(before: &str, after: &str, entities: &[String]) -> FactDelta {
    let mut changes = Vec::new();
    let before_numbers = numbers(before);
    let after_numbers = numbers(after);
    let added_numbers: Vec<String> = after_numbers.difference(&before_numbers).cloned().collect();
    let removed_numbers: Vec<String> = before_numbers.difference(&after_numbers).cloned().collect();
    if !added_numbers.is_empty() || !removed_numbers.is_empty() {
        changes.push(FactChange {
            kind: "number".into(),
            before: removed_numbers.join("、"),
            after: added_numbers.join("、"),
            message: "润色前后数字不一致".into(),
        });
    }

    let added_entities: Vec<String> = entities
        .iter()
        .filter(|name| !before.contains(name.as_str()) && after.contains(name.as_str()))
        .cloned()
        .collect();
    let removed_entities: Vec<String> = entities
        .iter()
        .filter(|name| before.contains(name.as_str()) && !after.contains(name.as_str()))
        .cloned()
        .collect();
    if !added_entities.is_empty() || !removed_entities.is_empty() {
        changes.push(FactChange {
            kind: "entity".into(),
            before: removed_entities.join("、"),
            after: added_entities.join("、"),
            message: "润色前后人名或实体不一致".into(),
        });
    }

    let death_keywords = ["死亡", "死了", "身亡", "尸", "遗体", "死", "dead"];
    let before_death = matching_sentences(before, &death_keywords);
    let after_death = matching_sentences(after, &death_keywords);
    if before_death != after_death {
        changes.push(FactChange {
            kind: "death".into(),
            before: before_death.into_iter().collect::<Vec<_>>().join(" / "),
            after: after_death.into_iter().collect::<Vec<_>>().join(" / "),
            message: "润色改变了死亡状态或死亡信息".into(),
        });
    }

    let before_possession = possession_objects(before);
    let after_possession = possession_objects(after);
    if before_possession != after_possession {
        changes.push(FactChange {
            kind: "possession".into(),
            before: before_possession.into_iter().collect::<Vec<_>>().join(" / "),
            after: after_possession.into_iter().collect::<Vec<_>>().join(" / "),
            message: "润色改变了持有物或物品状态".into(),
        });
    }

    let before_chars = before.chars().filter(|c| !c.is_whitespace()).count();
    let after_chars = after.chars().filter(|c| !c.is_whitespace()).count();
    let possible_inflation =
        before_chars > 0 && after_chars as f64 > before_chars as f64 * 1.25;
    FactDelta {
        blocked: !changes.is_empty(),
        changes,
        added_numbers,
        removed_numbers,
        added_entities,
        removed_entities,
        possible_inflation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_injected_number_name_and_death() {
        let before = "林澈握着旧钥匙，走进雨里。信上写着 7。";
        let after = "秦岚握着黄铜钥匙，走进雨里。信上写着 9。父亲已经死了。";
        let delta = compare(before, after, &["林澈".into(), "秦岚".into(), "父亲".into()]);
        assert!(delta.blocked);
        assert!(delta.changes.iter().any(|change| change.kind == "number"));
        assert!(delta.changes.iter().any(|change| change.kind == "entity"));
        assert!(delta.changes.iter().any(|change| change.kind == "death"));
        assert!(delta.changes.iter().any(|change| change.kind == "possession"));
    }

    #[test]
    fn plain_style_polish_passes() {
        let before = "雨落在旧灯上。林澈握着钥匙。";
        let after = "雨丝撞在旧灯的铁罩上。林澈仍旧握着钥匙，指节发白。";
        let delta = compare(before, after, &["林澈".into()]);
        assert!(!delta.blocked);
        assert!(delta.possible_inflation);
    }
}
