// The declarative HTTP plugin: one GET through the guard, the answer read as
// JSON, and the headline and rows picked out of it. No code runs.

use super::guard;
use super::manifest::{self, Manifest, Shown, Source};

pub type SecretLookup<'a> = &'a (dyn Fn(&str) -> Option<String> + Sync);

pub async fn poll(m: &Manifest, secret: SecretLookup<'_>) -> Result<Shown, String> {
    let Source::Http { url, headers } = &m.source else {
        return Err("not an http plugin".into());
    };
    // Secrets first: a missing one stops the poll before anything is sent.
    let filled = headers
        .iter()
        .map(|(k, v)| manifest::fill(v, secret).map(|v| (k.clone(), v)))
        .collect::<Result<Vec<_>, _>>()?;
    let client = guard::client(m).await?;
    let (status, body) = guard::get(&client, url, &filled).await?;
    if !(200..300).contains(&status) {
        return Err(format!("HTTP {status}"));
    }
    let value: serde_json::Value = serde_json::from_slice(&body).map_err(|_| "The answer is not JSON.".to_string())?;
    Ok(manifest::show(&m.show, &value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_http::{serve, Canned};
    use serde_json::json;

    fn run<F: std::future::Future>(f: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(f)
    }

    fn plugin(origin: &str) -> Manifest {
        let v = json!({
            "schema": 1, "id": "status", "name": "Status", "version": "1", "kind": "http",
            "secrets": [{ "key": "token", "label": "Token" }],
            "allowedHosts": ["127.0.0.1"], "allowPrivateNetwork": true,
            "http": { "url": format!("{origin}/api/v2/summary.json"), "headers": { "Authorization": "Bearer {{secret.token}}" } },
            "show": { "headline": "/status/description", "items": "/components", "title": "/name", "detail": "/status" }
        });
        manifest::parse(&serde_json::to_vec(&v).unwrap()).unwrap()
    }

    const SUMMARY: &str = r#"{"status":{"indicator":"minor","description":"Partial System Outage"},
      "components":[{"name":"Git Operations","status":"operational"},{"name":"Actions","status":"degraded_performance"}]}"#;

    #[test]
    fn a_poll_shows_what_the_manifest_points_at() {
        let (origin, seen) = serve(vec![Canned::json(200, SUMMARY)]);
        let secret = |k: &str| (k == "token").then(|| "abc".to_string());
        let shown = run(poll(&plugin(&origin), &secret)).unwrap();
        assert_eq!(shown.headline.as_deref(), Some("Partial System Outage"));
        assert_eq!(shown.items.len(), 2);
        assert_eq!((shown.items[1].title.as_str(), shown.items[1].detail.as_str()), ("Actions", "degraded_performance"));
        assert!(seen.lock().unwrap()[0].to_lowercase().contains("authorization: bearer abc"));
    }

    #[test]
    fn a_missing_secret_stops_the_poll_before_anything_is_sent() {
        let (origin, seen) = serve(vec![Canned::json(200, SUMMARY)]);
        let none = |_: &str| None;
        let err = run(poll(&plugin(&origin), &none)).unwrap_err();
        assert!(err.contains("token"), "{err}");
        assert!(seen.lock().unwrap().is_empty());
    }

    #[test]
    fn server_errors_and_non_json_answers_are_reported() {
        let secret = |_: &str| Some("x".to_string());
        let (origin, _) = serve(vec![Canned::json(503, "down")]);
        assert!(run(poll(&plugin(&origin), &secret)).unwrap_err().contains("503"));
        let (origin, _) = serve(vec![Canned::json(200, "<html>")]);
        assert!(run(poll(&plugin(&origin), &secret)).unwrap_err().to_lowercase().contains("json"));
    }
}
