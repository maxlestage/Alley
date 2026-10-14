//! La coloration du code partagé dans les salons, écrite en Rust : un
//! découpage en jetons (mots-clés, types, chaînes, nombres, commentaires,
//! fonctions, macros, attributs…) pour une trentaine de langages, et la
//! détection du langage quand le bloc ne le nomme pas.
//!
//! Pas d'expressions régulières ni de grammaires lourdes : un seul lecteur,
//! réglé par langage ([`Spec`]), plus des lecteurs dédiés pour le balisage
//! (HTML, XML), le CSS, le JSON, le YAML, le TOML et les diffs. Compilé aussi
//! hors du navigateur, pour les tests.

/// Ce qu'est un morceau de code (la classe CSS `tk-…`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Plain,
    Keyword,
    Type,
    Literal,
    String,
    Number,
    Comment,
    Function,
    Macro,
    Attribute,
    Variable,
    Operator,
    Tag,
    Key,
    Added,
    Removed,
    Meta,
}

impl Class {
    pub fn css(self) -> &'static str {
        match self {
            Class::Plain => "",
            Class::Keyword => "tk-kw",
            Class::Type => "tk-ty",
            Class::Literal => "tk-lit",
            Class::String => "tk-str",
            Class::Number => "tk-num",
            Class::Comment => "tk-com",
            Class::Function => "tk-fn",
            Class::Macro => "tk-mac",
            Class::Attribute => "tk-attr",
            Class::Variable => "tk-var",
            Class::Operator => "tk-op",
            Class::Tag => "tk-tag",
            Class::Key => "tk-key",
            Class::Added => "tk-add",
            Class::Removed => "tk-del",
            Class::Meta => "tk-meta",
        }
    }
}

/// Un morceau de code et ce qu'il est.
pub type Token<'a> = (Class, &'a str);

/// Les langages reconnus : `(nom affiché, alias)`.
const LANGS: &[(&str, &[&str])] = &[
    ("rust", &["rust", "rs"]),
    (
        "javascript",
        &["javascript", "js", "jsx", "mjs", "cjs", "node"],
    ),
    ("typescript", &["typescript", "ts", "tsx"]),
    ("python", &["python", "py", "py3"]),
    ("go", &["go", "golang"]),
    ("java", &["java"]),
    ("kotlin", &["kotlin", "kt", "kts"]),
    ("swift", &["swift"]),
    ("c", &["c", "h"]),
    ("cpp", &["cpp", "c++", "cc", "cxx", "hpp"]),
    ("csharp", &["csharp", "cs", "c#"]),
    ("php", &["php"]),
    ("ruby", &["ruby", "rb"]),
    ("dart", &["dart"]),
    ("scala", &["scala"]),
    ("lua", &["lua"]),
    (
        "bash",
        &["bash", "sh", "shell", "zsh", "console", "terminal"],
    ),
    ("powershell", &["powershell", "ps1", "pwsh"]),
    ("sql", &["sql", "postgres", "postgresql", "mysql", "sqlite"]),
    ("html", &["html", "htm", "vue", "svelte"]),
    ("xml", &["xml", "svg", "plist"]),
    ("css", &["css", "scss", "sass", "less"]),
    ("json", &["json", "jsonc", "json5"]),
    ("yaml", &["yaml", "yml"]),
    ("toml", &["toml", "ini", "cfg"]),
    ("dockerfile", &["dockerfile", "docker"]),
    ("diff", &["diff", "patch"]),
    ("markdown", &["markdown", "md"]),
];

/// Le nom d'un langage à partir de ce qu'on a écrit après ```` ``` ````.
pub fn language(name: &str) -> Option<&'static str> {
    let name = name.trim().to_ascii_lowercase();
    LANGS
        .iter()
        .find(|(_, aliases)| aliases.contains(&name.as_str()))
        .map(|(lang, _)| *lang)
}

/// Le nom à afficher en tête du bloc.
pub fn label(lang: &str) -> &'static str {
    match lang {
        "rust" => "Rust",
        "javascript" => "JavaScript",
        "typescript" => "TypeScript",
        "python" => "Python",
        "go" => "Go",
        "java" => "Java",
        "kotlin" => "Kotlin",
        "swift" => "Swift",
        "c" => "C",
        "cpp" => "C++",
        "csharp" => "C#",
        "php" => "PHP",
        "ruby" => "Ruby",
        "dart" => "Dart",
        "scala" => "Scala",
        "lua" => "Lua",
        "bash" => "Shell",
        "powershell" => "PowerShell",
        "sql" => "SQL",
        "html" => "HTML",
        "xml" => "XML",
        "css" => "CSS",
        "json" => "JSON",
        "yaml" => "YAML",
        "toml" => "TOML",
        "dockerfile" => "Dockerfile",
        "diff" => "Diff",
        "markdown" => "Markdown",
        _ => "Code",
    }
}

