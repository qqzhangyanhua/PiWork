use tokio::process::Command;

use crate::domain::environment::{RuntimeCheck, RuntimeStatus};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub async fn detect_runtime_status() -> RuntimeStatus {
    RuntimeStatus {
        python: check_version("python", &["--version"]).await,
        node: check_version("node", &["--version"]).await,
        git: check_version("git", &["--version"]).await,
    }
}

async fn check_version(program: &str, args: &[&str]) -> RuntimeCheck {
    let mut command = Command::new(program);
    command.args(args);
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let output = command.output().await;
    match output {
        Ok(output) if output.status.success() => {
            let raw: &[u8] = if !output.stdout.is_empty() {
                &output.stdout
            } else {
                &output.stderr
            };
            let text = String::from_utf8_lossy(raw).trim().to_string();
            RuntimeCheck {
                available: true,
                version: extract_version(&text),
            }
        }
        _ => RuntimeCheck {
            available: false,
            version: None,
        },
    }
}

fn extract_version(text: &str) -> Option<String> {
    text.split_whitespace().find_map(|token| {
        let candidate = token.strip_prefix('v').unwrap_or(token);
        candidate
            .chars()
            .next()
            .filter(|c| c.is_ascii_digit())
            .map(|_| candidate.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::extract_version;

    #[test]
    fn extracts_version_from_python_banner() {
        assert_eq!(extract_version("Python 3.11.6"), Some("3.11.6".into()));
    }

    #[test]
    fn extracts_version_from_node_banner() {
        assert_eq!(extract_version("v20.11.0"), Some("20.11.0".into()));
    }

    #[test]
    fn extracts_version_from_git_banner() {
        assert_eq!(
            extract_version("git version 2.43.0.windows.1"),
            Some("2.43.0.windows.1".into())
        );
    }

    #[test]
    fn returns_none_when_no_version_token_present() {
        assert_eq!(extract_version("not found"), None);
    }
}
