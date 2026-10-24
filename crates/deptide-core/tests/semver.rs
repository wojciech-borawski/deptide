use std::cmp::Ordering;

use deptide_core::util::semver::{Range, Version};

const PARSE: &[(&str, Option<&str>, Option<&str>)] = &[
    ("1.2.3", Some("1.2.3"), Some("1.2.3")),
    ("v1.2.3", Some("1.2.3"), Some("1.2.3")),
    ("=1.2.3", None, Some("1.2.3")),
    (" 1.2.3 ", Some("1.2.3"), Some("1.2.3")),
    ("\u{9}1.2.3\u{a}", Some("1.2.3"), Some("1.2.3")),
    ("\u{feff}1.2.3", Some("1.2.3"), Some("1.2.3")),
    ("\u{85}1.2.3", None, None),
    ("\u{a0}1.2.3\u{3000}", Some("1.2.3"), Some("1.2.3")),
    ("1.2.3+build.5", Some("1.2.3"), Some("1.2.3")),
    ("1.2.3+", None, None),
    ("01.2.3", None, None),
    ("1.2.3-01", None, None),
    ("1.2.3-0a", Some("1.2.3-0a"), Some("1.2.3-0a")),
    ("1.2.3--", Some("1.2.3--"), Some("1.2.3--")),
    ("9007199254740991.0.0", Some("9007199254740991.0.0"), Some("9007199254740991.0.0")),
    ("9007199254740992.0.0", None, None),
    ("900719925474100.0.0", Some("900719925474100.0.0"), Some("900719925474100.0.0")),
    ("1.2.3garbage", None, None),
    ("1.2", None, None),
    ("", None, None),
    ("vv1.2.3", None, Some("1.2.3")),
    ("V1.2.3", None, None),
    ("1.2.3-", None, None),
    ("1.2.3-a..b", None, None),
    ("1.2.3-ABC-2", Some("1.2.3-ABC-2"), Some("1.2.3-ABC-2")),
    ("1.2.3-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", Some("1.2.3-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"), Some("1.2.3-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")),
    ("1.2.3-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", None, None),
    ("v 1.2.3", None, Some("1.2.3")),
    ("1.2.3-9007199254740992", Some("1.2.3-9007199254740992"), Some("1.2.3-9007199254740992")),
    ("0.0.0", Some("0.0.0"), Some("0.0.0")),
];

