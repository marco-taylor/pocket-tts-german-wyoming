//! Native, selectively adapted pure Misaki rules. No G2P, no full replacement chain.
//! Source: semidark/misaki bbaf917e7bf7fbbe830f74f4acda6583ce1c0caf, Apache-2.0.
use fancy_regex::Regex;
use std::sync::LazyLock;
static ABBREVIATIONS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    crate::misaki_normalizer::ABBREVIATIONS
        .iter()
        .map(|(p, r)| (Regex::new(p).unwrap(), *r))
        .collect()
});
static CURRENCY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\A(?:([€$£¥])\s*(\d[\d.,]*)|(\d[\d.,]*)\s*([€$£¥]))").unwrap());
pub(crate) fn abbreviation_at(text: &str, start: usize, limit: usize) -> Option<(usize, String)> {
    let source = &text[..limit];
    if text[..start]
        .chars()
        .next_back()
        .is_some_and(|c| c.is_alphanumeric() || c == '_')
    {
        return None;
    }
    let prefix = source[start..]
        .split(|c: char| !c.is_alphabetic())
        .next()?
        .to_lowercase();
    if ![
        "dr", "prof", "hr", "fr", "dipl", "str", "nr", "tel", "abt", "gmbh", "ag", "z", "d", "u",
        "bzw", "usw", "etc", "ca", "vgl", "inkl", "exkl", "ggf", "i", "o",
    ]
    .contains(&prefix.as_str())
    {
        return None;
    }
    for (re, replacement) in ABBREVIATIONS.iter() {
        let Some(c) = re.captures_from_pos(source, start).ok().flatten() else {
            continue;
        };
        let m = c.get(0)?;
        if m.start() == start {
            return Some((m.end() - start, replacement.trim_end().to_owned()));
        }
    }
    None
}
pub(crate) fn currency_at(text: &str, start: usize, limit: usize) -> Option<(usize, String)> {
    if !text[start..limit].starts_with(|c: char| c.is_ascii_digit() || "€$£¥".contains(c)) {
        return None;
    }
    let c = CURRENCY.captures(&text[start..limit]).ok().flatten()?;
    let m = c.get(0)?;
    if m.start() != 0 {
        return None;
    }
    let (sym, amount) = if let Some(sym) = c.get(1) {
        (sym.as_str(), c.get(2)?.as_str())
    } else {
        (c.get(4)?.as_str(), c.get(3)?.as_str())
    };
    let mut end = start + m.end();
    let mut number = amount;
    // Preserve terminal punctuation; do not consume an incomplete numeric group.
    if c.get(1).is_some() && number.ends_with('.') {
        number = number.trim_end_matches('.');
        end -= amount.len() - number.len();
    }
    if text[end..]
        .chars()
        .next()
        .is_some_and(|c| c.is_alphanumeric() || c == '_')
    {
        return None;
    }
    // Validate the German grouping before adopting the upstream rounding/rendering.
    let mut parts = number.split(',');
    let integral = parts.next()?;
    let fractional = parts.next();
    if parts.next().is_some()
        || fractional.is_some_and(|f| f.is_empty() || !f.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    let groups: Vec<_> = integral.split('.').collect();
    if groups
        .iter()
        .any(|g| g.is_empty() || !g.bytes().all(|b| b.is_ascii_digit()))
        || groups.len() > 1 && (groups[0].len() > 3 || groups[1..].iter().any(|g| g.len() != 3))
    {
        return None;
    }
    if integral.replace('.', "").parse::<u64>().is_err() {
        return None;
    }
    Some((end - start, crate::misaki_normalizer::currency(sym, number)))
}
