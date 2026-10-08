use crate::config::GermanConfig;
use anyhow::{Result, ensure};
use ptts::Tokenizer;

pub fn prepare(text: &str, cfg: &GermanConfig) -> Result<(String, usize)> {
    let mut text = text.trim().to_owned();
    if !cfg.replace_characters.is_empty() {
        text = text
            .chars()
            .map(|c| {
                cfg.replace_characters
                    .get(&c)
                    .cloned()
                    .unwrap_or_else(|| c.to_string())
            })
            .collect();
        text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        text = regex::Regex::new(r"([.!?…])\s*[,;:]")?
            .replace_all(&text, "$1")
            .into_owned();
    }
    ensure!(!text.is_empty(), "text prompt cannot be empty");
    text = text.replace(['\n', '\r'], " ").replace("  ", " ");
    if cfg.remove_semicolons {
        text = text.replace(';', ",");
    }
    let after_eos = if text.split_whitespace().count() <= 4 {
        3
    } else {
        1
    };
    if cfg.capitalize_first_letter {
        let mut chars = text.chars();
        let first = chars.next().unwrap();
        if !first.is_uppercase() {
            text = first.to_uppercase().to_string() + chars.as_str();
        }
    }
    if cfg.append_terminal_punctuation {
        text = terminal_punctuation(&text);
    }
    if cfg.pad_with_spaces_for_short_inputs && text.split_whitespace().count() < 5 {
        text = "        ".to_owned() + &text;
    }
    // Current generate_audio_stream adds two to the text-dependent guess.
    Ok((
        text,
        cfg.model_recommended_frames_after_eos
            .unwrap_or(after_eos + 2),
    ))
}

fn terminal_punctuation(text: &str) -> String {
    let core = text.trim_end_matches(|c| "\"'”’)]» ".contains(c));
    let closers = text[core.len()..].trim();
    match core.chars().last() {
        None => text.to_owned(),
        Some(c) if ".!?…".contains(c) => text.to_owned(),
        Some(c) if ",;:-–—".contains(c) => format!(
            "{}.{closers}",
            core.trim_end_matches(|c| ",;:-–— ".contains(c))
        ),
        _ => format!("{text}."),
    }
}

fn boundaries(
    tokens: &[u32],
    marks: &[u32],
    tok: &dyn Tokenizer,
    decimal: bool,
) -> Result<Vec<usize>> {
    let mut result = vec![0];
    let mut prev_mark = false;
    for (i, id) in tokens.iter().enumerate() {
        if marks.contains(id) {
            prev_mark = true;
        } else {
            if prev_mark {
                let skip = if decimal {
                    let prefix = tok.decode(&tokens[..i])?;
                    let suffix = tok.decode(&tokens[i..])?;
                    let mut chars = prefix.chars().rev();
                    chars.next() == Some('.')
                        && chars.next().is_some_and(|c| c.is_numeric())
                        && suffix.chars().next().is_some_and(|c| c.is_numeric())
                } else {
                    false
                };
                if !skip {
                    result.push(i);
                }
            }
            prev_mark = false;
        }
    }
    result.push(tokens.len());
    Ok(result)
}
fn segments(
    tokens: &[u32],
    boundaries: &[usize],
    tok: &dyn Tokenizer,
) -> Result<Vec<(usize, String)>> {
    boundaries
        .windows(2)
        .map(|w| Ok((w[1] - w[0], tok.decode(&tokens[w[0]..w[1]])?)))
        .collect()
}

pub struct TextChunk {
    pub text: String,
    pub tokens: Vec<u32>,
    pub frames_after_eos: usize,
}

/// Temporary live-request diagnostics: make even ordinary spaces visible.
pub fn diagnostic_escaped(text: &str) -> String {
    let mut escaped = String::from("\"");
    for c in text.chars() {
        if c == ' ' {
            escaped.push_str("\\u{0020}");
        } else {
            escaped.extend(c.escape_default());
        }
    }
    escaped.push('"');
    escaped
}

pub fn chunks(
    text: &str,
    cfg: &GermanConfig,
    tok: &dyn Tokenizer,
    max: usize,
) -> Result<Vec<TextChunk>> {
    chunks_observed(text, cfg, tok, max, &mut |_, _| {})
}

/// Observe the exact input at the text-tokenization call sites; no text changes.
pub fn chunks_observed(
    text: &str,
    cfg: &GermanConfig,
    tok: &dyn Tokenizer,
    max: usize,
    observe: &mut dyn FnMut(&str, &str),
) -> Result<Vec<TextChunk>> {
    let (prepared, _) = prepare(text, cfg)?;
    observe("full_input", prepared.trim());
    let tokens = tok.encode(prepared.trim())?;
    let eos = tok.encode(".!...?")?;
    let marks = eos.get(1..).unwrap_or(&[]);
    let sentences = segments(&tokens, &boundaries(&tokens, marks, tok, true)?, tok)?;
    let fallback = tok.encode(",;:")?;
    let mut refined = vec![];
    for (n, sentence) in sentences {
        if n > max {
            observe("sentence_split", sentence.trim());
            let tokens = tok.encode(sentence.trim())?;
            let sub = segments(
                &tokens,
                &boundaries(&tokens, fallback.get(1..).unwrap_or(&[]), tok, false)?,
                tok,
            )?;
            if sub.len() > 1 {
                refined.extend(sub);
                continue;
            }
        }
        refined.push((n, sentence));
    }
    let mut strings = vec![];
    let mut current = String::new();
    let mut count = 0;
    for (n, sentence) in refined {
        if !current.is_empty() && count + n > max {
            strings.push(current.trim().to_owned());
            current.clear();
            count = 0;
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(&sentence);
        count += n;
    }
    if !current.is_empty() {
        strings.push(current.trim().to_owned());
    }
    strings
        .into_iter()
        .map(|s| {
            let (text, frames_after_eos) = prepare(&s, cfg)?;
            observe("synthesis_segment", &text);
            let tokens = tok.encode(&text)?;
            ensure!(!tokens.is_empty(), "tokenizer produced no tokens");
            Ok(TextChunk {
                text,
                tokens,
                frames_after_eos,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cfg() -> GermanConfig {
        serde_yaml::from_str(include_str!("../../../tests/fixtures/german.yaml")).unwrap()
    }
    #[test]
    fn current_cleanup_and_tail() {
        assert_eq!(
            prepare(" „hallo?“, (du); ", &cfg()).unwrap(),
            ("Hallo? du.".into(), 5)
        );
        assert_eq!(prepare("eins zwei drei vier fünf", &cfg()).unwrap().1, 3);
        assert!(prepare("  \n", &cfg()).is_err());
        assert_eq!(terminal_punctuation("Hallo,"), "Hallo.");
    }
}
