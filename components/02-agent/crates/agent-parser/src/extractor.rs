use agent_core::{
    types::{Language, ReferenceKind, Symbol, SymbolKind, SymbolReference},
    Result,
};
use regex::Regex;
use sha2::{Digest, Sha256};
use tree_sitter::{Node, Parser};

pub struct ExtractionResult {
    pub symbols: Vec<Symbol>,
    pub references: Vec<SymbolReference>,
}

pub struct CodeExtractor;

fn floor_char_boundary(content: &str, mut index: usize) -> usize {
    if index > content.len() {
        index = content.len();
    }
    while index > 0 && !content.is_char_boundary(index) {
        index -= 1;
    }
    index
}

/// `{file_path}::{scope}::{name}@{start_byte}`.
/// Scope is enclosing symbol names joined by `::`; an empty scope leaves that slot blank.
pub(crate) fn declaration_id(
    file_path: &str,
    scope: &[String],
    name: &str,
    start_byte: usize,
) -> String {
    format!("{file_path}::{}::{name}@{start_byte}", scope.join("::"))
}

fn scope_with(scope: &[String], name: &str) -> Vec<String> {
    let mut next = Vec::with_capacity(scope.len() + 1);
    next.extend_from_slice(scope);
    next.push(name.to_string());
    next
}

struct SourceSymbolRef<'a> {
    name: Option<&'a str>,
    id: Option<&'a str>,
}

struct ChildEnv {
    scope: Vec<String>,
    current: Option<String>,
    current_id: Option<String>,
}

struct WalkFrame<'a> {
    node: Node<'a>,
    scope: Vec<String>,
    current: Option<String>,
    current_id: Option<String>,
}

fn same_env(scope: &[String], current: Option<&str>, current_id: Option<&str>) -> ChildEnv {
    ChildEnv {
        scope: scope.to_vec(),
        current: current.map(str::to_string),
        current_id: current_id.map(str::to_string),
    }
}

fn enter_symbol(scope: &[String], name: &str, id: String) -> ChildEnv {
    ChildEnv {
        scope: scope_with(scope, name),
        current: Some(name.to_string()),
        current_id: Some(id),
    }
}

fn enter_scope(
    scope: &[String],
    current: Option<&str>,
    current_id: Option<&str>,
    name: &str,
) -> ChildEnv {
    ChildEnv {
        scope: scope_with(scope, name),
        current: current.map(str::to_string),
        current_id: current_id.map(str::to_string),
    }
}

/// Explicit heap stack. Nested expressions must not grow the native call stack.
fn walk_nodes<'a>(
    root: Node<'a>,
    current: Option<&str>,
    mut visit: impl FnMut(Node<'a>, &[String], Option<&str>, Option<&str>) -> ChildEnv,
) {
    let mut stack = vec![WalkFrame {
        node: root,
        scope: Vec::new(),
        current: current.map(str::to_string),
        current_id: None,
    }];
    while let Some(frame) = stack.pop() {
        let child_env = visit(
            frame.node,
            &frame.scope,
            frame.current.as_deref(),
            frame.current_id.as_deref(),
        );
        let mut cursor = frame.node.walk();
        let children: Vec<Node<'a>> = frame.node.children(&mut cursor).collect();
        for child in children.into_iter().rev() {
            stack.push(WalkFrame {
                node: child,
                scope: child_env.scope.clone(),
                current: child_env.current.clone(),
                current_id: child_env.current_id.clone(),
            });
        }
    }
}

fn rust_scope_type_name<'a>(content: &'a str, mut node: Node<'a>) -> Option<&'a str> {
    for _ in 0..8 {
        if matches!(node.kind(), "type_identifier" | "identifier") {
            let text = content.get(node.byte_range()).unwrap_or("").trim();
            return if text.is_empty() { None } else { Some(text) };
        }
        let next = node
            .child_by_field_name("name")
            .or_else(|| node.child_by_field_name("type"));
        match next {
            Some(inner) if inner.id() != node.id() => node = inner,
            _ => break,
        }
    }
    None
}

