use crate::db::IndexDatabase;
use agent_core::{AgentError, EdgeKind, GraphEdge, ReferenceKind, Result, Symbol, SymbolReference};
use rusqlite::params;
use serde_json::json;
use std::collections::HashMap;

pub enum GraphExportFormat {
    Json,
    Dot,
    Mermaid,
}

impl GraphExportFormat {
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_lowercase().as_str() {
            "json" => Ok(Self::Json),
            "dot" => Ok(Self::Dot),
            "mermaid" => Ok(Self::Mermaid),
            other => Err(AgentError::Config(format!(
                "unknown graph format '{other}' (expected json, dot, mermaid)"
            ))),
        }
    }
}

#[derive(Debug, Clone)]
pub struct GraphNode {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub datapoint_id: Option<String>,
}

pub fn file_node_id(dataset_id: &str, path: &str) -> String {
    format!("file:{dataset_id}:{path}")
}

pub fn symbol_node_id(dataset_id: &str, symbol_id: &str) -> String {
    format!("symbol:{dataset_id}:{symbol_id}")
}

pub fn name_node_id(dataset_id: &str, name: &str) -> String {
    format!("name:{dataset_id}:{name}")
}

pub fn commit_node_id(commit_id: &str) -> String {
    format!("commit:{commit_id}")
}

pub fn session_node_id(session_id: &str) -> String {
    format!("session:{session_id}")
}

impl IndexDatabase {
    pub fn upsert_node(
        &mut self,
        repo_id: &str,
        dataset_id: &str,
        id: &str,
        kind: &str,
        label: &str,
        datapoint_id: Option<&str>,
    ) -> Result<()> {
        self.conn
            .execute(
                r#"
                INSERT INTO graph_nodes (id, repo_id, dataset_id, kind, label, datapoint_id)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                ON CONFLICT(id) DO UPDATE SET
                    kind = excluded.kind,
                    label = excluded.label,
                    datapoint_id = excluded.datapoint_id
                "#,
                params![id, repo_id, dataset_id, kind, label, datapoint_id],
            )
            .map_err(|e| AgentError::Database(format!("Node upsert failed: {}", e)))?;
        Ok(())
    }

    pub fn upsert_edge(
        &mut self,
        repo_id: &str,
        dataset_id: &str,
        src_id: &str,
        dst_id: &str,
        kind: EdgeKind,
    ) -> Result<bool> {
        let changed = self
            .conn
            .execute(
                r#"
                INSERT OR IGNORE INTO graph_edges
                    (repo_id, dataset_id, src_id, dst_id, kind, payload)
                VALUES (?1, ?2, ?3, ?4, ?5, NULL)
                "#,
                params![repo_id, dataset_id, src_id, dst_id, kind.as_str()],
            )
            .map_err(|e| AgentError::Database(format!("Edge insert failed: {}", e)))?;
        Ok(changed > 0)
    }

    pub fn clear_projected_edges(&mut self, dataset_id: &str) -> Result<()> {
        self.conn
            .execute(
                r#"
                DELETE FROM graph_edges
                WHERE dataset_id = ?1
                  AND kind IN ('calls', 'imports', 'tests', 'owns')
                "#,
                params![dataset_id],
            )
            .map_err(|e| AgentError::Database(format!("Edge clear failed: {}", e)))?;
        Ok(())
    }

    pub fn delete_chunks(&mut self, dataset_id: &str, path: &str) -> Result<()> {
        let mut stmt = self
            .conn
            .prepare(
                r#"
                SELECT id FROM datapoints
                WHERE dataset_id = ?1 AND kind = 'chunk'
                  AND (path = ?2 OR substr(path, 1, length(?2) + 1) = ?2 || '#')
                "#,
            )
            .map_err(|e| AgentError::Database(format!("Prepare chunk delete failed: {}", e)))?;
        let ids: Vec<String> = stmt
            .query_map(params![dataset_id, path], |row| row.get(0))
            .map_err(|e| AgentError::Database(format!("Chunk query failed: {}", e)))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| AgentError::Database(format!("Chunk row failed: {}", e)))?;
        drop(stmt);

