use proc_macro2::{Delimiter, TokenStream, TokenTree};

#[derive(Debug)]
pub(super) enum TokenFinding {
    Path(Vec<String>),
    Method(String),
    Ident(String),
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
                if path.len() == 1 {
                    findings.push(TokenFinding::Ident(path[0].clone()));
                }
                index = cursor;
            }
            TokenTree::Punct(punct) if punct.as_char() == '.' => {
                if let (Some(TokenTree::Ident(method)), Some(TokenTree::Group(arguments))) =
                    (trees.get(index + 1), trees.get(index + 2))
                    && arguments.delimiter() == Delimiter::Parenthesis
                {
                    findings.push(TokenFinding::Method(method.to_string()));
                    scan_stream(&arguments.stream(), findings);
                    index += 3;
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

fn is_punct(tree: &TokenTree, expected: char) -> bool {
    matches!(tree, TokenTree::Punct(punct) if punct.as_char() == expected)
}
