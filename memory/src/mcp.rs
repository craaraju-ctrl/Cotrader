//! # MCP Server — Model Context Protocol for Universal Agent Access
//!
//! Exposes the Memory module as an MCP server so any MCP-compatible agent
//! (Claude Desktop, Cursor, Zed, etc.) can use it as a memory tool.
//!
//! ## Available Tools
//!
//! | Tool | Description |
//! |------|-------------|
//! | `memory_insert` | Store a new memory with content, type, and tier |
//! | `memory_search` | Search memories by text query |
//! | `memory_get` | Retrieve a specific memory by ID |
//! | `memory_delete` | Remove a memory by ID |
//! | `memory_stats` | Get storage statistics |
//! | `memory_health` | Check system health |
//! | `memory_promote` | Promote a record to a different tier |
//! | `memory_add_edge` | Create a relationship between two memories |
//!
//! ## Running as MCP Server
//!
//! ```bash
//! # stdio mode (for Claude Desktop integration)
//! cargo run --bin agentic-memory -- mcp
//!
//! # HTTP mode (for remote access)
//! MEMORY_MCP_PORT=3112 cargo run --bin agentic-memory -- mcp --http
//! ```

use serde::{Deserialize, Serialize};

/// MCP tool definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// MCP tool call request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolCall {
    pub name: String,
    pub arguments: serde_json::Value,
}

/// MCP tool call response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolResult {
    pub content: Vec<McpContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

/// MCP content block.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum McpContent {
    #[serde(rename = "text")]
    Text { text: String },
}

