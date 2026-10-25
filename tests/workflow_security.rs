//! Repository supply-chain policy. No jobs or release publication are executed here.
use serde_yaml::Value;
use std::fs;

fn workflow(name: &str) -> Value {
    serde_yaml::from_str(&fs::read_to_string(format!(".github/workflows/{name}.yml")).unwrap())
        .unwrap()
}

#[test]
fn external_actions_are_immutable_and_checkouts_do_not_persist_tokens() {
    for entry in fs::read_dir(".github/workflows").unwrap() {
        let entry = entry.unwrap();
        let yaml: Value = serde_yaml::from_str(&fs::read_to_string(entry.path()).unwrap()).unwrap();
        for (_, job) in yaml["jobs"].as_mapping().unwrap() {
            for step in job["steps"].as_sequence().unwrap() {
                let Some(action) = step["uses"].as_str() else {
                    continue;
                };
                if action.starts_with("./") {
                    continue;
                }
                let (_, rev) = action
                    .rsplit_once('@')
                    .expect("action must specify revision");
                assert!(
                    rev.len() == 40 && rev.bytes().all(|b| b.is_ascii_hexdigit()),
                    "{} has a mutable action ref: {action}",
                    entry.path().display()
                );
                if action.starts_with("actions/checkout@") {
                    assert_eq!(
                        step["with"]["persist-credentials"].as_bool(),
                        Some(false),
                        "checkout must not save tokens in .git/config"
                    );
                }
                if action.starts_with("dtolnay/rust-toolchain@") {
                    assert_eq!(step["with"]["toolchain"].as_str(), Some("1.93.1"));
                }
            }
        }
    }
    let dependabot: Value =
        serde_yaml::from_str(&fs::read_to_string(".github/dependabot.yml").unwrap()).unwrap();
    assert!(dependabot["updates"]
        .as_sequence()
        .unwrap()
        .iter()
        .any(|v| v["package-ecosystem"] == "github-actions"));
}

#[test]
fn release_and_refresh_builds_do_not_hold_write_tokens() {
    for name in [
        "release",
        "data-refresh",
        "bench-refresh-baseline",
        "benchmark-regression",
    ] {
        let yaml = workflow(name);
        assert_eq!(
            yaml["permissions"]["contents"].as_str(),
            Some("read"),
            "{name}"
        );
        assert!(
            yaml["permissions"]["pull-requests"].is_null(),
            "{name}: no global PR write grant"
        );
        let build_name = match name {
            "release" => "build",
            "benchmark-regression" => "criterion_gate",
            _ => "refresh",
        };
        let job = &yaml["jobs"][build_name];
        assert!(
            job["permissions"].is_null() || job["permissions"]["contents"].as_str() == Some("read")
        );
        assert_ne!(job["permissions"]["pull-requests"].as_str(), Some("write"));
        if let Some(steps) = job["steps"].as_sequence() {
            assert!(!steps.iter().any(|step| step["uses"]
                .as_str()
                .is_some_and(|s| s.starts_with("peter-evans/create-pull-request@")
                    || s.starts_with("marocchino/sticky-pull-request-comment@"))));
        }
    }
}

#[test]
fn release_is_gated_and_attests_the_uploaded_assets() {
    let release = workflow("release");
    let jobs = &release["jobs"];
    assert!(jobs["build"]["needs"]
        .as_sequence()
        .is_some_and(|v| v.contains(&Value::from("verify"))));
    let verify = &jobs["verify"]["steps"];
    let verify_text = serde_yaml::to_string(verify).unwrap();
    assert!(
        verify_text.contains("merge-base --is-ancestor"),
        "tag commit must belong to main"
    );
    assert!(
        verify_text.contains("cargo audit"),
        "release needs its own dependency audit gate"
    );
    let publish = &jobs["publish"];
    assert_eq!(publish["permissions"]["contents"].as_str(), Some("write"));
    assert_eq!(publish["permissions"]["id-token"].as_str(), Some("write"));
    assert_eq!(
        publish["permissions"]["attestations"].as_str(),
        Some("write")
    );
    assert!(publish["steps"]
        .as_sequence()
        .unwrap()
        .iter()
        .any(|step| step["uses"]
            .as_str()
            .is_some_and(|s| s.starts_with("actions/attest-build-provenance@"))
            && step["with"]["subject-path"].as_str() == Some("release-assets/*")));
}
