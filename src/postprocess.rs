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
    #[test]
    fn removes_spaces_between_chinese_characters() {
        assert_eq!(clean_transcript("你 好 世 界", false), "你好世界");
    }
}
