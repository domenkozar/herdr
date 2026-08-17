use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;

use crate::api::schema::{IntegrationTarget, Method, PaneReportAgentSessionParams, Request};

const CODEX_HOOK_INPUT_LIMIT: usize = 64 * 1024;
const CODEX_HOOK_REQUEST_TIMEOUT: Duration = Duration::from_millis(400);

#[derive(Deserialize)]
struct CodexHookInput {
    hook_event_name: Option<String>,
    session_id: Option<String>,
    transcript_path: Option<String>,
    source: Option<String>,
}

pub(super) fn run_integration_command(args: &[String]) -> std::io::Result<i32> {
    let Some(subcommand) = args.first().map(|arg| arg.as_str()) else {
        print_integration_help();
        return Ok(2);
    };

    match subcommand {
        "install" => integration_install(&args[1..]),
        "uninstall" => integration_uninstall(&args[1..]),
        "status" => integration_status(&args[1..]),
        // Integration assets call this intentionally hidden, best-effort path.
        "hook" => integration_hook(&args[1..]),
        "help" | "--help" | "-h" => {
            print_integration_help();
            Ok(0)
        }
        _ => {
            print_integration_help();
            Ok(2)
        }
    }
}

fn integration_hook(args: &[String]) -> std::io::Result<i32> {
    if args != ["codex", "session"] {
        return Ok(0);
    }
    let _ = report_codex_session_start();
    Ok(0)
}

fn report_codex_session_start() -> Option<()> {
    if std::env::var(crate::HERDR_ENV_VAR).ok().as_deref() != Some(crate::HERDR_ENV_VALUE) {
        return None;
    }
    let pane_id = nonempty_env(crate::integration::HERDR_PANE_ID_ENV_VAR)?;
    // Presence guard: the client resolves the socket path itself.
    nonempty_env(crate::api::SOCKET_PATH_ENV_VAR)?;

    let input =
        match crate::platform::read_limited_reader(std::io::stdin().lock(), CODEX_HOOK_INPUT_LIMIT)
            .ok()?
        {
            crate::platform::LimitedRead::Complete(bytes) => bytes,
            crate::platform::LimitedRead::Empty | crate::platform::LimitedRead::Oversized => {
                return None;
            }
        };
    let input: CodexHookInput = serde_json::from_slice(&input).ok()?;
    if input.hook_event_name.as_deref() != Some("SessionStart") {
        return None;
    }
    let session_id = input.session_id.filter(|value| !value.trim().is_empty())?;
    input
        .transcript_path
        .filter(|value| !value.trim().is_empty())?;
    if std::env::var("CODEX_THREAD_ID")
        .ok()
        .filter(|value| !value.is_empty())
        .is_some_and(|inherited| inherited != session_id)
    {
        return None;
    }

    let seq = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_nanos()
        .min(u128::from(u64::MAX)) as u64;
    let request = Request {
        id: format!("herdr:codex-hook:{}:{seq}", std::process::id()),
        method: Method::PaneReportAgentSession(PaneReportAgentSessionParams {
            pane_id,
            source: "herdr:codex".to_string(),
            agent: "codex".to_string(),
            seq: Some(seq),
            agent_session_id: Some(session_id),
            agent_session_path: None,
            session_start_source: input.source,
            reporter_pid: Some(std::process::id()),
        }),
    };
    let _ = crate::api::client::ApiClient::local()
        .request_value_with_timeout(&request, CODEX_HOOK_REQUEST_TIMEOUT);
    Some(())
}

fn nonempty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn integration_status(args: &[String]) -> std::io::Result<i32> {
    let outdated_only = match args {
        [] => false,
        [flag] if flag == "--outdated-only" => true,
        _ => {
            eprintln!("usage: herdr integration status [--outdated-only]");
            return Ok(2);
        }
    };

    if outdated_only {
        crate::integration::print_outdated_update_notice();
        return Ok(0);
    }

    for status in crate::integration::installed_integration_statuses() {
        let target = crate::integration::integration_target_label(status.target);
        let version = match status.installed_version {
            Some(version) => format!("v{version}"),
            None => "legacy".to_string(),
        };
        let state = match status.state {
            crate::integration::IntegrationStatusKind::NotInstalled => "not installed".to_string(),
            crate::integration::IntegrationStatusKind::Current => {
                format!("current ({version})")
            }
            crate::integration::IntegrationStatusKind::Outdated
                if status
                    .installed_version
                    .is_some_and(|installed| installed >= status.expected_version) =>
            {
                format!("needs repair ({version})")
            }
            crate::integration::IntegrationStatusKind::Outdated => {
                format!("outdated ({version} < v{})", status.expected_version)
            }
        };
        println!("{target}: {state} ({})", status.path.display());
    }

    Ok(0)
}

