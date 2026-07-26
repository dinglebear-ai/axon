use proc_macro2::{Delimiter, TokenStream, TokenTree};

#[derive(Debug)]
pub(super) enum TokenFinding {
    Path(Vec<String>),
    Method {
        receiver: Option<String>,
        method: String,
    },
    Member(String),
    Binding(String),
}

pub(super) fn scan(stream: &TokenStream) -> Vec<TokenFinding> {
    let mut findings = Vec::new();
    scan_stream(stream, &mut findings);
    findings
}

fn scan_stream(stream: &TokenStream, findings: &mut Vec<TokenFinding>) {
    let trees = stream.clone().into_iter().collect::<Vec<_>>();
    let mut index = 0;
    while index < trees.len() {
        match &trees[index] {
            TokenTree::Group(group) => {
                scan_stream(&group.stream(), findings);
                index += 1;
            }
            TokenTree::Ident(ident) => {
                let mut path = vec![ident.to_string()];
                let mut cursor = index + 1;
                while cursor + 2 < trees.len()
                    && is_punct(&trees[cursor], ':')
                    && is_punct(&trees[cursor + 1], ':')
                {
                    let TokenTree::Ident(next) = &trees[cursor + 2] else {
                        break;
                    };
                    path.push(next.to_string());
                    cursor += 3;
                }
                findings.push(TokenFinding::Path(path.clone()));
                if path.len() == 1 && preceding_token_is_let(&trees, index) {
                    findings.push(TokenFinding::Binding(path[0].clone()));
                }
                index = cursor;
            }
            TokenTree::Punct(punct) if punct.as_char() == '.' => {
                if let (Some(TokenTree::Ident(method)), Some(TokenTree::Group(arguments))) =
                    (trees.get(index + 1), trees.get(index + 2))
                    && arguments.delimiter() == Delimiter::Parenthesis
                {
                    findings.push(TokenFinding::Method {
                        receiver: preceding_ident(&trees, index),
                        method: method.to_string(),
                    });
                    scan_stream(&arguments.stream(), findings);
                    index += 3;
                } else if let Some(TokenTree::Ident(member)) = trees.get(index + 1) {
                    findings.push(TokenFinding::Member(member.to_string()));
                    index += 2;
                } else {
                    index += 1;
                }
            }
            // Literals deliberately stay opaque: provider-like strings are
            // data, not Rust paths or member access.
            _ => index += 1,
        }
    }
}

fn preceding_ident(trees: &[TokenTree], dot_index: usize) -> Option<String> {
    dot_index
        .checked_sub(1)
        .and_then(|index| trees.get(index))
        .and_then(|tree| match tree {
            TokenTree::Ident(ident) => Some(ident.to_string()),
            _ => None,
        })
}

fn preceding_token_is_let(trees: &[TokenTree], index: usize) -> bool {
    index
        .checked_sub(1)
        .and_then(|previous| trees.get(previous))
        .is_some_and(|tree| matches!(tree, TokenTree::Ident(ident) if ident == "let"))
}

fn is_punct(tree: &TokenTree, expected: char) -> bool {
    matches!(tree, TokenTree::Punct(punct) if punct.as_char() == expected)
}
