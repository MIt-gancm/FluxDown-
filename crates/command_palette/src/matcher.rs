//! 查询匹配与打分：模糊子序列 + 拼音（全拼 / 首字母 / 混拼，含多音字）。
//!
//! 文本切成「单元」：每个字母数字字符一个单元，汉字单元额外携带全部无声调读音；空白与
//! 标点不成单元，只把下一个单元标成词首。查询词按单元顺序匹配：一个单元要么被跳过，要么
//! 消耗查询的一段——普通字符恰好一个字符，汉字可以是字本身或任一读音的任意非空前缀，所以
//! `qbzt`、`quanbu`、`qbzanting`、`全部` 都命中「全部暂停」。动态规划取最高分。

use pinyin::ToPinyinMulti as _;

/// 每个被消耗的查询字符。
const SCORE_MATCH: i32 = 16;
/// 单元位于词首（文本开头、分隔符 / 驼峰 / 汉字边界后、每个汉字）。
const BONUS_BOUNDARY: i32 = 8;
/// 从文本第一个单元开始匹配。
const BONUS_FIRST_UNIT: i32 = 6;
/// 与上一个单元连续匹配。
const BONUS_CONSECUTIVE: i32 = 8;
/// 汉字读音被完整拼出（`zan` 比 `za` 更确定）。
const BONUS_FULL_SYLLABLE: i32 = 2;
/// 首个匹配前每跳过一个单元（只计前 [`LEADING_PENALTY_UNITS`] 个）。
const PENALTY_LEADING: i32 = 1;
const LEADING_PENALTY_UNITS: usize = 8;
/// 匹配中途断开的首个跳过单元 / 之后每个跳过单元。
const PENALTY_GAP_START: i32 = 3;
const PENALTY_GAP_EXTEND: i32 = 1;
/// 关键词（说明、分组名）命中相对标题 / 别名的降权。
const PENALTY_KEYWORD: i32 = 24;
/// 不可达状态。取 `MIN / 4` 保证加减分不溢出。
const UNREACHABLE: i32 = i32::MIN / 4;

struct Unit {
    /// 小写后的字符本身。
    ch: char,
    /// 汉字的无声调读音（多音字全部列出、去重）；非汉字为空。
    readings: Vec<&'static str>,
    boundary: bool,
}

/// 预处理后的可搜索文本；打开面板时为每个条目构建一次。
pub(crate) struct SearchText {
    units: Vec<Unit>,
}

impl SearchText {
    pub(crate) fn new(text: &str) -> Self {
        let mut units = Vec::with_capacity(text.len());
        let mut boundary = true;
        let mut prev_lowercase = false;
        for ch in text.chars() {
            if !ch.is_alphanumeric() {
                boundary = true;
                prev_lowercase = false;
                continue;
            }
            let mut readings: Vec<&'static str> = Vec::new();
            if let Some(multi) = ch.to_pinyin_multi() {
                for reading in multi {
                    let plain = reading.plain();
                    if !readings.contains(&plain) {
                        readings.push(plain);
                    }
                }
            }
            let han = !readings.is_empty();
            let camel = ch.is_uppercase() && prev_lowercase;
            units.push(Unit {
                ch: lowercase(ch),
                readings,
                boundary: boundary || camel || han,
            });
            // 汉字后紧跟的字母数字（「下载BT」的 B）同样是词首。
            boundary = han;
            prev_lowercase = ch.is_lowercase();
        }
        Self { units }
    }
}

/// 预处理后的查询：按空白拆成若干词，每个词必须命中（顺序无关），标点忽略。
pub(crate) struct Query {
    terms: Vec<Vec<char>>,
}

impl Query {
    pub(crate) fn new(raw: &str) -> Self {
        let terms = raw
            .split_whitespace()
            .map(|term| {
                term.chars()
                    .filter(|ch| ch.is_alphanumeric())
                    .map(lowercase)
                    .collect::<Vec<char>>()
            })
            .filter(|term| !term.is_empty())
            .collect();
        Self { terms }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }
}