const COMPARE: &[(&str, &str, Ordering)] = &[
    ("1.0.0-alpha", "1.0.0-alpha.1", Ordering::Less),
    ("1.0.0-alpha", "1.0.0-beta.11", Ordering::Less),
    ("1.0.0-alpha.1", "1.0.0-alpha.beta", Ordering::Less),
    ("1.0.0-alpha.1", "1.0.0-rc.1", Ordering::Less),
    ("1.0.0-alpha.beta", "1.0.0-beta", Ordering::Less),
    ("1.0.0-alpha.beta", "1.0.0", Ordering::Less),
    ("1.0.0-beta", "1.0.0-beta.2", Ordering::Less),
    ("1.0.0-beta", "1.0.0-2", Ordering::Greater),
    ("1.0.0-beta.2", "1.0.0-beta.11", Ordering::Less),
    ("1.0.0-beta.2", "1.0.0-10", Ordering::Greater),
    ("1.0.0-beta.11", "1.0.0-rc.1", Ordering::Less),
    ("1.0.0-beta.11", "1.0.0-9", Ordering::Greater),
    ("1.0.0-rc.1", "1.0.0", Ordering::Less),
    ("1.0.0-rc.1", "1.0.0-a", Ordering::Greater),
    ("1.0.0", "1.0.0-2", Ordering::Greater),
    ("1.0.0", "1.0.0-A", Ordering::Greater),
    ("1.0.0-2", "1.0.0-10", Ordering::Less),
    ("1.0.0-2", "1.0.0-ABC-1", Ordering::Less),
    ("1.0.0-10", "1.0.0-9", Ordering::Greater),
    ("1.0.0-10", "1.0.0-ABC-2", Ordering::Less),
    ("1.0.0-9", "1.0.0-a", Ordering::Less),
    ("1.0.0-9", "1.0.0+a", Ordering::Less),
    ("1.0.0-a", "1.0.0-A", Ordering::Greater),
    ("1.0.0-a", "1.0.0+b", Ordering::Less),
    ("1.0.0-A", "1.0.0-ABC-1", Ordering::Less),
    ("1.0.0-A", "900719925474100.0.0", Ordering::Less),
    ("1.0.0-ABC-1", "1.0.0-ABC-2", Ordering::Less),
    ("1.0.0-ABC-1", "900719925474101.0.0", Ordering::Less),
    ("1.0.0-ABC-2", "1.0.0+a", Ordering::Less),
    ("1.0.0-ABC-2", "9007199254740990.0.0", Ordering::Less),
    ("1.0.0+a", "1.0.0+b", Ordering::Equal),
    ("1.0.0+a", "9007199254740991.0.0", Ordering::Less),
    ("1.0.0+b", "900719925474100.0.0", Ordering::Less),
    ("1.0.0+b", "1.0.0-9007199254740993", Ordering::Greater),
    ("900719925474100.0.0", "900719925474101.0.0", Ordering::Less),
    (
        "900719925474100.0.0",
        "1.0.0-9007199254740992",
        Ordering::Greater,
    ),
    (
        "900719925474101.0.0",
        "9007199254740990.0.0",
        Ordering::Less,
    ),
    (
        "900719925474101.0.0",
        "1.0.0-9007199254740991",
        Ordering::Greater,
    ),
    (
        "9007199254740990.0.0",
        "9007199254740991.0.0",
        Ordering::Less,
    ),
    (
        "9007199254740990.0.0",
        "1.0.0-9007199254740990",
        Ordering::Greater,
    ),
    (
        "9007199254740991.0.0",
        "1.0.0-9007199254740993",
        Ordering::Greater,
    ),
    ("9007199254740991.0.0", "v1.2.3", Ordering::Greater),
    (
        "1.0.0-9007199254740993",
        "1.0.0-9007199254740992",
        Ordering::Equal,
    ),
    ("1.0.0-9007199254740993", "1.2.3", Ordering::Less),
    (
        "1.0.0-9007199254740992",
        "1.0.0-9007199254740991",
        Ordering::Greater,
    ),
    ("1.0.0-9007199254740992", "1.0.0-0a", Ordering::Less),
    (
        "1.0.0-9007199254740991",
        "1.0.0-9007199254740990",
        Ordering::Greater,
    ),
    ("1.0.0-9007199254740991", "1.0.0-1a", Ordering::Less),
    ("1.0.0-9007199254740990", "v1.2.3", Ordering::Less),
    ("1.0.0-9007199254740990", "1.0.0-alpha", Ordering::Less),
    ("v1.2.3", "1.2.3", Ordering::Equal),
    ("v1.2.3", "1.0.0-alpha.1", Ordering::Greater),
    ("1.2.3", "1.0.0-0a", Ordering::Greater),
    ("1.2.3", "1.0.0-alpha.beta", Ordering::Greater),
    ("1.0.0-0a", "1.0.0-1a", Ordering::Less),
    ("1.0.0-0a", "1.0.0-beta", Ordering::Less),
    ("1.0.0-1a", "1.0.0-alpha", Ordering::Less),
    ("1.0.0-1a", "1.0.0-beta.2", Ordering::Less),
];