/// Get all available MCP tools for the Memory module.
pub fn get_tools() -> Vec<McpTool> {
    vec![
        McpTool {
            name: "memory_insert".to_string(),
            description: "Store a new memory. Returns the record ID.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "content": {
                        "type": "string",
                        "description": "The content to remember"
                    },
                    "content_type": {
                        "type": "string",
                        "description": "Type of content (e.g., fact, event, note, procedure)",
                        "default": "note"
                    },
                    "tier": {
                        "type": "string",
                        "enum": ["working", "episodic", "semantic", "procedural"],
                        "description": "Memory tier to store in",
                        "default": "episodic"
                    },
                    "importance": {
                        "type": "number",
                        "description": "Importance score 0.0-1.0",
                        "default": 0.5
                    }
                },
                "required": ["content"]
            }),
        },
        McpTool {
            name: "memory_search".to_string(),
            description:
                "Search memories by text query. Returns matching records with relevance scores."
                    .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum results to return",
                        "default": 10
                    },
                    "tier": {
                        "type": "string",
                        "enum": ["working", "episodic", "semantic", "procedural"],
                        "description": "Search within a specific tier (optional)"
                    }
                },
                "required": ["query"]
            }),
        },
        McpTool {
            name: "memory_get".to_string(),
            description: "Retrieve a specific memory by its ID.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "The memory record ID"
                    }
                },
                "required": ["id"]
            }),
        },
        McpTool {
            name: "memory_delete".to_string(),
            description: "Remove a memory by its ID.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "The memory record ID to delete"
                    }
                },
                "required": ["id"]
            }),
        },
        McpTool {
            name: "memory_sleep_cycle".to_string(),
            description: "Run a full sleep cycle: consolidation + tier tuning + stale pruning + procedural distillation + reflexion. The main self-evolution command.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
        McpTool {
            name: "memory_namespace_create".to_string(),
            description: "Create a namespace for isolating memories across agents or users.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Namespace name (must be unique)"
                    },
                    "description": {
                        "type": "string",
                        "description": "Namespace description",
                        "default": ""
                    },
                    "owner": {
                        "type": "string",
                        "description": "Who owns this namespace"
                    },
                    "read_parents": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Namespace IDs this one can read from (inheritance)"
                    },
                    "write_children": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Namespace IDs that can read from this one"
                    }
                },
                "required": ["name", "owner"]
            }),
        },
        McpTool {
            name: "memory_namespace_list".to_string(),
            description: "List all namespaces.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
        McpTool {
            name: "memory_namespace_get".to_string(),
            description: "Get a namespace by ID.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "namespace_id": {
                        "type": "string",
                        "description": "The namespace ID"
                    }
                },
                "required": ["namespace_id"]
            }),
        },
        McpTool {
            name: "memory_namespace_get_by_name".to_string(),
            description: "Get a namespace by its unique name (not ID).".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "The namespace name"
                    }
                },
                "required": ["name"]
            }),
        },
        McpTool {
            name: "memory_namespace_check_access".to_string(),
            description: "Check if a namespace has read access to another namespace via the read_parents inheritance chain.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "reader_namespace": {
                        "type": "string",
                        "description": "The namespace that wants to read"
                    },
                    "target_namespace": {
                        "type": "string",
                        "description": "The namespace being read from"
                    }
                },
                "required": ["reader_namespace", "target_namespace"]
            }),
        },
        McpTool {
            name: "memory_stats".to_string(),
            description: "Get storage statistics (total records, embeddings, etc.).".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
        McpTool {
            name: "memory_health".to_string(),
            description: "Check system health status.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
        McpTool {
            name: "memory_promote".to_string(),
            description: "Promote a memory to a different tier (e.g., episodic → semantic)."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "The memory record ID"
                    },
                    "tier": {
                        "type": "string",
                        "enum": ["working", "episodic", "semantic", "procedural"],
                        "description": "Target tier"
                    }
                },
                "required": ["id", "tier"]
            }),
        },
        McpTool {
            name: "memory_add_edge".to_string(),
            description: "Create a relationship edge between two memories.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "source_id": {
                        "type": "string",
                        "description": "Source memory ID"
                    },
                    "target_id": {
                        "type": "string",
                        "description": "Target memory ID"
                    },
                    "relation_type": {
                        "type": "string",
                        "description": "Relationship type (e.g., related_to, causes, depends_on)"
                    },
                    "weight": {
                        "type": "number",
                        "description": "Relationship strength 0.0-1.0",
                        "default": 1.0
                    }
                },
                "required": ["source_id", "target_id", "relation_type"]
            }),
        },
        // ── Reflection Tools ───────────────────────────────────────────────
        McpTool {
            name: "memory_reflect".to_string(),
            description: "Record a reflection — the agent's inner monologue about its own memory state. Returns the stored reflection.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "topic": {
                        "type": "string",
                        "description": "What the agent is reflecting on (e.g., 'recent memory quality')"
                    },
                    "monologue": {
                        "type": "string",
                        "description": "The agent's inner monologue text"
                    },
                    "conclusion": {
                        "type": "string",
                        "description": "What the agent concluded from this reflection"
                    },
                    "planned_actions": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Actions the agent recommends or plans to take"
                    },
                    "confidence": {
                        "type": "number",
                        "description": "Confidence in the reflection's quality (0.0-1.0)",
                        "default": 0.5
                    },
                    "tags": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Tags for retrieval"
                    }
                },
                "required": ["topic", "monologue", "conclusion"]
            }),
        },
        McpTool {
            name: "memory_assess".to_string(),
            description: "Run a self-assessment of the memory system's health. Returns scores for quality, coherence, staleness, and diversity.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
        McpTool {
            name: "memory_reflexion_loop".to_string(),
            description: "Run a full reflexion loop: self-assess → reflect → plan. The core self-improvement mechanism.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "topic": {
                        "type": "string",
                        "description": "What to reflect on"
                    },
                    "observation": {
                        "type": "string",
                        "description": "What the agent observed about its memory state"
                    }
                },
                "required": ["topic", "observation"]
            }),
        },
        // ── Temporal Fact Tools ────────────────────────────────────────────
        McpTool {
            name: "memory_temporal_store".to_string(),
            description: "Store a temporal fact with versioning and decay tracking. Facts decay over time unless recalled.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "content": {
                        "type": "string",
                        "description": "The fact content"
                    },
                    "content_type": {
                        "type": "string",
                        "description": "Type of fact (e.g., fact, preference, event)",
                        "default": "fact"
                    },
                    "importance": {
                        "type": "number",
                        "description": "Importance score 0.0-1.0",
                        "default": 0.5
                    }
                },
                "required": ["content"]
            }),
        },
        McpTool {
            name: "memory_temporal_recall".to_string(),
            description: "Recall a temporal fact — boosts its decay score to prevent forgetting. Use this when the fact is still relevant.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "fact_id": {
                        "type": "string",
                        "description": "The temporal fact ID to recall"
                    }
                },
                "required": ["fact_id"]
            }),
        },
        McpTool {
            name: "memory_temporal_get".to_string(),
            description: "Get a temporal fact and its current decay state.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "fact_id": {
                        "type": "string",
                        "description": "The temporal fact ID"
                    }
                },
                "required": ["fact_id"]
            }),
        },
        McpTool {
            name: "memory_temporal_search".to_string(),
            description: "Search current (non-superseded) temporal facts by content.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum results",
                        "default": 10
                    }
                },
                "required": ["query"]
            }),
        },
        // ── Context Management Tools ───────────────────────────────────────
        McpTool {
            name: "memory_context_add".to_string(),
            description: "Add or update a context block in the context window. Pinned blocks (like persona) are always included.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "block_id": {
                        "type": "string",
                        "description": "Unique block identifier"
                    },
                    "label": {
                        "type": "string",
                        "description": "Block label (e.g., 'persona', 'user_preferences', 'recent')"
                    },
                    "content": {
                        "type": "string",
                        "description": "The text content of this block"
                    },
                    "pinned": {
                        "type": "boolean",
                        "description": "Whether this block is always included in context",
                        "default": false
                    },
                    "priority": {
                        "type": "integer",
                        "description": "Priority for eviction (lower = evicted first)",
                        "default": 50
                    }
                },
                "required": ["block_id", "label", "content"]
            }),
        },
        McpTool {
            name: "memory_context_render".to_string(),
            description: "Render the current context window — what the LLM sees. Shows pinned and dynamic blocks with token usage.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
        McpTool {
            name: "memory_context_get".to_string(),
            description: "Get a specific context block by ID.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "block_id": {
                        "type": "string",
                        "description": "The block ID"
                    }
                },
                "required": ["block_id"]
            }),
        },
    ]
}

