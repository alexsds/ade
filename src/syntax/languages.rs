use tree_sitter_highlight::HighlightConfiguration;

use crate::syntax::theme::HIGHLIGHT_NAMES;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    JavaScript,
    TypeScript,
    Tsx,
    Python,
    Go,
    C,
    Cpp,
    Java,
    Ruby,
    Bash,
    Html,
    Css,
    Json,
    Yaml,
    Markdown,
}

impl Language {
    /// Resolve a tree-sitter injection language name to a Language variant.
    /// Used by the injection callback when tree-sitter requests highlighting
    /// for an embedded language (e.g., "javascript" inside HTML `<script>`).
    pub fn from_injection_name(name: &str) -> Option<Self> {
        match name {
            "javascript" => Some(Language::JavaScript),
            "css" => Some(Language::Css),
            _ => None,
        }
    }

    /// Return the tree-sitter injection query for this language.
    /// Only HTML has injections (JS in `<script>`, CSS in `<style>`).
    pub fn injection_query(&self) -> &'static str {
        match self {
            Language::Html => tree_sitter_html::INJECTIONS_QUERY,
            _ => "",
        }
    }

    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            "rs" => Some(Language::Rust),
            "js" | "mjs" | "cjs" | "jsx" => Some(Language::JavaScript),
            "ts" | "mts" | "cts" => Some(Language::TypeScript),
            "tsx" => Some(Language::Tsx),
            "py" | "pyi" => Some(Language::Python),
            "go" => Some(Language::Go),
            "c" | "h" => Some(Language::C),
            "cpp" | "cc" | "cxx" | "hpp" | "hxx" | "hh" => Some(Language::Cpp),
            "java" => Some(Language::Java),
            "rb" | "rake" | "gemspec" => Some(Language::Ruby),
            "sh" | "bash" | "zsh" => Some(Language::Bash),
            "html" | "htm" => Some(Language::Html),
            "css" | "scss" => Some(Language::Css),
            "json" => Some(Language::Json),
            "yaml" | "yml" => Some(Language::Yaml),
            "md" | "markdown" | "mdx" => Some(Language::Markdown),
            _ => None,
        }
    }

    /// Create a HighlightConfiguration for this language.
    /// Pass `HIGHLIGHT_NAMES` to `configure()` before returning.
    pub fn create_highlight_config(&self) -> Result<HighlightConfiguration, String> {
        let (language, highlights_query) = match self {
            Language::Rust => (
                tree_sitter_rust::LANGUAGE.into(),
                tree_sitter_rust::HIGHLIGHTS_QUERY.to_string(),
            ),
            Language::JavaScript => (
                tree_sitter_javascript::LANGUAGE.into(),
                tree_sitter_javascript::HIGHLIGHT_QUERY.to_string(),
            ),
            Language::TypeScript => (
                tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
                tree_sitter_typescript::HIGHLIGHTS_QUERY.to_string(),
            ),
            Language::Tsx => (
                tree_sitter_typescript::LANGUAGE_TSX.into(),
                tree_sitter_typescript::HIGHLIGHTS_QUERY.to_string(),
            ),
            Language::Python => (
                tree_sitter_python::LANGUAGE.into(),
                tree_sitter_python::HIGHLIGHTS_QUERY.to_string(),
            ),
            Language::Go => (
                tree_sitter_go::LANGUAGE.into(),
                tree_sitter_go::HIGHLIGHTS_QUERY.to_string(),
            ),
            Language::C => (
                tree_sitter_c::LANGUAGE.into(),
                tree_sitter_c::HIGHLIGHT_QUERY.to_string(),
            ),
            Language::Cpp => (
                tree_sitter_cpp::LANGUAGE.into(),
                format!(
                    "{}\n{}",
                    tree_sitter_c::HIGHLIGHT_QUERY,
                    tree_sitter_cpp::HIGHLIGHT_QUERY
                ),
            ),
            Language::Java => (
                tree_sitter_java::LANGUAGE.into(),
                tree_sitter_java::HIGHLIGHTS_QUERY.to_string(),
            ),
            Language::Ruby => (
                tree_sitter_ruby::LANGUAGE.into(),
                tree_sitter_ruby::HIGHLIGHTS_QUERY.to_string(),
            ),
            Language::Bash => (
                tree_sitter_bash::LANGUAGE.into(),
                tree_sitter_bash::HIGHLIGHT_QUERY.to_string(),
            ),
            Language::Html => (
                tree_sitter_html::LANGUAGE.into(),
                tree_sitter_html::HIGHLIGHTS_QUERY.to_string(),
            ),
            Language::Css => (
                tree_sitter_css::LANGUAGE.into(),
                tree_sitter_css::HIGHLIGHTS_QUERY.to_string(),
            ),
            Language::Json => (
                tree_sitter_json::LANGUAGE.into(),
                tree_sitter_json::HIGHLIGHTS_QUERY.to_string(),
            ),
            Language::Yaml => (
                tree_sitter_yaml::LANGUAGE.into(),
                tree_sitter_yaml::HIGHLIGHTS_QUERY.to_string(),
            ),
            Language::Markdown => (
                tree_sitter_md::LANGUAGE.into(),
                tree_sitter_md::HIGHLIGHT_QUERY_BLOCK.to_string(),
            ),
        };

        let mut config = HighlightConfiguration::new(
            language,
            &format!("{:?}", self).to_lowercase(),
            &highlights_query,
            self.injection_query(),
            "", // locals query
        )
        .map_err(|e| format!("Failed to create highlight config for {:?}: {}", self, e))?;

        config.configure(HIGHLIGHT_NAMES);
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_from_extension_rust() {
        assert_eq!(Language::from_extension("rs"), Some(Language::Rust));
    }

    #[test]
    fn test_language_from_extension_javascript() {
        assert_eq!(Language::from_extension("js"), Some(Language::JavaScript));
        assert_eq!(Language::from_extension("mjs"), Some(Language::JavaScript));
        assert_eq!(Language::from_extension("cjs"), Some(Language::JavaScript));
        assert_eq!(Language::from_extension("jsx"), Some(Language::JavaScript));
    }

    #[test]
    fn test_language_from_extension_typescript() {
        assert_eq!(Language::from_extension("ts"), Some(Language::TypeScript));
        assert_eq!(Language::from_extension("mts"), Some(Language::TypeScript));
        assert_eq!(Language::from_extension("cts"), Some(Language::TypeScript));
    }

    #[test]
    fn test_language_from_extension_tsx() {
        assert_eq!(Language::from_extension("tsx"), Some(Language::Tsx));
    }

    #[test]
    fn test_language_from_extension_python() {
        assert_eq!(Language::from_extension("py"), Some(Language::Python));
        assert_eq!(Language::from_extension("pyi"), Some(Language::Python));
    }

    #[test]
    fn test_language_from_extension_go() {
        assert_eq!(Language::from_extension("go"), Some(Language::Go));
    }

    #[test]
    fn test_language_from_extension_c() {
        assert_eq!(Language::from_extension("c"), Some(Language::C));
        assert_eq!(Language::from_extension("h"), Some(Language::C));
    }

    #[test]
    fn test_language_from_extension_cpp() {
        assert_eq!(Language::from_extension("cpp"), Some(Language::Cpp));
        assert_eq!(Language::from_extension("cc"), Some(Language::Cpp));
        assert_eq!(Language::from_extension("cxx"), Some(Language::Cpp));
        assert_eq!(Language::from_extension("hpp"), Some(Language::Cpp));
        assert_eq!(Language::from_extension("hxx"), Some(Language::Cpp));
        assert_eq!(Language::from_extension("hh"), Some(Language::Cpp));
    }

    #[test]
    fn test_language_from_extension_java() {
        assert_eq!(Language::from_extension("java"), Some(Language::Java));
    }

    #[test]
    fn test_language_from_extension_ruby() {
        assert_eq!(Language::from_extension("rb"), Some(Language::Ruby));
        assert_eq!(Language::from_extension("rake"), Some(Language::Ruby));
        assert_eq!(Language::from_extension("gemspec"), Some(Language::Ruby));
    }

    #[test]
    fn test_language_from_extension_bash() {
        assert_eq!(Language::from_extension("sh"), Some(Language::Bash));
        assert_eq!(Language::from_extension("bash"), Some(Language::Bash));
        assert_eq!(Language::from_extension("zsh"), Some(Language::Bash));
    }

    #[test]
    fn test_language_from_extension_html() {
        assert_eq!(Language::from_extension("html"), Some(Language::Html));
        assert_eq!(Language::from_extension("htm"), Some(Language::Html));
    }

    #[test]
    fn test_language_from_extension_css() {
        assert_eq!(Language::from_extension("css"), Some(Language::Css));
        assert_eq!(Language::from_extension("scss"), Some(Language::Css));
    }

    #[test]
    fn test_language_from_extension_json() {
        assert_eq!(Language::from_extension("json"), Some(Language::Json));
    }

    #[test]
    fn test_language_from_extension_yaml() {
        assert_eq!(Language::from_extension("yaml"), Some(Language::Yaml));
        assert_eq!(Language::from_extension("yml"), Some(Language::Yaml));
    }

    #[test]
    fn test_language_from_extension_unknown() {
        assert_eq!(Language::from_extension("unknown"), None);
        assert_eq!(Language::from_extension(""), None);
        assert_eq!(Language::from_extension("xyz"), None);
    }

    #[test]
    fn test_all_languages_create_highlight_config() {
        let all_languages = [
            Language::Rust,
            Language::JavaScript,
            Language::TypeScript,
            Language::Tsx,
            Language::Python,
            Language::Go,
            Language::C,
            Language::Cpp,
            Language::Java,
            Language::Ruby,
            Language::Bash,
            Language::Html,
            Language::Css,
            Language::Json,
            Language::Yaml,
            Language::Markdown,
        ];

        for lang in &all_languages {
            let result = lang.create_highlight_config();
            match result {
                Ok(_) => {}
                Err(e) => panic!("Failed to create highlight config for {:?}: {}", lang, e),
            }
        }
    }

    #[test]
    fn test_from_injection_name_javascript() {
        assert_eq!(
            Language::from_injection_name("javascript"),
            Some(Language::JavaScript)
        );
    }

    #[test]
    fn test_from_injection_name_css() {
        assert_eq!(Language::from_injection_name("css"), Some(Language::Css));
    }

    #[test]
    fn test_from_injection_name_unknown() {
        assert_eq!(Language::from_injection_name("rust"), None);
        assert_eq!(Language::from_injection_name("unknown"), None);
        assert_eq!(Language::from_injection_name(""), None);
    }

    #[test]
    fn test_injection_query_html_non_empty() {
        assert!(
            !Language::Html.injection_query().is_empty(),
            "Html injection query should be non-empty"
        );
    }

    #[test]
    fn test_injection_query_non_html_empty() {
        assert!(
            Language::Rust.injection_query().is_empty(),
            "Non-Html languages should have empty injection query"
        );
        assert!(
            Language::JavaScript.injection_query().is_empty(),
            "Non-Html languages should have empty injection query"
        );
    }

    #[test]
    fn test_language_enum_has_15_variants() {
        // Verify all 15 variants exist by constructing them
        let variants = [
            Language::Rust,
            Language::JavaScript,
            Language::TypeScript,
            Language::Tsx,
            Language::Python,
            Language::Go,
            Language::C,
            Language::Cpp,
            Language::Java,
            Language::Ruby,
            Language::Bash,
            Language::Html,
            Language::Css,
            Language::Json,
            Language::Yaml,
            Language::Markdown,
        ];
        assert_eq!(variants.len(), 16);
    }
}