/// Method and path calls store the identifier only. `SymbolReference` has no receiver field.
fn rust_call_target<'a>(content: &'a str, func_node: Node<'a>) -> &'a str {
    let node = if func_node.kind() == "generic_function" {
        func_node
            .child_by_field_name("function")
            .unwrap_or(func_node)
    } else {
        func_node
    };
    let selected = match node.kind() {
        "field_expression" => node
            .child_by_field_name("field")
            .and_then(|field| content.get(field.byte_range())),
        "scoped_identifier" | "scoped_type_identifier" => node
            .child_by_field_name("name")
            .and_then(|name| content.get(name.byte_range())),
        _ => None,
    };
    if let Some(text) = selected.map(str::trim).filter(|text| !text.is_empty()) {
        return text;
    }
    let raw = content.get(node.byte_range()).unwrap_or("");
    let after_path = raw.rsplit("::").next().unwrap_or(raw);
    after_path.rsplit('.').next().unwrap_or(after_path).trim()
}

impl CodeExtractor {
    pub fn extract(file_path: &str, content: &str, language: Language) -> Result<ExtractionResult> {
        let mut result = match language {
            Language::Rust => Self::extract_rust(file_path, content)?,
            Language::Python => Self::extract_python(file_path, content)?,
            Language::C => Self::extract_c_cpp(file_path, content)?,
            Language::Cpp => Self::extract_grammar(file_path, content, crate::grammar::Kind::Cpp)?,
            Language::Bash => Self::extract_bash(file_path, content)?,
            Language::JavaScript => Self::extract_js_ts(file_path, content)?,
            Language::TypeScript => {
                let kind = if file_path.ends_with(".tsx") {
                    crate::grammar::Kind::Tsx
                } else {
                    crate::grammar::Kind::TypeScript
                };
                Self::extract_grammar(file_path, content, kind)?
            }
            Language::Dart => {
                Self::extract_grammar(file_path, content, crate::grammar::Kind::Dart)?
            }
            Language::Go => Self::extract_grammar(file_path, content, crate::grammar::Kind::Go)?,
            _ => Self::extract_fallback(file_path, content)?,
        };
        Self::retarget_test_references(&mut result.references);
        Ok(result)
    }

    fn extract_grammar(
        file_path: &str,
        content: &str,
        kind: crate::grammar::Kind,
    ) -> Result<ExtractionResult> {
        match crate::grammar::extract(file_path, content, kind) {
            Some(result) => Ok(result),
            // None means the parse itself failed, not that the module declared nothing.
            None => Self::extract_fallback(file_path, content),
        }
    }

    fn symbol_fingerprint(text: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(text.trim().as_bytes());
        hex::encode(hasher.finalize())
    }

    // -------------------------------------------------------------
    // RUST
    // -------------------------------------------------------------
    fn extract_rust(file_path: &str, content: &str) -> Result<ExtractionResult> {
        let mut parser = Parser::new();
        let _ = parser.set_language(&tree_sitter_rust::LANGUAGE.into());
        let tree = match parser.parse(content, None) {
            Some(t) => t,
            None => return Self::extract_fallback(file_path, content),
        };

        let mut symbols = Vec::new();
        let mut references = Vec::new();
        let root = tree.root_node();

        Self::walk_rust_node(
            file_path,
            content,
            root,
            &mut symbols,
            &mut references,
            None,
        );

        Ok(ExtractionResult {
            symbols,
            references,
        })
    }

    fn walk_rust_node(
        file_path: &str,
        content: &str,
        node: Node,
        symbols: &mut Vec<Symbol>,
        references: &mut Vec<SymbolReference>,
        current_symbol: Option<&str>,
    ) {
        walk_nodes(
            node,
            current_symbol,
            |node, scope, current_symbol, current_symbol_id| {
                Self::visit_rust_node(
                    file_path,
                    content,
                    node,
                    scope,
                    SourceSymbolRef {
                        name: current_symbol,
                        id: current_symbol_id,
                    },
                    symbols,
                    references,
                )
            },
        );
    }