fn integration_install(args: &[String]) -> std::io::Result<i32> {
    let Some(target) = parse_integration_target(args, "install")? else {
        return Ok(2);
    };

    match crate::integration::install_target(target) {
        Ok(messages) => {
            print_integration_messages(messages);
            Ok(0)
        }
        Err(err) => {
            eprintln!("{err}");
            Ok(1)
        }
    }
}

fn integration_uninstall(args: &[String]) -> std::io::Result<i32> {
    let Some(target) = parse_integration_target(args, "uninstall")? else {
        return Ok(2);
    };

    match crate::integration::uninstall_target(target) {
        Ok(messages) => {
            print_integration_messages(messages);
            Ok(0)
        }
        Err(err) => {
            eprintln!("{err}");
            Ok(1)
        }
    }
}

fn print_integration_messages(messages: Vec<String>) {
    for message in messages {
        println!("{message}");
    }
}

fn parse_integration_target(
    args: &[String],
    action: &str,
) -> std::io::Result<Option<IntegrationTarget>> {
    let Some(target) = args.first().map(|arg| arg.as_str()) else {
        eprintln!(
            "usage: herdr integration {action} <pi|omp|claude|codex|copilot|devin|droid|kimi|opencode|kilo|hermes|qodercli|qwen|cursor|mastracode|grok>"
        );
        return Ok(None);
    };
    if args.len() != 1 {
        eprintln!(
            "usage: herdr integration {action} <pi|omp|claude|codex|copilot|devin|droid|kimi|opencode|kilo|hermes|qodercli|qwen|cursor|mastracode|grok>"
        );
        return Ok(None);
    }

    let parsed = match target {
        "pi" => IntegrationTarget::Pi,
        "omp" => IntegrationTarget::Omp,
        "claude" => IntegrationTarget::Claude,
        "codex" => IntegrationTarget::Codex,
        "copilot" => IntegrationTarget::Copilot,
        "devin" => IntegrationTarget::Devin,
        "droid" => IntegrationTarget::Droid,
        "kimi" => IntegrationTarget::Kimi,
        "opencode" => IntegrationTarget::Opencode,
        "kilo" => IntegrationTarget::Kilo,
        "hermes" => IntegrationTarget::Hermes,
        "qodercli" => IntegrationTarget::Qodercli,
        "qwen" => IntegrationTarget::Qwen,
        "cursor" => IntegrationTarget::Cursor,
        "mastracode" => IntegrationTarget::Mastracode,
        "antigravity-cli" | "antigravity_cli" => IntegrationTarget::AntigravityCli,
        "grok" => IntegrationTarget::Grok,
        _ => {
            eprintln!("unknown integration target: {target}");
            eprintln!(
                "currently supported: pi, omp, claude, codex, copilot, devin, droid, kimi, opencode, kilo, hermes, qodercli, qwen, cursor, mastracode, antigravity-cli, grok"
            );
            return Ok(None);
        }
    };

    Ok(Some(parsed))
}

fn print_integration_help() {
    eprintln!("herdr integration commands:");
    eprintln!("  herdr integration install pi");
    eprintln!("  herdr integration install omp");
    eprintln!("  herdr integration install claude");
    eprintln!("  herdr integration install codex");
    eprintln!("  herdr integration install copilot");
    eprintln!("  herdr integration install devin");
    eprintln!("  herdr integration install droid");
    eprintln!("  herdr integration install kimi");
    eprintln!("  herdr integration install opencode");
    eprintln!("  herdr integration install kilo");
    eprintln!("  herdr integration install hermes");
    eprintln!("  herdr integration install qodercli");
    eprintln!("  herdr integration install qwen");
    eprintln!("  herdr integration install cursor");
    eprintln!("  herdr integration install mastracode");
    eprintln!("  herdr integration install antigravity-cli");
    eprintln!("  herdr integration install grok");
    eprintln!("  herdr integration uninstall pi");
    eprintln!("  herdr integration uninstall omp");
    eprintln!("  herdr integration uninstall claude");
    eprintln!("  herdr integration uninstall codex");
    eprintln!("  herdr integration uninstall copilot");
    eprintln!("  herdr integration uninstall devin");
    eprintln!("  herdr integration uninstall droid");
    eprintln!("  herdr integration uninstall kimi");
    eprintln!("  herdr integration uninstall opencode");
    eprintln!("  herdr integration uninstall kilo");
    eprintln!("  herdr integration uninstall hermes");
    eprintln!("  herdr integration uninstall qodercli");
    eprintln!("  herdr integration uninstall qwen");
    eprintln!("  herdr integration uninstall cursor");
    eprintln!("  herdr integration uninstall mastracode");
    eprintln!("  herdr integration uninstall antigravity-cli");
    eprintln!("  herdr integration uninstall grok");
    eprintln!("  herdr integration status [--outdated-only]");
}