/// Execute an MCP tool call against the Memory API.
pub async fn execute_tool(call: &McpToolCall, api_url: &str) -> McpToolResult {
    let client = reqwest::Client::new();
    let base = api_url.trim_end_matches('/');

    let result = match call.name.as_str() {
        "memory_insert" => {
            let content = call.arguments["content"].as_str().unwrap_or("");
            let content_type = call.arguments["content_type"].as_str().unwrap_or("note");
            let tier = call.arguments["tier"].as_str().unwrap_or("episodic");
            let importance = call.arguments["importance"].as_f64().unwrap_or(0.5);

            let body = serde_json::json!({
                "id": uuid::Uuid::now_v7().to_string(),
                "content": content,
                "content_type": content_type,
                "tier": tier,
                "importance": importance,
            });

            match client
                .post(format!("{}/records", base))
                .json(&body)
                .send()
                .await
            {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        match resp.json::<serde_json::Value>().await {
                            Ok(json) => Ok(format!("Memory stored with ID: {}", json["id"])),
                            Err(e) => Err(format!("Parse error: {}", e)),
                        }
                    } else {
                        Err(format!("HTTP {}: store failed", status))
                    }
                }
                Err(e) => Err(format!("Request failed: {}", e)),
            }
        }

        "memory_search" => {
            let query = call.arguments["query"].as_str().unwrap_or("");
            let limit = call.arguments["limit"].as_u64().unwrap_or(10);
            let tier = call.arguments["tier"].as_str();

            let url = if let Some(t) = tier {
                format!(
                    "{}/search?q={}&tier={}&limit={}",
                    base,
                    urlencoding::encode(query),
                    t,
                    limit
                )
            } else {
                format!(
                    "{}/search?q={}&limit={}",
                    base,
                    urlencoding::encode(query),
                    limit
                )
            };

            match client.get(&url).send().await {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        match resp.json::<Vec<serde_json::Value>>().await {
                            Ok(results) => {
                                if results.is_empty() {
                                    Ok("No matching memories found.".to_string())
                                } else {
                                    let formatted: Vec<String> = results
                                        .iter()
                                        .enumerate()
                                        .map(|(i, r)| {
                                            let id = r["record"]["id"]
                                                .as_str()
                                                .or_else(|| r["id"].as_str())
                                                .unwrap_or("?");
                                            let content = r["record"]["content"]
                                                .as_str()
                                                .or_else(|| r["content"].as_str())
                                                .unwrap_or("");
                                            let score = r["score"].as_f64().unwrap_or(0.0);
                                            format!(
                                                "{}. [{}] (score: {:.2}) {}",
                                                i + 1,
                                                id,
                                                score,
                                                content
                                            )
                                        })
                                        .collect();
                                    Ok(formatted.join("\n"))
                                }
                            }
                            Err(e) => Err(format!("Parse error: {}", e)),
                        }
                    } else {
                        Err(format!("HTTP {}: search failed", status))
                    }
                }
                Err(e) => Err(format!("Request failed: {}", e)),
            }
        }

        "memory_get" => {
            let id = call.arguments["id"].as_str().unwrap_or("");
            match client.get(format!("{}/records/{}", base, id)).send().await {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        match resp.json::<serde_json::Value>().await {
                            Ok(json) => Ok(serde_json::to_string_pretty(&json).unwrap_or_default()),
                            Err(e) => Err(format!("Parse error: {}", e)),
                        }
                    } else {
                        Err(format!("HTTP {}: record not found", status))
                    }
                }
                Err(e) => Err(format!("Request failed: {}", e)),
            }
        }

        "memory_delete" => {
            let id = call.arguments["id"].as_str().unwrap_or("");
            match client
                .delete(format!("{}/records/{}", base, id))
                .send()
                .await
            {
                Ok(resp) => {
                    if resp.status().is_success() {
                        Ok(format!("Memory '{}' deleted.", id))
                    } else {
                        Err(format!("Delete failed (status {})", resp.status()))
                    }
                }
                Err(e) => Err(format!("Request failed: {}", e)),
            }
        }

        "memory_sleep_cycle" => {
            match client.post(format!("{}/sleep-cycle", base)).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        match resp.json::<serde_json::Value>().await {
                            Ok(json) => {
                                let consolidation = json.get("consolidation_report").and_then(|c| c.get("records_processed")).map_or(0, |v| v.as_u64().unwrap_or(0));
                                let evicted = json.get("consolidation_report").and_then(|c| c.get("records_evicted")).map_or(0, |v| v.as_u64().unwrap_or(0));
                                let promoted = json.get("consolidation_report").and_then(|c| c.get("records_promoted")).map_or(0, |v| v.as_u64().unwrap_or(0));
                                let tuning = json.get("tuning_events").map_or(0, |v| v.as_array().map_or(0, |a| a.len()));
                                let pruning = json.get("pruning_events").map_or(0, |v| v.as_array().map_or(0, |a| a.len()));
                                let distillation = json.get("distillation_events").map_or(0, |v| v.as_array().map_or(0, |a| a.len()));
                                let health = json.get("assessment").and_then(|a| a.get("overall_health")).map_or(0.0, |v| v.as_f64().unwrap_or(0.0));
                                let duration = json.get("duration_ms").map_or(0, |v| v.as_u64().unwrap_or(0));
                                Ok(format!(
                                    "Sleep cycle complete ({}ms):\n  Consolidation: {} processed, {} evicted, {} promoted\n  Evolution: {} tuning, {} pruning, {} distillation events\n  Health: {:.0}%",
                                    duration, consolidation, evicted, promoted, tuning, pruning, distillation, health * 100.0
                                ))
                            }
                            Err(e) => Err(format!("Parse error: {}", e)),
                        }
                    } else {
                        Err(format!("HTTP {}: sleep cycle failed", resp.status()))
                    }
                }
                Err(e) => Err(format!("Request failed: {}", e)),
            }
        }

        "memory_namespace_create" => {
            let name = call.arguments["name"].as_str().unwrap_or("");
            let owner = call.arguments["owner"].as_str().unwrap_or("");
            let description = call.arguments["description"].as_str().unwrap_or("");
            let read_parents: Vec<String> = call.arguments["read_parents"]
                .as_array()
                .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();
            let write_children: Vec<String> = call.arguments["write_children"]
                .as_array()
                .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();

            let body = serde_json::json!({
                "name": name,
                "owner": owner,
                "description": description,
                "read_parents": read_parents,
                "write_children": write_children,
            });

            match client.post(format!("{}/namespaces", base)).json(&body).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        match resp.json::<serde_json::Value>().await {
                            Ok(json) => Ok(format!("Namespace '{}' created with ID: {}", name, json["namespace_id"])),
                            Err(e) => Err(format!("Parse error: {}", e)),
                        }
                    } else {
                        Err(format!("HTTP {}: create failed", resp.status()))
                    }
                }
                Err(e) => Err(format!("Request failed: {}", e)),
            }
        }

        "memory_namespace_list" => {
            match client.get(format!("{}/namespaces", base)).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        match resp.json::<Vec<serde_json::Value>>().await {
                            Ok(namespaces) => {
                                if namespaces.is_empty() {
                                    Ok("No namespaces found.".to_string())
                                } else {
                                    let formatted: Vec<String> = namespaces
                                        .iter()
                                        .map(|ns| {
                                            let id = ns["namespace_id"].as_str().unwrap_or("?");
                                            let name = ns["name"].as_str().unwrap_or("?");
                                            let owner = ns["owner"].as_str().unwrap_or("?");
                                            format!("[{}] {} (owner: {})", id, name, owner)
                                        })
                                        .collect();
                                    Ok(formatted.join("\n"))
                                }
                            }
                            Err(e) => Err(format!("Parse error: {}", e)),
                        }
                    } else {
                        Err(format!("HTTP {}: list failed", resp.status()))
                    }
                }
                Err(e) => Err(format!("Request failed: {}", e)),
            }
        }

        "memory_namespace_get" => {
            let ns_id = call.arguments["namespace_id"].as_str().unwrap_or("");
            match client.get(format!("{}/namespaces/{}", base, ns_id)).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        match resp.json::<serde_json::Value>().await {
                            Ok(json) => Ok(serde_json::to_string_pretty(&json).unwrap_or_default()),
                            Err(e) => Err(format!("Parse error: {}", e)),
                        }
                    } else {
                        Err(format!("HTTP {}: not found", resp.status()))
                    }
                }
                Err(e) => Err(format!("Request failed: {}", e)),
            }
        }

        "memory_namespace_get_by_name" => {
            let name = call.arguments["name"].as_str().unwrap_or("");
            match client.get(format!("{}/namespaces/name/{}", base, urlencoding::encode(name))).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        match resp.json::<serde_json::Value>().await {
                            Ok(json) => Ok(serde_json::to_string_pretty(&json).unwrap_or_default()),
                            Err(e) => Err(format!("Parse error: {}", e)),
                        }
                    } else {
                        Err(format!("HTTP {}: namespace '{}' not found", resp.status(), name))
                    }
                }
                Err(e) => Err(format!("Request failed: {}", e)),
            }
        }

        "memory_namespace_check_access" => {
            let reader_ns = call.arguments["reader_namespace"].as_str().unwrap_or("");
            let target_ns = call.arguments["target_namespace"].as_str().unwrap_or("");
            let body = serde_json::json!({
                "reader_namespace": reader_ns,
                "target_namespace": target_ns,
            });
            match client.post(format!("{}/namespaces/check-read", base)).json(&body).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        match resp.json::<serde_json::Value>().await {
                            Ok(json) => {
                                let can_read = json["can_read"].as_bool().unwrap_or(false);
                                Ok(format!(
                                    "Namespace '{}' {} read access to '{}'.",
                                    reader_ns,
                                    if can_read { "has" } else { "does NOT have" },
                                    target_ns
                                ))
                            }
                            Err(e) => Err(format!("Parse error: {}", e)),
                        }
                    } else {
                        Err(format!("HTTP {}: access check failed", resp.status()))
                    }
                }
                Err(e) => Err(format!("Request failed: {}", e)),
            }
        }

        "memory_stats" => match client.get(format!("{}/stats", base)).send().await {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    match resp.json::<serde_json::Value>().await {
                        Ok(json) => Ok(serde_json::to_string_pretty(&json).unwrap_or_default()),
                        Err(e) => Err(format!("Parse error: {}", e)),
                    }
                } else {
                    Err(format!("HTTP {}: stats failed", status))
                }
            }
            Err(e) => Err(format!("Request failed: {}", e)),
        },

        "memory_health" => match client.get(format!("{}/health", base)).send().await {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    match resp.json::<serde_json::Value>().await {
                        Ok(json) => Ok(serde_json::to_string_pretty(&json).unwrap_or_default()),
                        Err(e) => Err(format!("Parse error: {}", e)),
                    }
                } else {
                    Err(format!("HTTP {}: health check failed", status))
                }
            }
            Err(e) => Err(format!("Request failed: {}", e)),
        },

        "memory_promote" => {
            let id = call.arguments["id"].as_str().unwrap_or("");
            let tier = call.arguments["tier"].as_str().unwrap_or("semantic");
            match client
                .post(format!("{}/tiers/promote/{}/{}", base, id, tier))
                .send()
                .await
            {
                Ok(resp) => {
                    if resp.status().is_success() {
                        Ok(format!("Memory '{}' promoted to {}.", id, tier))
                    } else {
                        Err(format!("Promote failed (status {})", resp.status()))
                    }
                }
                Err(e) => Err(format!("Request failed: {}", e)),
            }
        }

        "memory_add_edge" => {
            let source = call.arguments["source_id"].as_str().unwrap_or("");
            let target = call.arguments["target_id"].as_str().unwrap_or("");
            let relation = call.arguments["relation_type"]
                .as_str()
                .unwrap_or("related_to");
            let weight = call.arguments["weight"].as_f64().unwrap_or(1.0);

            let body = serde_json::json!({
                "source_id": source,
                "target_id": target,
                "relation_type": relation,
                "weight": weight,
            });

            match client
                .post(format!("{}/graph/edges", base))
                .json(&body)
                .send()
                .await
            {
                Ok(resp) => {
                    if resp.status().is_success() {
                        Ok(format!(
                            "Edge created: {} --[{}]--> {}",
                            source, relation, target
                        ))
                    } else {
                        Err(format!("Edge creation failed (status {})", resp.status()))
                    }
                }
                Err(e) => Err(format!("Request failed: {}", e)),
            }
        }

        // ── Reflection Tools ─────────────────────────────────────────────
        "memory_reflect" => {
            let topic = call.arguments["topic"].as_str().unwrap_or("");
            let monologue = call.arguments["monologue"].as_str().unwrap_or("");
            let conclusion = call.arguments["conclusion"].as_str().unwrap_or("");
            let planned_actions: Vec<String> = call.arguments["planned_actions"]
                .as_array()
                .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();
            let confidence = call.arguments["confidence"].as_f64().unwrap_or(0.5);
            let tags: Vec<String> = call.arguments["tags"]
                .as_array()
                .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();

            let body = serde_json::json!({
                "topic": topic,
                "monologue": monologue,
                "conclusion": conclusion,
                "planned_actions": planned_actions,
                "confidence": confidence,
                "tags": tags,
            });

            match client.post(format!("{}/reflections", base)).json(&body).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        match resp.json::<serde_json::Value>().await {
                            Ok(json) => Ok(format!("Reflection stored: {}", json)),
                            Err(e) => Err(format!("Parse error: {}", e)),
                        }
                    } else {
                        Err(format!("HTTP {}: reflect failed", resp.status()))
                    }
                }
                Err(e) => Err(format!("Request failed: {}", e)),
            }
        }

        "memory_assess" => {
            match client.post(format!("{}/assessments", base)).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        match resp.json::<serde_json::Value>().await {
                            Ok(json) => Ok(serde_json::to_string_pretty(&json).unwrap_or_default()),
                            Err(e) => Err(format!("Parse error: {}", e)),
                        }
                    } else {
                        Err(format!("HTTP {}: assessment failed", resp.status()))
                    }
                }
                Err(e) => Err(format!("Request failed: {}", e)),
            }
        }

        "memory_reflexion_loop" => {
            let topic = call.arguments["topic"].as_str().unwrap_or("general");
            let observation = call.arguments["observation"].as_str().unwrap_or("");
            let body = serde_json::json!({
                "topic": topic,
                "observation": observation,
            });
            match client.post(format!("{}/reflexion", base)).json(&body).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        match resp.json::<serde_json::Value>().await {
                            Ok(json) => Ok(serde_json::to_string_pretty(&json).unwrap_or_default()),
                            Err(e) => Err(format!("Parse error: {}", e)),
                        }
                    } else {
                        Err(format!("HTTP {}: reflexion failed", resp.status()))
                    }
                }
                Err(e) => Err(format!("Request failed: {}", e)),
            }
        }

        // ── Temporal Fact Tools ──────────────────────────────────────────
        "memory_temporal_store" => {
            let content = call.arguments["content"].as_str().unwrap_or("");
            let content_type = call.arguments["content_type"].as_str().unwrap_or("fact");
            let importance = call.arguments["importance"].as_f64().unwrap_or(0.5);

            let body = serde_json::json!({
                "content": content,
                "content_type": content_type,
                "importance": importance,
            });

            match client.post(format!("{}/temporal/facts", base)).json(&body).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        match resp.json::<serde_json::Value>().await {
                            Ok(json) => Ok(format!("Temporal fact stored: {}", json["fact_id"])),
                            Err(e) => Err(format!("Parse error: {}", e)),
                        }
                    } else {
                        Err(format!("HTTP {}: store failed", resp.status()))
                    }
                }
                Err(e) => Err(format!("Request failed: {}", e)),
            }
        }

        "memory_temporal_recall" => {
            let fact_id = call.arguments["fact_id"].as_str().unwrap_or("");
            let body = serde_json::json!({"fact_id": fact_id});
            match client.post(format!("{}/temporal/recall", base)).json(&body).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        match resp.json::<serde_json::Value>().await {
                            Ok(json) => Ok(format!("Fact recalled. Decay score: {}, Recall count: {}",
                                json["decay_score"], json["recall_count"])),
                            Err(e) => Err(format!("Parse error: {}", e)),
                        }
                    } else {
                        Err(format!("HTTP {}: recall failed", resp.status()))
                    }
                }
                Err(e) => Err(format!("Request failed: {}", e)),
            }
        }

        "memory_temporal_get" => {
            let fact_id = call.arguments["fact_id"].as_str().unwrap_or("");
            match client.get(format!("{}/temporal/facts/{}", base, fact_id)).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        match resp.json::<serde_json::Value>().await {
                            Ok(json) => Ok(serde_json::to_string_pretty(&json).unwrap_or_default()),
                            Err(e) => Err(format!("Parse error: {}", e)),
                        }
                    } else {
                        Err(format!("HTTP {}: not found", resp.status()))
                    }
                }
                Err(e) => Err(format!("Request failed: {}", e)),
            }
        }

        "memory_temporal_search" => {
            let query = call.arguments["query"].as_str().unwrap_or("");
            let limit = call.arguments["limit"].as_u64().unwrap_or(10);
            match client.get(format!("{}/temporal/facts/search?q={}&limit={}", base, urlencoding::encode(query), limit)).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        match resp.json::<Vec<serde_json::Value>>().await {
                            Ok(results) => {
                                if results.is_empty() {
                                    Ok("No temporal facts found.".to_string())
                                } else {
                                    let formatted: Vec<String> = results
                                        .iter()
                                        .enumerate()
                                        .map(|(i, r)| {
                                            let id = r["fact_id"].as_str().unwrap_or("?");
                                            let content = r["content"].as_str().unwrap_or("");
                                            let decay = r["decay_score"].as_f64().unwrap_or(0.0);
                                            format!("{}. [{}] (decay: {:.2}) {}", i + 1, id, decay, content)
                                        })
                                        .collect();
                                    Ok(formatted.join("\n"))
                                }
                            }
                            Err(e) => Err(format!("Parse error: {}", e)),
                        }
                    } else {
                        Err(format!("HTTP {}: search failed", resp.status()))
                    }
                }
                Err(e) => Err(format!("Request failed: {}", e)),
            }
        }

        // ── Context Management Tools ─────────────────────────────────────
        "memory_context_add" => {
            let block_id = call.arguments["block_id"].as_str().unwrap_or("");
            let label = call.arguments["label"].as_str().unwrap_or("");
            let content = call.arguments["content"].as_str().unwrap_or("");
            let pinned = call.arguments["pinned"].as_bool().unwrap_or(false);
            let priority = call.arguments["priority"].as_i64().unwrap_or(50);

            let body = serde_json::json!({
                "block_id": block_id,
                "label": label,
                "content": content,
                "pinned": pinned,
                "priority": priority,
            });

            match client.post(format!("{}/context/blocks", base)).json(&body).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        Ok(format!("Context block '{}' updated.", block_id))
                    } else {
                        Err(format!("HTTP {}: upsert failed", resp.status()))
                    }
                }
                Err(e) => Err(format!("Request failed: {}", e)),
            }
        }

        "memory_context_render" => {
            match client.get(format!("{}/context/render", base)).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        match resp.json::<serde_json::Value>().await {
                            Ok(json) => Ok(format!(
                                "Context Window ({} bytes used / {} capacity):\n\n{}",
                                json["size_bytes"], json["capacity_bytes"], json["context"]
                            )),
                            Err(e) => Err(format!("Parse error: {}", e)),
                        }
                    } else {
                        Err(format!("HTTP {}: render failed", resp.status()))
                    }
                }
                Err(e) => Err(format!("Request failed: {}", e)),
            }
        }

        "memory_context_get" => {
            let block_id = call.arguments["block_id"].as_str().unwrap_or("");
            match client.get(format!("{}/context/blocks/{}", base, block_id)).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        match resp.json::<serde_json::Value>().await {
                            Ok(json) => Ok(serde_json::to_string_pretty(&json).unwrap_or_default()),
                            Err(e) => Err(format!("Parse error: {}", e)),
                        }
                    } else {
                        Err(format!("HTTP {}: not found", resp.status()))
                    }
                }
                Err(e) => Err(format!("Request failed: {}", e)),
            }
        }

        _ => Err(format!("Unknown tool: {}", call.name)),
    };

    match result {
        Ok(text) => McpToolResult {
            content: vec![McpContent::Text { text }],
            is_error: None,
        },
        Err(error) => McpToolResult {
            content: vec![McpContent::Text { text: error }],
            is_error: Some(true),
        },
    }
}

