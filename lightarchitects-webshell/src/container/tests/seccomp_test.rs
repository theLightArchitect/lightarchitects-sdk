#[cfg(test)]
mod tests {
    use super::super::super::seccomp_resolver;
    use serde_json::Value;

    #[test]
    fn seccomp_profile_is_valid_json() {
        let temp = seccomp_resolver::write_seccomp_profile().expect("write seccomp profile");
        let content = std::fs::read_to_string(temp.path()).expect("read temp file");
        let _: Value = serde_json::from_str(&content).expect("parse as JSON");
    }

    #[test]
    fn seccomp_profile_blocks_namespace_syscalls() {
        let temp = seccomp_resolver::write_seccomp_profile().expect("write seccomp profile");
        let content = std::fs::read_to_string(temp.path()).expect("read temp file");
        let json: Value = serde_json::from_str(&content).expect("parse as JSON");

        let syscalls = json["syscalls"][0]["names"]
            .as_array()
            .expect("syscalls[0].names is array");
        let names: Vec<&str> = syscalls.iter().filter_map(|v| v.as_str()).collect();

        assert!(names.contains(&"unshare"), "unshare must be blocked");
        assert!(names.contains(&"clone"), "clone must be blocked");
        assert!(names.contains(&"setns"), "setns must be blocked");
    }

    #[test]
    fn seccomp_profile_has_restricted_permissions() {
        let temp = seccomp_resolver::write_seccomp_profile().expect("write seccomp profile");
        let metadata = std::fs::metadata(temp.path()).expect("stat temp file");
        let mode = metadata.permissions().mode();
        assert_eq!(
            mode & 0o777,
            0o400,
            "seccomp profile must have 0o400 permissions"
        );
    }
}
