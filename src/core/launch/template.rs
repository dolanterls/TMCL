use std::collections::BTreeMap;

use crate::core::error::{Error, Result};

pub fn expand(template: &str, values: &BTreeMap<String, String>) -> Result<String> {
    let mut output = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("${") {
        output.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let end = after
            .find('}')
            .ok_or_else(|| Error::Launch(format!("unterminated variable in {template}")))?;
        let key = &after[..end];
        // Version JSONs written by other launchers (PCL2, HMCL, ...) carry their
        // own placeholders. Dropping an unknown one keeps the launch running
        // instead of failing on metadata this launcher never wrote; the token is
        // removed rather than left literal so it cannot leak into a path.
        if let Some(value) = values.get(key) {
            output.push_str(value);
        }
        rest = &after[end + 1..];
    }
    output.push_str(rest);
    Ok(output)
}

pub fn redact_token(value: &str, token: &str) -> String {
    if token.is_empty() {
        value.to_owned()
    } else {
        value.replace(token, "***")
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{expand, redact_token};

    #[test]
    fn expands_known_variables_and_redacts_token() {
        let mut values = BTreeMap::new();
        values.insert("game_directory".into(), "C:/Games/.minecraft".into());
        values.insert("auth_access_token".into(), "secret-token".into());
        let expanded = expand("${game_directory} ${auth_access_token}", &values).expect("expand");
        assert_eq!(
            redact_token(&expanded, "secret-token"),
            "C:/Games/.minecraft ***"
        );
    }

    #[test]
    fn foreign_placeholders_are_dropped_instead_of_failing_the_launch() {
        // PCL2 puts `${primary_jar_name}` into the Forge ignore list.
        let mut values = BTreeMap::new();
        values.insert("version_name".into(), "BattleArmory".into());
        let expanded = expand(
            "-DignoreList=forge-,${version_name}.jar,${primary_jar_name}",
            &values,
        )
        .expect("expand");
        assert_eq!(expanded, "-DignoreList=forge-,BattleArmory.jar,");
        assert!(expand("${unterminated", &values).is_err());
    }
}