/// Run the MCP server in stdio mode (for Claude Desktop integration).
pub async fn run_stdio(api_url: &str) {
    use tokio::io::{AsyncBufReadExt, BufReader};

    let stdin = tokio::io::stdin();
    let reader = BufReader::new(stdin);
    let mut lines = reader.lines();

    eprintln!("Memory MCP server running (stdio mode)...");
    eprintln!("API URL: {}", api_url);

    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim().is_empty() {
            continue;
        }

        let response = match serde_json::from_str::<serde_json::Value>(&line) {
            Ok(msg) => {
                let method = msg["method"].as_str().unwrap_or("");

                match method {
                    "initialize" => serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": msg["id"],
                        "result": {
                            "protocolVersion": "2024-11-05",
                            "capabilities": {
                                "tools": {}
                            },
                            "serverInfo": {
                                "name": "agentic-memory",
                                "version": "1.0.0"
                            }
                        }
                    }),

                    "tools/list" => {
                        let tools = get_tools();
                        serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": msg["id"],
                            "result": {
                                "tools": tools
                            }
                        })
                    }

                    "tools/call" => {
                        let params = &msg["params"];
                        let tool_name = params["name"].as_str().unwrap_or("");
                        let arguments = params["arguments"].clone();

                        let call = McpToolCall {
                            name: tool_name.to_string(),
                            arguments,
                        };

                        let result = execute_tool(&call, api_url).await;

                        serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": msg["id"],
                            "result": result
                        })
                    }

                    _ => serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": msg["id"],
                        "error": {
                            "code": -32601,
                            "message": format!("Method not found: {}", method)
                        }
                    }),
                }
            }
            Err(e) => serde_json::json!({
                "jsonrpc": "2.0",
                "id": null,
                "error": {
                    "code": -32700,
                    "message": format!("Parse error: {}", e)
                }
            }),
        };

        println!("{}", serde_json::to_string(&response).unwrap());
    }
}
