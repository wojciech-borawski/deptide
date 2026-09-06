use crate::domain::Diagnosis;

struct Rule {
    code: &'static str,
    markers: &'static [&'static str],
    title: &'static str,
    hint: &'static str,
}

const RULES: &[Rule] = &[
    Rule {
        code: "ERESOLVE",
        markers: &["ERESOLVE", "unable to resolve dependency tree", "conflicting peer dependency"],
        title: "Peer dependency conflict",
        hint: "Add --legacy-peer-deps or --force to the install flags, or align the peer versions of the selected libraries.",
    },
    Rule {
        code: "E401",
        markers: &["E401", "E403", "ENEEDAUTH", "Unable to authenticate", "authentication token"],
        title: "Registry authentication failed",
        hint: "The registry token is missing or expired: run npm login for the scope or refresh the token in .npmrc.",
    },
    Rule {
        code: "E404",
        markers: &["E404", "404 Not Found", "is not in this registry"],
        title: "Package or version not found on the registry",
        hint: "Check the version spelling and that .npmrc points the scope at the registry that has the prerelease.",
    },
    Rule {
        code: "ETARGET",
        markers: &["ETARGET", "No matching version found"],
        title: "No matching version",
        hint: "The requested version does not exist on the registry; pick one of the published versions.",
    },
    Rule {
        code: "EINTEGRITY",
        markers: &["EINTEGRITY", "integrity checksum failed"],
        title: "Integrity check failed",
        hint: "The cached tarball does not match the registry: run npm cache clean --force and retry.",
    },
    Rule {
        code: "ENETWORK",
        markers: &["ENOTFOUND", "ECONNRESET", "ETIMEDOUT", "EAI_AGAIN", "ECONNREFUSED", "network timeout", "socket hang up"],
        title: "Network problem",
        hint: "The registry could not be reached: check VPN, proxy and .npmrc registry URL, then retry.",
    },
    Rule {
        code: "ELOCKED",
        markers: &["EPERM", "EBUSY", "EACCES", "operation not permitted", "resource busy or locked"],
        title: "File locked or not writable",
        hint: "Close editors, dev servers or file watchers that hold node_modules open, then retry.",
    },
    Rule {
        code: "ELOCKFILE",
        markers: &["lock file's", "package-lock.json", "npm ci can only install"],
        title: "Lockfile out of sync",
        hint: "The lockfile does not match package.json; run npm install once by hand or delete the lockfile.",
    },
    Rule {
        code: "TSERROR",
        markers: &["error TS", "TS2", "Type error"],
        title: "TypeScript build errors",
        hint: "The build failed on type errors after the update; open the log and start with the first error.",
    },
    Rule {
        code: "EMODULE",
        markers: &["Cannot find module", "Module not found", "Could not resolve"],
        title: "Missing module",
        hint: "A module the build needs is not installed; check the install output and the library's exports.",
    },
    Rule {
        code: "ENOMEM",
        markers: &["heap out of memory", "ENOMEM", "Allocation failed"],
        title: "Out of memory",
        hint: "The build ran out of memory; lower the parallelism or raise NODE_OPTIONS=--max-old-space-size.",
    },
];

pub fn diagnose(lines: &[String]) -> Option<Diagnosis> {
    let tail: Vec<&str> = lines.iter().rev().take(200).map(String::as_str).collect();

    RULES
        .iter()
        .find(|rule| {
            tail.iter()
                .any(|line| rule.markers.iter().any(|marker| line.contains(marker)))
        })
        .map(|rule| Diagnosis {
            code: rule.code.to_string(),
            title: rule.title.to_string(),
            hint: rule.hint.to_string(),
        })
}
