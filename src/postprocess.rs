use crate::config::{ChineseVariant, DictionaryEntry, TextProcessingConfig, VoiceCommandAction};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryMode {
    Normal,
    CopyOnly,
    Discard,
}

pub struct ProcessedTranscript {
    pub text: String,
    pub delivery: DeliveryMode,
}

pub fn clean_transcript(text: &str, append_space: bool) -> String {
    let mut output = text
        .chars()
        .filter(|ch| !ch.is_control() && *ch != '\u{feff}' && *ch != '\u{fffd}')
        .collect::<String>();
    output = output.split_whitespace().collect::<Vec<_>>().join(" ");
    output = fix_cjk_spacing(&output);
    if append_space
        && !output
            .chars()
            .last()
            .map(char::is_whitespace)
            .unwrap_or(false)
    {
        output.push(' ');
    }
    output
}

pub fn format_transcript(text: &str, config: &TextProcessingConfig, append_space: bool) -> String {
    format_cleaned_transcript(clean_transcript(text, false), config, append_space)
}

pub fn process_transcript(
    text: &str,
    config: &TextProcessingConfig,
    append_space: bool,
) -> AppResult<ProcessedTranscript> {
    let cleaned = clean_transcript(text, false);
    let (output, delivery) = if config.voice_commands_enabled {
        if let Some(command) = config
            .voice_commands
            .iter()
            .find(|command| command.phrase == cleaned)
        {
            if command.phrase.trim().is_empty() {
                return Err(AppError::Configuration("語音命令片語不可為空".to_string()));
            }
            match &command.action {
                VoiceCommandAction::InsertText { text } => (text.clone(), DeliveryMode::Normal),
                VoiceCommandAction::CopyOnly { text } => (text.clone(), DeliveryMode::CopyOnly),
                VoiceCommandAction::Discard => {
                    return Ok(ProcessedTranscript {
                        text: String::new(),
                        delivery: DeliveryMode::Discard,
                    });
                }
            }
        } else {
            (cleaned, DeliveryMode::Normal)
        }
    } else {
        (cleaned, DeliveryMode::Normal)
    };

    Ok(ProcessedTranscript {
        text: format_cleaned_transcript(output, config, append_space),
        delivery,
    })
}

fn format_cleaned_transcript(
    mut output: String,
    config: &TextProcessingConfig,
    append_space: bool,
) -> String {
    if config.normalize_chinese_punctuation {
        output = normalize_chinese_punctuation(&output);
    }
    output = convert_chinese_variant(&output, config.chinese_variant);
    output = apply_dictionary_once(&output, &config.dictionary);
    if append_space && !output.ends_with(char::is_whitespace) {
        output.push(' ');
    }
    output
}

fn apply_dictionary_once(text: &str, dictionary: &[DictionaryEntry]) -> String {
    let mut entries = dictionary
        .iter()
        .filter(|entry| !entry.source.is_empty())
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.source.len()));

    let mut output = String::with_capacity(text.len());
    let mut index = 0;
    while index < text.len() {
        let matched = entries.iter().copied().find(|entry| {
            text[index..].starts_with(&entry.source)
                && replacement_boundaries_match(text, index, &entry.source)
        });
        if let Some(entry) = matched {
            output.push_str(&entry.replacement);
            index += entry.source.len();
        } else {
            let Some(ch) = text[index..].chars().next() else {
                break;
            };
            output.push(ch);
            index += ch.len_utf8();
        }
    }
    output
}

fn replacement_boundaries_match(text: &str, start: usize, source: &str) -> bool {
    let first = source.chars().next();
    let last = source.chars().next_back();
    let before = text[..start].chars().next_back();
    let after = text[start + source.len()..].chars().next();

    let starts_safely = !first.is_some_and(is_ascii_word)
        || before.is_none_or(|character| !is_ascii_word(character));
    let ends_safely =
        !last.is_some_and(is_ascii_word) || after.is_none_or(|character| !is_ascii_word(character));
    starts_safely && ends_safely
}