/// Devine le langage d'un bloc qui ne le nomme pas.
pub fn detect(code: &str) -> &'static str {
    let t = code.trim_start();
    let has = |s: &str| code.contains(s);
    let lines = || code.lines().map(str::trim_start);
    if t.starts_with("#!") {
        if t.starts_with("#!/usr/bin/env python") || t.starts_with("#!/usr/bin/python") {
            return "python";
        }
        if t.starts_with("#!/usr/bin/env node") {
            return "javascript";
        }
        return "bash";
    }
    if t.starts_with("<?php") {
        return "php";
    }
    if t.starts_with("<!DOCTYPE") || t.starts_with("<!doctype") || t.starts_with("<html") {
        return "html";
    }
    if t.starts_with("<?xml") {
        return "xml";
    }
    if t.starts_with("diff --git") || t.starts_with("--- ") || (t.starts_with("@@ ") && has(" @@"))
    {
        return "diff";
    }
    if (t.starts_with('{') || t.starts_with('['))
        && serde_json::from_str::<serde_json::Value>(code).is_ok()
    {
        return "json";
    }
    if lines().any(|l| l.starts_with("FROM ")) && lines().any(|l| l.starts_with("RUN ")) {
        return "dockerfile";
    }
    if has("fn ") && (has("let ") || has("->") || has("::") || has("pub ") || has("impl "))
        || has("println!")
        || has("#[derive")
    {
        return "rust";
    }
    if has("package main") || (has("func ") && has(":=")) {
        return "go";
    }
    if lines().any(|l| l.starts_with("def ") || l.starts_with("from ") && l.contains(" import "))
        || (has("import ") && has(":\n") && !has(";"))
        || has("print(") && has(":\n") && !has("{")
    {
        return "python";
    }
    if has("#include") {
        return if has("std::") || has("cout") || has("class ") {
            "cpp"
        } else {
            "c"
        };
    }
    if has("public static void main") || has("System.out.") {
        return "java";
    }
    if has("using System") || has("Console.Write") {
        return "csharp";
    }
    if has("interface ") && (has(": string") || has(": number")) || has(": string") && has("=>") {
        return "typescript";
    }
    if has("function ")
        || has("=>")
        || has("const ") && has(";")
        || has("console.log")
        || has("document.")
        || has("require(")
    {
        return "javascript";
    }
    let upper = code.to_ascii_uppercase();
    if (upper.contains("SELECT ") && upper.contains(" FROM "))
        || upper.contains("CREATE TABLE")
        || upper.contains("INSERT INTO")
    {
        return "sql";
    }
    if t.starts_with('<') && has("</") {
        return "html";
    }
    if lines().any(|l| l.starts_with('[') && l.ends_with(']')) && lines().any(|l| l.contains(" = "))
    {
        return "toml";
    }
    if has("{") && has("}") && lines().any(|l| l.contains(": ") && l.ends_with(';')) {
        return "css";
    }
    if lines().filter(|l| !l.is_empty()).count() > 0
        && lines()
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .all(|l| l.starts_with("- ") || l.contains(": ") || l.ends_with(':'))
    {
        return "yaml";
    }
    if lines().any(|l| l.starts_with("$ ") || l.starts_with("sudo ") || l.starts_with("cd "))
        || has("echo ")
        || has("export ")
    {
        return "bash";
    }
    "text"
}

/// Les règles d'un langage pour le lecteur générique.
struct Spec {
    keywords: &'static [&'static str],
    types: &'static [&'static str],
    literals: &'static [&'static str],
    line_comments: &'static [&'static str],
    block_comment: Option<(&'static str, &'static str)>,
    quotes: &'static str,
    /// Les mots-clés ne tiennent pas compte de la casse (SQL).
    case_insensitive: bool,
    /// `nom!` est une macro (Rust).
    bang_macros: bool,
    /// `#[…]` est un attribut (Rust).
    hash_attributes: bool,
    /// `@nom` est un décorateur ou une annotation.
    at_annotations: bool,
    /// `$nom` est une variable (shell, PHP, PowerShell).
    dollar_variables: bool,
    /// Un nom en majuscule initiale est un type.
    capitalized_types: bool,
    /// `"""…"""` est une chaîne (Python, Kotlin, Swift…).
    triple_quotes: bool,
}

const C_LIKE: Spec = Spec {
    keywords: &[],
    types: &[],
    literals: &["true", "false", "null"],
    line_comments: &["//"],
    block_comment: Some(("/*", "*/")),
    quotes: "\"'",
    case_insensitive: false,
    bang_macros: false,
    hash_attributes: false,
    at_annotations: false,
    dollar_variables: false,
    capitalized_types: true,
    triple_quotes: false,
};

