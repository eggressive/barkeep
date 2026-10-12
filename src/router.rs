/// Deterministic query routing.
///
/// - plain text      -> app search
/// - `? <prompt>`    -> ask the AI (streams the answer)
/// - `# <tool>`      -> MCP tool search / invoke
#[derive(Debug, PartialEq)]
pub enum Mode {
    Apps { query: String },
    Ask { prompt: String },
    Tool { name: String, raw_args: String },
}

pub fn parse(raw: &str) -> Mode {
    let raw = raw.trim_start();
    if let Some(prompt) = raw.strip_prefix('?') {
        return Mode::Ask {
            prompt: prompt.trim_start().to_string(),
        };
    }
    if let Some(rest) = raw.strip_prefix('#') {
        let rest = rest.trim_start();
        let (name, args) = match rest.split_once(char::is_whitespace) {
            Some((name, args)) => (name.to_string(), args.trim().to_string()),
            None => (rest.to_string(), String::new()),
        };
        return Mode::Tool {
            name,
            raw_args: args,
        };
    }
    Mode::Apps {
        query: raw.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Mode::*;

    #[test]
    fn plain_text_is_app_search() {
        assert_eq!(
            parse("firefox"),
            Apps {
                query: "firefox".into()
            }
        );
    }

    #[test]
    fn empty_is_app_search() {
        assert_eq!(parse(""), Apps { query: "".into() });
        assert_eq!(parse("   "), Apps { query: "".into() });
    }

    #[test]
    fn question_mark_is_ask() {
        assert_eq!(
            parse("?why is the sky blue"),
            Ask {
                prompt: "why is the sky blue".into()
            }
        );
    }

    #[test]
    fn tool_without_args() {
        assert_eq!(
            parse("#echo"),
            Tool {
                name: "echo".into(),
                raw_args: "".into()
            }
        );
    }

    #[test]
    fn tool_with_args() {
        assert_eq!(
            parse("#echo {\"text\": \"hi\"}"),
            Tool {
                name: "echo".into(),
                raw_args: "{\"text\": \"hi\"}".into()
            }
        );
    }

    #[test]
    fn whitespace_after_prefix() {
        assert_eq!(
            parse("?  hello"),
            Ask {
                prompt: "hello".into()
            }
        );
        assert_eq!(
            parse("#  echo {}"),
            Tool {
                name: "echo".into(),
                raw_args: "{}".into()
            }
        );
    }
}
