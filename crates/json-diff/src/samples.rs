//! The documents shown on first launch. One service config, before and after
//! a release, chosen so every kind of difference appears at least once while
//! the whole thing stays readable (about 150 lines a side, 16 when folded):
//!
//! - `version`, `runtime.replicas`: a value changed in place (yellow, with
//!   the changed characters highlighted);
//! - `description`: a long line edited in the middle (horizontal scrolling,
//!   word-level highlights);
//! - `runtime.timeout`: a number became a string (type change, yellow);
//! - `database.pool.max`: a change three levels deep (yellow trail through
//!   the folded keys above it);
//! - `features`: an array element removed and another added;
//! - `endpoints`: a record inserted right next to a modified one, and a
//!   record removed (elements paired by similarity, not position);
//! - `flags`: a key added and a key removed inside an existing object;
//! - `legacy_soap` removed and `tracing` added at the top level (red/green);
//! - `metadata`: same content, keys reordered (disappears with Sort keys);
//! - `regions`: a long section with no changes (folding hides the noise).

pub const ORIGINAL: &str = r##"{
  "service": "checkout-api",
  "version": "2.3.1",
  "description": "Handles carts, pricing and payment for the web and mobile storefronts. Talks to the inventory service over gRPC and to the payment gateway over HTTPS with mutual TLS.",
  "owner": {
    "team": "payments",
    "lead": "Omkar Mohanty",
    "slack": "#payments-oncall"
  },
  "runtime": {
    "image": "registry.internal/checkout-api:2.3.1",
    "replicas": 3,
    "timeout": 30,
    "resources": { "cpu": "500m", "memory": "512Mi" }
  },
  "database": {
    "engine": "postgres",
    "host": "db-primary.internal",
    "port": 5432,
    "pool": { "min": 2, "max": 10, "idle_timeout_s": 300 },
    "replicas": ["db-replica-1.internal", "db-replica-2.internal"]
  },
  "features": ["carts", "coupons", "gift-cards", "guest-checkout"],
  "endpoints": [
    { "path": "/cart", "method": "GET", "rate_limit": 1000 },
    { "path": "/cart", "method": "POST", "rate_limit": 500 },
    { "path": "/checkout", "method": "POST", "rate_limit": 100 },
    { "path": "/coupons", "method": "GET", "rate_limit": 300 },
    { "path": "/orders/{id}", "method": "GET", "rate_limit": 800 },
    { "path": "/legacy/quote", "method": "POST", "rate_limit": 50 }
  ],
  "flags": {
    "new_pricing": false,
    "apple_pay": true,
    "beta_banner": true
  },
  "legacy_soap": {
    "enabled": true,
    "wsdl": "https://checkout.internal/legacy.wsdl"
  },
  "metadata": {
    "created": "2023-02-14",
    "tier": "critical",
    "cost_center": "CC-1042"
  },
  "regions": [
    { "code": "us-east-1", "name": "N. Virginia", "primary": true },
    { "code": "us-west-2", "name": "Oregon", "primary": false },
    { "code": "eu-west-1", "name": "Ireland", "primary": false },
    { "code": "eu-central-1", "name": "Frankfurt", "primary": false },
    { "code": "ap-south-1", "name": "Mumbai", "primary": false },
    { "code": "ap-southeast-1", "name": "Singapore", "primary": false },
    { "code": "ap-northeast-1", "name": "Tokyo", "primary": false },
    { "code": "sa-east-1", "name": "São Paulo", "primary": false }
  ]
}"##;

