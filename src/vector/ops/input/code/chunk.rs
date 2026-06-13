#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SymbolKind {
    Function,
    Method,
    Struct,
    Enum,
    Trait,
    Impl,
    Const,
    Static,
    Type,
    Mod,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChunkSource {
    TreeSitter,
    Markdown,
    Prose,
}

impl ChunkSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TreeSitter => "tree_sitter",
            Self::Markdown => "markdown",
            Self::Prose => "prose",
        }
    }
}

impl SymbolKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Method => "method",
            Self::Struct => "struct",
            Self::Enum => "enum",
            Self::Trait => "trait",
            Self::Impl => "impl",
            Self::Const => "const",
            Self::Static => "static",
            Self::Type => "type",
            Self::Mod => "mod",
            Self::Other => "other",
        }
    }

    pub const fn is_tiny_merge_eligible(self) -> bool {
        matches!(self, Self::Const | Self::Static | Self::Type)
    }

    /// Parse the canonical string form (the inverse of [`as_str`]). Keeps the
    /// stored-payload → ranking round-trip honest: a renamed variant breaks this
    /// at compile time rather than silently mismatching a hand-typed literal.
    ///
    /// [`as_str`]: SymbolKind::as_str
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(value: &str) -> Option<Self> {
        let kind = match value {
            "function" => Self::Function,
            "method" => Self::Method,
            "struct" => Self::Struct,
            "enum" => Self::Enum,
            "trait" => Self::Trait,
            "impl" => Self::Impl,
            "const" => Self::Const,
            "static" => Self::Static,
            "type" => Self::Type,
            "mod" => Self::Mod,
            "other" => Self::Other,
            _ => return None,
        };
        Some(kind)
    }

    /// Whether a chunk carrying this symbol kind should be treated as a primary
    /// code-search target (and receive the symbol boost). Declaration-only kinds
    /// (`Mod`) and the catch-all (`Other`) are deliberately excluded. Exhaustive
    /// so adding a variant forces this decision rather than defaulting silently.
    pub const fn is_source_symbol(self) -> bool {
        match self {
            Self::Function
            | Self::Method
            | Self::Struct
            | Self::Enum
            | Self::Trait
            | Self::Impl
            | Self::Const
            | Self::Static
            | Self::Type => true,
            Self::Mod | Self::Other => false,
        }
    }
}

/// A code symbol a chunk belongs to. A symbol always has a `kind`; the `name`
/// is optional (anonymous `impl` blocks, or a merged tiny-declaration group
/// whose individual names were dropped). Pairing them in one type makes the
/// "a name always has a kind" invariant unrepresentable to violate — there is
/// no way to express a name without a kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub kind: SymbolKind,
    pub name: Option<String>,
}

/// Owned code chunk metadata. Never store tree-sitter nodes here; they borrow
/// the parse tree and cannot safely cross this pure chunking boundary.
///
/// `text` is the rendered chunk to embed. Post-processing may prepend leading
/// comments or synthesized declaration headers, so byte spans identify the
/// original source region used to produce the chunk rather than promising
/// `text == source[byte_start..byte_end]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeChunk {
    pub text: String,
    pub byte_start: usize,
    pub byte_end: usize,
    pub start_line: u32,
    pub end_line: u32,
    pub declaration_start_line: u32,
    pub declaration_end_line: u32,
    pub symbol: Option<Symbol>,
    pub source: ChunkSource,
}

impl CodeChunk {
    pub fn symbol_kind(&self) -> Option<SymbolKind> {
        self.symbol.as_ref().map(|s| s.kind)
    }

    pub fn symbol_name(&self) -> Option<&str> {
        self.symbol.as_ref().and_then(|s| s.name.as_deref())
    }

    pub fn symbol_kind_str(&self) -> Option<&'static str> {
        self.symbol_kind().map(SymbolKind::as_str)
    }
}