        for id in ids {
            self.conn
                .execute("DELETE FROM graph_nodes WHERE id = ?1", params![id])
                .map_err(|e| AgentError::Database(format!("Chunk node delete failed: {}", e)))?;
            self.conn
                .execute("DELETE FROM datapoints WHERE id = ?1", params![id])
                .map_err(|e| AgentError::Database(format!("Chunk delete failed: {}", e)))?;
        }
        Ok(())
    }

    pub fn list_symbols(&self, repo_id: &str) -> Result<Vec<Symbol>> {
        let mut stmt = self
            .conn
            .prepare(
                r#"
                SELECT id, name, qualified_name, kind, file_path, start_line, end_line,
                       signature, doc_comment, fingerprint
                FROM symbols WHERE repo_id = ?1 ORDER BY file_path, start_line
                "#,
            )
            .map_err(|e| AgentError::Database(format!("Prepare symbols failed: {}", e)))?;
        let rows = stmt
            .query_map(params![repo_id], |row| {
                let kind: String = row.get(3)?;
                Ok(Symbol {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    qualified_name: row.get(2)?,
                    kind: agent_core::SymbolKind::from_str_kind(&kind),
                    file_path: row.get(4)?,
                    start_line: row.get::<_, i64>(5)? as usize,
                    end_line: row.get::<_, i64>(6)? as usize,
                    signature: row.get(7)?,
                    doc_comment: row.get(8)?,
                    fingerprint: row.get(9)?,
                })
            })
            .map_err(|e| AgentError::Database(format!("Symbol query failed: {}", e)))?;
        let mut symbols = Vec::new();
        for symbol in rows {
            symbols.push(
                symbol.map_err(|e| AgentError::Database(format!("Symbol row failed: {}", e)))?,
            );
        }
        Ok(symbols)
    }

    pub fn list_references(&self, repo_id: &str) -> Result<Vec<SymbolReference>> {
        let mut stmt = self
            .conn
            .prepare(
                r#"
                SELECT source_file, source_symbol_name, target_name, target_symbol_id, kind, line_number
                FROM symbol_references WHERE repo_id = ?1 ORDER BY source_file, line_number
                "#,
            )
            .map_err(|e| AgentError::Database(format!("Prepare references failed: {}", e)))?;
        let rows = stmt
            .query_map(params![repo_id], |row| {
                let kind: String = row.get(4)?;
                Ok(SymbolReference {
                    source_file: row.get(0)?,
                    source_symbol_name: row.get(1)?,
                    target_name: row.get(2)?,
                    target_symbol_id: row.get(3)?,
                    kind: ReferenceKind::from_str_kind(&kind),
                    line_number: row.get::<_, i64>(5)? as usize,
                })
            })
            .map_err(|e| AgentError::Database(format!("Reference query failed: {}", e)))?;
        let mut references = Vec::new();
        for reference in rows {
            references.push(
                reference
                    .map_err(|e| AgentError::Database(format!("Reference row failed: {}", e)))?,
            );
        }
        Ok(references)
    }

    pub fn list_nodes(&self, dataset_id: &str) -> Result<Vec<GraphNode>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, kind, label, datapoint_id FROM graph_nodes WHERE dataset_id = ?1 ORDER BY id",
            )
            .map_err(|e| AgentError::Database(format!("Prepare nodes failed: {}", e)))?;
        let rows = stmt
            .query_map(params![dataset_id], |row| {
                Ok(GraphNode {
                    id: row.get(0)?,
                    kind: row.get(1)?,
                    label: row.get(2)?,
                    datapoint_id: row.get(3)?,
                })
            })
            .map_err(|e| AgentError::Database(format!("Node query failed: {}", e)))?;
        let mut nodes = Vec::new();
        for node in rows {
            nodes.push(node.map_err(|e| AgentError::Database(format!("Node row failed: {}", e)))?);
        }
        Ok(nodes)
    }

    pub fn list_edges(&self, dataset_id: &str) -> Result<Vec<GraphEdge>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT src_id, dst_id, kind FROM graph_edges WHERE dataset_id = ?1 ORDER BY id",
            )
            .map_err(|e| AgentError::Database(format!("Prepare edges failed: {}", e)))?;
        let rows = stmt
            .query_map(params![dataset_id], |row| {
                let kind: String = row.get(2)?;
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, kind))
            })
            .map_err(|e| AgentError::Database(format!("Edge query failed: {}", e)))?;
        let mut edges = Vec::new();
        for row in rows {
            let (src_id, dst_id, kind) =
                row.map_err(|e| AgentError::Database(format!("Edge row failed: {}", e)))?;
            if let Some(kind) = EdgeKind::parse(&kind) {
                edges.push(GraphEdge {
                    src_id,
                    dst_id,
                    kind,
                });
            }
        }
        Ok(edges)
    }

    /// Outgoing walk from `start`, depth-capped. Depth 0 is `start` itself.
    pub fn traverse(
        &self,
        dataset_id: &str,
        start: &str,
        depth: usize,
    ) -> Result<Vec<(String, usize)>> {
        if depth > 8 {
            return Err(AgentError::Config(
                "graph traversal depth cannot exceed 8".into(),
            ));
        }
        let mut stmt = self
            .conn
            .prepare(
                r#"
                WITH RECURSIVE walk(id, depth) AS (
                    SELECT ?1, 0
                    UNION ALL
                    SELECT e.dst_id, walk.depth + 1
                    FROM graph_edges e
                    JOIN walk ON e.src_id = walk.id
                    WHERE e.dataset_id = ?2 AND walk.depth < ?3
                )
                SELECT id, MIN(depth) AS depth
                FROM walk
                GROUP BY id
                ORDER BY depth, id
                "#,
            )
            .map_err(|e| AgentError::Database(format!("Prepare traversal failed: {}", e)))?;
        let rows = stmt
            .query_map(params![start, dataset_id, depth as i64], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
            })
            .map_err(|e| AgentError::Database(format!("Traversal query failed: {}", e)))?;
        let mut found = Vec::new();
        for row in rows {
            found.push(
                row.map_err(|e| AgentError::Database(format!("Traversal row failed: {}", e)))?,
            );
        }
        Ok(found)
    }

    pub fn export_graph(&self, dataset_id: &str, format: GraphExportFormat) -> Result<String> {
        let nodes = self.list_nodes(dataset_id)?;
        let edges = self.list_edges(dataset_id)?;
        Ok(match format {
            GraphExportFormat::Json => serde_json::to_string_pretty(&json!({
                "nodes": nodes.iter().map(|node| json!({
                    "id": node.id,
                    "kind": node.kind,
                    "label": node.label,
                    "datapoint_id": node.datapoint_id,
                })).collect::<Vec<_>>(),
                "edges": edges.iter().map(|edge| json!({
                    "src": edge.src_id,
                    "dst": edge.dst_id,
                    "kind": edge.kind.as_str(),
                })).collect::<Vec<_>>(),
            }))
            .map_err(|e| AgentError::General(format!("Graph JSON failed: {}", e)))?,
            GraphExportFormat::Dot => render_dot(&nodes, &edges),
            GraphExportFormat::Mermaid => render_mermaid(&nodes, &edges),
        })
    }

    pub fn delete_file_nodes_not_in(&mut self, dataset_id: &str, keep: &[String]) -> Result<()> {
        let nodes = self.list_nodes(dataset_id)?;
        for node in nodes {
            if node.kind == "file" && !keep.iter().any(|path| path == &node.label) {
                self.conn
                    .execute("DELETE FROM graph_nodes WHERE id = ?1", params![node.id])
                    .map_err(|e| {
                        AgentError::Database(format!("Stale file node delete failed: {}", e))
                    })?;
            }
        }
        Ok(())
    }
}