/// 一个条目的全部可搜索文本。
pub(crate) struct Searchable {
    /// 标题：允许模糊（中间可断开）。
    pub(crate) title: SearchText,
    /// 与标题同权的别名（另一语言的标题、同义词）：允许模糊。
    pub(crate) aliases: Vec<SearchText>,
    /// 说明、分组等补充文本：只接受连续命中，并降权。
    pub(crate) keywords: Vec<SearchText>,
}

/// 打分器；复用内部行缓冲，逐键输入时不重复分配。
#[derive(Default)]
pub(crate) struct Scorer {
    current: Vec<i32>,
    next: Vec<i32>,
}

impl Scorer {
    /// 条目得分：每个查询词取标题 / 别名 / 关键词中的最高分后求和；任一词无命中返回 `None`。
    pub(crate) fn score(&mut self, query: &Query, item: &Searchable) -> Option<i32> {
        let mut total = 0;
        for term in &query.terms {
            let mut best = self.score_text(term, &item.title, true);
            for alias in &item.aliases {
                best = best.max(self.score_text(term, alias, true));
            }
            for keyword in &item.keywords {
                let score = self
                    .score_text(term, keyword, false)
                    .map(|score| score - PENALTY_KEYWORD);
                best = best.max(score);
            }
            total += best?;
        }
        Some(total)
    }

    /// 单个查询词对单个文本的最高分；`allow_gaps = false` 时命中必须连续。
    fn score_text(&mut self, term: &[char], text: &SearchText, allow_gaps: bool) -> Option<i32> {
        let len = term.len();
        if len == 0 || text.units.is_empty() {
            return None;
        }
        // 状态 = (已消耗查询字符数 j, 上一单元是否命中 c)，下标 j * 2 + c。
        let width = (len + 1) * 2;
        self.current.clear();
        self.current.resize(width, UNREACHABLE);
        self.next.clear();
        self.next.resize(width, UNREACHABLE);
        self.current[0] = 0;
        let mut best = UNREACHABLE;

        for (index, unit) in text.units.iter().enumerate() {
            self.next.fill(UNREACHABLE);
            for consumed in 0..=len {
                for matched_prev in 0..2 {
                    let score = self.current[consumed * 2 + matched_prev];
                    if score <= UNREACHABLE {
                        continue;
                    }
                    // 跳过本单元。
                    let skip = if consumed == 0 {
                        Some(if index < LEADING_PENALTY_UNITS {
                            PENALTY_LEADING
                        } else {
                            0
                        })
                    } else if consumed == len {
                        Some(0)
                    } else if !allow_gaps {
                        None
                    } else if matched_prev == 1 {
                        Some(PENALTY_GAP_START)
                    } else {
                        Some(PENALTY_GAP_EXTEND)
                    };
                    if let Some(cost) = skip {
                        relax(&mut self.next[consumed * 2], score - cost);
                    }
                    if consumed == len {
                        continue;
                    }

                    let mut bonus = 0;
                    if unit.boundary {
                        bonus += BONUS_BOUNDARY;
                    }
                    if index == 0 {
                        bonus += BONUS_FIRST_UNIT;
                    }
                    if matched_prev == 1 {
                        bonus += BONUS_CONSECUTIVE;
                    }

                    if term[consumed] == unit.ch {
                        relax(
                            &mut self.next[(consumed + 1) * 2 + 1],
                            score + SCORE_MATCH + bonus,
                        );
                    }
                    for reading in &unit.readings {
                        let bytes = reading.as_bytes();
                        let mut taken = 0;
                        let mut gained = bonus;
                        while taken < bytes.len()
                            && consumed + taken < len
                            && term[consumed + taken] == char::from(bytes[taken])
                        {
                            taken += 1;
                            gained += SCORE_MATCH;
                            let full = if taken == bytes.len() {
                                BONUS_FULL_SYLLABLE
                            } else {
                                0
                            };
                            relax(
                                &mut self.next[(consumed + taken) * 2 + 1],
                                score + gained + full,
                            );
                        }
                    }
                }
            }
            best = best.max(self.next[len * 2]).max(self.next[len * 2 + 1]);
            std::mem::swap(&mut self.current, &mut self.next);
        }
        (best > UNREACHABLE).then_some(best)
    }
}

