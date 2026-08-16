use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use super::EngineEvent;

#[derive(Default)]
pub(super) struct SensitiveRedactor {
    values: Vec<String>,
    path_matchers: Vec<PathMatcher>,
}

struct PathMatcher {
    component_alternatives: Vec<Vec<String>>,
}

impl SensitiveRedactor {
    pub(super) fn from_secrets<I, S>(secrets: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::from_secrets_and_local_paths(secrets, std::iter::empty::<PathBuf>())
    }

    pub(super) fn from_secrets_and_local_paths<I, S, P>(secrets: I, local_paths: P) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
        P: IntoIterator,
        P::Item: Into<PathBuf>,
    {
        #[cfg(windows)]
        let discover_aliases = windows_path_aliases;
        #[cfg(not(windows))]
        let discover_aliases = |_path: &Path| Vec::new();
        Self::from_inputs_with_alias_discovery(secrets, local_paths, discover_aliases)
    }

    fn from_inputs_with_alias_discovery<I, S, P, F>(
        secrets: I,
        local_paths: P,
        mut discover_aliases: F,
    ) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
        P: IntoIterator,
        P::Item: Into<PathBuf>,
        F: FnMut(&Path) -> Vec<String>,
    {
        let mut redaction_values = Vec::new();
        for value in secrets.into_iter().map(Into::into) {
            register_exact(&mut redaction_values, value);
        }
        let mut path_matchers = Vec::new();
        for path in local_paths.into_iter().map(Into::into) {
            let rendered = path.to_string_lossy();
            register_path_variants(&mut redaction_values, &rendered);
            if let Some(local_path) = local_path_for_alias_discovery(&path) {
                let mut aliases = vec![local_path.to_string_lossy().into_owned()];
                for alias in discover_aliases(&local_path) {
                    register_path_variants(&mut redaction_values, &alias);
                    aliases.push(alias);
                }
                register_path_variants(&mut redaction_values, &aliases[0]);
                if let Some(matcher) = PathMatcher::from_aliases(aliases) {
                    path_matchers.push(matcher);
                }
            }
        }
        redaction_values.sort_by_key(|value| std::cmp::Reverse(value.len()));
        Self {
            values: redaction_values,
            path_matchers,
        }
    }

    pub(super) fn redact_text(&self, text: &str) -> String {
        let rendered = self.values.iter().fold(text.to_owned(), |rendered, value| {
            replace_unicode_case_insensitive(&rendered, value, "[REDACTED]")
        });
        self.path_matchers
            .iter()
            .fold(rendered, |rendered, matcher| {
                replace_path_matches(&rendered, matcher, "[REDACTED]")
            })
    }

    /// Fail-closed sanitization for unknown/raw RPC messages. Sensitive-looking
    /// keys keep their name for diagnostics, but their value is never exposed.
    pub(super) fn redact_raw_value(&self, value: &mut Value) {
        match value {
            Value::String(text) => *text = self.redact_text(text),
            Value::Array(values) => {
                for value in values {
                    self.redact_raw_value(value);
                }
            }
            Value::Object(object) => {
                for (key, value) in object {
                    if sensitive_json_key(key) {
                        *value = Value::String("[REDACTED]".into());
                    } else {
                        self.redact_raw_value(value);
                    }
                }
            }
            Value::Null | Value::Bool(_) | Value::Number(_) => {}
        }
    }

    /// Known tool payloads have defined semantics, so redact only registered
    /// data while walking both object keys and values before JSON serialization.
    pub(super) fn redact_structured_value(&self, value: &mut Value) {
        match value {
            Value::String(text) => *text = self.redact_text(text),
            Value::Array(values) => {
                for value in values {
                    self.redact_structured_value(value);
                }
            }
            Value::Object(object) => {
                let mut redacted = Map::with_capacity(object.len());
                for (key, mut value) in std::mem::take(object) {
                    self.redact_structured_value(&mut value);
                    redacted.insert(self.redact_text(&key), value);
                }
                *object = redacted;
            }
            Value::Null | Value::Bool(_) | Value::Number(_) => {}
        }
    }

