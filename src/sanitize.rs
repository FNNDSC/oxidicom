use regex::Regex;
use std::sync::OnceLock;

/// Replace disallowed characters with "_".
/// https://github.com/FNNDSC/pypx/blob/7619c15f4d2303d6d5ca7c255d81d06c7ab8682b/pypx/repack.py#L424
///
/// Also, it's necessary to handle NUL bytes...
pub(crate) fn sanitize<S: AsRef<str>>(s: S) -> String {
    let s_nonull = s.as_ref().replace('\0', "");
    VALID_CHARS_RE
        .get_or_init(|| Regex::new(r#"[^A-Za-z0-9\.\-]+"#).unwrap())
        .replace_all(&s_nonull, "_")
        .to_string()
}

static VALID_CHARS_RE: OnceLock<Regex> = OnceLock::new();
