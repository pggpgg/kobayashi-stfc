//! Utility for detecting whether the built frontend SPA is available.
//!
//! The SPA is served from `frontend/dist` by `routes::serve_spa_static_fallback` (not `ServeDir`).
//! Responses use `Cache-Control` (immutable for `assets/*`, `no-cache` for `index.html`) and
//! `tower_http::compression::CompressionLayer` (gzip/br) on the router.

use std::path::PathBuf;

/// Returns true if `frontend/dist` (or `dist`) exists in the current working
/// directory so the SPA can be served.
pub fn static_files_available() -> bool {
    locate_dist_dir().is_some()
}

pub fn locate_dist_dir() -> Option<PathBuf> {
    let base = std::env::current_dir().ok()?;
    [base.join("frontend/dist"), base.join("dist")]
        .into_iter()
        .find(|p| p.is_dir())
}

/// Warn when tracked `index.html` references hashed bundles that are not on disk
/// (`frontend/dist/assets/` is gitignored; run `cd frontend && npm run build`).
pub fn warn_if_spa_assets_missing() {
    let Some(dir) = locate_dist_dir() else {
        return;
    };
    let index_path = dir.join("index.html");
    let Ok(index) = std::fs::read_to_string(&index_path) else {
        tracing::warn!(
            "SPA directory exists but {} is missing or unreadable",
            index_path.display()
        );
        return;
    };

    let mut missing = Vec::new();
    for asset_rel in referenced_spa_assets(&index) {
        let full = dir.join(asset_rel.trim_start_matches('/'));
        if !full.is_file() {
            missing.push(asset_rel);
        }
    }

    if missing.is_empty() {
        return;
    }

    tracing::warn!(
        "SPA assets missing (UI will not load until rebuilt): {} — run: cd frontend && npm install && npm run build",
        missing.join(", ")
    );
}

fn referenced_spa_assets(index_html: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in index_html.lines() {
        let trimmed = line.trim();
        if !trimmed.contains("src=\"/assets/") && !trimmed.contains("href=\"/assets/") {
            continue;
        }
        for token in trimmed.split('"') {
            if token.starts_with("/assets/") {
                out.push(token.to_string());
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::referenced_spa_assets;

    #[test]
    fn referenced_spa_assets_parses_vite_index_html() {
        let html = r#"<!DOCTYPE html>
<html lang="en">
  <head>
    <script type="module" crossorigin src="/assets/index-abc123.js"></script>
    <link rel="stylesheet" crossorigin href="/assets/index-def456.css">
  </head>
  <body><div id="root"></div></body>
</html>"#;
        assert_eq!(
            referenced_spa_assets(html),
            vec![
                "/assets/index-abc123.js".to_string(),
                "/assets/index-def456.css".to_string(),
            ]
        );
    }
}