    pub(super) fn redact_event(&self, event: &mut EngineEvent) {
        let redact = |value: &mut String| *value = self.redact_text(value);
        let redact_many = |values: &mut Vec<String>| {
            for value in values {
                redact(value);
            }
        };
        match event {
            EngineEvent::RunStarted { model_label } => redact(model_label),
            EngineEvent::AssistantDelta { text } | EngineEvent::ThoughtDelta { text } => {
                redact(text)
            }
            EngineEvent::ToolStarted {
                tool_call_id,
                tool_name,
                ..
            } => {
                redact(tool_call_id);
                redact(tool_name);
            }
            EngineEvent::ToolFinished {
                tool_call_id,
                tool_name,
                output_summary,
                ..
            }
            | EngineEvent::ToolProgress {
                tool_call_id,
                tool_name,
                output_summary,
            } => {
                redact(tool_call_id);
                redact(tool_name);
                redact(output_summary);
            }
            EngineEvent::RunCompleted {
                summary,
                artifacts,
                validation,
                limitations,
            } => {
                redact(summary);
                redact_many(artifacts);
                redact_many(validation);
                redact_many(limitations);
            }
            EngineEvent::RunFailed { message } => redact(message),
            EngineEvent::PlanChanged { plan_id, text, .. } => {
                redact(plan_id);
                redact(text);
            }
            EngineEvent::ToolPending {
                tool_call_id,
                tool_name,
                input_summary,
            } => {
                redact(tool_call_id);
                redact(tool_name);
                redact(input_summary);
            }
            EngineEvent::PermissionRequested {
                request_id,
                tool_call_id,
                title,
                detail,
            } => {
                redact(request_id);
                if let Some(tool_call_id) = tool_call_id {
                    redact(tool_call_id);
                }
                redact(title);
                redact(detail);
            }
            EngineEvent::PermissionResolved { request_id, .. } => redact(request_id),
            EngineEvent::Waiting { reason } => redact(reason),
            EngineEvent::SessionChanged { reason, .. } => {
                if let Some(reason) = reason {
                    redact(reason);
                }
            }
            EngineEvent::ArtifactProduced { path } => redact(path),
            EngineEvent::ValidationProduced {
                command, summary, ..
            } => {
                redact(command);
                redact(summary);
            }
            EngineEvent::RawEngineEvent { kind, payload_json } => {
                redact(kind);
                redact(payload_json);
            }
            EngineEvent::Liveness { .. } | EngineEvent::UsageUpdated { .. } => {}
        }
    }
}

fn register_exact(values: &mut Vec<String>, value: String) {
    if !value.is_empty()
        && !values
            .iter()
            .any(|existing| existing.to_lowercase() == value.to_lowercase())
    {
        values.push(value);
    }
}

fn register_path_variants(values: &mut Vec<String>, value: &str) {
    if value.is_empty() {
        return;
    }
    let backslash = value.replace('/', "\\");
    let plain = if let Some(unc) = backslash.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else if let Some(disk) = backslash.strip_prefix(r"\\?\") {
        disk.to_owned()
    } else {
        backslash
    };
    let mut variants = vec![plain.clone(), plain.replace('\\', "/")];
    if plain.starts_with(r"\\") {
        let unc = plain.trim_start_matches('\\');
        variants.push(format!(r"\\?\UNC\{unc}"));
        variants.push(format!("//?/UNC/{}", unc.replace('\\', "/")));
    } else if plain.as_bytes().get(1) == Some(&b':') {
        variants.push(format!(r"\\?\{plain}"));
        variants.push(format!("//?/{}", plain.replace('\\', "/")));
    }
    for variant in variants {
        register_exact(values, variant);
    }
}

impl PathMatcher {
    fn from_aliases(aliases: impl IntoIterator<Item = String>) -> Option<Self> {
        let mut component_alternatives: Vec<Vec<String>> = Vec::new();
        for alias in aliases {
            let Some(components) = local_path_components(&alias) else {
                continue;
            };
            if component_alternatives.is_empty() {
                component_alternatives = components
                    .into_iter()
                    .map(|component| vec![component])
                    .collect();
                continue;
            }
            if components.len() != component_alternatives.len() {
                continue;
            }
            for (alternatives, component) in component_alternatives.iter_mut().zip(components) {
                register_exact(alternatives, component);
            }
        }
        (!component_alternatives.is_empty()).then_some(Self {
            component_alternatives,
        })
    }

    fn match_len(&self, text: &str) -> Option<usize> {
        let mut offset = local_verbatim_prefix_len(text).unwrap_or(0);
        for (index, alternatives) in self.component_alternatives.iter().enumerate() {
            let matched = alternatives
                .iter()
                .filter_map(|alternative| {
                    unicode_case_insensitive_prefix_len(&text[offset..], alternative)
                })
                .max()?;
            offset += matched;
            if index + 1 < self.component_alternatives.len() {
                let separators = text[offset..]
                    .chars()
                    .take_while(|character| matches!(character, '\\' | '/'))
                    .map(char::len_utf8)
                    .sum::<usize>();
                if separators == 0 {
                    return None;
                }
                offset += separators;
            }
        }
        Some(offset)
    }
}