fn spec(lang: &str) -> Option<Spec> {
    Some(match lang {
        "rust" => Spec {
            keywords: &[
                "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else",
                "enum", "extern", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod",
                "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super",
                "trait", "type", "unsafe", "use", "where", "while", "yield",
            ],
            types: &[
                "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128",
                "usize", "f32", "f64", "bool", "char", "str", "String", "Vec", "Option", "Result",
                "Box", "Rc", "Arc",
            ],
            literals: &["true", "false", "None", "Some", "Ok", "Err"],
            bang_macros: true,
            hash_attributes: true,
            quotes: "\"",
            ..C_LIKE
        },
        "javascript" | "typescript" => Spec {
            keywords: &[
                "async",
                "await",
                "break",
                "case",
                "catch",
                "class",
                "const",
                "continue",
                "debugger",
                "default",
                "delete",
                "do",
                "else",
                "export",
                "extends",
                "finally",
                "for",
                "from",
                "function",
                "if",
                "import",
                "in",
                "instanceof",
                "let",
                "new",
                "of",
                "return",
                "static",
                "super",
                "switch",
                "this",
                "throw",
                "try",
                "typeof",
                "var",
                "void",
                "while",
                "yield",
                "interface",
                "type",
                "enum",
                "implements",
                "private",
                "public",
                "protected",
                "readonly",
                "as",
                "declare",
                "namespace",
                "abstract",
            ],
            types: &[
                "string", "number", "boolean", "any", "unknown", "never", "object", "Promise",
                "Array", "Map", "Set", "Record",
            ],
            literals: &["true", "false", "null", "undefined", "NaN", "Infinity"],
            quotes: "\"'`",
            at_annotations: true,
            ..C_LIKE
        },
        "python" => Spec {
            keywords: &[
                "and", "as", "assert", "async", "await", "break", "class", "continue", "def",
                "del", "elif", "else", "except", "finally", "for", "from", "global", "if",
                "import", "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return",
                "try", "while", "with", "yield", "match", "case", "self",
            ],
            types: &[
                "int", "float", "str", "bool", "list", "dict", "set", "tuple", "bytes", "object",
                "None",
            ],
            literals: &["True", "False", "None"],
            line_comments: &["#"],
            block_comment: None,
            at_annotations: true,
            triple_quotes: true,
            ..C_LIKE
        },
        "go" => Spec {
            keywords: &[
                "break",
                "case",
                "chan",
                "const",
                "continue",
                "default",
                "defer",
                "else",
                "fallthrough",
                "for",
                "func",
                "go",
                "goto",
                "if",
                "import",
                "interface",
                "map",
                "package",
                "range",
                "return",
                "select",
                "struct",
                "switch",
                "type",
                "var",
            ],
            types: &[
                "bool", "byte", "error", "float32", "float64", "int", "int8", "int16", "int32",
                "int64", "rune", "string", "uint", "uint8", "uint16", "uint32", "uint64", "any",
            ],
            literals: &["true", "false", "nil", "iota"],
            quotes: "\"'`",
            ..C_LIKE
        },
        "java" | "kotlin" | "scala" | "dart" | "csharp" | "swift" => Spec {
            keywords: &[
                "abstract",
                "as",
                "async",
                "await",
                "break",
                "case",
                "catch",
                "class",
                "const",
                "continue",
                "data",
                "def",
                "default",
                "do",
                "else",
                "enum",
                "extends",
                "extension",
                "final",
                "finally",
                "for",
                "fun",
                "func",
                "get",
                "guard",
                "if",
                "implements",
                "import",
                "in",
                "init",
                "interface",
                "internal",
                "is",
                "let",
                "namespace",
                "new",
                "object",
                "open",
                "operator",
                "override",
                "package",
                "private",
                "protected",
                "public",
                "readonly",
                "return",
                "sealed",
                "set",
                "static",
                "struct",
                "super",
                "switch",
                "this",
                "throw",
                "throws",
                "trait",
                "try",
                "using",
                "val",
                "var",
                "void",
                "when",
                "where",
                "while",
                "with",
                "yield",
            ],
            types: &[
                "int", "long", "short", "byte", "float", "double", "boolean", "bool", "char",
                "string", "String", "Int", "Long", "Double", "Float", "Boolean", "Any", "Unit",
                "List", "Map", "Set", "var", "dynamic",
            ],
            literals: &["true", "false", "null", "nil"],
            at_annotations: true,
            triple_quotes: true,
            ..C_LIKE
        },
        "c" | "cpp" => Spec {
            keywords: &[
                "auto",
                "break",
                "case",
                "catch",
                "class",
                "const",
                "constexpr",
                "continue",
                "default",
                "delete",
                "do",
                "else",
                "enum",
                "explicit",
                "extern",
                "for",
                "friend",
                "goto",
                "if",
                "inline",
                "namespace",
                "new",
                "noexcept",
                "operator",
                "private",
                "protected",
                "public",
                "return",
                "sizeof",
                "static",
                "struct",
                "switch",
                "template",
                "this",
                "throw",
                "try",
                "typedef",
                "typename",
                "union",
                "using",
                "virtual",
                "volatile",
                "while",
            ],
            types: &[
                "bool", "char", "double", "float", "int", "long", "short", "signed", "unsigned",
                "void", "size_t", "uint8_t", "uint16_t", "uint32_t", "uint64_t", "int32_t",
                "int64_t", "string", "vector", "map",
            ],
            literals: &["true", "false", "NULL", "nullptr"],
            ..C_LIKE
        },
        "php" => Spec {
            keywords: &[
                "abstract",
                "as",
                "break",
                "case",
                "catch",
                "class",
                "const",
                "continue",
                "declare",
                "default",
                "do",
                "echo",
                "else",
                "elseif",
                "extends",
                "final",
                "finally",
                "fn",
                "for",
                "foreach",
                "function",
                "global",
                "if",
                "implements",
                "include",
                "interface",
                "match",
                "namespace",
                "new",
                "private",
                "protected",
                "public",
                "require",
                "return",
                "static",
                "switch",
                "throw",
                "trait",
                "try",
                "use",
                "while",
                "yield",
            ],
            types: &[
                "int", "float", "string", "bool", "array", "void", "mixed", "self",
            ],
            literals: &["true", "false", "null", "TRUE", "FALSE", "NULL"],
            line_comments: &["//", "#"],
            dollar_variables: true,
            ..C_LIKE
        },
        "ruby" => Spec {
            keywords: &[
                "alias",
                "and",
                "begin",
                "break",
                "case",
                "class",
                "def",
                "defined?",
                "do",
                "else",
                "elsif",
                "end",
                "ensure",
                "for",
                "if",
                "in",
                "module",
                "next",
                "not",
                "or",
                "redo",
                "rescue",
                "retry",
                "return",
                "self",
                "super",
                "then",
                "unless",
                "until",
                "when",
                "while",
                "yield",
                "require",
                "attr_accessor",
                "puts",
            ],
            types: &[],
            literals: &["true", "false", "nil"],
            line_comments: &["#"],
            block_comment: None,
            at_annotations: false,
            ..C_LIKE
        },
        "lua" => Spec {
            keywords: &[
                "and", "break", "do", "else", "elseif", "end", "for", "function", "goto", "if",
                "in", "local", "not", "or", "repeat", "return", "then", "until", "while",
            ],
            types: &[],
            literals: &["true", "false", "nil"],
            line_comments: &["--"],
            block_comment: Some(("--[[", "]]")),
            ..C_LIKE
        },
        "bash" | "dockerfile" => Spec {
            keywords: &[
                "if",
                "then",
                "else",
                "elif",
                "fi",
                "for",
                "while",
                "until",
                "do",
                "done",
                "case",
                "esac",
                "in",
                "function",
                "return",
                "local",
                "export",
                "readonly",
                "source",
                "alias",
                "unset",
                "set",
                "shift",
                "exit",
                "echo",
                "cd",
                "sudo",
                "FROM",
                "RUN",
                "CMD",
                "COPY",
                "ADD",
                "ENV",
                "ARG",
                "WORKDIR",
                "EXPOSE",
                "ENTRYPOINT",
                "USER",
                "VOLUME",
                "LABEL",
                "AS",
            ],
            types: &[],
            literals: &["true", "false"],
            line_comments: &["#"],
            block_comment: None,
            dollar_variables: true,
            capitalized_types: false,
            ..C_LIKE
        },
        "powershell" => Spec {
            keywords: &[
                "begin", "break", "catch", "class", "continue", "do", "else", "elseif", "end",
                "exit", "filter", "finally", "for", "foreach", "function", "if", "in", "param",
                "process", "return", "switch", "throw", "trap", "try", "until", "while",
            ],
            types: &[],
            literals: &["$true", "$false", "$null"],
            line_comments: &["#"],
            block_comment: Some(("<#", "#>")),
            dollar_variables: true,
            case_insensitive: true,
            ..C_LIKE
        },
        "sql" => Spec {
            keywords: &[
                "select",
                "from",
                "where",
                "and",
                "or",
                "not",
                "insert",
                "into",
                "values",
                "update",
                "set",
                "delete",
                "create",
                "table",
                "drop",
                "alter",
                "add",
                "index",
                "primary",
                "key",
                "foreign",
                "references",
                "join",
                "left",
                "right",
                "inner",
                "outer",
                "on",
                "group",
                "by",
                "order",
                "having",
                "limit",
                "offset",
                "as",
                "distinct",
                "union",
                "all",
                "case",
                "when",
                "then",
                "else",
                "end",
                "is",
                "in",
                "like",
                "between",
                "exists",
                "default",
                "unique",
                "constraint",
                "begin",
                "commit",
                "rollback",
                "returning",
                "with",
                "asc",
                "desc",
                "if",
            ],
            types: &[
                "int",
                "integer",
                "bigint",
                "smallint",
                "text",
                "varchar",
                "char",
                "boolean",
                "bool",
                "date",
                "timestamp",
                "timestamptz",
                "json",
                "jsonb",
                "uuid",
                "serial",
                "real",
                "float",
                "numeric",
                "decimal",
                "blob",
            ],
            literals: &["null", "true", "false"],
            line_comments: &["--"],
            quotes: "'\"`",
            case_insensitive: true,
            capitalized_types: false,
            ..C_LIKE
        },
        _ => return None,
    })
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Colore `code` écrit en `lang` (un nom rendu par [`language`] ou
/// [`detect`]) : les morceaux mis bout à bout redonnent exactement `code`.
pub fn highlight<'a>(code: &'a str, lang: &str) -> Vec<Token<'a>> {
    let mut out = match lang {
        "html" | "xml" => markup(code),
        "css" => css(code),
        "json" => json(code),
        "yaml" => yaml(code),
        "toml" => toml(code),
        "diff" => diff(code),
        "markdown" => markdown(code),
        _ => match spec(lang) {
            Some(spec) => generic(code, &spec),
            None => vec![(Class::Plain, code)],
        },
    };
    merge(code, &mut out);
    out
}

