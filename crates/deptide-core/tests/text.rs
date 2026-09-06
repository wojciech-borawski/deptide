use deptide_core::domain::PackageSpec;
use deptide_core::util::text::{slugify, strip_ansi, unique_name};
use deptide_core::util::time::format_duration;

#[test]
fn slugify_keeps_only_safe_characters() {
    assert_eq!(slugify("  My Run: core @ 3.1 !!"), "my-run-core-3.1");
    assert_eq!(slugify("---"), "");
    assert_eq!(slugify("already_fine-1.2"), "already_fine-1.2");
}

#[test]
fn unique_name_appends_a_counter() {
    let taken = vec!["web".to_string(), "web-2".to_string()];
    assert_eq!(unique_name("web", &taken), "web-3");
    assert_eq!(unique_name("api", &taken), "api");
}

#[test]
fn strip_ansi_removes_colour_codes() {
    assert_eq!(
        strip_ansi("\u{1b}[31mnpm ERR!\u{1b}[0m code"),
        "npm ERR! code"
    );
    assert_eq!(strip_ansi("plain"), "plain");
}

#[test]
fn durations_are_human_readable() {
    assert_eq!(format_duration(900), "0.9s");
    assert_eq!(format_duration(65_000), "1m 05s");
    assert_eq!(format_duration(3_723_000), "1h 02m 03s");
}

#[test]
fn package_specs_parse_scoped_names() {
    let spec = PackageSpec::parse("@acme/core@3.1.0-ABC-1").expect("valid");
    assert_eq!(spec.name, "@acme/core");
    assert_eq!(spec.version, "3.1.0-ABC-1");
    assert_eq!(spec.to_spec(), "@acme/core@3.1.0-ABC-1");

    assert!(PackageSpec::parse("@acme/core").is_none());
    assert!(PackageSpec::parse("core@").is_none());
    assert!(PackageSpec::parse("@1.0.0").is_none());
}