fn relax(slot: &mut i32, value: i32) {
    if value > *slot {
        *slot = value;
    }
}

fn lowercase(ch: char) -> char {
    ch.to_lowercase().next().unwrap_or(ch)
}

#[cfg(test)]
mod tests {
    use super::{Query, Scorer, SearchText, Searchable};

    fn item(title: &str, aliases: &[&str], keywords: &[&str]) -> Searchable {
        Searchable {
            title: SearchText::new(title),
            aliases: aliases.iter().map(|text| SearchText::new(text)).collect(),
            keywords: keywords.iter().map(|text| SearchText::new(text)).collect(),
        }
    }

    fn score(query: &str, item: &Searchable) -> Option<i32> {
        Scorer::default().score(&Query::new(query), item)
    }

    #[test]
    fn pinyin_initials_full_and_mixed_spellings_match_han_titles() {
        let pause_all = item("全部暂停", &[], &[]);
        for query in [
            "qbzt",
            "quanbuzanting",
            "qbzanting",
            "quanbzt",
            "全部",
            "暂停",
            "zant",
        ] {
            assert!(score(query, &pause_all).is_some(), "{query} should match");
        }
        assert!(score("qbzx", &pause_all).is_none());
    }

    #[test]
    fn heteronym_readings_are_all_searchable() {
        // 「重」读 chong / zhong，「行」读 xing / hang。
        assert!(score("cxxz", &item("重新下载", &[], &[])).is_some());
        assert!(score("chongxin", &item("重新下载", &[], &[])).is_some());
        assert!(score("yinhang", &item("银行", &[], &[])).is_some());
    }

    #[test]
    fn english_fuzzy_prefers_prefix_and_word_starts_over_scattered_letters() {
        let pause_all = item("Pause All", &[], &[]);
        let scattered = item("Open parallel audio sessions", &[], &[]);
        let prefix = score("pause", &pause_all).unwrap_or_default();
        let fuzzy = score("pause", &scattered).unwrap_or_default();
        assert!(prefix > fuzzy, "prefix {prefix} vs scattered {fuzzy}");
        // 跨词连续与驼峰词首。
        assert!(score("pauseall", &pause_all).is_some());
        assert!(score("ws", &item("WebSocket", &[], &[])).is_some());
        assert!(score("useragent", &item("User-Agent", &[], &[])).is_some());
    }

    #[test]
    fn every_whitespace_term_must_match_in_any_order() {
        let resume = item("Resume All", &[], &[]);
        assert!(score("all resume", &resume).is_some());
        assert!(score("resume quit", &resume).is_none());
    }

    #[test]
    fn aliases_let_english_queries_find_chinese_titles() {
        let quit = item("退出", &["Quit"], &[]);
        assert!(score("quit", &quit).is_some());
        assert!(score("tc", &quit).is_some());
    }

    #[test]
    fn keywords_need_contiguous_hits_and_rank_below_titles() {
        let described = item("最大连接数", &[], &["每个任务同时建立的连接上限"]);
        assert!(score("任务同时", &described).is_some());
        // 关键词中断开的字母不算命中。
        assert!(score("mgtl", &described).is_none());
        let titled = item("任务同时", &[], &[]);
        let via_title = score("rwts", &titled).unwrap_or_default();
        let via_keyword = score("rwts", &described).unwrap_or_default();
        assert!(via_title > via_keyword);
    }

    #[test]
    fn blank_or_punctuation_only_query_is_empty() {
        assert!(Query::new("   ").is_empty());
        assert!(Query::new(" - / ").is_empty());
        assert!(!Query::new("a").is_empty());
    }
}
