use super::*;

pub async fn native_callback(
    State(state): State<AuthState>,
    Query(query): Query<NativePollQuery>,
) -> Result<Response, AuthError> {
    let state_param = query.state.trim();
    if state_param.is_empty() {
        return Err(AuthError::Validation(
            "missing `state` parameter".to_string(),
        ));
    }
    let _ = state;
    let _ = query.code;

    let mut response = (
        StatusCode::GONE,
        axum::response::Html(
            r#"<!doctype html><html><body style="font-family:sans-serif;background:#07131c;color:#e6f4fb;text-align:center;padding-top:4rem"><h2>Sign-in link expired</h2><p>Return to the palette and start sign-in again.</p></body></html>"#,
        ),
    )
        .into_response();
    no_store(&mut response);
    Ok(response)
}

pub async fn native_poll(
    State(state): State<AuthState>,
    Query(query): Query<NativePollQuery>,
) -> Result<Response, AuthError> {
    let state_param = query.state.trim();
    if state_param.is_empty() {
        return Err(AuthError::Validation(
            "missing `state` parameter".to_string(),
        ));
    }
    let mut response = if let Some(row) = state
        .store
        .take_native_authorization_result(state_param)
        .await?
    {
        Json(NativePollResponse {
            code: Some(row.code),
        })
        .into_response()
    } else {
        (
            StatusCode::ACCEPTED,
            Json(NativePollResponse { code: None }),
        )
            .into_response()
    };
    no_store(&mut response);
    Ok(response)
}

pub(super) fn no_store(response: &mut Response) {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
        .headers_mut()
        .insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
}
