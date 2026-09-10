use clap::Parser;

pub mod args;
pub mod inspect;
pub mod plain;
pub mod runner;

pub use args::{Cli, Command, RunArgs, build_engagement};
pub use inspect::inspect;
pub use runner::run;

pub async fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Run {
            goal,
            workspace,
            run: run_root,
            resume,
            auto,
            plain,
            engagement,
            engagement_kind,
            target,
            off_limits,
            flag_format,
            objective,
            max_turns,
            context_tokens,
            max_tokens,
            headless,
        } => {
            let engagement = build_engagement(
                engagement.as_deref(),
                engagement_kind.as_deref(),
                target,
                off_limits,
                flag_format,
                objective,
            )?;
            run(RunArgs {
                goal,
                workspace,
                run: run_root,
                resume,
                auto,
                plain,
                engagement,
                max_turns,
                context_tokens,
                max_tokens,
                headless,
            })
            .await
        }
        Command::Inspect { run, agent } => inspect(&run, agent.as_deref()),
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use tokio::io::BufReader;

    use pentesting::settings::ProviderSettingsStore;

    use super::plain::{read_bounded_line, select_operation_or_shutdown};
    use super::runner::load_provider_slot;

    #[tokio::test]
    async fn plain_input_reader_never_buffers_beyond_its_line_limit() {
        let mut accepted = BufReader::new(&b"hello\nnext\n"[..]);
        assert_eq!(
            read_bounded_line(&mut accepted, 5)
                .await
                .unwrap()
                .as_deref(),
            Some("hello")
        );
        assert_eq!(
            read_bounded_line(&mut accepted, 5)
                .await
                .unwrap()
                .as_deref(),
            Some("next")
        );

        let mut oversized = BufReader::new(&b"123456\n"[..]);
        assert_eq!(
            read_bounded_line(&mut oversized, 5)
                .await
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::InvalidData
        );
    }

    #[tokio::test]
    async fn shutdown_wins_while_a_plain_operation_is_pending() {
        let result = select_operation_or_shutdown(
            std::future::pending::<Result<(), io::Error>>(),
            std::future::ready(Ok(())),
        )
        .await
        .unwrap();

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn saved_provider_context_limit_is_applied_before_runtime_creation() {
        let directory = tempfile::tempdir().unwrap();
        let store = ProviderSettingsStore::new(directory.path());
        store
            .save(&pentesting::settings::ProviderSettings {
                provider: "openai-compatible".to_owned(),
                base_url: "https://example.test/v1".to_owned(),
                model: "small-model".to_owned(),
                api_key: "secret".to_owned(),
                context_tokens: 16_384,
                max_output_tokens: None,
            })
            .unwrap();

        let slot = load_provider_slot(&store).await.unwrap();

        assert_eq!(slot.runtime_context_limit(), 16_384);
        assert!(slot.is_configured());
    }
}
