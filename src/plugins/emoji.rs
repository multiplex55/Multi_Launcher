use crate::actions::Action;
use crate::plugin::Plugin;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

pub struct EmojiPlugin {
    matcher: SkimMatcherV2,
}

impl EmojiPlugin {
    pub fn new() -> Self {
        Self {
            matcher: SkimMatcherV2::default(),
        }
    }
}

impl Default for EmojiPlugin {
    fn default() -> Self {
        Self::new()
    }
}

const EMOJIS: &[(&str, &str)] = &[
    // Common emoji shortcuts
    ("😀", "grinning"),
    ("😃", "smile"),
    ("😄", "happy"),
    ("😉", "wink"),
    ("😍", "love"),
    ("😂", "lol"),
    ("🤣", "rofl"),
    ("😎", "cool"),
    ("🥳", "party"),
    ("🤔", "thinking"),
    ("🤗", "hug"),
    ("🙃", "upside"),
    ("😭", "cry"),
    ("😅", "sweat_smile"),
    ("😇", "angel"),
    ("😡", "angry"),
    ("👍", "thumbs up"),
    ("🙏", "pray"),
    ("🎉", "celebrate"),
    ("❤", "heart"),
    ("💔", "broken heart"),
    ("✨", "sparkles"),
    ("🔥", "fire"),
    ("🤯", "mind blown"),
    ("🥺", "pleading"),
    ("🤓", "nerd"),
    ("🤩", "star struck"),
    ("🤡", "clown"),
    ("🥶", "cold"),
    ("🥴", "woozy"),
    ("🤪", "crazy"),
    ("😤", "frustrated"),
    ("🤮", "vomit"),
    ("👀", "eyes"),
    // Kaomojis
    ("¯\\_(ツ)_/¯", "shrug"),
    ("(╯°□°）╯︵ ┻━┻", "table flip"),
    ("(ᵔᴥᵔ)", "bear"),
    ("(ง'̀-'́)ง", "fight"),
    ("( ͡° ͜ʖ ͡°)", "lenny"),
    ("(ಥ﹏ಥ)", "cry kaomoji"),
    ("(^_^)", "happy kaomoji"),
    ("o_O", "confused"),
    ("(>_<)", "annoyed"),
    ("(¬_¬)", "unamused"),
    ("(☞ﾟヮﾟ)☞", "point"),
    ("(づ｡◕‿‿◕｡)づ", "hug kaomoji"),
    ("(╯︵╰,)", "sad"),
    ("(ღ˘⌣˘ღ)", "love kaomoji"),
    ("ヽ(•‿•)ノ", "excited"),
    ("≧◡≦", "smile kaomoji"),
    ("(｡◕‿◕｡)", "cute"),
];

impl Plugin for EmojiPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        const LIST_PREFIX: &str = "emoji list";
        let trimmed = query.trim();

        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, LIST_PREFIX) {
            let filter = rest.trim().to_lowercase();
            return EMOJIS
                .iter()
                .filter(|(_, name)| {
                    filter.is_empty() || self.matcher.fuzzy_match(name, &filter).is_some()
                })
                .map(|(emoji, _)| Action {
                    label: (*emoji).into(),
                    desc: "Emoji".into(),
                    action: format!("clipboard:{emoji}"),
                    args: None,
                })
                .collect();
        }

        const PREFIX: &str = "emoji ";
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, PREFIX) {
            let filter = rest.trim().to_lowercase();
            if !filter.is_empty() {
                return EMOJIS
                    .iter()
                    .filter(|(_, name)| self.matcher.fuzzy_match(name, &filter).is_some())
                    .map(|(emoji, _)| Action {
                        label: (*emoji).into(),
                        desc: "Emoji".into(),
                        action: format!("clipboard:{emoji}"),
                        args: None,
                    })
                    .collect();
            }
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        "emoji"
    }

    fn description(&self) -> &str {
        "Emoji search (prefix: `emoji`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "emoji".into(),
                desc: "Emoji".into(),
                action: "query:emoji ".into(),
                args: None,
            },
            Action {
                label: "emoji list".into(),
                desc: "Emoji".into(),
                action: "query:emoji list".into(),
                args: None,
            },
        ]
    }
}
