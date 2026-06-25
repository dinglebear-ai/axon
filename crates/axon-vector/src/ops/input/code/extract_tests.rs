use super::*;

#[test]
fn rust_extracts_function_and_method_symbols() {
    let src = r#"
struct Response;

impl Response {
    pub fn parse(&self) {}
}

fn free_fn() {}
"#;

    let symbols = extract_symbols(src, Extractor::Rust);
    assert!(symbols.iter().any(|sym| {
        sym.name.as_deref() == Some("Response::parse") && sym.kind == SymbolKind::Method
    }));
    assert!(
        symbols.iter().any(|sym| {
            sym.name.as_deref() == Some("free_fn") && sym.kind == SymbolKind::Function
        })
    );
}

#[test]
fn go_extracts_function_and_receiver_method_symbols() {
    let src = r#"
package demo

type Response struct {}

func (r *Response) Parse() {}

func Free() {}
"#;

    let symbols = extract_symbols(src, Extractor::Go);
    assert!(
        symbols
            .iter()
            .any(|sym| sym.name.as_deref() == Some("Response.Parse")
                && sym.kind == SymbolKind::Method)
    );
    assert!(
        symbols
            .iter()
            .any(|sym| sym.name.as_deref() == Some("Free") && sym.kind == SymbolKind::Function)
    );
}

#[test]
fn python_extracts_class_function_and_method_symbols() {
    let src = r#"
class ClientSession:
    async def call_tool(self, name: str):
        return name

def helper():
    return 1
"#;

    let symbols = extract_symbols(src, Extractor::Python);
    assert!(symbols.iter().any(|sym| {
        sym.name.as_deref() == Some("ClientSession") && sym.kind == SymbolKind::Struct
    }));
    assert!(symbols.iter().any(|sym| {
        sym.name.as_deref() == Some("ClientSession::call_tool") && sym.kind == SymbolKind::Method
    }));
    assert!(
        symbols
            .iter()
            .any(|sym| sym.name.as_deref() == Some("helper") && sym.kind == SymbolKind::Function)
    );
}

#[test]
fn typescript_extracts_classes_interfaces_functions_and_methods() {
    let src = r#"
export interface Transport {
  start(): Promise<void>;
}

export class Client {
  async connect(url: string): Promise<void> {
    console.log(url);
  }
}

export function createClient(): Client {
  return new Client();
}
"#;

    let symbols = extract_symbols(src, Extractor::TypeScript);
    assert!(
        symbols.iter().any(|sym| {
            sym.name.as_deref() == Some("Transport") && sym.kind == SymbolKind::Type
        })
    );
    assert!(
        symbols
            .iter()
            .any(|sym| sym.name.as_deref() == Some("Client") && sym.kind == SymbolKind::Struct)
    );
    assert!(symbols.iter().any(|sym| {
        sym.name.as_deref() == Some("Client::connect") && sym.kind == SymbolKind::Method
    }));
    assert!(symbols.iter().any(|sym| {
        sym.name.as_deref() == Some("createClient") && sym.kind == SymbolKind::Function
    }));
}

#[test]
fn javascript_extracts_classes_functions_and_methods() {
    let src = r#"
class Server {
  handle(request) {
    return request;
  }
}

export function makeServer() {
  return new Server();
}
"#;

    let symbols = extract_symbols(src, Extractor::JavaScript);
    assert!(
        symbols
            .iter()
            .any(|sym| sym.name.as_deref() == Some("Server") && sym.kind == SymbolKind::Struct)
    );
    assert!(symbols.iter().any(|sym| {
        sym.name.as_deref() == Some("Server::handle") && sym.kind == SymbolKind::Method
    }));
    assert!(
        symbols.iter().any(
            |sym| sym.name.as_deref() == Some("makeServer") && sym.kind == SymbolKind::Function
        )
    );
}

#[test]
fn bash_extracts_function_symbols() {
    let src = r#"
setup_env() {
  export AXON=1
}

function run_server {
  echo running
}
"#;

    let symbols = extract_symbols(src, Extractor::Bash);
    assert!(
        symbols
            .iter()
            .any(|sym| sym.name.as_deref() == Some("setup_env")
                && sym.kind == SymbolKind::Function)
    );
    assert!(
        symbols.iter().any(
            |sym| sym.name.as_deref() == Some("run_server") && sym.kind == SymbolKind::Function
        )
    );
}