pub const MODIFIED: &str = r##"{
  "service": "checkout-api",
  "version": "2.4.0",
  "description": "Handles carts, pricing, payment and returns for the web and mobile storefronts. Talks to the inventory service over gRPC and to the payment gateway over HTTPS with mutual TLS.",
  "owner": {
    "team": "payments",
    "lead": "Omkar Mohanty",
    "slack": "#payments-oncall"
  },
  "runtime": {
    "image": "registry.internal/checkout-api:2.4.0",
    "replicas": 5,
    "timeout": "30s",
    "resources": { "cpu": "500m", "memory": "512Mi" }
  },
  "database": {
    "engine": "postgres",
    "host": "db-primary.internal",
    "port": 5432,
    "pool": { "min": 2, "max": 25, "idle_timeout_s": 300 },
    "replicas": ["db-replica-1.internal", "db-replica-2.internal"]
  },
  "features": ["carts", "coupons", "guest-checkout", "buy-now-pay-later"],
  "endpoints": [
    { "path": "/cart", "method": "GET", "rate_limit": 1000 },
    { "path": "/cart", "method": "POST", "rate_limit": 500 },
    { "path": "/checkout/preview", "method": "POST", "rate_limit": 200 },
    { "path": "/checkout", "method": "POST", "rate_limit": 150 },
    { "path": "/coupons", "method": "GET", "rate_limit": 300 },
    { "path": "/orders/{id}", "method": "GET", "rate_limit": 800 }
  ],
  "flags": {
    "new_pricing": true,
    "apple_pay": true,
    "klarna": true
  },
  "metadata": {
    "tier": "critical",
    "cost_center": "CC-1042",
    "created": "2023-02-14"
  },
  "regions": [
    { "code": "us-east-1", "name": "N. Virginia", "primary": true },
    { "code": "us-west-2", "name": "Oregon", "primary": false },
    { "code": "eu-west-1", "name": "Ireland", "primary": false },
    { "code": "eu-central-1", "name": "Frankfurt", "primary": false },
    { "code": "ap-south-1", "name": "Mumbai", "primary": false },
    { "code": "ap-southeast-1", "name": "Singapore", "primary": false },
    { "code": "ap-northeast-1", "name": "Tokyo", "primary": false },
    { "code": "sa-east-1", "name": "São Paulo", "primary": false }
  ],
  "tracing": {
    "enabled": true,
    "exporter": "otlp",
    "sample_rate": 0.1
  }
}"##;

#[cfg(test)]
mod tests {
    use super::*;
    use json_diff_core::{
        ChangeKind, DiffOptions, Folds, LineStatus, diff_json_texts, diff_json_texts_folded,
    };

    /// The sample must keep demonstrating everything its doc comment lists.
    #[test]
    fn sample_shows_every_kind_of_difference() {
        let diff = diff_json_texts(ORIGINAL, MODIFIED, &DiffOptions::default());
        let changes = diff.changes.expect("both sides are valid JSON");
        let has = |path: &str, kind| changes.iter().any(|c| c.path == path && c.kind == kind);
        assert!(has("/version", ChangeKind::Modified));
        assert!(has("/description", ChangeKind::Modified));
        assert!(has("/runtime/timeout", ChangeKind::Modified));
        assert!(has("/database/pool/max", ChangeKind::Modified));
        assert!(has("/features/2", ChangeKind::Removed));
        assert!(has("/features/3", ChangeKind::Added));
        // The inserted endpoint is an addition and its neighbour a
        // modification, not two modifications.
        assert!(has("/endpoints/2", ChangeKind::Added));
        assert!(has("/endpoints/3/rate_limit", ChangeKind::Modified));
        assert!(has("/endpoints/5", ChangeKind::Removed));
        assert!(has("/flags/klarna", ChangeKind::Added));
        assert!(has("/flags/beta_banner", ChangeKind::Removed));
        assert!(has("/legacy_soap", ChangeKind::Removed));
        assert!(has("/tracing", ChangeKind::Added));
        assert!(!changes.iter().any(|c| c.path.starts_with("/metadata")));
        assert!(!changes.iter().any(|c| c.path.starts_with("/regions")));

        // Long enough to scroll, short enough to read.
        assert!((120..=200).contains(&diff.view.right.len()));
        // Key order alone differs in `metadata`: only Sort keys hides it.
        let reordered = |sort_keys| {
            let options = DiffOptions {
                sort_keys,
                ..DiffOptions::default()
            };
            let view = diff_json_texts(ORIGINAL, MODIFIED, &options).view;
            view.hunks.iter().any(|hunk| {
                view.right[hunk.right.clone()]
                    .iter()
                    .any(|line| line.contains("\"created\""))
            })
        };
        assert!(reordered(false));
        assert!(!reordered(true));

        // Folded, it opens as one line per top-level key.
        let folded = diff_json_texts_folded(
            ORIGINAL,
            MODIFIED,
            &DiffOptions::default(),
            &Folds::default(),
        );
        assert!(folded.view.right.len() <= 16);
        let status = |key: &str| {
            let line = folded
                .view
                .right
                .iter()
                .position(|line| line.contains(key))
                .unwrap();
            folded.view.right_status[line]
        };
        assert_eq!(status("\"database\""), LineStatus::Modified);
        assert_eq!(status("\"tracing\""), LineStatus::Added);
        assert_eq!(status("\"regions\""), LineStatus::Unchanged);
    }
}