fn render_dot(nodes: &[GraphNode], edges: &[GraphEdge]) -> String {
    let mut out = String::from("digraph knowledge {\n");
    for node in nodes {
        out.push_str(&format!(
            "  \"{}\" [label=\"{}\"];\n",
            escape_dot(&node.id),
            escape_dot(&node.label)
        ));
    }
    for edge in edges {
        out.push_str(&format!(
            "  \"{}\" -> \"{}\" [label=\"{}\"];\n",
            escape_dot(&edge.src_id),
            escape_dot(&edge.dst_id),
            edge.kind.as_str()
        ));
    }
    out.push_str("}\n");
    out
}

fn render_mermaid(nodes: &[GraphNode], edges: &[GraphEdge]) -> String {
    let mut ids = HashMap::new();
    for (index, node) in nodes.iter().enumerate() {
        ids.insert(node.id.clone(), format!("n{index}"));
    }
    let mut out = String::from("flowchart LR\n");
    for node in nodes {
        let mermaid_id = &ids[&node.id];
        out.push_str(&format!(
            "  {mermaid_id}[\"{}\"]\n",
            escape_mermaid(&node.label)
        ));
    }
    for edge in edges {
        let Some(src) = ids.get(&edge.src_id) else {
            continue;
        };
        let Some(dst) = ids.get(&edge.dst_id) else {
            continue;
        };
        out.push_str(&format!(
            "  {src} -->|{}| {dst}\n",
            escape_mermaid(edge.kind.as_str())
        ));
    }
    out
}

fn escape_dot(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn escape_mermaid(value: &str) -> String {
    value.replace('"', "'").replace('\n', " ")
}
