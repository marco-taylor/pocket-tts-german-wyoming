use pocket_tts_wyoming::{
    german_normalizer::{normalize, normalize_safe, normalize_with_rules},
    safe_rules::{ACCEPTED, Rule},
};
#[test]
fn prices_are_exact() {
    for (raw, want) in [
        ("1 €", "ein Euro"),
        ("1,50 €", "ein Euro und fünfzig Cent"),
        ("12,99 Euro", "zwölf Euro und neunundneunzig Cent"),
        ("0,284 Euro", "null Komma zwei acht vier Euro"),
        ("28,4 ct", "achtundzwanzig Komma vier Cent"),
        ("1 Cent", "ein Cent"),
        (
            "2.450,50 €",
            "zweitausendvierhundertfünfzig Euro und fünfzig Cent",
        ),
        ("0,0005 €", "null Komma null null null fünf Euro"),
    ] {
        assert_eq!(normalize_safe(raw), want, "{raw}");
    }
}
#[test]
fn no_broad_title_company_expansions() {
    for raw in [
        "Fr. Müller",
        "Fr. Abend",
        "am Fr. Abend",
        "Freitag Abend",
        "Firma Muster AG",
        "Status AG",
        "Dr. Müller",
        "Ich trinke Dr. Pepper.",
    ] {
        assert_eq!(normalize_safe(raw), normalize(raw));
    }
}
#[test]
fn protected_tokens() {
    for raw in [
        "192.0.2.100",
        "127.0.0.1",
        "1.2.3",
        "v1.2.3",
        "Version 1.2.3",
        "http://192.0.2.100:8123",
        "https://example.com/test",
        "/api/test/123",
        "/srv/example/appdata/test",
        "Home Assistant HA TTS STT LLM CPU GPU Q8 PCM16 de-DE",
        "Name \"ca. 20\" bleibt.",
        "Name \"12,99 €\" bleibt.",
        "Name \"7. Okt. 2026\" bleibt.",
    ] {
        for &rule in ACCEPTED {
            assert_eq!(
                normalize_with_rules(raw, &[rule]),
                normalize(raw),
                "{rule:?}: {raw}"
            );
        }
        assert_eq!(normalize_safe(raw), normalize(raw));
    }
}
#[test]
fn accepted_rules_are_atomic_and_idempotent() {
    for raw in [
        "Das gilt z. B. für die Küche bzw. das Bad.",
        "Gerät Nr. 7 kostet ca. 12,99 € inkl. Steuer.",
        "Am 7. Okt. 2026 sind Heizung etc. aktiviert.",
    ] {
        let s = normalize_safe(raw);
        assert_eq!(normalize_safe(&s), s);
    }
}
#[test]
fn all_calendar_contexts() {
    let months = [
        "Januar",
        "Februar",
        "März",
        "April",
        "Mai",
        "Juni",
        "Juli",
        "August",
        "September",
        "Oktober",
        "November",
        "Dezember",
    ];
    for year in [2024, 2025, 2026] {
        for month in months {
            for day in 1..=31 {
                for context in ["", "der ", "am ", "vom ", "zum ", "bis zum "] {
                    for dot in ["", "."] {
                        for leading in [false, true] {
                            for with_year in [false, true] {
                                let d = if leading {
                                    format!("{day:02}")
                                } else {
                                    day.to_string()
                                };
                                let y = if with_year {
                                    format!(" {year}")
                                } else {
                                    String::new()
                                };
                                let input = format!("{context}{d}{dot} {month}{y}");
                                let expected = normalize(&input);
                                assert_eq!(normalize_safe(&input), expected, "{input}");
                            }
                        }
                    }
                }
            }
        }
    }
}
#[test]
fn guard_approximation() {
    assert_eq!(normalize_safe("ca. -Modell"), normalize("ca. -Modell"));
    assert_eq!(
        normalize_with_rules("ca. -20 Grad", &[Rule::Approx]),
        "circa minus zwanzig Grad"
    );
}
