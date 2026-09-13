//! Execute a declared application directly. Environment templates support data
//! substitution and defaults, never shell commands, word splitting or globbing.
use crate::Result;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const LIMIT: usize = 1024 * 1024;
type Environment = BTreeMap<OsString, OsString>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Configuration {
    source: PathBuf,
    destination: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    program: PathBuf,
    #[serde(default)]
    arguments: Vec<String>,
    #[serde(default)]
    environment: BTreeMap<String, String>,
    #[serde(default)]
    path: Vec<String>,
    configuration: Option<Configuration>,
}

fn variable(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|c| c == b'_' || c.is_ascii_alphabetic())
        && bytes.all(|c| c == b'_' || c.is_ascii_alphanumeric())
}

fn append(output: &mut OsString, value: impl AsRef<OsStr>) -> Result {
    if output
        .as_bytes()
        .len()
        .saturating_add(value.as_ref().as_bytes().len())
        > LIMIT
    {
        return Err("launch environment exceeds its limit".into());
    }
    output.push(value);
    Ok(())
}

fn expand(template: &str, environment: &Environment, depth: usize) -> Result<OsString> {
    if depth > 16 || template.len() > LIMIT || template.contains('\0') {
        return Err("invalid launch environment template".into());
    }
    let bytes = template.as_bytes();
    let mut result = OsString::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        let start = cursor;
        while cursor < bytes.len() && !b"$\\`".contains(&bytes[cursor]) {
            cursor += 1;
        }
        append(&mut result, &template[start..cursor])?;
        if cursor == bytes.len() {
            break;
        }
        if bytes[cursor] == b'`' {
            return Err("commands are not permitted in launch templates".into());
        }
        if bytes[cursor] == b'\\' {
            cursor += 1;
            if cursor < bytes.len() && b"$\\`\"\n".contains(&bytes[cursor]) {
                if bytes[cursor] != b'\n' {
                    append(&mut result, &template[cursor..cursor + 1])?;
                }
                cursor += 1;
            } else {
                append(&mut result, "\\")?;
            }
            continue;
        }
        cursor += 1;
        if bytes.get(cursor) == Some(&b'(') {
            return Err("commands are not permitted in launch templates".into());
        }
        let braced = bytes.get(cursor) == Some(&b'{');
        cursor += usize::from(braced);
        let name_start = cursor;
        while cursor < bytes.len()
            && (bytes[cursor] == b'_' || bytes[cursor].is_ascii_alphanumeric())
        {
            cursor += 1;
        }
        let name = &template[name_start..cursor];
        if !braced && name.is_empty() {
            // A dollar without a parameter name is ordinary quoted data. Shell
            // special parameters remain unsupported rather than inheriting
            // launcher-specific PID, exit status or positional arguments.
            if bytes.get(cursor).is_some_and(|c| b"?*!@#-$".contains(c)) {
                return Err("unsupported launch environment substitution".into());
            }
            append(&mut result, "$")?;
            continue;
        }
        if !variable(name) {
            return Err("unsupported launch environment substitution".into());
        }
        let value = environment.get(OsStr::new(name));
        if !braced {
            append(
                &mut result,
                value.ok_or("undefined launch environment variable")?,
            )?;
            continue;
        }
        if bytes.get(cursor) == Some(&b'}') {
            cursor += 1;
            append(
                &mut result,
                value.ok_or("undefined launch environment variable")?,
            )?;
            continue;
        }
        let null_is_missing = bytes.get(cursor) == Some(&b':');
        cursor += usize::from(null_is_missing);
        let operator = *bytes.get(cursor).ok_or("incomplete launch substitution")?;
        if !matches!(operator, b'-' | b'+') {
            return Err("unsupported launch environment substitution".into());
        }
        cursor += 1;
        let alternate_start = cursor;
        let mut nesting = 1usize;
        while cursor < bytes.len() {
            if bytes[cursor] == b'\\' {
                cursor += 1;
                if cursor < bytes.len() {
                    cursor += template[cursor..].chars().next().unwrap().len_utf8();
                }
                continue;
            }
            if bytes[cursor..].starts_with(b"${") {
                nesting += 1;
                if nesting > 16 {
                    return Err("launch substitution exceeds its depth limit".into());
                }
                cursor += 2;
                continue;
            }
            if bytes[cursor] == b'}' {
                nesting -= 1;
                if nesting == 0 {
                    break;
                }
            }
            cursor += template[cursor..].chars().next().unwrap().len_utf8();
        }
        if nesting != 0 {
            return Err("incomplete launch substitution".into());
        }
        let present = value.is_some_and(|value| !null_is_missing || !value.is_empty());
        if (operator == b'-' && !present) || (operator == b'+' && present) {
            append(
                &mut result,
                expand(&template[alternate_start..cursor], environment, depth + 1)?,
            )?;
        } else if operator == b'-' {
            append(&mut result, value.unwrap())?;
        }
        cursor += 1;
    }
    Ok(result)
}