/// Recolle les morceaux voisins de même classe (tous des tranches de
/// `code`, repérées par leur position).
fn merge<'a>(code: &'a str, tokens: &mut Vec<Token<'a>>) {
    let base = code.as_ptr() as usize;
    let inside = |t: &str| {
        let p = t.as_ptr() as usize;
        p >= base && p + t.len() <= base + code.len()
    };
    if !tokens.iter().all(|(_, t)| inside(t)) {
        return;
    }
    let offset = |t: &str| t.as_ptr() as usize - base;
    let mut merged: Vec<(Class, usize, usize)> = Vec::with_capacity(tokens.len());
    for &(class, text) in tokens.iter() {
        if text.is_empty() {
            continue;
        }
        let (start, end) = (offset(text), offset(text) + text.len());
        match merged.last_mut() {
            Some((c, _, e)) if *c == class && *e == start => *e = end,
            _ => merged.push((class, start, end)),
        }
    }
    *tokens = merged
        .into_iter()
        .map(|(class, start, end)| (class, &code[start..end]))
        .collect();
}

/// Un petit lecteur : la position courante dans le texte.
struct Reader<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(src: &'a str) -> Self {
        Reader { src, pos: 0 }
    }
    fn rest(&self) -> &'a str {
        &self.src[self.pos..]
    }
    fn done(&self) -> bool {
        self.pos >= self.src.len()
    }
    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }
    fn peek2(&self) -> Option<char> {
        self.rest().chars().nth(1)
    }
    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }
    fn slice(&self, from: usize) -> &'a str {
        &self.src[from..self.pos]
    }
    fn starts(&self, s: &str) -> bool {
        self.rest().starts_with(s)
    }
    fn skip(&mut self, n: usize) {
        self.pos = (self.pos + n).min(self.src.len());
    }
    /// Avance jusqu'à `end` compris (ou jusqu'au bout).
    fn until(&mut self, end: &str) {
        match self.rest().find(end) {
            Some(i) => self.pos += i + end.len(),
            None => self.pos = self.src.len(),
        }
    }
    /// Avance jusqu'à la fin de la ligne (sans le retour).
    fn line(&mut self) {
        match self.rest().find('\n') {
            Some(i) => self.pos += i,
            None => self.pos = self.src.len(),
        }
    }
    fn while_(&mut self, f: impl Fn(char) -> bool) {
        while let Some(c) = self.peek() {
            if !f(c) {
                break;
            }
            self.bump();
        }
    }
    /// Une chaîne délimitée par `quote`, avec les échappements `\`.
    fn string(&mut self, quote: char) {
        self.bump();
        while let Some(c) = self.bump() {
            if c == '\\' {
                self.bump();
            } else if c == quote || (c == '\n' && quote != '`') {
                break;
            }
        }
    }
    fn number(&mut self) {
        if self.starts("0x") || self.starts("0b") || self.starts("0o") {
            self.skip(2);
            self.while_(|c| c.is_ascii_hexdigit() || c == '_');
        } else {
            self.while_(|c| c.is_ascii_digit() || c == '_');
            if self.peek() == Some('.') && self.peek2().is_some_and(|c| c.is_ascii_digit()) {
                self.bump();
                self.while_(|c| c.is_ascii_digit() || c == '_');
            }
            if matches!(self.peek(), Some('e' | 'E'))
                && self
                    .peek2()
                    .is_some_and(|c| c.is_ascii_digit() || c == '-' || c == '+')
            {
                self.bump();
                self.bump();
                self.while_(|c| c.is_ascii_digit());
            }
        }
        // Un suffixe : 10u32, 1.5f, 100n…
        self.while_(|c| c.is_ascii_alphanumeric());
    }
}

