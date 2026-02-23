use crate::server::api;
use crate::server::static_files;
use crate::server::sync;

pub struct HttpResponse {
    pub status_code: u16,
    pub status_text: &'static str,
    pub content_type: &'static str,
    pub body: String,
}

impl HttpResponse {
    pub fn to_http_string(&self) -> String {
        format!(
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            self.status_code,
            self.status_text,
            self.content_type,
            self.body.len(),
            self.body
        )
    }
}

pub fn route_request(
    method: &str,
    path: &str,
    body: &str,
    sync_token: Option<&str>,
) -> HttpResponse {
    if let Some(response) = static_files::try_serve_static(method, path) {
        return response;
    }
    match (method, path) {
        ("GET", "/") => HttpResponse {
            status_code: 200,
            status_text: "OK",
            content_type: "text/html; charset=utf-8",
            body: index_html(),
        },
        ("GET", "/api/health") => match api::health_payload() {
            Ok(payload) => HttpResponse {
                status_code: 200,
                status_text: "OK",
                content_type: "application/json",
                body: payload,
            },
            Err(err) => error_response(500, "Internal Server Error", &err.to_string()),
        },
        (method, path) if method == "GET" && path.starts_with("/api/officers") && !path.starts_with("/api/officers/") => {
            match api::officers_payload(path) {
                Ok(payload) => HttpResponse {
                    status_code: 200,
                    status_text: "OK",
                    content_type: "application/json",
                    body: payload,
                },
                Err(err) => error_response(500, "Internal Server Error", &err.to_string()),
            }
        }
        ("GET", "/api/ships") => match api::ships_payload() {
            Ok(payload) => HttpResponse {
                status_code: 200,
                status_text: "OK",
                content_type: "application/json",
                body: payload,
            },
            Err(err) => error_response(500, "Internal Server Error", &err.to_string()),
        },
        ("GET", "/api/hostiles") => match api::hostiles_payload() {
            Ok(payload) => HttpResponse {
                status_code: 200,
                status_text: "OK",
                content_type: "application/json",
                body: payload,
            },
            Err(err) => error_response(500, "Internal Server Error", &err.to_string()),
        },
        ("GET", "/api/data/version") => match api::data_version_payload() {
            Ok(payload) => HttpResponse {
                status_code: 200,
                status_text: "OK",
                content_type: "application/json",
                body: payload,
            },
            Err(err) => error_response(500, "Internal Server Error", &err.to_string()),
        },
        ("POST", "/api/simulate") => match api::simulate_payload(body) {
            Ok(payload) => HttpResponse {
                status_code: 200,
                status_text: "OK",
                content_type: "application/json",
                body: payload,
            },
            Err(api::SimulateError::Parse(err)) => {
                error_response(400, "Bad Request", &format!("Invalid request body: {err}"))
            }
            Err(api::SimulateError::Validation(msg)) => {
                error_response(400, "Bad Request", &msg)
            }
        },
        ("GET", "/api/profile") => match api::profile_get_payload() {
            Ok(payload) => HttpResponse {
                status_code: 200,
                status_text: "OK",
                content_type: "application/json",
                body: payload,
            },
            Err(err) => error_response(500, "Internal Server Error", &err.to_string()),
        },
        ("PUT", "/api/profile") => match api::profile_put_payload(body) {
            Ok(payload) => HttpResponse {
                status_code: 200,
                status_text: "OK",
                content_type: "application/json",
                body: payload,
            },
            Err(err) => error_response(400, "Bad Request", &err.to_string()),
        },
        ("POST", "/api/officers/import") => match api::officers_import_payload(body) {
            Ok(payload) => HttpResponse {
                status_code: 200,
                status_text: "OK",
                content_type: "application/json",
                body: payload,
            },
            Err(err) => error_response(400, "Bad Request", &err.to_string()),
        },
        ("GET", "/api/presets") => match api::presets_list_payload() {
            Ok(payload) => HttpResponse {
                status_code: 200,
                status_text: "OK",
                content_type: "application/json",
                body: payload,
            },
            Err(err) => error_response(500, "Internal Server Error", &err.to_string()),
        },
        (method, path) if method == "GET" && path.starts_with("/api/presets/") => {
            let id = path.trim_start_matches("/api/presets/").split('/').next().unwrap_or("");
            match api::preset_get_payload(id) {
                Ok(payload) => HttpResponse {
                    status_code: 200,
                    status_text: "OK",
                    content_type: "application/json",
                    body: payload,
                },
                Err(api::PresetError::NotFound) => error_response(404, "Not Found", "Preset not found"),
                Err(err) => error_response(500, "Internal Server Error", &err.to_string()),
            }
        }
        ("POST", "/api/presets") => match api::preset_post_payload(body) {
            Ok(payload) => HttpResponse {
                status_code: 200,
                status_text: "OK",
                content_type: "application/json",
                body: payload,
            },
            Err(err) => error_response(400, "Bad Request", &err.to_string()),
        },
        (method, path) if method == "GET" && path.starts_with("/api/optimize/estimate") => {
            match api::optimize_estimate_payload(path) {
                Ok(payload) => HttpResponse {
                    status_code: 200,
                    status_text: "OK",
                    content_type: "application/json",
                    body: payload,
                },
                Err(api::OptimizePayloadError::Parse(err)) => {
                    error_response(400, "Bad Request", &format!("Invalid request: {err}"))
                }
                Err(api::OptimizePayloadError::Validation(validation)) => {
                    validation_error_response(400, "Bad Request", validation)
                }
            }
        }
        ("POST", "/api/optimize") => match api::optimize_payload(body) {
            Ok(payload) => HttpResponse {
                status_code: 200,
                status_text: "OK",
                content_type: "application/json",
                body: payload,
            },
            Err(api::OptimizePayloadError::Parse(err)) => {
                error_response(400, "Bad Request", &format!("Invalid request body: {err}"))
            }
            Err(api::OptimizePayloadError::Validation(validation)) => {
                validation_error_response(400, "Bad Request", validation)
            }
        },
        ("POST", "/api/sync/ingress") => sync::ingress_payload(body, sync_token),
        _ => error_response(404, "Not Found", "Route not found"),
    }
}