fn prepare(manifest: Manifest, inherited: &Environment) -> Result<Command> {
    if manifest.version != 1
        || !manifest.program.is_absolute()
        || manifest.arguments.len() > 256
        || manifest.environment.len() > 4096
        || manifest.path.len() > 256
    {
        return Err("invalid native launch manifest".into());
    }
    let mut environment = inherited.clone();
    let configuration = manifest
        .configuration
        .map(|configuration| -> Result<_> {
            if !configuration.source.is_absolute() {
                return Err("configuration source must be absolute".into());
            }
            let destination = PathBuf::from(expand(&configuration.destination, &environment, 0)?);
            if !destination.is_absolute() {
                return Err("configuration destination must be absolute".into());
            }
            Ok((configuration.source, destination))
        })
        .transpose()?;
    if manifest.program.as_os_str().as_bytes().contains(&0)
        || manifest
            .arguments
            .iter()
            .any(|argument| argument.contains('\0'))
    {
        return Err("invalid native launch program or argument".into());
    }
    if !manifest.path.is_empty() {
        let mut path = OsString::new();
        for part in manifest.path {
            let part = expand(&part, &environment, 0)?;
            if part.is_empty() || part.as_bytes().contains(&b':') {
                return Err("invalid declared executable search path".into());
            }
            if !path.is_empty() {
                append(&mut path, ":")?;
            }
            append(&mut path, part)?;
        }
        if let Some(inherited) = environment
            .get(OsStr::new("PATH"))
            .filter(|v| !v.is_empty())
        {
            append(&mut path, ":")?;
            append(&mut path, inherited)?;
        }
        environment.insert("PATH".into(), path);
    }
    let mut declared_bytes = 0usize;
    for (name, template) in manifest.environment {
        if !variable(&name) {
            return Err("invalid launch environment variable name".into());
        }
        let value = expand(&template, &environment, 0)?;
        declared_bytes = declared_bytes
            .saturating_add(name.len())
            .saturating_add(value.as_bytes().len());
        if declared_bytes > LIMIT {
            return Err("declared launch environment exceeds its total limit".into());
        }
        environment.insert(name.into(), value);
    }
    // Invalid declarations must never partially refresh the configuration.
    if let Some((source, destination)) = configuration {
        crate::materialize::materialize(&source, &destination)?;
    }
    let mut command = Command::new(manifest.program);
    command
        .args(manifest.arguments)
        .env_clear()
        .envs(environment);
    Ok(command)
}

pub fn run(manifest: &Path, arguments: impl IntoIterator<Item = OsString>) -> Result {
    let bytes = seele_runtime::fs::read_bounded(manifest, LIMIT, false)?;
    let manifest: Manifest = serde_json::from_slice(&bytes)?;
    let mut command = prepare(manifest, &std::env::vars_os().collect())?;
    command.args(arguments);
    Err(command.exec().into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_expansion_matches_nested_defaults_without_executing_values() {
        let env = BTreeMap::from([
            ("HOME".into(), "/tmp/a b/🦀".into()),
            ("EMPTY".into(), "".into()),
            ("TEXT".into(), "$(false) `false` *.md".into()),
        ]);
        assert_eq!(
            expand("${CACHE:-$HOME/.cache}/glow", &env, 0).unwrap(),
            "/tmp/a b/🦀/.cache/glow"
        );
        assert_eq!(
            expand("${EMPTY-default}/${EMPTY:-default}", &env, 0).unwrap(),
            "/default"
        );
        assert_eq!(
            expand("${HOME:+yes}/${MISSING+no}", &env, 0).unwrap(),
            "yes/"
        );
        assert_eq!(expand("${TEXT}", &env, 0).unwrap(), "$(false) `false` *.md");
        assert_eq!(
            expand(r"\$HOME/\`literal\`", &env, 0).unwrap(),
            "$HOME/`literal`"
        );
        assert_eq!(expand("trailing $", &env, 0).unwrap(), "trailing $");
        assert_eq!(expand("${MISSING:-$}/$.txt", &env, 0).unwrap(), "$/$.txt");
        for template in [
            "$(false)",
            "`false`",
            "$MISSING",
            "${HOME",
            "${HOME:=bad}",
            "${0}",
        ] {
            assert!(expand(template, &env, 0).is_err(), "{template}");
        }
    }

    #[test]
    fn arbitrary_unix_home_bytes_are_retained() {
        let home = OsStr::from_bytes(b"/tmp/home-\xff").to_os_string();
        let env = BTreeMap::from([("HOME".into(), home)]);
        assert_eq!(
            expand("$HOME/file", &env, 0).unwrap().as_bytes(),
            b"/tmp/home-\xff/file"
        );
    }

    #[test]
    fn ordered_exports_see_prefixed_path_and_prior_exports() {
        let manifest = serde_json::from_value(serde_json::json!({
            "version":1,"program":"/fixture/app","path":["/fixture/bin"],
            "environment":{"A":"${PATH}","B":"${A}/b"}
        }))
        .unwrap();
        let command = prepare(
            manifest,
            &BTreeMap::from([("PATH".into(), "/original/bin".into())]),
        )
        .unwrap();
        let env: BTreeMap<_, _> = command
            .get_envs()
            .map(|(k, v)| (k.to_os_string(), v.unwrap().to_os_string()))
            .collect();
        assert_eq!(env[OsStr::new("B")], "/fixture/bin:/original/bin/b");
    }
}
