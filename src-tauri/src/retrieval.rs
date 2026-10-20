use std::sync::OnceLock;

use jieba_rs::Jieba;

fn jieba() -> &'static Jieba {
    static INSTANCE: OnceLock<Jieba> = OnceLock::new();
    INSTANCE.get_or_init(Jieba::new)
}

pub fn tokenize(text: &str) -> String {
    jieba()
        .cut(text, false)
        .into_iter()
        .filter(|token| !token.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_chinese_into_searchable_terms() {
        let tokens = tokenize("林澈在雨夜握紧旧钥匙");
        assert!(tokens.contains("林澈") || tokens.contains("雨夜"));
        assert!(tokens.split(' ').count() >= 3);
    }
}
