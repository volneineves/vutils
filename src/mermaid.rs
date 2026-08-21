use meraid::{Layout, Renderer, Theme, ThemeType, parse_mermaid};

use crate::{Result, VutilsError};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CharacterSet {
    #[default]
    Unicode,
    Ascii,
}

/// Render a supported Mermaid diagram as plain terminal text.
///
/// The renderer supports flowcharts, sequence, class, state, entity-relationship,
/// and pie diagrams. It is deliberately monochrome so its output remains safe for
/// pipelines and embedded terminal interfaces.
pub fn render(source: &str, character_set: CharacterSet) -> Result<String> {
    let source = source.trim();
    let source = source
        .strip_prefix('\u{feff}')
        .unwrap_or(source)
        .trim_start();
    if source.is_empty() {
        return Err(VutilsError::InvalidInput(
            "Mermaid source cannot be empty".into(),
        ));
    }

    let diagram = parse_mermaid(source)
        .map_err(|error| VutilsError::InvalidInput(format!("invalid Mermaid diagram: {error}")))?;
    let layout = Layout::new(&diagram).layout();
    let renderer = Renderer::new(Theme::get(ThemeType::Mono))
        .ascii_only(matches!(character_set, CharacterSet::Ascii));

    Ok(renderer.render(&diagram, &layout).trim_end().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_unicode_flowchart() {
        let output = render(
            "flowchart LR\n  start[Edit] --> done[Rendered]",
            CharacterSet::Unicode,
        )
        .unwrap();

        assert!(output.contains("Edit"));
        assert!(output.contains("Rendered"));
        assert!(output.contains('┌'));
    }

    #[test]
    fn renders_ascii_sequence_diagram_without_box_drawing_glyphs() {
        let output = render(
            "sequenceDiagram\n  Editor->>Renderer: source\n  Renderer-->>Editor: preview",
            CharacterSet::Ascii,
        )
        .unwrap();

        assert!(output.contains("Editor"));
        assert!(output.contains("preview"));
        assert!(!output.contains(['┌', '─', '│', '▶']));
    }

    #[test]
    fn accepts_a_utf8_bom_and_leading_whitespace() {
        let output = render(
            "\u{feff}\n  stateDiagram-v2\n  [*] --> Editing",
            CharacterSet::Unicode,
        )
        .unwrap();

        assert!(output.contains("Editing"));
    }

    #[test]
    fn renders_each_documented_diagram_family() {
        let examples = [
            ("sequenceDiagram\n  A->>B: Hello", "Hello"),
            (
                "classDiagram\n  class Editor\n  class Renderer\n  Editor --> Renderer",
                "Editor --> Renderer",
            ),
            (
                "stateDiagram-v2\n  [*] --> Editing\n  Editing --> Rendered",
                "Rendered",
            ),
            ("erDiagram\n  USER ||--o{ POST : writes", "USER"),
            ("pie title Usage\n  \"CLI\" : 60\n  \"TUI\" : 40", "CLI"),
        ];

        for (source, expected) in examples {
            let output = render(source, CharacterSet::Unicode).unwrap();
            assert!(
                output.contains(expected),
                "rendered output did not contain {expected:?}:\n{output}"
            );
        }
    }

    #[test]
    fn rejects_empty_and_unsupported_diagrams() {
        let empty = render(" \n", CharacterSet::Unicode).unwrap_err();
        assert!(empty.to_string().contains("cannot be empty"));

        let unsupported = render("gitGraph\n  commit", CharacterSet::Unicode).unwrap_err();
        assert!(unsupported.to_string().contains("unsupported diagram type"));
    }
}