const SATISFIES_VERSIONS: [&str; 16] = [
    "1.2.3",
    "1.2.4",
    "2.0.0",
    "0.9.0",
    "1.2.3-beta.2",
    "1.2.3-beta.3",
    "1.2.4-beta.1",
    "0.2.5",
    "0.0.3",
    "0.0.4",
    "2.3.4",
    "2.3.5",
    "2.4.0",
    "3.1.0-ABC-2",
    "9007199254740991.0.0",
    "v1.2.3",
];
const SATISFIES: &[(&str, &str)] = &[
    ("*", "1111000111111011"),
    ("", "1111000111111011"),
    ("x", "1111000111111011"),
    ("1.x", "1100000000000001"),
    ("1.2.x", "1100000000000001"),
    ("^1.2.3", "1100000000000001"),
    ("^0.2.3", "0000000100000000"),
    ("^0.0.3", "0000000010000000"),
    ("^1.2.3-beta.2", "1100110000000001"),
    ("~1.2.3", "1100000000000001"),
    ("~1.2", "1100000000000001"),
    ("~1", "1100000000000001"),
    ("~>1.2", "1100000000000001"),
    (">=1.2.3 <2.0.0", "1100000000000001"),
    (">1.2.3", "0110000000111010"),
    ("<=1.2.3", "1001000111000001"),
    ("<1.2.3", "0001000111000000"),
    ("=1.2.3", "1000000000000001"),
    ("1.2.3", "1000000000000001"),
    ("1.2.3 - 2.3.4", "1110000000100001"),
    ("1.2 - 2.3", "1110000000110001"),
    ("1.2.3 - 2", "1110000000111001"),
    (">= 1.2.3", "1110000000111011"),
    (">=1.2.3 || <1.0.0", "1111000111111011"),
    ("1.2.3 || 2.x", "1010000000111001"),
    ("^1.x", "1100000000000001"),
    ("^ 1.2.3", "1100000000000001"),
    ("~ 1.2.3", "1100000000000001"),
    (">=0.0.0", "1111000111111011"),
    ("latest", "0000000000000000"),
    ("1.2.3garbage", "0000000000000000"),
    (">=1.0.0-0", "1110000000111011"),
    ("<2.0.0-0", "1101000111000001"),
    ("^9007199254740991.0.0", "0000000000000000"),
    ("v1.2.3", "1000000000000001"),
    ("=v1.2.3", "1000000000000001"),
    ("1.2.3+build", "1000000000000001"),
    (">1", "0010000000111010"),
    ("<=1.2", "1101000111000001"),
    (">01.2.3", "0000000000000000"),
    ("x.x.x", "1111000111111011"),
    ("^0.0.x", "0000000011000000"),
    ("^0.x", "0001000111000000"),
    ("~0.0.1", "0000000011000000"),
    (">=1.2.3-beta.2 <1.2.4", "1000110000000001"),
    ("  >=1.2.3\t<2  ", "1100000000000001"),
    ("1.2.3 - 2.3.4-beta", "1110000000000001"),
    ("*-beta", "0000000000000000"),
    (">=9007199254740991.0.0", "0000000000000010"),
    ("1.2.3 2.0.0", "0000000000000000"),
    ("<0.0.0-0", "0000000000000000"),
    (">1.2.3 <1.2.3", "0000000000000000"),
    ("1 || 2 || 3", "1110000000111001"),
    ("~1.2.3-beta.2", "1100110000000001"),
    ("^1.2.3 >1.2.3-beta.3 <=2", "1100000000000001"),
];

#[test]
fn versions_parse_and_clean_like_node_semver() {
    for (text, parsed, cleaned) in PARSE {
        let shown = |version: Option<Version>| version.map(|version| version.to_string());
        assert_eq!(
            shown(Version::parse(text)).as_deref(),
            *parsed,
            "parse {text:?}"
        );
        assert_eq!(
            shown(Version::clean(text)).as_deref(),
            *cleaned,
            "clean {text:?}"
        );
    }
}

#[test]
fn versions_compare_like_node_semver_with_prereleases_below_releases() {
    for (left, right, expected) in COMPARE {
        let left_version = Version::parse(left).unwrap_or_else(|| panic!("{left} is valid"));
        let right_version = Version::parse(right).unwrap_or_else(|| panic!("{right} is valid"));
        assert_eq!(
            left_version.cmp(&right_version),
            *expected,
            "{left} vs {right}"
        );
    }
}

#[test]
fn ranges_are_satisfied_like_node_semver() {
    for (range, expected) in SATISFIES {
        let actual: String = SATISFIES_VERSIONS
            .iter()
            .map(|version| {
                let satisfied = match (Range::parse(range), Version::parse(version)) {
                    (Some(range), Some(version)) => range.satisfies(&version),
                    _ => false,
                };
                if satisfied {
                    '1'
                } else {
                    '0'
                }
            })
            .collect();
        assert_eq!(actual, *expected, "{range:?} over {SATISFIES_VERSIONS:?}");
    }
}
