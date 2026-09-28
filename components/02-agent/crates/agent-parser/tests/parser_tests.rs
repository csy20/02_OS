use agent_core::types::{Language, ReferenceKind, SymbolKind};
use agent_parser::CodeExtractor;

#[test]
fn test_rust_parsing() {
    let code = r#"
        pub struct AuthService;

        impl AuthService {
            pub fn rotate_refresh_token(user: &str) -> bool {
                verify_session(user);
                true
            }
        }

        #[test]
        fn test_rotate() {
            rotate_refresh_token("alice");
        }
    "#;

    let res = CodeExtractor::extract("src/auth.rs", code, Language::Rust).unwrap();
    assert!(res
        .symbols
        .iter()
        .any(|s| s.name == "AuthService" && s.kind == SymbolKind::Struct));
    assert!(res
        .symbols
        .iter()
        .any(|s| s.name == "rotate_refresh_token" && s.kind == SymbolKind::Function));
    assert!(res.symbols.iter().any(|s| s.name == "test_rotate"));

    // Check calls and tests
    assert!(res
        .references
        .iter()
        .any(|r| r.target_name == "verify_session" && r.kind == ReferenceKind::Calls));
    assert!(res.references.iter().any(|r| {
        r.kind == ReferenceKind::Tests
            && r.source_symbol_name.as_deref() == Some("test_rotate")
            && r.target_name == "rotate_refresh_token"
    }));
    assert!(res
        .references
        .iter()
        .all(|r| !(r.kind == ReferenceKind::Tests && r.target_name == "test_rotate")));
}

#[test]
fn test_python_parsing() {
    let code = r#"
import os

class TokenManager:
    def refresh(self, token):
        validate_signature(token)
        return True

def test_refresh_token():
    tm = TokenManager()
    tm.refresh("abc")
    "#;

    let res = CodeExtractor::extract("token_mgr.py", code, Language::Python).unwrap();
    assert!(res
        .symbols
        .iter()
        .any(|s| s.name == "TokenManager" && s.kind == SymbolKind::Class));
    assert!(res.symbols.iter().any(|s| s.name == "refresh"));
    assert!(res.symbols.iter().any(|s| s.name == "test_refresh_token"));
    assert!(res
        .references
        .iter()
        .any(|r| r.target_name == "validate_signature"));
}

#[test]
fn test_bash_parsing() {
    let script = r#"
#!/usr/bin/env bash

function build_iso() {
    echo "Building"
    mkarchiso -v /profile
}
    "#;

    let res = CodeExtractor::extract("build.sh", script, Language::Bash).unwrap();
    assert!(res
        .symbols
        .iter()
        .any(|s| s.name == "build_iso" && s.kind == SymbolKind::Function));
    assert!(res
        .references
        .iter()
        .any(|r| r.target_name == "mkarchiso" && r.kind == ReferenceKind::Calls));
}