    fn visit_rust_node<'a>(
        file_path: &str,
        content: &'a str,
        node: Node<'a>,
        scope: &[String],
        current: SourceSymbolRef<'_>,
        symbols: &mut Vec<Symbol>,
        references: &mut Vec<SymbolReference>,
    ) -> ChildEnv {
        let current_symbol = current.name;
        let current_symbol_id = current.id;
        let kind = node.kind();
        let mut child_env = same_env(scope, current_symbol, current_symbol_id);

        match kind {
            "function_item" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = &content[name_node.byte_range()];
                    let start_line = node.start_position().row + 1;
                    let end_line = node.end_position().row + 1;
                    let full_text = &content[node.byte_range()];
                    let sig = full_text.lines().next().unwrap_or(name).to_string();

                    // Check for #[test] attribute or test_ prefix without heap allocations
                    let mut has_test_attr = false;
                    let mut prev_sibling = node.prev_sibling();
                    while let Some(sibling) = prev_sibling {
                        if sibling.kind() == "attribute_item" {
                            let attr_text = &content[sibling.byte_range()];
                            if attr_text.contains("test") {
                                has_test_attr = true;
                                break;
                            }
                            prev_sibling = sibling.prev_sibling();
                        } else {
                            break;
                        }
                    }
                    let pre_start =
                        floor_char_boundary(content, node.start_byte().saturating_sub(256));
                    let pre_slice = &content[pre_start..node.start_byte()];
                    let is_test = name.starts_with("test_")
                        || full_text.contains("#[test]")
                        || has_test_attr
                        || pre_slice.contains("#[test]");

                    let sym = Symbol {
                        id: declaration_id(file_path, scope, name, node.start_byte()),
                        name: name.to_string(),
                        qualified_name: format!("{}::{}", file_path, name),
                        kind: SymbolKind::Function,
                        file_path: file_path.to_string(),
                        start_line,
                        end_line,
                        signature: Some(sig),
                        doc_comment: None,
                        fingerprint: Self::symbol_fingerprint(full_text),
                    };
                    symbols.push(sym);
                    child_env = enter_symbol(
                        scope,
                        name,
                        declaration_id(file_path, scope, name, node.start_byte()),
                    );

                    if is_test {
                        // Mark test reference
                        references.push(SymbolReference {
                            source_file: file_path.to_string(),
                            source_symbol_name: Some(name.to_string()),
                            source_symbol_id: child_env.current_id.clone(),
                            target_name: name.to_string(),
                            target_symbol_id: None,
                            kind: ReferenceKind::Tests,
                            line_number: start_line,
                        });
                    }
                }
            }
            "struct_item" | "enum_item" | "trait_item" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = &content[name_node.byte_range()];
                    let start_line = node.start_position().row + 1;
                    let end_line = node.end_position().row + 1;
                    let full_text = &content[node.byte_range()];
                    let sym_kind = match kind {
                        "struct_item" => SymbolKind::Struct,
                        "enum_item" => SymbolKind::Enum,
                        _ => SymbolKind::Trait,
                    };

                    symbols.push(Symbol {
                        id: declaration_id(file_path, scope, name, node.start_byte()),
                        name: name.to_string(),
                        qualified_name: format!("{}::{}", file_path, name),
                        kind: sym_kind,
                        file_path: file_path.to_string(),
                        start_line,
                        end_line,
                        signature: full_text.lines().next().map(|s| s.to_string()),
                        doc_comment: None,
                        fingerprint: Self::symbol_fingerprint(full_text),
                    });
                    child_env = enter_symbol(
                        scope,
                        name,
                        declaration_id(file_path, scope, name, node.start_byte()),
                    );
                }
            }
            "mod_item" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = &content[name_node.byte_range()];
                    if !name.is_empty() {
                        child_env = enter_scope(scope, current_symbol, current_symbol_id, name);
                    }
                }
            }
            "impl_item" => {
                if let Some(type_node) = node.child_by_field_name("type") {
                    if let Some(name) = rust_scope_type_name(content, type_node) {
                        child_env = enter_scope(scope, current_symbol, current_symbol_id, name);
                    }
                }
            }
            "call_expression" => {
                if let Some(func_node) = node.child_by_field_name("function") {
                    let clean_target = rust_call_target(content, func_node);
                    let line = node.start_position().row + 1;

                    references.push(SymbolReference {
                        source_file: file_path.to_string(),
                        source_symbol_name: current_symbol.map(|s| s.to_string()),
                        source_symbol_id: current_symbol_id.map(str::to_string),
                        target_name: clean_target.to_string(),
                        target_symbol_id: None,
                        kind: ReferenceKind::Calls,
                        line_number: line,
                    });
                }
            }
            "use_declaration" => {
                let use_text = &content[node.byte_range()];
                let line = node.start_position().row + 1;
                let target = use_text
                    .trim_start_matches("use ")
                    .trim_end_matches(';')
                    .trim();
                references.push(SymbolReference {
                    source_file: file_path.to_string(),
                    source_symbol_name: None,
                    source_symbol_id: None,
                    target_name: target.to_string(),
                    target_symbol_id: None,
                    kind: ReferenceKind::Imports,
                    line_number: line,
                });
            }
            _ => {}
        }

        child_env
    }

    // -------------------------------------------------------------
    // PYTHON
    // -------------------------------------------------------------
    fn extract_python(file_path: &str, content: &str) -> Result<ExtractionResult> {
        let mut parser = Parser::new();
        let _ = parser.set_language(&tree_sitter_python::LANGUAGE.into());
        let tree = match parser.parse(content, None) {
            Some(t) => t,
            None => return Self::extract_fallback(file_path, content),
        };

        let mut symbols = Vec::new();
        let mut references = Vec::new();
        let root = tree.root_node();

        Self::walk_python_node(
            file_path,
            content,
            root,
            &mut symbols,
            &mut references,
            None,
        );

        Ok(ExtractionResult {
            symbols,
            references,
        })
    }

    fn walk_python_node(
        file_path: &str,
        content: &str,
        node: Node,
        symbols: &mut Vec<Symbol>,
        references: &mut Vec<SymbolReference>,
        current_symbol: Option<&str>,
    ) {
        walk_nodes(
            node,
            current_symbol,
            |node, scope, current_symbol, current_symbol_id| {
                Self::visit_python_node(
                    file_path,
                    content,
                    node,
                    scope,
                    SourceSymbolRef {
                        name: current_symbol,
                        id: current_symbol_id,
                    },
                    symbols,
                    references,
                )
            },
        );
    }

    fn visit_python_node<'a>(
        file_path: &str,
        content: &'a str,
        node: Node<'a>,
        scope: &[String],
        current: SourceSymbolRef<'_>,
        symbols: &mut Vec<Symbol>,
        references: &mut Vec<SymbolReference>,
    ) -> ChildEnv {
        let current_symbol = current.name;
        let current_symbol_id = current.id;
        let kind = node.kind();
        let mut child_env = same_env(scope, current_symbol, current_symbol_id);

        match kind {
            "function_definition" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = &content[name_node.byte_range()];
                    let start_line = node.start_position().row + 1;
                    let end_line = node.end_position().row + 1;
                    let full_text = &content[node.byte_range()];
                    let sig = full_text.lines().next().unwrap_or(name).to_string();

                    symbols.push(Symbol {
                        id: declaration_id(file_path, scope, name, node.start_byte()),
                        name: name.to_string(),
                        qualified_name: format!("{}::{}", file_path, name),
                        kind: SymbolKind::Function,
                        file_path: file_path.to_string(),
                        start_line,
                        end_line,
                        signature: Some(sig),
                        doc_comment: None,
                        fingerprint: Self::symbol_fingerprint(full_text),
                    });
                    child_env = enter_symbol(
                        scope,
                        name,
                        declaration_id(file_path, scope, name, node.start_byte()),
                    );

                    if name.starts_with("test_") {
                        references.push(SymbolReference {
                            source_file: file_path.to_string(),
                            source_symbol_name: Some(name.to_string()),
                            source_symbol_id: child_env.current_id.clone(),
                            target_name: name.to_string(),
                            target_symbol_id: None,
                            kind: ReferenceKind::Tests,
                            line_number: start_line,
                        });
                    }
                }
            }
            "class_definition" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = &content[name_node.byte_range()];
                    let start_line = node.start_position().row + 1;
                    let end_line = node.end_position().row + 1;
                    let full_text = &content[node.byte_range()];

                    symbols.push(Symbol {
                        id: declaration_id(file_path, scope, name, node.start_byte()),
                        name: name.to_string(),
                        qualified_name: format!("{}::{}", file_path, name),
                        kind: SymbolKind::Class,
                        file_path: file_path.to_string(),
                        start_line,
                        end_line,
                        signature: full_text.lines().next().map(|s| s.to_string()),
                        doc_comment: None,
                        fingerprint: Self::symbol_fingerprint(full_text),
                    });
                    child_env = enter_symbol(
                        scope,
                        name,
                        declaration_id(file_path, scope, name, node.start_byte()),
                    );
                }
            }
            "call" => {
                if let Some(func_node) = node.child_by_field_name("function") {
                    let call_name = &content[func_node.byte_range()];
                    let clean = call_name.split('.').next_back().unwrap_or(call_name).trim();
                    let line = node.start_position().row + 1;

                    references.push(SymbolReference {
                        source_file: file_path.to_string(),
                        source_symbol_name: current_symbol.map(|s| s.to_string()),
                        source_symbol_id: current_symbol_id.map(str::to_string),
                        target_name: clean.to_string(),
                        target_symbol_id: None,
                        kind: ReferenceKind::Calls,
                        line_number: line,
                    });
                }
            }
            "import_statement" | "import_from_statement" => {
                let text = &content[node.byte_range()];
                let line = node.start_position().row + 1;
                references.push(SymbolReference {
                    source_file: file_path.to_string(),
                    source_symbol_name: None,
                    source_symbol_id: None,
                    target_name: text.trim().to_string(),
                    target_symbol_id: None,
                    kind: ReferenceKind::Imports,
                    line_number: line,
                });
            }
            _ => {}
        }

        child_env
    }

    // -------------------------------------------------------------
    // C / C++
    // -------------------------------------------------------------
    fn extract_c_cpp(file_path: &str, content: &str) -> Result<ExtractionResult> {
        let mut parser = Parser::new();
        let _ = parser.set_language(&tree_sitter_c::LANGUAGE.into());
        let tree = match parser.parse(content, None) {
            Some(t) => t,
            None => return Self::extract_fallback(file_path, content),
        };

        let mut symbols = Vec::new();
        let mut references = Vec::new();
        let root = tree.root_node();

        Self::walk_c_node(
            file_path,
            content,
            root,
            &mut symbols,
            &mut references,
            None,
        );

        Ok(ExtractionResult {
            symbols,
            references,
        })
    }

    fn walk_c_node(
        file_path: &str,
        content: &str,
        node: Node,
        symbols: &mut Vec<Symbol>,
        references: &mut Vec<SymbolReference>,
        current_symbol: Option<&str>,
    ) {
        walk_nodes(
            node,
            current_symbol,
            |node, scope, current_symbol, current_symbol_id| {
                Self::visit_c_node(
                    file_path,
                    content,
                    node,
                    scope,
                    SourceSymbolRef {
                        name: current_symbol,
                        id: current_symbol_id,
                    },
                    symbols,
                    references,
                )
            },
        );
    }

    fn visit_c_node<'a>(
        file_path: &str,
        content: &'a str,
        node: Node<'a>,
        scope: &[String],
        current: SourceSymbolRef<'_>,
        symbols: &mut Vec<Symbol>,
        references: &mut Vec<SymbolReference>,
    ) -> ChildEnv {
        let current_symbol = current.name;
        let current_symbol_id = current.id;
        let kind = node.kind();
        let mut child_env = same_env(scope, current_symbol, current_symbol_id);

        match kind {
            "function_definition" => {
                if let Some(declarator) = node.child_by_field_name("declarator") {
                    let decl_text = &content[declarator.byte_range()];
                    let func_name = decl_text
                        .split('(')
                        .next()
                        .unwrap_or(decl_text)
                        .trim()
                        .trim_start_matches('*');
                    let start_line = node.start_position().row + 1;
                    let end_line = node.end_position().row + 1;
                    let full_text = &content[node.byte_range()];

                    symbols.push(Symbol {
                        id: declaration_id(file_path, scope, func_name, node.start_byte()),
                        name: func_name.to_string(),
                        qualified_name: format!("{}::{}", file_path, func_name),
                        kind: SymbolKind::Function,
                        file_path: file_path.to_string(),
                        start_line,
                        end_line,
                        signature: full_text.lines().next().map(|s| s.to_string()),
                        doc_comment: None,
                        fingerprint: Self::symbol_fingerprint(full_text),
                    });
                    child_env = enter_symbol(
                        scope,
                        func_name,
                        declaration_id(file_path, scope, func_name, node.start_byte()),
                    );
                }
            }
            "struct_specifier" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = &content[name_node.byte_range()];
                    let start_line = node.start_position().row + 1;
                    let end_line = node.end_position().row + 1;
                    let full_text = &content[node.byte_range()];

                    symbols.push(Symbol {
                        id: declaration_id(file_path, scope, name, node.start_byte()),
                        name: name.to_string(),
                        qualified_name: format!("{}::{}", file_path, name),
                        kind: SymbolKind::Struct,
                        file_path: file_path.to_string(),
                        start_line,
                        end_line,
                        signature: full_text.lines().next().map(|s| s.to_string()),
                        doc_comment: None,
                        fingerprint: Self::symbol_fingerprint(full_text),
                    });
                    child_env = enter_scope(scope, current_symbol, current_symbol_id, name);
                }
            }
            "call_expression" => {
                if let Some(func_node) = node.child_by_field_name("function") {
                    let target = &content[func_node.byte_range()];
                    let line = node.start_position().row + 1;
                    references.push(SymbolReference {
                        source_file: file_path.to_string(),
                        source_symbol_name: current_symbol.map(|s| s.to_string()),
                        source_symbol_id: current_symbol_id.map(str::to_string),
                        target_name: target.to_string(),
                        target_symbol_id: None,
                        kind: ReferenceKind::Calls,
                        line_number: line,
                    });
                }
            }
            "preproc_include" => {
                let text = &content[node.byte_range()];
                let line = node.start_position().row + 1;
                references.push(SymbolReference {
                    source_file: file_path.to_string(),
                    source_symbol_name: None,
                    source_symbol_id: None,
                    target_name: text.trim().to_string(),
                    target_symbol_id: None,
                    kind: ReferenceKind::Imports,
                    line_number: line,
                });
            }
            _ => {}
        }

        child_env
    }

    // -------------------------------------------------------------
    // BASH
    // -------------------------------------------------------------
    fn extract_bash(file_path: &str, content: &str) -> Result<ExtractionResult> {
        let mut parser = Parser::new();
        let _ = parser.set_language(&tree_sitter_bash::LANGUAGE.into());
        let tree = match parser.parse(content, None) {
            Some(t) => t,
            None => return Self::extract_fallback(file_path, content),
        };

        let mut symbols = Vec::new();
        let mut references = Vec::new();
        let root = tree.root_node();

        Self::walk_bash_node(
            file_path,
            content,
            root,
            &mut symbols,
            &mut references,
            None,
        );

        Ok(ExtractionResult {
            symbols,
            references,
        })
    }

    fn walk_bash_node(
        file_path: &str,
        content: &str,
        node: Node,
        symbols: &mut Vec<Symbol>,
        references: &mut Vec<SymbolReference>,
        current_symbol: Option<&str>,
    ) {
        walk_nodes(
            node,
            current_symbol,
            |node, scope, current_symbol, current_symbol_id| {
                Self::visit_bash_node(
                    file_path,
                    content,
                    node,
                    scope,
                    SourceSymbolRef {
                        name: current_symbol,
                        id: current_symbol_id,
                    },
                    symbols,
                    references,
                )
            },
        );
    }

    fn visit_bash_node<'a>(
        file_path: &str,
        content: &'a str,
        node: Node<'a>,
        scope: &[String],
        current: SourceSymbolRef<'_>,
        symbols: &mut Vec<Symbol>,
        references: &mut Vec<SymbolReference>,
    ) -> ChildEnv {
        let current_symbol = current.name;
        let current_symbol_id = current.id;
        let kind = node.kind();
        let mut child_env = same_env(scope, current_symbol, current_symbol_id);

        match kind {
            "function_definition" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = &content[name_node.byte_range()];
                    let start_line = node.start_position().row + 1;
                    let end_line = node.end_position().row + 1;
                    let full_text = &content[node.byte_range()];

                    symbols.push(Symbol {
                        id: declaration_id(file_path, scope, name, node.start_byte()),
                        name: name.to_string(),
                        qualified_name: format!("{}::{}", file_path, name),
                        kind: SymbolKind::Function,
                        file_path: file_path.to_string(),
                        start_line,
                        end_line,
                        signature: Some(format!("function {}()", name)),
                        doc_comment: None,
                        fingerprint: Self::symbol_fingerprint(full_text),
                    });
                    child_env = enter_symbol(
                        scope,
                        name,
                        declaration_id(file_path, scope, name, node.start_byte()),
                    );
                }
            }
            "variable_assignment" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = &content[name_node.byte_range()];
                    let start_line = node.start_position().row + 1;
                    let end_line = node.end_position().row + 1;
                    let full_text = &content[node.byte_range()];

                    symbols.push(Symbol {
                        id: declaration_id(file_path, scope, name, node.start_byte()),
                        name: name.to_string(),
                        qualified_name: format!("{}::{}", file_path, name),
                        kind: SymbolKind::Variable,
                        file_path: file_path.to_string(),
                        start_line,
                        end_line,
                        signature: Some(full_text.lines().next().unwrap_or(name).to_string()),
                        doc_comment: None,
                        fingerprint: Self::symbol_fingerprint(full_text),
                    });
                    child_env = enter_scope(scope, current_symbol, current_symbol_id, name);
                }
            }
            "command" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let cmd_name = &content[name_node.byte_range()];
                    let line = node.start_position().row + 1;

                    references.push(SymbolReference {
                        source_file: file_path.to_string(),
                        source_symbol_name: current_symbol.map(|s| s.to_string()),
                        source_symbol_id: current_symbol_id.map(str::to_string),
                        target_name: cmd_name.to_string(),
                        target_symbol_id: None,
                        kind: ReferenceKind::Calls,
                        line_number: line,
                    });
                }
            }
            _ => {}
        }

        child_env
    }

    // -------------------------------------------------------------
    // JAVASCRIPT / TYPESCRIPT
    // -------------------------------------------------------------
    fn extract_js_ts(file_path: &str, content: &str) -> Result<ExtractionResult> {
        let mut parser = Parser::new();
        let _ = parser.set_language(&tree_sitter_javascript::LANGUAGE.into());
        let tree = match parser.parse(content, None) {
            Some(t) => t,
            None => return Self::extract_fallback(file_path, content),
        };

        let mut symbols = Vec::new();
        let mut references = Vec::new();
        let root = tree.root_node();

        Self::walk_js_node(
            file_path,
            content,
            root,
            &mut symbols,
            &mut references,
            None,
        );

        Ok(ExtractionResult {
            symbols,
            references,
        })
    }

    fn walk_js_node(
        file_path: &str,
        content: &str,
        node: Node,
        symbols: &mut Vec<Symbol>,
        references: &mut Vec<SymbolReference>,
        current_symbol: Option<&str>,
    ) {
        walk_nodes(
            node,
            current_symbol,
            |node, scope, current_symbol, current_symbol_id| {
                Self::visit_js_node(
                    file_path,
                    content,
                    node,
                    scope,
                    SourceSymbolRef {
                        name: current_symbol,
                        id: current_symbol_id,
                    },
                    symbols,
                    references,
                )
            },
        );
    }

    fn visit_js_node<'a>(
        file_path: &str,
        content: &'a str,
        node: Node<'a>,
        scope: &[String],
        current: SourceSymbolRef<'_>,
        symbols: &mut Vec<Symbol>,
        references: &mut Vec<SymbolReference>,
    ) -> ChildEnv {
        let current_symbol = current.name;
        let current_symbol_id = current.id;
        let kind = node.kind();
        let mut child_env = same_env(scope, current_symbol, current_symbol_id);

        match kind {
            "function_declaration" | "method_definition" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = &content[name_node.byte_range()];
                    let start_line = node.start_position().row + 1;
                    let end_line = node.end_position().row + 1;
                    let full_text = &content[node.byte_range()];

                    symbols.push(Symbol {
                        id: declaration_id(file_path, scope, name, node.start_byte()),
                        name: name.to_string(),
                        qualified_name: format!("{}::{}", file_path, name),
                        kind: SymbolKind::Function,
                        file_path: file_path.to_string(),
                        start_line,
                        end_line,
                        signature: full_text.lines().next().map(|s| s.to_string()),
                        doc_comment: None,
                        fingerprint: Self::symbol_fingerprint(full_text),
                    });
                    child_env = enter_symbol(
                        scope,
                        name,
                        declaration_id(file_path, scope, name, node.start_byte()),
                    );
                }
            }
            "class_declaration" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = &content[name_node.byte_range()];
                    let start_line = node.start_position().row + 1;
                    let end_line = node.end_position().row + 1;
                    let full_text = &content[node.byte_range()];

                    symbols.push(Symbol {
                        id: declaration_id(file_path, scope, name, node.start_byte()),
                        name: name.to_string(),
                        qualified_name: format!("{}::{}", file_path, name),
                        kind: SymbolKind::Class,
                        file_path: file_path.to_string(),
                        start_line,
                        end_line,
                        signature: full_text.lines().next().map(|s| s.to_string()),
                        doc_comment: None,
                        fingerprint: Self::symbol_fingerprint(full_text),
                    });
                    child_env = enter_symbol(
                        scope,
                        name,
                        declaration_id(file_path, scope, name, node.start_byte()),
                    );
                }
            }
            "call_expression" => {
                if let Some(func_node) = node.child_by_field_name("function") {
                    let target = &content[func_node.byte_range()];
                    let clean = target.split('.').next_back().unwrap_or(target).trim();
                    let line = node.start_position().row + 1;

                    let is_test_runner = clean == "test" || clean == "it" || clean == "describe";

                    references.push(SymbolReference {
                        source_file: file_path.to_string(),
                        source_symbol_name: current_symbol.map(|s| s.to_string()),
                        source_symbol_id: current_symbol_id.map(str::to_string),
                        target_name: clean.to_string(),
                        target_symbol_id: None,
                        kind: if is_test_runner {
                            ReferenceKind::Tests
                        } else {
                            ReferenceKind::Calls
                        },
                        line_number: line,
                    });
                }
            }
            "import_statement" => {
                let text = &content[node.byte_range()];
                let line = node.start_position().row + 1;
                references.push(SymbolReference {
                    source_file: file_path.to_string(),
                    source_symbol_name: None,
                    source_symbol_id: None,
                    target_name: text.trim().to_string(),
                    target_symbol_id: None,
                    kind: ReferenceKind::Imports,
                    line_number: line,
                });
            }
            _ => {}
        }

        child_env
    }

    fn extract_fallback(file_path: &str, content: &str) -> Result<ExtractionResult> {
        let mut symbols = Vec::new();
        let references = Vec::new();

        let func_re = Regex::new(r"^(?:pub\s+)?(?:async\s+)?def\s+([A-Za-z0-9_]+)|^fn\s+([A-Za-z0-9_]+)|function\s+([A-Za-z0-9_]+)").unwrap();

        let mut byte = 0usize;
        for (idx, line) in content.split('\n').enumerate() {
            let line_num = idx + 1;
            if let Some(caps) = func_re.captures(line.trim()) {
                let name = caps.get(1).or_else(|| caps.get(2)).or_else(|| caps.get(3));
                if let Some(m) = name {
                    let sym_name = m.as_str();
                    let leading = line.len() - line.trim_start().len();
                    symbols.push(Symbol {
                        id: declaration_id(file_path, &[], sym_name, byte + leading),
                        name: sym_name.to_string(),
                        qualified_name: format!("{}::{}", file_path, sym_name),
                        kind: SymbolKind::Function,
                        file_path: file_path.to_string(),
                        start_line: line_num,
                        end_line: line_num,
                        signature: Some(line.trim().to_string()),
                        doc_comment: None,
                        fingerprint: Self::symbol_fingerprint(line),
                    });
                }
            }
            byte += line.len() + 1;
        }

        Ok(ExtractionResult {
            symbols,
            references,
        })
    }

    /// Point test references at the code under test instead of the test function itself.
    fn retarget_test_references(references: &mut [SymbolReference]) {
        let calls: Vec<SymbolReference> = references
            .iter()
            .filter(|r| {
                r.kind == ReferenceKind::Calls
                    && r.source_symbol_name
                        .as_deref()
                        .is_some_and(|s| s.starts_with("test_"))
                    && !is_test_helper(&r.target_name)
            })
            .cloned()
            .collect();

        for reference in references.iter_mut() {
            if reference.kind != ReferenceKind::Tests {
                continue;
            }
            let Some(source) = reference.source_symbol_name.clone() else {
                continue;
            };
            if reference.target_name != source {
                continue;
            }
            let own_calls: Vec<&SymbolReference> = calls
                .iter()
                .filter(|call| {
                    call.source_file == reference.source_file
                        && call.source_symbol_name.as_deref() == Some(source.as_str())
                        && call.source_symbol_id == reference.source_symbol_id
                })
                .collect();
            if let Some(target) = preferred_test_target(&source, &own_calls) {
                if let Some(call) = own_calls.iter().find(|call| call.target_name == target) {
                    reference.line_number = call.line_number;
                }
                reference.target_name = target;
            }
        }
    }
}

