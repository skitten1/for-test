#!/usr/bin/env python3

from pathlib import Path
import sys

if len(sys.argv) != 2:
    print("Usage: apply_backend_changes.py <path-to-auth.rs>")
    sys.exit(2)

path = Path(sys.argv[1])
if not path.is_file():
    print(f"ERROR: file not found: {path}")
    sys.exit(1)

text = path.read_text(encoding="utf-8")

# Do not apply the modification twice.
if "let app_auth_header = req" in text:
    print("Platform auth changes already applied.")
    sys.exit(0)

old = '''    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| {
            tracing::warn!("Missing authorization header for {} {}", method, uri);
            ApiError::unauthorized("Missing authorization header")
        })?;

    let token = auth_header.strip_prefix("Bearer ").ok_or_else(|| {
        tracing::warn!("Invalid authorization header format for {} {}", method, uri);
        ApiError::unauthorized("Invalid authorization header format")
    })?;
'''

new = '''    // Manual fallback login uses the application's own JWT in a dedicated
    // header. Never read it from Authorization: that header belongs to the
    // platform Keycloak/ext_authz chain.
    let app_auth_header = req
        .headers()
        .get("x-sr-admin-token")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    // Normal platform navigation: ext_authz has already authenticated and
    // authorized the Keycloak bearer token before the request reaches Axum.
    // In this mode the per-release SQLite admin account is the application's
    // service identity. Project/user access is enforced by platform ext_authz,
    // not by the application's separate SQLite Casbin database.
    if app_auth_header.is_none() {
        let platform_header = req
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "));

        if platform_header.is_none() {
            tracing::warn!("Missing platform or Admin UI token for {} {}", method, uri);
            return Err(ApiError::unauthorized("Missing platform authorization"));
        }

        let service_user = sqlx::query_as::<_, (i64, Option<i64>)>(
            "SELECT id, NULLIF(organization_id, 0) FROM users WHERE username = 'admin' LIMIT 1",
        )
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);

        let Some((user_id, organization_id)) = service_user else {
            tracing::error!("Platform mode requires the provisioned local 'admin' user");
            return Err(ApiError::unauthorized("Admin UI service user is not provisioned"));
        };

        let needs_password_change: bool = sqlx::query_scalar(
            "SELECT first_log FROM users WHERE id = ?",
        )
        .bind(user_id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(Some(true))
        .unwrap_or(true);

        if needs_password_change {
            tracing::error!("Provisioner has not completed the initial admin password change");
            return Err(ApiError::unauthorized("Admin UI provisioning is incomplete"));
        }

        let username = "admin".to_string();
        req.extensions_mut().insert(user_id);
        req.extensions_mut().insert(username.clone());
        req.extensions_mut().insert(OrgContext {
            user_id,
            username,
            organization_id,
            is_super_admin: true,
        });

        return Ok(next.run(req).await);
    }

    let token = app_auth_header.unwrap();
'''

count = text.count(old)

if count != 1:
    print(
        f"ERROR: expected exactly one original auth block, found {count}. "
        "No changes made."
    )
    sys.exit(1)

updated = text.replace(old, new, 1)
path.write_text(updated, encoding="utf-8")

print(f"Applied platform auth changes to {path}")
