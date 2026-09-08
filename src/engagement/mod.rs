pub mod constants;
pub mod error;
pub mod types;
pub mod validation;

pub use constants::{
    MAX_ENGAGEMENT_TEXT_BYTES, MAX_OFF_LIMITS, authorized_engagement_doctrine,
    ctf_solve_loop_doctrine, execution_style_directive,
};
pub use error::EngagementError;
pub use types::{Engagement, EngagementKind};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_kind_case_insensitively() {
        assert_eq!(EngagementKind::parse("CTF").unwrap(), EngagementKind::Ctf);
        assert_eq!(
            EngagementKind::parse(" pentest ").unwrap(),
            EngagementKind::Pentest
        );
        assert!(EngagementKind::parse("nonsense").is_err());
    }

    #[test]
    fn flags_override_file_values() {
        let base = Engagement::from_json(r#"{"kind":"lab","scope":"10.0.0.0/24"}"#).unwrap();
        let merged = base
            .overlay(
                Some(EngagementKind::Ctf),
                Some("10.10.10.5".to_owned()),
                None,
                Some(r"flag\{[^}]+\}".to_owned()),
                None,
                None,
            )
            .unwrap();
        assert_eq!(merged.kind, EngagementKind::Ctf);
        assert_eq!(merged.scope.as_deref(), Some("10.10.10.5"));
        assert_eq!(merged.flag_format.as_deref(), Some(r"flag\{[^}]+\}"));
    }

    #[test]
    fn context_block_never_renders_when_field_absent() {
        let engagement = Engagement {
            kind: EngagementKind::Ctf,
            ..Engagement::default()
        };
        let block = engagement.render_context();
        assert!(block.contains("kind: ctf"));
        assert!(!block.contains("scope:"));
    }

    #[test]
    fn extract_flag_matches_regex_from_output() {
        let engagement = Engagement {
            flag_format: Some(r"flag\{[^}]+\}".to_owned()),
            ..Engagement::default()
        };
        let output = "nmap done\nleaked: flag{r34l_flag} on the box\n";
        assert_eq!(
            engagement.extract_flag(output).as_deref(),
            Some("flag{r34l_flag}")
        );
        assert_eq!(engagement.extract_flag("no flag here"), None);
    }

    #[test]
    fn extract_flag_none_without_format() {
        assert_eq!(Engagement::default().extract_flag("flag{x}"), None);
    }

    #[test]
    fn rejects_oversized_field() {
        let big = "a".repeat(MAX_ENGAGEMENT_TEXT_BYTES + 1);
        let engagement = Engagement {
            scope: Some(big),
            ..Engagement::default()
        };
        assert!(engagement.validate().is_err());
    }

    #[test]
    fn rejects_aggregate_prompt_inflation_from_individually_valid_fields() {
        let engagement = Engagement {
            off_limits: (0..MAX_OFF_LIMITS)
                .map(|index| format!("host-{index}-{}", "x".repeat(512)))
                .collect(),
            ..Engagement::default()
        };

        assert!(engagement.validate().is_err());
    }

    #[test]
    fn ctf_solve_loop_and_doctrines_are_embedded_and_consistent() {
        assert!(!authorized_engagement_doctrine().is_empty());
        assert!(!execution_style_directive().is_empty());
        let ctf = ctf_solve_loop_doctrine();
        assert!(!ctf.is_empty());
        assert!(ctf.contains("Diagnostic Tracer Bullets"));
        assert!(ctf.contains("Silent Wall vs. Live Seam"));
        assert!(ctf.contains("Self-Reflection & Meta-Cognitive Audit"));
        assert!(ctf.contains("Universal Client Compatibility"));
    }
}