fn generic<'a>(code: &'a str, spec: &Spec) -> Vec<Token<'a>> {
    let mut out = Vec::new();
    let mut r = Reader::new(code);
    // Le nom qui suit `fn`, `def`, `func`… est celui d'une fonction.
    let mut naming = false;
    let word_in = |list: &[&str], w: &str| {
        if spec.case_insensitive {
            list.iter().any(|k| k.eq_ignore_ascii_case(w))
        } else {
            list.contains(&w)
        }
    };
    while !r.done() {
        let start = r.pos;
        let c = r.peek().unwrap_or(' ');
        if let Some((open, close)) = spec.block_comment
            && r.starts(open)
        {
            r.skip(open.len());
            r.until(close);
            out.push((Class::Comment, r.slice(start)));
            continue;
        }
        if spec.line_comments.iter().any(|l| r.starts(l))
            // `#` d'un attribut Rust, `$#` du shell : pas des commentaires.
            && !(spec.hash_attributes && r.starts("#["))
        {
            r.line();
            out.push((Class::Comment, r.slice(start)));
            continue;
        }
        if spec.triple_quotes && (r.starts("\"\"\"") || r.starts("'''")) {
            let q = &r.rest()[..3];
            r.skip(3);
            r.until(q);
            out.push((Class::String, r.slice(start)));
            continue;
        }
        if spec.hash_attributes && r.starts("#[") || r.starts("#![") {
            let mut depth = 0;
            while let Some(c) = r.bump() {
                match c {
                    '[' => depth += 1,
                    ']' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    '\n' => break,
                    _ => {}
                }
            }
            out.push((Class::Attribute, r.slice(start)));
            continue;
        }
        // Les durées de vie Rust (`'a`) ne sont pas des caractères.
        if spec.bang_macros && c == '\'' {
            let after: String = r.rest().chars().skip(1).take(3).collect();
            let mut chars = after.chars();
            let first = chars.next();
            let closes = after.chars().nth(1) == Some('\'')
                || (first == Some('\\') && after.chars().nth(2) == Some('\''));
            if !closes && first.is_some_and(is_ident_start) {
                r.bump();
                r.while_(is_ident);
                out.push((Class::Attribute, r.slice(start)));
                continue;
            }
            r.string('\'');
            out.push((Class::String, r.slice(start)));
            continue;
        }
        if spec.quotes.contains(c) {
            r.string(c);
            out.push((Class::String, r.slice(start)));
            continue;
        }
        // Les chaînes brutes et préfixées : r"…", b"…", f"…".
        if matches!(c, 'r' | 'b' | 'f' | 'u' | 'R' | 'B' | 'F' | '@')
            && r.peek2().is_some_and(|q| spec.quotes.contains(q))
        {
            r.bump();
            let q = r.peek().unwrap_or('"');
            r.string(q);
            out.push((Class::String, r.slice(start)));
            continue;
        }
        if c.is_ascii_digit() {
            r.number();
            out.push((Class::Number, r.slice(start)));
            continue;
        }
        if spec.dollar_variables && c == '$' {
            r.bump();
            if r.peek() == Some('{') {
                r.until("}");
            } else {
                r.while_(|c| is_ident(c) || "@#?*!".contains(c));
            }
            let text = r.slice(start);
            let class = if word_in(spec.literals, text) {
                Class::Literal
            } else {
                Class::Variable
            };
            out.push((class, text));
            continue;
        }
        if spec.at_annotations && c == '@' && r.peek2().is_some_and(is_ident_start) {
            r.bump();
            r.while_(|c| is_ident(c) || c == '.');
            out.push((Class::Attribute, r.slice(start)));
            continue;
        }
        if is_ident_start(c) {
            r.while_(is_ident);
            let word = r.slice(start);
            let next = r.peek();
            let class = if spec.bang_macros && next == Some('!') && r.peek2() != Some('=') {
                r.bump();
                Class::Macro
            } else if naming && !word_in(spec.keywords, word) {
                Class::Function
            } else if word_in(spec.keywords, word) {
                Class::Keyword
            } else if word_in(spec.literals, word) {
                Class::Literal
            } else if word_in(spec.types, word) {
                Class::Type
            } else if next == Some('(') || (spec.bang_macros && r.starts("::<")) {
                Class::Function
            } else if spec.capitalized_types
                && word.starts_with(|c: char| c.is_uppercase())
                && !word
                    .chars()
                    .all(|c| c.is_uppercase() || c == '_' || c.is_ascii_digit())
            {
                Class::Type
            } else {
                Class::Plain
            };
            naming = class == Class::Keyword
                && matches!(word, "fn" | "def" | "func" | "function" | "fun" | "sub");
            out.push((class, r.slice(start)));
            continue;
        }
        if !c.is_whitespace() {
            naming = false;
        }
        if "+-*/%=<>!&|^~?:".contains(c) {
            r.while_(|c| "+-*/%=<>!&|^~?:".contains(c));
            out.push((Class::Operator, r.slice(start)));
            continue;
        }
        r.bump();
        out.push((Class::Plain, r.slice(start)));
    }
    out
}