fn validation_error_response(
    status_code: u16,
    status_text: &'static str,
    payload: api::ValidationErrorResponse,
) -> HttpResponse {
    let fallback =
        "{\n  \"status\": \"error\",\n  \"message\": \"Validation failed\"\n}".to_string();

    HttpResponse {
        status_code,
        status_text,
        content_type: "application/json",
        body: serde_json::to_string_pretty(&payload).unwrap_or(fallback),
    }
}

fn error_response(status_code: u16, status_text: &'static str, message: &str) -> HttpResponse {
    HttpResponse {
        status_code,
        status_text,
        content_type: "application/json",
        body: format!(
            "{{\n  \"status\": \"error\",\n  \"message\": {}\n}}",
            serde_json::to_string(message).unwrap_or_else(|_| "\"Unknown error\"".to_string())
        ),
    }
}

fn index_html() -> String {
    r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width,initial-scale=1" />
  <title>Kobayashi API Console</title>
  <style>
    body { font-family: Arial, sans-serif; max-width: 900px; margin: 24px auto; padding: 0 12px; }
    h1 { margin-bottom: 8px; }
    .card { border: 1px solid #ddd; border-radius: 8px; padding: 14px; margin: 14px 0; }
    label { display:block; margin: 8px 0 4px; font-weight: 600; }
    input { width: 100%; padding: 8px; box-sizing: border-box; }
    button { margin-top: 12px; padding: 8px 14px; }
    pre { background: #111; color: #aef2ae; padding: 12px; overflow: auto; border-radius: 6px; min-height: 180px; }
  </style>
</head>
<body>
  <h1>Kobayashi Local API</h1>
  <p>Infrastructure scaffold for browser-based access to optimization endpoints.</p>

  <div class="card">
    <strong>Health</strong>
    <div><button id="health-btn">GET /api/health</button></div>
  </div>

  <div class="card">
    <strong>Optimize</strong>
    <label for="ship">Ship</label>
    <input id="ship" value="Saladin" />
    <label for="hostile">Hostile</label>
    <input id="hostile" value="Explorer_30" />
    <label for="sims">Fight iterations per crew</label>
    <div style="display:flex;align-items:center;gap:8px;flex-wrap:wrap;">
      <input id="sims" type="number" min="1" max="100000" value="5000" style="width:90px" />
      <button type="button" class="sims-preset" data-sims="1000">1k</button>
      <button type="button" class="sims-preset" data-sims="5000">5k</button>
      <button type="button" class="sims-preset" data-sims="10000">10k</button>
      <button type="button" class="sims-preset" data-sims="50000">50k</button>
    </div>
    <p id="estimate-msg" style="margin:8px 0 0;font-size:0.9rem;color:#666;"></p>
    <div><button id="optimize-btn">POST /api/optimize</button></div>
  </div>

  <pre id="output">Ready.</pre>

  <script>
    const output = document.getElementById('output');
    const shipEl = document.getElementById('ship');
    const hostileEl = document.getElementById('hostile');
    const simsEl = document.getElementById('sims');
    const estimateEl = document.getElementById('estimate-msg');

    let estimateTimer = null;
    function fetchEstimate() {
      const ship = shipEl.value.trim();
      const hostile = hostileEl.value.trim();
      const sims = Math.max(1, Math.min(100000, Number(simsEl.value) || 5000));
      if (!ship || !hostile) { estimateEl.textContent = ''; return; }
      const url = '/api/optimize/estimate?ship=' + encodeURIComponent(ship) + '&hostile=' + encodeURIComponent(hostile) + '&sims=' + sims;
      fetch(url).then(r => r.ok ? r.json() : null).then(data => {
        if (data) estimateEl.textContent = 'Estimated time: ~' + (data.estimated_seconds < 1 ? '<1' : data.estimated_seconds.toFixed(1)) + ' s (' + data.estimated_candidates + ' crews)';
        else estimateEl.textContent = '';
      }).catch(() => { estimateEl.textContent = ''; });
    }
    function scheduleEstimate() {
      if (estimateTimer) clearTimeout(estimateTimer);
      estimateTimer = setTimeout(fetchEstimate, 300);
    }
    shipEl.addEventListener('input', scheduleEstimate);
    hostileEl.addEventListener('input', scheduleEstimate);
    simsEl.addEventListener('input', scheduleEstimate);
    fetchEstimate();

    document.querySelectorAll('.sims-preset').forEach(btn => {
      btn.addEventListener('click', () => { simsEl.value = btn.dataset.sims; scheduleEstimate(); });
    });

    async function request(path, options) {
      output.textContent = 'Loading…';
      const response = await fetch(path, options);
      const text = await response.text();
      let display = 'HTTP ' + response.status + '\\n' + text;
      if (options && options.method === 'POST' && path === '/api/optimize') {
        try {
          const j = JSON.parse(text);
          if (j.duration_ms != null) display = 'Completed in ' + (j.duration_ms / 1000).toFixed(1) + ' s\\n\\n' + display;
        } catch (e) {}
      }
      output.textContent = display;
    }

    document.getElementById('health-btn').addEventListener('click', () => {
      request('/api/health', { method: 'GET' });
    });

    document.getElementById('optimize-btn').addEventListener('click', () => {
      const payload = {
        ship: shipEl.value,
        hostile: hostileEl.value,
        sims: Math.max(1, Math.min(100000, Number(simsEl.value) || 5000)),
      };
      request('/api/optimize', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      });
    });
  </script>
</body>
</html>
"#
    .to_string()
}
