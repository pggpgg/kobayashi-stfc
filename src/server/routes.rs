use crate::server::api;

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

pub fn route_request(method: &str, path: &str, body: &str) -> HttpResponse {
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
        ("POST", "/api/optimize") => match api::optimize_payload(body) {
            Ok(payload) => HttpResponse {
                status_code: 200,
                status_text: "OK",
                content_type: "application/json",
                body: payload,
            },
            Err(err) => error_response(400, "Bad Request", &format!("Invalid request body: {err}")),
        },
        _ => error_response(404, "Not Found", "Route not found"),
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
    <label for="sims">Simulations</label>
    <input id="sims" type="number" value="5000" />
    <div><button id="optimize-btn">POST /api/optimize</button></div>
  </div>

  <pre id="output">Ready.</pre>

  <script>
    const output = document.getElementById('output');

    async function request(path, options) {
      output.textContent = 'Loading…';
      const response = await fetch(path, options);
      const text = await response.text();
      output.textContent = `HTTP ${response.status}\n${text}`;
    }

    document.getElementById('health-btn').addEventListener('click', () => {
      request('/api/health', { method: 'GET' });
    });

    document.getElementById('optimize-btn').addEventListener('click', () => {
      const payload = {
        ship: document.getElementById('ship').value,
        hostile: document.getElementById('hostile').value,
        sims: Number(document.getElementById('sims').value),
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