/// HTML et XML : balises, attributs, chaînes, commentaires, et le contenu.
fn markup(code: &str) -> Vec<Token<'_>> {
    let mut out = Vec::new();
    let mut r = Reader::new(code);
    while !r.done() {
        let start = r.pos;
        if r.starts("<!--") {
            r.until("-->");
            out.push((Class::Comment, r.slice(start)));
        } else if r.starts("<!") || r.starts("<?") {
            r.until(">");
            out.push((Class::Meta, r.slice(start)));
        } else if r.starts("<") && r.peek2().is_some_and(|c| c.is_alphabetic() || c == '/') {
            r.bump();
            if r.peek() == Some('/') {
                r.bump();
            }
            r.while_(|c| c.is_alphanumeric() || "-:_.".contains(c));
            out.push((Class::Tag, r.slice(start)));
            // Les attributs, jusqu'à `>`.
            while let Some(c) = r.peek() {
                let s = r.pos;
                if c == '>' || r.starts("/>") {
                    if c == '/' {
                        r.bump();
                    }
                    r.bump();
                    out.push((Class::Tag, r.slice(s)));
                    break;
                } else if c == '"' || c == '\'' {
                    r.string(c);
                    out.push((Class::String, r.slice(s)));
                } else if c.is_alphabetic() || c == '@' || c == ':' {
                    r.while_(|c| c.is_alphanumeric() || "-:_@.".contains(c));
                    out.push((Class::Attribute, r.slice(s)));
                } else {
                    r.bump();
                    out.push((Class::Plain, r.slice(s)));
                }
            }
        } else if r.starts("&") {
            r.while_(|c| c != ';' && !c.is_whitespace());
            if r.peek() == Some(';') {
                r.bump();
            }
            out.push((Class::Literal, r.slice(start)));
        } else {
            r.bump();
            r.while_(|c| c != '<' && c != '&');
            out.push((Class::Plain, r.slice(start)));
        }
    }
    out
}

fn css(code: &str) -> Vec<Token<'_>> {
    let mut out = Vec::new();
    let mut r = Reader::new(code);
    let mut depth = 0usize;
    while !r.done() {
        let start = r.pos;
        let c = r.peek().unwrap_or(' ');
        if r.starts("/*") {
            r.until("*/");
            out.push((Class::Comment, r.slice(start)));
        } else if r.starts("//") {
            r.line();
            out.push((Class::Comment, r.slice(start)));
        } else if c == '"' || c == '\'' {
            r.string(c);
            out.push((Class::String, r.slice(start)));
        } else if c == '@' {
            r.bump();
            r.while_(|c| is_ident(c) || c == '-');
            out.push((Class::Keyword, r.slice(start)));
        } else if c == '{' || c == '}' {
            r.bump();
            depth = if c == '{' {
                depth + 1
            } else {
                depth.saturating_sub(1)
            };
            out.push((Class::Plain, r.slice(start)));
        } else if depth > 0 && (is_ident_start(c) || c == '-') {
            r.while_(|c| is_ident(c) || c == '-');
            let is_property = r.rest().trim_start_matches([' ', '\t']).starts_with(':')
                && !r.rest().trim_start_matches([' ', '\t']).starts_with("::");
            let class = if is_property {
                Class::Key
            } else if r.peek() == Some('(') {
                Class::Function
            } else {
                Class::Literal
            };
            out.push((class, r.slice(start)));
        } else if c == '#' && depth > 0 {
            r.bump();
            r.while_(|c| c.is_ascii_hexdigit());
            out.push((Class::Number, r.slice(start)));
        } else if c.is_ascii_digit() || (c == '.' && r.peek2().is_some_and(|c| c.is_ascii_digit()))
        {
            r.bump();
            r.while_(|c| c.is_ascii_digit() || c == '.');
            r.while_(|c| c.is_alphabetic() || c == '%');
            out.push((Class::Number, r.slice(start)));
        } else if depth == 0 && !c.is_whitespace() {
            // Au moins un caractère : la lecture avance toujours.
            r.bump();
            r.while_(|c| c != '{' && c != ',' && c != '\n' && !r_starts_comment(c));
            out.push((Class::Tag, r.slice(start)));
        } else {
            r.bump();
            out.push((Class::Plain, r.slice(start)));
        }
    }
    out
}

fn r_starts_comment(c: char) -> bool {
    c == '/'
}

fn json(code: &str) -> Vec<Token<'_>> {
    let mut out = Vec::new();
    let mut r = Reader::new(code);
    while !r.done() {
        let start = r.pos;
        let c = r.peek().unwrap_or(' ');
        if c == '"' {
            r.string('"');
            let is_key = r.rest().trim_start().starts_with(':');
            out.push((
                if is_key { Class::Key } else { Class::String },
                r.slice(start),
            ));
        } else if c.is_ascii_digit() || c == '-' {
            r.bump();
            r.while_(|c| c.is_ascii_digit() || ".eE+-".contains(c));
            out.push((Class::Number, r.slice(start)));
        } else if c.is_alphabetic() {
            r.while_(char::is_alphabetic);
            out.push((Class::Literal, r.slice(start)));
        } else if r.starts("//") {
            r.line();
            out.push((Class::Comment, r.slice(start)));
        } else {
            r.bump();
            out.push((Class::Plain, r.slice(start)));
        }
    }
    out
}

/// Une valeur simple (YAML, TOML) : chaîne, nombre, booléen.
fn scalar<'a>(out: &mut Vec<Token<'a>>, value: &'a str) {
    let t = value.trim();
    if t.is_empty() {
        out.push((Class::Plain, value));
        return;
    }
    let lead = &value[..value.len() - value.trim_start().len()];
    let tail = &value[lead.len() + t.len()..];
    out.push((Class::Plain, lead));
    let class = if t.starts_with('"') || t.starts_with('\'') {
        Class::String
    } else if t.parse::<f64>().is_ok() {
        Class::Number
    } else if matches!(
        t,
        "true" | "false" | "null" | "~" | "yes" | "no" | "True" | "False" | "on" | "off"
    ) {
        Class::Literal
    } else {
        Class::String
    };
    out.push((class, t));
    out.push((Class::Plain, tail));
}

/// Coupe une ligne avant un commentaire `#` (hors chaînes).
fn split_comment(line: &str) -> (&str, &str) {
    let mut quote = None;
    for (i, c) in line.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), _) if c == q => quote = None,
            (None, '#') if i == 0 || line[..i].ends_with(' ') => {
                return (&line[..i], &line[i..]);
            }
            _ => {}
        }
    }
    (line, "")
}