fn is_ascii_word(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn normalize_chinese_punctuation(text: &str) -> String {
    let chars = text.chars().collect::<Vec<_>>();
    let mut output = String::with_capacity(text.len());
    for (index, ch) in chars.iter().copied().enumerate() {
        let left = chars[..index]
            .iter()
            .rev()
            .copied()
            .find(|candidate| !candidate.is_whitespace());
        let right = chars[index + 1..]
            .iter()
            .copied()
            .find(|candidate| !candidate.is_whitespace());
        let touches_cjk = left.is_some_and(is_cjk) || right.is_some_and(is_cjk);
        let normalized = if touches_cjk {
            match ch {
                ',' => '，',
                '.' if !(left.is_some_and(|value| value.is_ascii_digit())
                    && right.is_some_and(|value| value.is_ascii_digit())) =>
                {
                    '。'
                }
                '!' => '！',
                '?' => '？',
                ':' => '：',
                ';' => '；',
                _ => ch,
            }
        } else {
            ch
        };
        output.push(normalized);
    }
    remove_spaces_around_cjk_punctuation(&output)
}

fn remove_spaces_around_cjk_punctuation(text: &str) -> String {
    let chars = text.chars().collect::<Vec<_>>();
    let mut output = String::with_capacity(text.len());
    for (index, ch) in chars.iter().copied().enumerate() {
        if ch == ' ' {
            let left = index
                .checked_sub(1)
                .and_then(|value| chars.get(value))
                .copied();
            let right = chars.get(index + 1).copied();
            if left.is_some_and(is_cjk_punctuation) || right.is_some_and(is_cjk_punctuation) {
                continue;
            }
        }
        output.push(ch);
    }
    output
}

fn is_cjk_punctuation(ch: char) -> bool {
    matches!(ch, '，' | '。' | '！' | '？' | '：' | '；')
}

fn convert_chinese_variant(text: &str, variant: ChineseVariant) -> String {
    match variant {
        ChineseVariant::Preserve => text.to_string(),
        ChineseVariant::Traditional => zhhz::Converter::new(zhhz::Config::S2twp).convert(text),
        ChineseVariant::Simplified => zhhz::Converter::new(zhhz::Config::Tw2sp).convert(text),
    }
}

fn fix_cjk_spacing(text: &str) -> String {
    let chars = text.chars().collect::<Vec<_>>();
    let mut output = String::with_capacity(text.len());
    for (index, current) in chars.iter().enumerate() {
        if *current == ' ' && index > 0 && index + 1 < chars.len() {
            let left = chars[index - 1];
            let right = chars[index + 1];
            if is_cjk(left) && is_cjk(right) {
                continue;
            }
        }
        output.push(*current);
    }
    output
}

fn is_cjk(ch: char) -> bool {
    matches!(ch as u32, 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        ChineseVariant, DictionaryEntry, TextProcessingConfig, VoiceCommand, VoiceCommandAction,
    };
    #[test]
    fn removes_spaces_between_chinese_characters() {
        assert_eq!(clean_transcript("你 好 世 界", false), "你好世界");
    }

    #[test]
    fn normalizes_chinese_punctuation_without_touching_decimal_points() {
        assert_eq!(
            normalize_chinese_punctuation("你好,世界!版本 3.14 ok?"),
            "你好，世界！版本 3.14 ok?"
        );
    }

    #[test]
    fn converts_between_taiwan_traditional_and_simplified() {
        assert_eq!(
            convert_chinese_variant("软件和鼠标", ChineseVariant::Traditional),
            "軟體和滑鼠"
        );
        assert_eq!(
            convert_chinese_variant("軟體和滑鼠", ChineseVariant::Simplified),
            "软件和鼠标"
        );
        assert_eq!(
            convert_chinese_variant("原樣", ChineseVariant::Preserve),
            "原樣"
        );
    }

    #[test]
    fn dictionary_uses_safe_ascii_word_boundaries_and_cjk_phrases() {
        let entries = vec![
            DictionaryEntry {
                source: "cat".to_string(),
                replacement: "dog".to_string(),
            },
            DictionaryEntry {
                source: "OpenAI".to_string(),
                replacement: "歐噴 AI".to_string(),
            },
            DictionaryEntry {
                source: "語音輸入".to_string(),
                replacement: "SpeakType".to_string(),
            },
        ];

        assert_eq!(
            apply_dictionary_once("concatenate cat OpenAIx OpenAI 好用的語音輸入", &entries),
            "concatenate dog OpenAIx 歐噴 AI 好用的SpeakType"
        );
    }

    #[test]
    fn dictionary_replacements_are_not_processed_recursively() {
        let entries = vec![DictionaryEntry {
            source: "a".to_string(),
            replacement: "aa".to_string(),
        }];

        assert_eq!(apply_dictionary_once("a", &entries), "aa");
    }

    fn command_config(action: VoiceCommandAction) -> TextProcessingConfig {
        TextProcessingConfig {
            voice_commands_enabled: true,
            voice_commands: vec![VoiceCommand {
                phrase: "刪除輸出".to_string(),
                action,
            }],
            ..TextProcessingConfig::default()
        }
    }

    #[test]
    fn voice_command_requires_strict_full_match() {
        let config = command_config(VoiceCommandAction::Discard);

        let matched = process_transcript("刪除輸出", &config, false).expect("process command");
        assert_eq!(matched.delivery, DeliveryMode::Discard);

        for phrase in ["請刪除輸出", "刪除輸出。", "刪除輸出吧"] {
            let result = process_transcript(phrase, &config, false).expect("process text");
            assert_eq!(result.delivery, DeliveryMode::Normal);
            assert!(!result.text.is_empty());
        }
    }

    #[test]
    fn voice_commands_are_disabled_by_default() {
        let mut config = command_config(VoiceCommandAction::Discard);
        config.voice_commands_enabled = false;

        let result = process_transcript("刪除輸出", &config, false).expect("process text");

        assert_eq!(result.delivery, DeliveryMode::Normal);
        assert_eq!(result.text, "刪除輸出");
    }

    #[test]
    fn voice_command_can_insert_text_or_force_copy_only() {
        let insert = command_config(VoiceCommandAction::InsertText {
            text: "\n".to_string(),
        });
        let copy = command_config(VoiceCommandAction::CopyOnly {
            text: "安全文字".to_string(),
        });

        let inserted = process_transcript("刪除輸出", &insert, false).expect("insert text");
        let copied = process_transcript("刪除輸出", &copy, false).expect("copy text");

        assert_eq!(inserted.text, "\n");
        assert_eq!(inserted.delivery, DeliveryMode::Normal);
        assert_eq!(copied.text, "安全文字");
        assert_eq!(copied.delivery, DeliveryMode::CopyOnly);
    }
}
