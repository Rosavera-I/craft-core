//! API client for communicating with CRAFT Web Dashboard backend

use serde::{Deserialize, Serialize};

/// Default API base URL for local development
const DEFAULT_API_URL: &str = "http://127.0.0.1:3000";

/// API response wrapper matching backend format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<ApiError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

/// Harness information from API
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HarnessInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub source: String,
    pub authors: Vec<String>,
    pub installed_at: String,
}

/// Memory fact from API
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryFact {
    pub scope: String,
    pub key: String,
    pub value: String,
    pub created_at: i64,
}

/// Memory search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySearchResult {
    pub facts: Vec<MemoryFact>,
    pub total: usize,
}

/// Runtime status from API
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeStatus {
    pub active: bool,
    pub current_harness: Option<String>,
    pub last_activity: Option<String>,
    pub stats: RuntimeStats,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct RuntimeStats {
    pub memory_facts_count: usize,
    pub installed_harnesses: usize,
    pub compositions_created: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositionRequest {
    pub harness_names: Vec<String>,
    #[serde(default = "default_strategy")]
    pub strategy: String,
}

fn default_strategy() -> String {
    "ordered-merge".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositionPlan {
    pub strategy: String,
    pub harnesses: Vec<CompositionHarness>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositionHarness {
    pub name: String,
    pub version: String,
    pub source: String,
    pub path: String,
}

/// API client for making requests to the backend
#[derive(Debug, Clone)]
pub struct ApiClient {
    base_url: String,
}

impl Default for ApiClient {
    fn default() -> Self {
        Self::new(DEFAULT_API_URL)
    }
}

impl ApiClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
        }
    }

    /// Test helper to create with custom base URL
    #[cfg(test)]
    pub fn test() -> Self {
        Self::new("http://127.0.0.1:3000")
    }

    /// GET request helper
    async fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T, String> {
        self.request::<(), T>("GET", path, None).await
    }

    /// POST request helper
    async fn post<B: Serialize, T: for<'de> Deserialize<'de>>(&self, path: &str, body: &B) -> Result<T, String> {
        self.request("POST", path, Some(body)).await
    }

    /// Generic request helper
    async fn request<B: Serialize, T: for<'de> Deserialize<'de>>(
        &self,
        method: &str,
        path: &str,
        body: Option<&B>,
    ) -> Result<T, String> {
        let url = format!("{}{}", self.base_url, path);
        
        // Use web_sys for fetch in browser
        let window = web_sys::window().ok_or("No window")?;
        let mut opts = web_sys::RequestInit::new();
        opts.method(method);
        
        if let Some(b) = body {
            let json = serde_json::to_string(b).map_err(|e| e.to_string())?;
            opts.body(Some(&wasm_bindgen::JsValue::from_str(&json)));
        }
        
        let request = web_sys::Request::new_with_str_and_init(&url, &opts)
            .map_err(|_| "Failed to create request")?;
        
        let response = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
            .await
            .map_err(|_| "Request failed")?;
        
        let response: web_sys::Response = response.dyn_into().map_err(|_| "Invalid response")?;
        let json = wasm_bindgen_futures::JsFuture::from(response.json().map_err(|_| "No JSON")?)
            .await
            .map_err(|_| "Failed to parse JSON")?;
        
        let api_response: ApiResponse<T> = serde_wasm_bindgen::from_value(&json)
            .map_err(|e| format!("Parse error: {}", e))?;
        
        if let Some(data) = api_response.data {
            Ok(data)
        } else if let Some(error) = api_response.error {
            Err(format!("{}: {}", error.code, error.message))
        } else {
            Err("Empty response".to_string())
        }
    }

    /// List all installed harnesses
    pub async fn list_harnesses(&self) -> Result<Vec<HarnessInfo>, String> {
        self.get("/api/v1/harnesses").await
    }

    /// Get specific harness
    pub async fn get_harness(&self, name: &str) -> Result<HarnessInfo, String> {
        self.get(&format!("/api/v1/harnesses/{}", name)).await
    }

    /// Search memory facts
    pub async fn search_memory(&self, query: &str, scope: Option<&str>) -> Result<MemorySearchResult, String> {
        let mut url = format!("/api/v1/memory/search?q={}", urlencoding::encode(query));
        if let Some(s) = scope {
            url.push_str(&format!("&scope={}", s));
        }
        self.get(&url).await
    }

    /// List memory facts
    pub async fn list_memory_facts(&self, scope: Option<&str>) -> Result<Vec<MemoryFact>, String> {
        let mut url = "/api/v1/memory/facts".to_string();
        if let Some(s) = scope {
            url.push_str(&format!("?scope={}", s));
        }
        self.get(&url).await
    }

    /// Create memory fact
    pub async fn create_memory_fact(&self, scope: &str, key: &str, value: &str) -> Result<MemoryFact, String> {
        #[derive(Serialize)]
        struct Request {
            scope: String,
            key: String,
            value: String,
        }
        
        let body = Request {
            scope: scope.to_string(),
            key: key.to_string(),
            value: value.to_string(),
        };
        
        self.post("/api/v1/memory/facts", &body).await
    }

    /// Get runtime status
    pub async fn get_status(&self) -> Result<RuntimeStatus, String> {
        self.get("/api/v1/status").await
    }

    /// Plan composition
    pub async fn compose_plan(&self, harness_names: Vec<String>, strategy: &str) -> Result<CompositionPlan, String> {
        let body = CompositionRequest {
            harness_names,
            strategy: strategy.to_string(),
        };
        self.post("/api/v1/compose/plan", &body).await
    }
}

/// WebSocket validation client
pub struct ValidationWebSocket {
    ws: Option<web_sys::WebSocket>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum ValidationMessage {
    #[serde(rename = "validate")]
    Validate { request: ValidationRequest },
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidationRequest {
    pub harness_names: Vec<String>,
    pub strategy: String,
}

impl ValidationWebSocket {
    pub fn new(base_url: &str) -> Self {
        let ws = web_sys::WebSocket::new(&format!("{}/ws/validate", base_url.replace("http", "ws")))
            .ok();
        Self { ws }
    }

    pub fn send_validate(&self, harness_names: Vec<String>, strategy: String) -> Result<(), String> {
        if let Some(ref ws) = self.ws {
            let msg = ValidationMessage::Validate {
                request: ValidationRequest { harness_names, strategy },
            };
            let json = serde_json::to_string(&msg).map_err(|e| e.to_string())?;
            ws.send_with_str(&json).map_err(|_| "Send failed")?;
            Ok(())
        } else {
            Err("WebSocket not connected".to_string())
        }
    }

    pub fn on_message<F: FnMut(String) + 'static>(&self, mut callback: F) -> Result<(), String> {
        if let Some(ref ws) = self.ws {
            let closure = wasm_bindgen::closure::Closure::wrap(Box::new(
                move |event: web_sys::MessageEvent| {
                    if let Ok(text) = event.data().dyn_into::<js_sys::JsString>() {
                        callback(text.as_string().unwrap_or_default());
                    }
                }
            ) as Box<dyn FnMut(_)>);
            
            ws.set_onmessage(Some(closure.as_ref().unchecked_ref()));
            closure.forget();
            Ok(())
        } else {
            Err("WebSocket not connected".to_string())
        }
    }
}