fn yaml(code: &str) -> Vec<Token<'_>> {
    let mut out = Vec::new();
    for line in code.split_inclusive('\n') {
        let (body, comment) = split_comment(line);
        let trimmed = body.trim_start();
        let indent = &body[..body.len() - trimmed.len()];
        out.push((Class::Plain, indent));
        let (marker, rest) = match trimmed.strip_prefix("- ") {
            Some(rest) => (&trimmed[..2], rest),
            None => ("", trimmed),
        };
        out.push((Class::Operator, marker));
        match rest.find(':').filter(|i| {
            let after = &rest[i + 1..];
            after.is_empty() || after.starts_with([' ', '\n', '\r'])
        }) {
            Some(i) if !rest.starts_with(['"', '\'']) => {
                out.push((Class::Key, &rest[..i]));
                out.push((Class::Plain, &rest[i..i + 1]));
                scalar(&mut out, &rest[i + 1..]);
            }
            _ if trimmed.starts_with("---") => out.push((Class::Meta, rest)),
            _ => scalar(&mut out, rest),
        }
        out.push((Class::Comment, comment));
    }
    out
}

fn toml(code: &str) -> Vec<Token<'_>> {
    let mut out = Vec::new();
    for line in code.split_inclusive('\n') {
        let (body, comment) = split_comment(line);
        let trimmed = body.trim_start();
        let indent = &body[..body.len() - trimmed.len()];
        out.push((Class::Plain, indent));
        if trimmed.starts_with('[') {
            let end = trimmed.trim_end();
            out.push((Class::Tag, end));
            out.push((Class::Plain, &trimmed[end.len()..]));
        } else if let Some(i) = trimmed.find('=') {
            out.push((Class::Key, &trimmed[..i]));
            out.push((Class::Operator, &trimmed[i..i + 1]));
            scalar(&mut out, &trimmed[i + 1..]);
        } else {
            out.push((Class::Plain, trimmed));
        }
        out.push((Class::Comment, comment));
    }
    out
}

fn diff(code: &str) -> Vec<Token<'_>> {
    code.split_inclusive('\n')
        .map(|line| {
            let class = if line.starts_with("+++") || line.starts_with("---") {
                Class::Meta
            } else if line.starts_with('+') {
                Class::Added
            } else if line.starts_with('-') {
                Class::Removed
            } else if line.starts_with("@@") || line.starts_with("diff ") {
                Class::Meta
            } else {
                Class::Plain
            };
            (class, line)
        })
        .collect()
}

fn markdown(code: &str) -> Vec<Token<'_>> {
    code.split_inclusive('\n')
        .map(|line| {
            let t = line.trim_start();
            let class = if t.starts_with('#') {
                Class::Keyword
            } else if t.starts_with("- ") || t.starts_with("* ") || t.starts_with("> ") {
                Class::Operator
            } else if t.starts_with("```") {
                Class::Meta
            } else {
                Class::Plain
            };
            (class, line)
        })
        .collect()
}