fn preferred_test_target(test_name: &str, calls: &[&SymbolReference]) -> Option<String> {
    let stripped = test_name
        .strip_prefix("test_")
        .filter(|name| !name.is_empty())
        .unwrap_or(test_name);
    let targets: Vec<&str> = calls
        .iter()
        .map(|call| call.target_name.as_str())
        .filter(|target| !is_test_helper(target))
        .collect();

    if let Some(exact) = targets.iter().copied().find(|target| *target == stripped) {
        return Some(exact.to_string());
    }
    if let Some(related) = targets.iter().copied().find(|target| {
        target.starts_with(stripped) || (stripped.starts_with(*target) && target.len() >= 4)
    }) {
        return Some(related.to_string());
    }
    if !stripped.is_empty() && stripped != test_name {
        return Some(stripped.to_string());
    }
    targets.first().map(|target| (*target).to_string())
}

fn is_test_helper(name: &str) -> bool {
    matches!(
        name,
        "assert"
            | "assert_eq"
            | "assert_ne"
            | "assert_matches"
            | "debug_assert"
            | "debug_assert_eq"
            | "unwrap"
            | "expect"
            | "panic"
            | "todo"
            | "unimplemented"
            | "dbg"
            | "println"
            | "print"
            | "eprintln"
            | "format"
            | "write"
            | "writeln"
            | "vec"
            | "Some"
            | "None"
            | "Ok"
            | "Err"
            | "clone"
            | "to_string"
            | "to_owned"
            | "new"
            | "default"
            | "String"
            | "matches"
            | "pytest"
            | "raises"
    ) || name.starts_with("assert")
}