fn replace_path_matches(text: &str, matcher: &PathMatcher, replacement: &str) -> String {
    let mut rendered = String::with_capacity(text.len());
    let mut copied = 0;
    let mut cursor = 0;
    while cursor < text.len() {
        if let Some(length) = matcher.match_len(&text[cursor..]) {
            rendered.push_str(&text[copied..cursor]);
            rendered.push_str(replacement);
            cursor += length;
            copied = cursor;
        } else {
            cursor += text[cursor..].chars().next().unwrap().len_utf8();
        }
    }
    rendered.push_str(&text[copied..]);
    rendered
}

fn unicode_case_insensitive_prefix_len(text: &str, expected: &str) -> Option<usize> {
    let expected = expected.to_lowercase();
    let mut folded = String::new();
    for (start, character) in text.char_indices() {
        folded.extend(character.to_lowercase());
        if folded.len() >= expected.len() {
            return (folded == expected).then_some(start + character.len_utf8());
        }
    }
    (folded == expected).then_some(text.len())
}

fn local_path_components(path: &str) -> Option<Vec<String>> {
    let normalized = path.replace('/', "\\");
    let normalized = strip_local_verbatim_prefix(&normalized)?;
    let components = normalized
        .split('\\')
        .filter(|component| !component.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    (!components.is_empty()).then_some(components)
}

fn local_verbatim_prefix_len(path: &str) -> Option<usize> {
    let bytes = path.as_bytes();
    (bytes.len() >= 4
        && matches!(bytes[0], b'\\' | b'/')
        && bytes[1] == bytes[0]
        && bytes[2] == b'?'
        && matches!(bytes[3], b'\\' | b'/'))
    .then_some(4)
}

fn strip_local_verbatim_prefix(path: &str) -> Option<&str> {
    let without_prefix = local_verbatim_prefix_len(path).map_or(path, |length| &path[length..]);
    let lower = without_prefix.to_ascii_lowercase();
    if lower.starts_with("unc\\")
        || path.starts_with(r"\\") && local_verbatim_prefix_len(path).is_none()
    {
        None
    } else {
        Some(without_prefix)
    }
}

fn local_path_for_alias_discovery(path: &Path) -> Option<PathBuf> {
    let rendered = path.to_string_lossy().replace('/', "\\");
    strip_local_verbatim_prefix(&rendered).map(PathBuf::from)
}

fn replace_unicode_case_insensitive(text: &str, needle: &str, replacement: &str) -> String {
    if needle.is_empty() {
        return text.to_owned();
    }

    let folded_needle = needle.to_lowercase();
    let mut folded_text = String::new();
    let mut source_boundaries = vec![Some(0_usize)];
    for (start, character) in text.char_indices() {
        let end = start + character.len_utf8();
        let folded_character = character.to_lowercase().collect::<String>();
        let prior_len = folded_text.len();
        folded_text.push_str(&folded_character);
        source_boundaries.resize(folded_text.len() + 1, None);
        source_boundaries[prior_len] = Some(start);
        source_boundaries[folded_text.len()] = Some(end);
    }

    let mut rendered = String::with_capacity(text.len());
    let mut source_offset = 0;
    let mut search_offset = 0;
    while search_offset <= folded_text.len() {
        let Some(relative) = folded_text[search_offset..].find(&folded_needle) else {
            break;
        };
        let folded_start = search_offset + relative;
        let folded_end = folded_start + folded_needle.len();
        if let (Some(start), Some(end)) = (
            source_boundaries.get(folded_start).copied().flatten(),
            source_boundaries.get(folded_end).copied().flatten(),
        ) {
            if start >= source_offset {
                rendered.push_str(&text[source_offset..start]);
                rendered.push_str(replacement);
                source_offset = end;
            }
            search_offset = folded_end;
        } else {
            search_offset = folded_text[folded_start..]
                .char_indices()
                .nth(1)
                .map_or(folded_text.len() + 1, |(next, _)| folded_start + next);
        }
    }
    rendered.push_str(&text[source_offset..]);
    rendered
}

#[cfg(windows)]
fn windows_path_aliases(path: &Path) -> Vec<String> {
    if !path.exists() {
        return Vec::new();
    }
    let mut aliases = vec![path.to_string_lossy().into_owned()];
    if let Ok(canonical) = std::fs::canonicalize(path) {
        aliases.push(canonical.to_string_lossy().into_owned());
        aliases.push(dunce::simplified(&canonical).to_string_lossy().into_owned());
    }
    let candidates = aliases.clone();
    for candidate in candidates {
        let long = windows_path_name(&candidate, false);
        let short = windows_path_name(&candidate, true);
        if let Some(long) = &long {
            aliases.push(long.clone());
        }
        if let Some(short) = &short {
            aliases.push(short.clone());
        }
    }
    aliases
}

#[cfg(windows)]
fn windows_path_name(value: &str, short: bool) -> Option<String> {
    use std::{ffi::OsStr, os::windows::ffi::OsStrExt};

    #[link(name = "kernel32")]
    unsafe extern "system" {
        #[link_name = "GetLongPathNameW"]
        fn get_long_path_name_w(long: *const u16, output: *mut u16, capacity: u32) -> u32;
        #[link_name = "GetShortPathNameW"]
        fn get_short_path_name_w(long: *const u16, output: *mut u16, capacity: u32) -> u32;
    }

    let input = OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let mut output = vec![0_u16; 32_768];
    let length = unsafe {
        if short {
            get_short_path_name_w(input.as_ptr(), output.as_mut_ptr(), output.len() as u32)
        } else {
            get_long_path_name_w(input.as_ptr(), output.as_mut_ptr(), output.len() as u32)
        }
    } as usize;
    (length > 0 && length < output.len()).then(|| String::from_utf16_lossy(&output[..length]))
}

fn sensitive_json_key(key: &str) -> bool {
    json_key_tokens(key).any(|token| {
        matches!(
            token.as_str(),
            "key"
                | "keys"
                | "token"
                | "tokens"
                | "secret"
                | "secrets"
                | "authorization"
                | "path"
                | "paths"
                | "directory"
                | "directories"
                | "session"
                | "sessions"
                | "model"
                | "models"
        )
    })
}

fn json_key_tokens(key: &str) -> impl Iterator<Item = String> + '_ {
    let characters = key.char_indices().collect::<Vec<_>>();
    let mut tokens = Vec::new();
    let mut start = None;
    for (position, &(index, character)) in characters.iter().enumerate() {
        if !character.is_alphanumeric() {
            if let Some(token_start) = start.take() {
                tokens.push(key[token_start..index].to_ascii_lowercase());
            }
            continue;
        }
        let previous_is_lower = position
            .checked_sub(1)
            .and_then(|previous| characters.get(previous))
            .is_some_and(|(_, previous)| previous.is_lowercase());
        let next_is_lower = characters
            .get(position + 1)
            .is_some_and(|(_, next)| next.is_lowercase());
        let camel_boundary = character.is_uppercase() && (previous_is_lower || next_is_lower);
        if camel_boundary && let Some(token_start) = start.replace(index) {
            tokens.push(key[token_start..index].to_ascii_lowercase());
        } else if start.is_none() {
            start = Some(index);
        }
    }
    if let Some(token_start) = start {
        tokens.push(key[token_start..].to_ascii_lowercase());
    }
    tokens.into_iter()
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, path::PathBuf};

    use serde_json::json;

    use super::SensitiveRedactor;

    #[test]
    fn structured_redaction_handles_json_keys_and_escaped_values_before_serialization() {
        let secret = "secret\"\\value";
        let mut value = json!({ secret: { "nested": secret } });
        SensitiveRedactor::from_secrets([secret]).redact_structured_value(&mut value);
        let rendered = serde_json::to_string(&value).unwrap();
        assert_eq!(rendered, r#"{"[REDACTED]":{"nested":"[REDACTED]"}}"#);
        assert!(!rendered.contains("secret"));
    }

    #[test]
    fn unicode_case_variants_are_redacted() {
        let redactor = SensitiveRedactor::from_secrets(["C:\\Ünicode\\秘密"]);
        assert_eq!(
            redactor.redact_text("open c:\\ünicode\\秘密 now"),
            "open [REDACTED] now"
        );
    }

    #[test]
    fn scalar_secrets_are_registered_exactly_without_path_normalization() {
        let secret = r"abc/def\ghi";
        let redactor = SensitiveRedactor::from_secrets([secret]);

        assert_eq!(redactor.redact_text(secret), "[REDACTED]");
        assert_eq!(redactor.redact_text(r"abc\def\ghi"), r"abc\def\ghi");
        assert_eq!(redactor.redact_text("abc/def/ghi"), "abc/def/ghi");
    }

    #[test]
    fn raw_key_heuristic_does_not_treat_monkey_as_a_sensitive_key() {
        let mut value = json!({"monkey": "banana", "apiKey": "private"});

        SensitiveRedactor::default().redact_raw_value(&mut value);

        assert_eq!(value["monkey"], "banana");
        assert_eq!(value["apiKey"], "[REDACTED]");
    }

    #[test]
    fn redactor_never_probes_arbitrary_secret_values() {
        let probes = Cell::new(0);
        let _redactor = SensitiveRedactor::from_inputs_with_alias_discovery(
            [r"C:\looks\like\a\path", r"\\server\share\secret"],
            std::iter::empty::<PathBuf>(),
            |_| {
                probes.set(probes.get() + 1);
                Vec::new()
            },
        );

        assert_eq!(probes.get(), 0);
    }

    #[test]
    fn redactor_rejects_unc_paths_before_local_path_probing() {
        let probes = Cell::new(0);
        let redactor = SensitiveRedactor::from_inputs_with_alias_discovery(
            std::iter::empty::<String>(),
            [PathBuf::from(r"\\server\share\private")],
            |_| {
                probes.set(probes.get() + 1);
                Vec::new()
            },
        );

        assert_eq!(probes.get(), 0);
        assert_eq!(
            redactor.redact_text(r"\\server\share\private"),
            "[REDACTED]"
        );
    }

    #[test]
    fn redactor_strips_verbatim_disk_prefix_but_rejects_verbatim_unc_before_probing() {
        let probed = std::cell::RefCell::new(Vec::new());
        let redactor = SensitiveRedactor::from_inputs_with_alias_discovery(
            std::iter::empty::<String>(),
            [
                PathBuf::from(r"\\?\C:\private\local"),
                PathBuf::from(r"\\?\UNC\server\share\private"),
            ],
            |path| {
                probed.borrow_mut().push(path.to_path_buf());
                Vec::new()
            },
        );

        assert_eq!(&*probed.borrow(), &[PathBuf::from(r"C:\private\local")]);
        assert_eq!(
            redactor.redact_text(r"\\?\UNC\server\share\private"),
            "[REDACTED]"
        );
    }

    #[test]
    fn redactor_matches_mixed_component_aliases_from_the_discovery_seam() {
        let long = r"C:\Long Component One\Long Component Two\file.txt";
        let short = r"C:\LONGCO~1\LONGCO~2\file.txt";
        let mixed = r"C:\LONGCO~1\Long Component Two\file.txt";

        let redactor = SensitiveRedactor::from_inputs_with_alias_discovery(
            std::iter::empty::<String>(),
            [PathBuf::from(long)],
            |_| vec![short.into()],
        );
        assert_eq!(redactor.redact_text(mixed), "[REDACTED]");
    }

    #[test]
    fn redactor_matches_more_than_eight_mixed_path_components_without_fail_open() {
        let long = r"C:\Long One\Long Two\Long Three\Long Four\Long Five\Long Six\Long Seven\Long Eight\Long Nine\file.txt";
        let short = r"C:\LONGON~1\LONGTW~1\LONGTH~1\LONGFO~1\LONGFI~1\LONGSI~1\LONGSE~1\LONGEI~1\LONGNI~1\file.txt";
        let mixed = r"c:/LONGON~1/Long Two/LONGTH~1/Long Four/LONGFI~1/Long Six/LONGSE~1/Long Eight/LONGNI~1/file.txt";
        let redactor = SensitiveRedactor::from_inputs_with_alias_discovery(
            std::iter::empty::<String>(),
            [PathBuf::from(long)],
            |_| vec![short.into()],
        );

        assert_eq!(redactor.redact_text(mixed), "[REDACTED]");
    }

    #[cfg(windows)]
    #[test]
    fn redactor_discovers_mixed_windows_short_and_long_components_when_available() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary
            .path()
            .join("Long Component Alpha")
            .join("Long Component Beta");
        std::fs::create_dir_all(&path).unwrap();
        let rendered = path.to_string_lossy();
        let Some(long) = super::windows_path_name(&rendered, false) else {
            return;
        };
        let Some(short) = super::windows_path_name(&rendered, true) else {
            return;
        };
        let long_parts = long.split('\\').collect::<Vec<_>>();
        let short_parts = short.split('\\').collect::<Vec<_>>();
        if long_parts.len() != short_parts.len() {
            return;
        }
        let Some(changed) = long_parts
            .iter()
            .zip(&short_parts)
            .position(|(long, short)| !long.eq_ignore_ascii_case(short))
        else {
            // 8.3 naming can be disabled per volume. That platform has no real
            // short alias to exercise and should not rely on a fabricated one.
            return;
        };
        let mut mixed_parts = long_parts;
        mixed_parts[changed] = short_parts[changed];
        let mixed = mixed_parts.join("\\");
        let redactor =
            SensitiveRedactor::from_secrets_and_local_paths(std::iter::empty::<String>(), [path]);

        assert_eq!(redactor.redact_text(&mixed), "[REDACTED]");
    }
}