/// Les morceaux, ligne par ligne (un morceau sur plusieurs lignes, comme
/// un commentaire de bloc, est coupé à chaque fin de ligne).
pub fn lines<'a>(tokens: &[Token<'a>]) -> Vec<Vec<Token<'a>>> {
    let mut lines = vec![Vec::new()];
    for &(class, text) in tokens {
        let mut parts = text.split('\n').peekable();
        while let Some(part) = parts.next() {
            if !part.is_empty() {
                lines.last_mut().expect("une ligne").push((class, part));
            }
            if parts.peek().is_some() {
                lines.push(Vec::new());
            }
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classes<'a>(code: &'a str, lang: &str) -> Vec<(Class, &'a str)> {
        let tokens = highlight(code, lang);
        let joined: String = tokens.iter().map(|(_, t)| *t).collect();
        assert_eq!(joined, code, "le code est restitué à l'identique ({lang})");
        tokens
            .into_iter()
            .filter(|(c, _)| *c != Class::Plain)
            .collect()
    }

    fn has(tokens: &[(Class, &str)], class: Class, text: &str) -> bool {
        tokens.contains(&(class, text))
    }

    #[test]
    fn rust() {
        let t = classes(
            "#[derive(Debug)]\npub fn main<'a>(x: &'a str) -> u32 {\n    // salut\n    let s = \"chaîne\";\n    println!(\"{s}\");\n    Some(42u32).unwrap_or(0x1F)\n}",
            "rust",
        );
        assert!(has(&t, Class::Attribute, "#[derive(Debug)]"));
        assert!(has(&t, Class::Keyword, "pub"));
        assert!(has(&t, Class::Keyword, "fn"));
        assert!(has(&t, Class::Function, "main"));
        assert!(has(&t, Class::Attribute, "'a"));
        assert!(has(&t, Class::Type, "u32"));
        assert!(has(&t, Class::Comment, "// salut"));
        assert!(has(&t, Class::String, "\"chaîne\""));
        assert!(has(&t, Class::Macro, "println!"));
        assert!(has(&t, Class::Literal, "Some"));
        assert!(has(&t, Class::Number, "42u32"));
        assert!(has(&t, Class::Number, "0x1F"));
        assert!(has(&classes("let c = 'x';", "rust"), Class::String, "'x'"));
    }

    #[test]
    fn javascript_python_sql_shell() {
        let t = classes(
            "const f = async () => { await fetch(`/api/${id}`); } // fin",
            "javascript",
        );
        assert!(has(&t, Class::Keyword, "const"));
        assert!(has(&t, Class::String, "`/api/${id}`"));
        assert!(has(&t, Class::Function, "fetch"));
        assert!(has(&t, Class::Comment, "// fin"));

        let t = classes(
            "@app.get('/')\ndef index(n: int) -> str:\n    \"\"\"Doc\"\"\"\n    return f\"{n}\"  # ok",
            "python",
        );
        assert!(has(&t, Class::Attribute, "@app.get"));
        assert!(has(&t, Class::Keyword, "def"));
        assert!(has(&t, Class::Function, "index"));
        assert!(has(&t, Class::Type, "int"));
        assert!(has(&t, Class::String, "\"\"\"Doc\"\"\""));
        assert!(has(&t, Class::String, "f\"{n}\""));
        assert!(has(&t, Class::Comment, "# ok"));

        let t = classes("SELECT name FROM users WHERE id = 3; -- un", "sql");
        assert!(has(&t, Class::Keyword, "SELECT"));
        assert!(has(&t, Class::Number, "3"));
        assert!(has(&t, Class::Comment, "-- un"));

        let t = classes("export PATH=\"$HOME/bin\" # chemin\necho ${USER}", "bash");
        assert!(has(&t, Class::Keyword, "export"));
        assert!(has(&t, Class::Comment, "# chemin"));
        assert!(has(&t, Class::Variable, "${USER}"));
    }

    #[test]
    fn markup_css_data() {
        let t = classes(
            "<!-- c --><a href=\"/x\" class='y'>Hi &amp; bye</a>",
            "html",
        );
        assert!(has(&t, Class::Comment, "<!-- c -->"));
        assert!(has(&t, Class::Tag, "<a"));
        assert!(has(&t, Class::Attribute, "href"));
        assert!(has(&t, Class::String, "\"/x\""));
        assert!(has(&t, Class::Literal, "&amp;"));
        assert!(has(&t, Class::Tag, "</a>"));

        let t = classes(
            "@media (max-width: 600px) {\n  .a:hover { color: #fff; margin: 0 2px; }\n}",
            "css",
        );
        assert!(has(&t, Class::Keyword, "@media"));
        assert!(has(&t, Class::Key, "color"));
        assert!(has(&t, Class::Number, "#fff"));
        assert!(has(&t, Class::Number, "2px"));

        let t = classes("{\"a\": [1, -2.5e3, true, null, \"x\"]}", "json");
        assert!(has(&t, Class::Key, "\"a\""));
        assert!(has(&t, Class::Number, "-2.5e3"));
        assert!(has(&t, Class::Literal, "true"));
        assert!(has(&t, Class::String, "\"x\""));

        let t = classes(
            "name: alley # nom\nport: 8080\nitems:\n  - debug: true\n",
            "yaml",
        );
        assert!(has(&t, Class::Key, "name"));
        assert!(has(&t, Class::Number, "8080"));
        assert!(has(&t, Class::Comment, "# nom\n"));
        assert!(has(&t, Class::Literal, "true"));

        let t = classes("[package]\nname = \"alley\" # nom\nversion = 1\n", "toml");
        assert!(has(&t, Class::Tag, "[package]"));
        assert!(has(&t, Class::Key, "name "));
        assert!(has(&t, Class::String, "\"alley\""));

        let t = classes("@@ -1 +1 @@\n-avant\n+après\n", "diff");
        assert!(has(&t, Class::Removed, "-avant\n"));
        assert!(has(&t, Class::Added, "+après\n"));
    }

    #[test]
    fn languages_are_named_and_detected() {
        assert_eq!(language("RS"), Some("rust"));
        assert_eq!(language("c++"), Some("cpp"));
        assert_eq!(language("yml"), Some("yaml"));
        assert_eq!(language("brainfuck"), None);
        for (code, lang) in [
            ("fn main() {\n    let x = 1;\n}", "rust"),
            ("def f(x):\n    return x\n", "python"),
            ("const x = () => 1;", "javascript"),
            (
                "interface A { b: string }\nconst f = (a: A) => a.b;",
                "typescript",
            ),
            ("package main\n\nfunc main() {}", "go"),
            ("#include <stdio.h>\nint main() {}", "c"),
            ("SELECT * FROM t WHERE a = 1", "sql"),
            ("{\"a\": 1}", "json"),
            ("<div class=\"a\">x</div>", "html"),
            ("$ cargo build\n$ cargo test", "bash"),
            ("diff --git a/x b/x\n", "diff"),
            ("[server]\nport = 80", "toml"),
            ("name: alley\nport: 80", "yaml"),
            ("bonjour tout le monde", "text"),
        ] {
            assert_eq!(detect(code), lang, "{code}");
        }
    }

    #[test]
    fn every_language_survives_odd_input() {
        let odd = [
            "",
            "\"",
            "'",
            "`",
            "#",
            "<",
            "/*",
            "<!--",
            "\\",
            "@",
            "$",
            "#[",
            "'a",
            "r\"x",
            "\"\"\"",
            "日本語 🦀 ünïcödé \"é\" 'ç'",
            "a: b: c\n- - x\n[[t]]\n= 1",
            "0x 1e 1.e5 .5 9_",
            "<a b=\"c'>\n</",
        ];
        for (lang, _) in LANGS {
            for code in odd {
                let joined: String = highlight(code, lang).iter().map(|(_, t)| *t).collect();
                assert_eq!(joined, code, "{lang} : {code:?}");
            }
        }
    }

    #[test]
    fn random_text_never_hangs_or_loses_characters() {
        // Un générateur pseudo-aléatoire fixe (reproductible).
        let mut seed: u64 = 0x2545_F491_4F6C_DD1D;
        let alphabet: Vec<char> = "ab_Z09 \n\t\"'`#/*<>!-=:;,.{}[]()$@&\\é🦀"
            .chars()
            .collect();
        for _ in 0..2000 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let len = (seed % 24) as usize;
            let code: String = (0..len)
                .map(|i| alphabet[((seed >> (i % 50)) as usize + i * 7) % alphabet.len()])
                .collect();
            for (lang, _) in LANGS {
                let joined: String = highlight(&code, lang).iter().map(|(_, t)| *t).collect();
                assert_eq!(joined, code, "{lang} : {code:?}");
            }
        }
    }

    #[test]
    fn lines_split_multiline_tokens() {
        let tokens = highlight("/* a\nb */ x", "c");
        let lines = lines(&tokens);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], vec![(Class::Comment, "/* a")]);
        assert_eq!(lines[1][0], (Class::Comment, "b */"));
    }
}
